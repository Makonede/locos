/// Syscall interface for user programs
///
/// Syscalls use the `syscall` instruction on x86_64
/// Calling convention:
/// - rax: syscall number
/// - rdi: arg1
/// - rsi: arg2
/// - rdx: arg3
/// - r10: arg4
/// - r8: arg5
/// - r9: arg6
///   Return value in rax
use x86_64::VirtAddr;
use x86_64::registers::control::EferFlags;
use x86_64::registers::rflags::RFlags;
use x86_64::registers::model_specific::{LStar, Star, SFMask, Efer};
use x86_64::structures::gdt::SegmentSelector;
use crate::tasks::scheduler::exit_task;
use crate::{debug, info, trace};
use crate::gdt::{KERNEL_CODE_SEGMENT_INDEX, KERNEL_DATA_SEGMENT_INDEX, USER_CODE_SEGMENT_INDEX, USER_DATA_SEGMENT_INDEX};

/// Initialize syscall support
/// Sets up the MSRs for the `syscall` instruction
pub fn init_syscall() {
    unsafe {
        let efer_val = Efer::read();
        Efer::write(efer_val | EferFlags::SYSTEM_CALL_EXTENSIONS);

        let kernel_cs = SegmentSelector::new(KERNEL_CODE_SEGMENT_INDEX, x86_64::PrivilegeLevel::Ring0);
        let kernel_ss = SegmentSelector::new(KERNEL_DATA_SEGMENT_INDEX, x86_64::PrivilegeLevel::Ring0);
        let user_cs_32 = SegmentSelector::new(USER_DATA_SEGMENT_INDEX, x86_64::PrivilegeLevel::Ring3);
        let user_cs = SegmentSelector::new(USER_CODE_SEGMENT_INDEX, x86_64::PrivilegeLevel::Ring3);

        Star::write(user_cs_32, user_cs, kernel_cs, kernel_ss).unwrap();
        LStar::write(VirtAddr::from_ptr(syscall_handler as *const ()));
        SFMask::write(RFlags::INTERRUPT_FLAG);
    }

    info!("Syscall support initialized");
}

/// Assembly syscall handler entry point
/// Saves registers on stack (Linux pt_regs style) and calls handle_syscall
///
/// TODO: NOT SMP SAFE
#[unsafe(naked)]
unsafe extern "C" fn syscall_handler() {
    core::arch::naked_asm!(
        "mov [rip + {USER_RSP}], rsp",

        "mov rsp, [rip + {KERNEL_SYSCALL_STACK}]",

        "push qword ptr [rip + {USER_RSP}]",  // user rsp
        "push r11",
        "push rcx",
        "push rax",
        "push rdi",
        "push rsi",
        "push rdx",
        "push r10",
        "push r8",
        "push r9",
        "push rbx",
        "push rbp",
        "push r12",
        "push r13",
        "push r14",
        "push r15",

        "mov rdi, rsp",
        "call {handle_syscall}",

        "pop r15",
        "pop r14",
        "pop r13",
        "pop r12",
        "pop rbp",
        "pop rbx",
        "pop r9",
        "pop r8",
        "pop r10",
        "pop rdx",
        "pop rsi",
        "pop rdi",
        "add rsp, 8",
        "pop rcx",
        "pop r11",
        "pop rsp",

        "sysretq",
        USER_RSP = sym USER_RSP,
        KERNEL_SYSCALL_STACK = sym KERNEL_SYSCALL_STACK,
        handle_syscall = sym handle_syscall,
    )
}

/// Temporary storage for user RSP during syscall
/// ts very ugly
static mut USER_RSP: u64 = 0;

/// Kernel stack for syscall handling
/// TODO: replace with something better asap
static mut KERNEL_SYSCALL_STACK: u64 = 0;

/// Syscall register state (Linux pt_regs style)
///
/// This structure matches the exact stack layout created by syscall_handler.
/// Registers are pushed in reverse order so the struct layout matches stack order.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct SyscallRegs {
    // callee saved
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub rbp: u64,
    pub rbx: u64,

    // beginning of syscall arguments
    /// argument 6
    pub r9: u64,
    /// argument 5
    pub r8: u64,
    /// argument 4
    pub r10: u64,
    /// argument 3
    pub rdx: u64,
    /// argument 2
    pub rsi: u64,
    /// argument 1
    pub rdi: u64,

    /// syscall number (original value in rax)
    pub rax: u64,
    pub rip: u64,
    pub rflags: u64,
    pub rsp: u64,
}

/// Syscall numbers
#[repr(u64)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyscallNumber {
    Exit = 0,
    Write = 1,
    Read = 2,
}

impl SyscallNumber {
    pub fn from_u64(n: u64) -> Option<Self> {
        match n {
            0 => Some(SyscallNumber::Exit),
            1 => Some(SyscallNumber::Write),
            2 => Some(SyscallNumber::Read),
            _ => None,
        }
    }
}

/// Syscall handler - called from assembly stub with pointer to pt_regs
///
/// # Safety
/// Must only be called from syscall interrupt handler
pub unsafe extern "C" fn handle_syscall(regs: *mut SyscallRegs) -> u64 {
    let regs = unsafe { &*regs };
    
    let syscall = match SyscallNumber::from_u64(regs.rax) {
        Some(s) => s,
        None => {
            debug!("Unknown syscall number: {}", regs.rax);
            return u64::MAX; // Error
        }
    };

    debug!("Syscall: {:?}(rdi={:#x}, rsi={:#x}, rdx={:#x})", syscall, regs.rdi, regs.rsi, regs.rdx);

    match syscall {
        SyscallNumber::Exit => sys_exit(regs.rdi as i32),
        SyscallNumber::Write => sys_write(regs.rdi as i32, regs.rsi as usize as *const u8, regs.rdx as usize),
        SyscallNumber::Read => unimplemented!("need to read from keyboard"),
    }
}

/// sys_exit - terminate the calling task
///
/// # Arguments
/// * `exit_code` - Exit status code
///
/// # Returns
/// Never returns (task is terminated)
fn sys_exit(_exit_code: i32) -> u64 {
    trace!("Task exiting with code {}", _exit_code);
    
    exit_task();
}

/// sys_write - write to a file descriptor
///
/// # Arguments
/// * `fd` - File descriptor (0=stdin, 1=stdout, 2=stderr)
/// * `buf` - Pointer to buffer in user space
/// * `count` - Number of bytes to write
///
/// # Returns
/// Number of bytes written, or -1 on error
fn sys_write(fd: i32, buf: *const u8, count: usize) -> u64 {
    use crate::{print, serial_print};
    
    if fd != 1 && fd != 2 {
        debug!("sys_write: unsupported fd {}", fd);
        return u64::MAX;
    }
    
    let buf_addr = buf as usize;
    if buf_addr >= 0x0000_8000_0000_0000 || buf_addr.saturating_add(count) >= 0x0000_8000_0000_0000 {
        debug!("sys_write: invalid buffer address {:#x}", buf_addr);
        return u64::MAX;
    }
    
    if count == 0 {
        return 0;
    }
    
    let slice = unsafe { core::slice::from_raw_parts(buf, count) };
    
    let output = match core::str::from_utf8(slice) {
        Ok(s) => s,
        Err(_) => {
            debug!("sys_write: invalid UTF-8 in buffer");
            return u64::MAX; // Error
        }
    };
    
    serial_print!("{}", output);
    if fd == 1 {
        print!("{}", output);
    }
    
    count as u64
}
