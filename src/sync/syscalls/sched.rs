#[inline]
pub fn sched_yield() {
	let res = unsafe { syscalls::syscall!(syscalls::Sysno::sched_yield) };
	debug_assert_eq!(res, Ok(0));
}
