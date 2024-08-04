use core::ffi::c_int;

pub fn gettid() -> c_int {
	unsafe {
		let ret = syscalls::syscall!(syscalls::Sysno::gettid);
		debug_assert!(ret.is_ok());
		ret.unwrap_unchecked() as c_int
	}
}
