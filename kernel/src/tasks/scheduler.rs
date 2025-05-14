use core::{arch::naked_asm, ptr::NonNull};

use alloc::collections::vec_deque::VecDeque;
use spin::Mutex;
use x86_64::{registers::{rflags, segmentation::{Segment, CS, SS}}, structures::paging::Mapper};

use crate::{gdt::KERNEL_CODE_SEGMENT_INDEX, memory::PAGE_TABLE};

static TASK_SCHEDULER: Mutex<TaskScheduler> = Mutex::new(TaskScheduler::new());

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
#[derive(Clone, Copy, Debug)]
#[repr(C)]
struct ProcessControlBlock {
    pub regs: TaskRegisters,
    pub state: TaskState,
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

// Stores task registers in reverse order of stack push during context switch
#[derive(Clone, Copy, Debug)]
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
#[naked]
#[unsafe(no_mangle)]
unsafe extern "x86-interrupt" fn schedule() {
    unsafe {
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
            "mov rdi, rsp", // put current task's stack pointer
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
}

/// inner function to switch tasks
#[unsafe(no_mangle)]
unsafe extern "C" fn schedule_inner(current_task_context: *mut TaskRegisters) {
    let mut scheduler = TASK_SCHEDULER.lock();

    if scheduler.task_list.front().unwrap().state == TaskState::Terminated {
        scheduler.task_list.pop_front();
    }
    
    // save current task context
    let mut head = scheduler.task_list.pop_front().unwrap();
    head.regs = unsafe { *current_task_context };
    scheduler.task_list.push_back(head);

    // run front task
    let next_task = scheduler.task_list.front().unwrap();
    unsafe { *current_task_context = next_task.regs };
}
