use core::{arch::naked_asm, ptr::NonNull};

use alloc::collections::vec_deque::VecDeque;
use spin::Mutex;
use x86_64::{
    instructions::interrupts::{self},
    registers::{
        control::Cr3,
        rflags::{self},
        segmentation::{CS, SS, Segment},
    },
    structures::paging::PhysFrame,
};

use crate::{
    debug, info, interrupts::apic::LAPIC_TIMER_VECTOR, tasks::kernelslab::STACK_ALLOCATOR, trace,
};

static TASK_SCHEDULER: Mutex<TaskScheduler> = Mutex::new(TaskScheduler::new());

/// stack size of kernel task in pages. Must be power of 2
pub const KSTACK_SIZE: u8 = 4;

/// adds the current kernel task to a pcb
///
/// this task should never finish
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
        task_type: TaskType::Kernel,
        regs: current_regs,
        state: TaskState::Running,        // Mark as currently running
        stack_start: NonNull::dangling(), // Kernel uses its own stack
        cr3: Cr3::read().0,
    };
    scheduler.task_list.push_front(current_task);
    debug!(
        "Added current kernel task to scheduler with uninit registers",
        current_regs.interrupt_rip, current_regs.interrupt_rsp
    );
}

/// adds a new kernel task to the scheduler
/// Each kernel task has a stack size of KSTACK_SIZE - 1, for a guard page
///
/// task should be a pointer to the function to run
pub fn kcreate_task(task_ptr: fn() -> !, name: &str) {
    let mut stack_allocator = STACK_ALLOCATOR.lock();
    let stack_start = stack_allocator.get_stack();

    let mut scheduler = TASK_SCHEDULER.lock();
    let task = ProcessControlBlock {
        task_type: TaskType::Kernel,
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
            interrupt_rflags: rflags::read_raw(),
            interrupt_rsp: stack_start.as_ptr() as u64,
            interrupt_ss: SS::get_reg().0 as u64,
        },
        state: TaskState::Ready,
        stack_start,
        cr3: Cr3::read().0,
    };
    scheduler.task_list.push_back(task);
    info!("created task {:?}", name);
    trace!("created task {:?}", task);
}

/// Exits a task
///
/// should be called at the end of every running kernel task when it wants to terminate
#[inline]
pub fn kexit_task() -> ! {
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
    pub stack_start: NonNull<()>,
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
}

/// Type of a task
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TaskType {
    Kernel,
    User, // TODO!
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
        "call schedule_inner", // call scheduler with rsp
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
    );
}

/// inner function to switch tasks
#[unsafe(no_mangle)]
unsafe extern "C" fn schedule_inner(current_task_context: *mut TaskRegisters) {
    let mut scheduler = TASK_SCHEDULER.lock();

    // save current task context first
    let mut current_task = scheduler.task_list.pop_front().unwrap();

    if current_task.state == TaskState::Terminated {
        trace!("task ended at {:#X}", current_task.regs.interrupt_rsp);
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
    unsafe { *current_task_context = next_task.regs };
}
