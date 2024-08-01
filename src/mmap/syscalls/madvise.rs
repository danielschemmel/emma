use core::ffi::{c_uint, c_void};
use core::ptr::NonNull;

use static_assertions::const_assert_eq;

const_assert_eq!(linux_raw_sys::general::MADV_NORMAL, 0);
bitflags::bitflags! {
	pub struct MAdviseAdvice: c_uint {
		const RANDOM = linux_raw_sys::general::MADV_RANDOM;
		const SEQUENTIAL = linux_raw_sys::general::MADV_SEQUENTIAL;
		const WILLNEED = linux_raw_sys::general::MADV_WILLNEED;
		const DONTNEED = linux_raw_sys::general::MADV_DONTNEED;
		const REMOVE = linux_raw_sys::general::MADV_REMOVE;
		const DONTFORK = linux_raw_sys::general::MADV_DONTFORK;
		const DOFORK = linux_raw_sys::general::MADV_DOFORK;
		const HWPOISON = linux_raw_sys::general::MADV_HWPOISON;
		const MERGEABLE = linux_raw_sys::general::MADV_MERGEABLE;
		const SOFT_OFFLINE = linux_raw_sys::general::MADV_SOFT_OFFLINE;
		const HUGEPAGE = linux_raw_sys::general::MADV_HUGEPAGE;
		const NOHUGEPAGE = linux_raw_sys::general::MADV_NOHUGEPAGE;
		const COLLAPSE = linux_raw_sys::general::MADV_COLLAPSE;
		const DONTDUMP = linux_raw_sys::general::MADV_DONTDUMP;
		const DODUMP = linux_raw_sys::general::MADV_DODUMP;
		const FREE = linux_raw_sys::general::MADV_FREE;
		const WIPEONFORK = linux_raw_sys::general::MADV_WIPEONFORK;
		const KEEPONFORK = linux_raw_sys::general::MADV_KEEPONFORK;
		const COLD = linux_raw_sys::general::MADV_COLD;
		const PAGEOUT = linux_raw_sys::general::MADV_PAGEOUT;
		const POPULATE_READ = linux_raw_sys::general::MADV_POPULATE_READ;
		const POPULATE_WRITE = linux_raw_sys::general::MADV_POPULATE_WRITE;
	}
}

/// `int madvise(void addr[.length], size_t length, int advice);`
#[inline]
pub unsafe fn madvise(addr: NonNull<c_void>, length: usize, advice: MAdviseAdvice) -> Result<(), syscalls::Errno> {
	syscalls::syscall!(syscalls::Sysno::madvise, addr.as_ptr(), length, advice.bits()).map(|ret| {
		debug_assert_eq!(ret, 0);
	})
}
