use core::arch::naked_asm;

use alloc::collections::vec_deque::VecDeque;
use spin::Mutex;

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
    pub state: TaskRegisters,
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
    // these might not be present!
    interrupt_rsp: u64,
    interrupt_ss: u64,
}

/// add main to the process list
pub fn initialize_multitasking() {
    todo!()
}

/// switch to a task
#[naked]
#[unsafe(no_mangle)]
unsafe extern "C" fn schedule() {
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
    
    let mut head = scheduler.task_list.pop_front()
        .expect("no processes in queue. perhaps you forgot to add main?");

    head.state = unsafe { *current_task_context };

    scheduler.task_list.push_back(head);

    let next_task = scheduler.task_list.pop_front().unwrap();
    unsafe { *current_task_context = next_task.state };
}
