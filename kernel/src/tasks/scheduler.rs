//! Task scheduler for preemptive multitasking.
//!
//! Provides round-robin scheduling for both kernel and user tasks.

use core::{arch::naked_asm, error::Error};

use alloc::{boxed::Box, collections::vec_deque::VecDeque, format};
use spin::Mutex;
use x86_64::{
    VirtAddr,
    instructions::interrupts::{self},
    registers::{
        control::Cr3,
        rflags::{self},
        segmentation::{CS, SS, Segment},
    },
    structures::paging::{FrameAllocator, FrameDeallocator, Mapper, OffsetPageTable, Page, PageTable, PageTableFlags, PhysFrame},
};

use crate::{
    debug, gdt::{USER_CODE_SEGMENT_INDEX, USER_DATA_SEGMENT_INDEX, set_kernel_stack}, info, interrupts::apic::LAPIC_TIMER_VECTOR, memory::FRAME_ALLOCATOR, syscall::set_syscall_stack, tasks::kernelslab::{INITIAL_STACK_PAGES, STACK_ALLOCATOR, get_user_stack, return_user_stack}, trace
};

/// Global task scheduler instance
static TASK_SCHEDULER: Mutex<TaskScheduler> = Mutex::new(TaskScheduler::new());

/// Stack size for kernel tasks in pages (must be power of 2)
pub const KSTACK_SIZE: u8 = 4;

/// Initialize multitasking by adding the current kernel task to the scheduler
///
/// This task should never finish.
pub fn kinit_multitasking() {
    let current_regs = TaskRegisters {
        rax: 0,
        rbx: 0,
        rcx: 0,
        rdx: 0,
        rsi: 0,
        rdi: 0,
        rbp: 0,
        r8: 0,
        r9: 0,
        r10: 0,
        r11: 0,
        r12: 0,
        r13: 0,
        r14: 0,
        r15: 0,
        interrupt_rip: 0,
        interrupt_cs: CS::get_reg().0 as u64,
        interrupt_rflags: rflags::read_raw(),
        interrupt_rsp: 0,
        interrupt_ss: SS::get_reg().0 as u64,
    };

    let mut scheduler = TASK_SCHEDULER.lock();
    let current_task = ProcessControlBlock {
        task_type: TaskType::Kernel {
            stack_start: None,
        },
        regs: current_regs,
        state: TaskState::Running,        // Mark as currently running
        cr3: Cr3::read().0,
    };
    scheduler.task_list.push_front(current_task);
    debug!(
        "Added current kernel task to scheduler with uninit registers",
    );
}

/// Create a new kernel task and add it to the scheduler
///
/// Each kernel task has a stack size of KSTACK_SIZE - 1 pages (with 1 guard page).
///
/// # Arguments
/// * `task_ptr` - Function pointer to run as the task
/// * `name` - Name of the task for debugging
pub fn kcreate_task(task_ptr: fn() -> !, name: &str) {
    let mut stack_allocator = STACK_ALLOCATOR.lock();
    let stack_start = stack_allocator.get_stack().expect("Failed to allocate kernel stack");

    let mut scheduler = TASK_SCHEDULER.lock();
    let task = ProcessControlBlock {
        task_type: TaskType::Kernel {
            stack_start: Some(stack_start),
        },
        regs: TaskRegisters {
            rax: 0,
            rbx: 0,
            rcx: 0,
            rdx: 0,
            rsi: 0,
            rdi: 0,
            rbp: 0,
            r8: 0,
            r9: 0,
            r10: 0,
            r11: 0,
            r12: 0,
            r13: 0,
            r14: 0,
            r15: 0,

            interrupt_rip: task_ptr as usize as u64,
            interrupt_cs: CS::get_reg().0 as u64,
            interrupt_rflags: rflags::read_raw() | 0x200,
            interrupt_rsp: stack_start.as_u64(),
            interrupt_ss: SS::get_reg().0 as u64,
        },
        state: TaskState::Ready,
        cr3: Cr3::read().0,
    };
    scheduler.task_list.push_back(task);
    info!("created task {:?}", name);
    trace!("created task {:?}", task);
}

/// Reconstruct an OffsetPageTable from a CR3 value
///
/// # Safety
/// The caller must ensure that the CR3 points to a valid page table.
unsafe fn get_user_page_table_from_cr3(cr3: PhysFrame) -> OffsetPageTable<'static> {
    let hhdm_offset = FRAME_ALLOCATOR.lock().as_ref().unwrap().hddm_offset;
    let l4_virt = VirtAddr::new(cr3.start_address().as_u64() + hhdm_offset);
    let l4_table: &mut PageTable = unsafe { &mut *l4_virt.as_mut_ptr() };
    unsafe { OffsetPageTable::new(l4_table, VirtAddr::new(hhdm_offset)) }
}

