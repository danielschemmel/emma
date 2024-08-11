use core::ffi::c_int;

use static_assertions::const_assert_eq;

pub type Pid = u32;
pub type Tid = u32;

const_assert_eq!(linux_raw_sys::general::__kernel_pid_t::BITS, u32::BITS);

pub fn getpid() -> Pid {
	unsafe {
		let ret = syscalls::syscall!(syscalls::Sysno::getpid);
		debug_assert!(ret.is_ok());
		ret.unwrap_unchecked() as Pid
	}
}

pub fn gettid() -> Tid {
	unsafe {
		let ret = syscalls::syscall!(syscalls::Sysno::gettid);
		debug_assert!(ret.is_ok());
		ret.unwrap_unchecked() as Tid
	}
}

pub unsafe fn kill(pid: c_int, sig: c_int) -> Result<(), syscalls::Errno> {
	syscalls::syscall!(syscalls::Sysno::kill, pid, sig).map(|ret| {
		debug_assert_eq!(ret, 0);
	})
}
