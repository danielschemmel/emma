use core::ffi::c_void;
use core::num::NonZero;
use core::ptr::NonNull;

/// `void *mremap(void old_address[.old_size], size_t old_size, size_t new_size, 0);`
#[inline]
pub unsafe fn mremap_resize(
	address: NonNull<c_void>,
	old_size: NonZero<usize>,
	new_size: NonZero<usize>,
) -> Result<(), syscalls::Errno> {
	syscalls::syscall!(
		syscalls::Sysno::mremap,
		address.as_ptr(),
		old_size.get(),
		new_size.get()
	)
	.map(|ret| {
		debug_assert_eq!(ret as *const c_void, address.as_ptr().cast_const());
	})
}