/// Recursively deallocate all page table frames in the user space portion
///
/// Processes entries 0-255 of a page table hierarchy.
///
/// # Safety
/// - The page table must be valid and not in use
/// - Should only be called on user page tables, not the kernel page table
/// - The page table must not be the currently active page table
unsafe fn deallocate_user_page_table_recursive(table_frame: PhysFrame, level: u8) {
    let hhdm_offset = FRAME_ALLOCATOR.lock().as_ref().unwrap().hddm_offset;
    let table_virt = VirtAddr::new(table_frame.start_address().as_u64() + hhdm_offset);
    let table: &PageTable = unsafe { &*table_virt.as_ptr() };

    for i in 0..256 {
        let entry = &table[i];

        if entry.flags().contains(PageTableFlags::PRESENT) {
            let child_frame = entry.frame().unwrap();

            if level > 1 {
                unsafe {
                    deallocate_user_page_table_recursive(child_frame, level - 1);
                }
            }

            unsafe {
                FRAME_ALLOCATOR.lock().as_mut().unwrap().deallocate_frame(child_frame);
            }
        }
    }
}

/// Creates a new user page table by copying the kernel's page table
///
/// Returns the physical frame of the new page table
/// Remember to dealloc frame
fn create_user_page_table() -> PhysFrame {
    let mut frame_allocator = FRAME_ALLOCATOR.lock();
    let frame_allocator = frame_allocator.as_mut().unwrap();

    let new_l4_frame = frame_allocator
        .allocate_frame()
        .expect("failed to allocate frame for user page table");

    let hhdm_offset = frame_allocator.hddm_offset;
    let new_l4_virt = VirtAddr::new(new_l4_frame.start_address().as_u64() + hhdm_offset);
    let new_l4_table: &mut PageTable = unsafe { &mut *new_l4_virt.as_mut_ptr() };

    new_l4_table.zero();

    let current_l4_frame = Cr3::read().0;
    let current_l4_virt = VirtAddr::new(current_l4_frame.start_address().as_u64() + hhdm_offset);
    let current_l4_table: &PageTable = unsafe { &*current_l4_virt.as_ptr() };

    for i in 256..512 {
        new_l4_table[i] = current_l4_table[i].clone();
    }

    debug!("Created user page table at {:#x}", new_l4_frame.start_address());
    new_l4_frame
}

