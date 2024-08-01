use core::ffi::c_void;
use core::num::NonZero;
use core::ptr::NonNull;

/// `int munmap(void addr[.length], size_t length);`
#[inline]
pub unsafe fn munmap(addr: NonNull<c_void>, length: NonZero<usize>) -> Result<(), syscalls::Errno> {
	syscalls::syscall!(syscalls::Sysno::munmap, addr.as_ptr(), length.get()).map(|ret| {
		debug_assert_eq!(ret, 0);
	})
}
