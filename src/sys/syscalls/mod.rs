use core::ffi::c_int;

pub fn gettid() -> c_int {
	unsafe {
		let ret = syscalls::syscall!(syscalls::Sysno::gettid);
		debug_assert!(ret.is_ok());
		ret.unwrap_unchecked() as c_int
	}
}

pub unsafe fn kill(pid: c_int, sig: c_int) -> Result<(), syscalls::Errno> {
	syscalls::syscall!(syscalls::Sysno::kill, pid, sig).map(|ret| {
		debug_assert_eq!(ret, 0);
	})
}