/// Creates a new userspace task
///
/// # Arguments
/// * `entry_point` - Virtual address where the user code starts
/// * `code` - Optional program code to load at entry_point address
/// * `name` - Name of the task for debugging
pub fn ucreate_task(entry_point: VirtAddr, code: Option<&[u8]>, name: &str) -> Result<(), Box<dyn Error>> {
    if entry_point.as_u64() >= 0x0000_8000_0000_0000 {
        return Err("Entry point must be in user address space (< 0x0000_8000_0000_0000)".into());
    }

    let user_cr3 = create_user_page_table();

    let hhdm_offset = FRAME_ALLOCATOR.lock().as_ref().unwrap().hddm_offset;
    let user_l4_virt = VirtAddr::new(user_cr3.start_address().as_u64() + hhdm_offset);
    let user_l4_table: &mut PageTable = unsafe { &mut *user_l4_virt.as_mut_ptr() };
    let mut user_page_table = unsafe { OffsetPageTable::new(user_l4_table, VirtAddr::new(hhdm_offset)) };

    if let Some(code_data) = code { // deallocated on task exit
        let code_start_page = Page::containing_address(entry_point);
        let code_end_page = Page::containing_address(entry_point + (code_data.len() as u64 - 1));
        
        let mut code_offset = 0;
        for page in Page::range_inclusive(code_start_page, code_end_page) {
            let frame = {
                let mut frame_allocator = FRAME_ALLOCATOR.lock();
                frame_allocator.as_mut().unwrap()
                    .allocate_frame()
                    .ok_or("Failed to allocate frame for code")?
            };
            
            unsafe {
                user_page_table.map_to(
                    page,
                    frame,
                    PageTableFlags::PRESENT | PageTableFlags::USER_ACCESSIBLE | PageTableFlags::WRITABLE,
                    FRAME_ALLOCATOR.lock().as_mut().unwrap(),
                ).map_err(|e| format!("Failed to map code page: {e:?}"))?
                .flush();
            }
            
            let frame_virt = VirtAddr::new(frame.start_address().as_u64() + hhdm_offset);
            let bytes_to_copy = core::cmp::min(4096, code_data.len() - code_offset);
            unsafe {
                core::ptr::copy_nonoverlapping(
                    code_data[code_offset..].as_ptr(),
                    frame_virt.as_mut_ptr::<u8>(),
                    bytes_to_copy,
                );
            }
            code_offset += bytes_to_copy;
        }
        debug!("Mapped {} bytes of code at {:#x}", code_data.len(), entry_point);
    }

    let stack_allocation = match get_user_stack(&mut user_page_table) {
        Ok(alloc) => alloc,
        Err(e) => {
            unsafe {
                use x86_64::structures::paging::FrameDeallocator;
                FRAME_ALLOCATOR.lock().as_mut().unwrap().deallocate_frame(user_cr3);
            }
            return Err(e.into());
        }
    };

    let kernel_stack = STACK_ALLOCATOR.lock().get_stack().map_err(|e| -> Box<dyn Error> {
        unsafe {
            let mut user_page_table = get_user_page_table_from_cr3(user_cr3);
            return_user_stack(&mut user_page_table, UserInfo {
                stack_start: stack_allocation.stack_start,
                stack_end: stack_allocation.stack_end,
                stack_size: INITIAL_STACK_PAGES,
                kernel_stack: VirtAddr::zero(),
            });
            FRAME_ALLOCATOR.lock().as_mut().unwrap().deallocate_frame(user_cr3);
        }
        e.into()
    })?;

    let mut scheduler = TASK_SCHEDULER.lock();
    let task = ProcessControlBlock {
        task_type: TaskType::User(UserInfo {
            stack_start: stack_allocation.stack_start,
            stack_end: stack_allocation.stack_end,
            stack_size: INITIAL_STACK_PAGES,
            kernel_stack,
        }),
        regs: TaskRegisters {
            rax: 0,
            rbx: 0,
            rcx: 0,
            rdx: 0,
            rsi: 0,
            rdi: 0,
            rbp: 0,
            r8: 0,
            r9: 0,
            r10: 0,
            r11: 0,
            r12: 0,
            r13: 0,
            r14: 0,
            r15: 0,

            interrupt_rip: entry_point.as_u64(),
            interrupt_cs: ((USER_CODE_SEGMENT_INDEX << 3) | 3) as u64,
            interrupt_rflags: rflags::read_raw() | 0x200, // Enable interrupts
            interrupt_rsp: stack_allocation.stack_start.as_u64(),
            interrupt_ss: ((USER_DATA_SEGMENT_INDEX << 3) | 3) as u64,
        },
        state: TaskState::Ready,
        cr3: user_cr3,
    };
    scheduler.task_list.push_back(task);
    info!("created user task {:?} at {:#x}", name, entry_point);
    trace!("created user task {:?}", task);
    Ok(())
}

/// Get the current task's stack bounds and CR3
///
/// Returns (stack_bottom, stack_top, cr3, is_user_task)
/// Returns None if no task is running or if it's a kernel task
pub fn get_current_task_stack_info() -> Option<(VirtAddr, VirtAddr, PhysFrame)> {
    let scheduler = TASK_SCHEDULER.lock();
    let task = scheduler.task_list.front()?;

    if let TaskType::User(user_info) = task.task_type {
        Some((user_info.stack_end, user_info.stack_start, task.cr3))
    } else {
        None
    }
}

/// Try to grow the user stack by mapping a new page
///
/// Returns true if the fault was successfully handled (stack grew),
/// false if the fault is not a valid stack growth (e.g., stack overflow)
///
/// # Arguments
/// * `fault_addr` - The virtual address that caused the page fault
///
/// # Safety
/// This function must only be called from the page fault handler
pub unsafe fn try_grow_user_stack(fault_addr: VirtAddr) -> Result<(), StackGrowthError> {
    let Some((stack_bottom, stack_top, user_cr3)) = get_current_task_stack_info() else {
        return Err(StackGrowthError::NotUserTask);
    };

    if fault_addr < stack_bottom {
        debug!(
            "Stack overflow detected: fault at {:#x}, stack_bottom {:#x}",
            fault_addr, stack_bottom
        );
        return Err(StackGrowthError::StackOverflow);
    }

    if fault_addr >= stack_top {
        return Err(StackGrowthError::StackUnderflow);
    }

    let page = Page::containing_address(fault_addr);

    debug!(
        "Growing user stack: mapping page at {:#x} (fault at {:#x})",
        page.start_address(),
        fault_addr
    );

    let mut user_page_table = unsafe { get_user_page_table_from_cr3(user_cr3) };

    let frame = {
        let mut frame_allocator = FRAME_ALLOCATOR.lock();
        let frame_allocator = frame_allocator.as_mut().unwrap();
        match frame_allocator.allocate_frame() {
            Some(frame) => frame,
            None => {
                debug!("Failed to allocate frame for stack growth");
                return Err(StackGrowthError::Other);
            }
        }
    };

    match unsafe {
        user_page_table.map_to(
            page,
            frame,
            PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER_ACCESSIBLE,
            FRAME_ALLOCATOR.lock().as_mut().unwrap(),
        )
    } {
        Ok(flush) => {
            flush.flush();
            trace!("Successfully mapped stack page at {:#x}", page.start_address());

            let mut scheduler = TASK_SCHEDULER.lock();
            if let Some(task) = scheduler.task_list.front_mut()
                && let TaskType::User(ref mut user_info) = task.task_type {
                    user_info.stack_size += 1;
                    trace!("Updated stack_size to {} pages", user_info.stack_size);
                }

            Ok(())
        }
        Err(e) => {
            debug!("Failed to map stack page: {:?}", e);
            unsafe {
                use x86_64::structures::paging::FrameDeallocator;
                FRAME_ALLOCATOR.lock().as_mut().unwrap().deallocate_frame(frame);
            }
            Err(StackGrowthError::Other)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StackGrowthError {
    StackOverflow,
    StackUnderflow,
    NotUserTask,
    Other,
}

/// Yields the current task to the scheduler, waiting for an interrupt
pub fn kyield_task(interrupt: u8) {
    interrupts::disable();
    {
        let mut scheduler = TASK_SCHEDULER.lock();
        let current_task = scheduler.task_list.front_mut().unwrap();
        current_task.state = TaskState::Waiting(WaitReason::Interrupt(interrupt));
    }
    interrupts::enable();

    unsafe {
        core::arch::asm!("int {}", const LAPIC_TIMER_VECTOR);
    }
}

/// wakes all tasks waiting for specified interrupt
/// 
/// O(n) but doesnt matter in this stage
pub fn wake_tasks(interrupt: u8) {
    let mut scheduler = TASK_SCHEDULER.lock();
    scheduler
        .task_list
        .iter_mut()
        .filter(|x| x.state == TaskState::Waiting(WaitReason::Interrupt(interrupt)))
        .for_each(|x| x.state = TaskState::Ready);
}

/// Terminates the current task, handing control to the scheduler
///
/// should be called at the end of every running task when it wants to terminate
#[inline]
pub fn exit_task() -> ! {
    interrupts::disable();
    {
        let mut scheduler = TASK_SCHEDULER.lock();
        let current_task = scheduler.task_list.front_mut().unwrap();
        current_task.state = TaskState::Terminated;
    }
    interrupts::enable();

    unsafe {
        core::arch::asm!("int {}", const LAPIC_TIMER_VECTOR, options(noreturn));
    }
}

struct TaskScheduler {
    task_list: VecDeque<ProcessControlBlock>,
}

unsafe impl Send for TaskScheduler {}

impl TaskScheduler {
    const fn new() -> Self {
        TaskScheduler {
            task_list: VecDeque::new(),
        }
    }
}

/// Stores information about a running process
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
struct ProcessControlBlock {
    pub task_type: TaskType,
    pub regs: TaskRegisters,
    pub state: TaskState,
    /// page table for process
    pub cr3: PhysFrame,
}

/// State of a task
/// - Ready: Task is ready to run
/// - Running: Task is currently running
/// - Terminated: Task has finished running
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TaskState {
    Ready,
    Running,
    Terminated,
    Waiting(WaitReason),
}

/// Why are we waiting
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WaitReason {
    Interrupt(u8),
}

/// Information about a user task's stack
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UserInfo {
    pub stack_start: VirtAddr,
    pub stack_end: VirtAddr,
    pub stack_size: u64,
    pub kernel_stack: VirtAddr,
}

/// Type of a task
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TaskType {
    Kernel {
        stack_start: Option<VirtAddr>,
    },
    User(UserInfo),
}

// Stores task registers in reverse order of stack push during context switch
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
struct TaskRegisters {
    r15: u64,
    r14: u64,
    r13: u64,
    r12: u64,
    r11: u64,
    r10: u64,
    r9: u64,
    r8: u64,
    rbp: u64,
    rdi: u64,
    rsi: u64,
    rdx: u64,
    rcx: u64,
    rbx: u64,
    rax: u64,

    // pushed by cpu after interrupt
    interrupt_rip: u64,
    interrupt_cs: u64,
    interrupt_rflags: u64,
    interrupt_rsp: u64,
    interrupt_ss: u64,
}

/// switch to a task
///
/// # Safety
/// what do you think might be unsafe about this
#[unsafe(naked)]
#[unsafe(no_mangle)]
pub unsafe extern "x86-interrupt" fn schedule() {
    naked_asm!(
        "push rax",
        "push rbx",
        "push rcx",
        "push rdx",
        "push rsi",
        "push rdi",
        "push rbp",
        "push r8",
        "push r9",
        "push r10",
        "push r11",
        "push r12",
        "push r13",
        "push r14",
        "push r15",
        "mov rdi, rsp",        // put current task's stack pointer
        "call {schedule_inner}", // call scheduler with rsp
        // send EOI to lapic using MSR 0x80B
        "xor eax, eax",
        "xor edx, edx",
        "mov ecx, 0x80B",
        "wrmsr",
        // pop new task registers in reverse order
        "pop r15",
        "pop r14",
        "pop r13",
        "pop r12",
        "pop r11",
        "pop r10",
        "pop r9",
        "pop r8",
        "pop rbp",
        "pop rdi",
        "pop rsi",
        "pop rdx",
        "pop rcx",
        "pop rbx",
        "pop rax",
        "iretq",
        schedule_inner = sym schedule_inner,
    );
}

/// inner function to switch tasks
unsafe extern "C" fn schedule_inner(current_task_context: *mut TaskRegisters) {
    let mut scheduler = TASK_SCHEDULER.lock();

    // save current task context first
    let mut current_task = scheduler.task_list.pop_front().unwrap();

    if current_task.state == TaskState::Terminated {
        trace!("task ended at {:#X}", current_task.regs.interrupt_rsp);
        match current_task.task_type {
            TaskType::Kernel { stack_start: Some(stack_start) } => {
                STACK_ALLOCATOR.lock().return_stack(stack_start);
            }
            TaskType::User(user_info) => {
                STACK_ALLOCATOR.lock().return_stack(user_info.kernel_stack);

                debug!("User task terminated, deallocating all user memory");

                unsafe {
                    deallocate_user_page_table_recursive(current_task.cr3, 4);
                }
                debug!("User task page tables and all mapped frames deallocated");

                unsafe {
                    use x86_64::structures::paging::FrameDeallocator;
                    FRAME_ALLOCATOR.lock().as_mut().unwrap().deallocate_frame(current_task.cr3);
                }
                debug!("User task CR3 frame deallocated at {:#x}", current_task.cr3.start_address());
            }
            _ => {}
        }
    } else if let TaskState::Waiting(WaitReason::Interrupt(_interrupt)) = current_task.state {
        current_task.regs = unsafe { *current_task_context };
        scheduler.task_list.push_back(current_task);
    } else {
        current_task.state = TaskState::Ready;
        current_task.regs = unsafe { *current_task_context };
        trace!("task registers: {:?}", current_task.regs);
        scheduler.task_list.push_back(current_task);
        trace!("task paused at {:#X}", current_task.regs.interrupt_rsp);

        trace!(
            "{:#X}",
            scheduler.task_list.front_mut().unwrap().regs.interrupt_rsp
        );
    }

    // run front task
    let next_task = scheduler.task_list.front_mut().unwrap();

    #[cfg(test)]
    {
        if current_task == *next_task {
            use crate::testing::{QemuExitCode, exit_qemu};
            exit_qemu(QemuExitCode::Success);
        }
    }

    trace!("task for next: {:?}", next_task);
    trace!("next task at {:#X}", next_task.regs.interrupt_rsp);
    next_task.state = TaskState::Running;

    if let TaskType::User(user_info) = next_task.task_type {
        unsafe {
            set_kernel_stack(user_info.kernel_stack);
            set_syscall_stack(user_info.kernel_stack);
        }
    }

    let current_cr3 = Cr3::read().0;
    if current_cr3 != next_task.cr3 {
        trace!("Switching CR3 from {:#x} to {:#x}", current_cr3.start_address(), next_task.cr3.start_address());
        unsafe {
            Cr3::write(next_task.cr3, x86_64::registers::control::Cr3Flags::empty());
        }
    }

    unsafe { *current_task_context = next_task.regs };
}
