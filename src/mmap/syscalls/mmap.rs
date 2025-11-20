use core::ffi::{c_int, c_uint, c_void};
use core::num::NonZero;
use core::ptr::NonNull;

use const_format::assertc_eq;

assertc_eq!(linux_raw_sys::general::PROT_NONE, 0u32);
bitflags::bitflags! {
	#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
	pub struct MMapProt: c_uint {
		const EXEC = linux_raw_sys::general::PROT_EXEC;
		const READ = linux_raw_sys::general::PROT_READ;
		const WRITE = linux_raw_sys::general::PROT_WRITE;
	}

	#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
	pub struct MMapFlags: c_uint {
		const SHARED = linux_raw_sys::general::MAP_SHARED;
		const SHARED_VALIDATE = linux_raw_sys::general::MAP_SHARED_VALIDATE;
		const PRIVATE = linux_raw_sys::general::MAP_PRIVATE;
		const ANONYMOUS = linux_raw_sys::general::MAP_ANONYMOUS;
		const FIXED = linux_raw_sys::general::MAP_FIXED;
		const FIXED_NOREPLACE = linux_raw_sys::general::MAP_FIXED_NOREPLACE;
		const GROWSDOWN = linux_raw_sys::general::MAP_GROWSDOWN;
		const HUGETLB = linux_raw_sys::general::MAP_HUGETLB;
		const HUGE_2MB = linux_raw_sys::general::MAP_HUGE_2MB;
		const HUGE_1GB = linux_raw_sys::general::MAP_HUGE_1GB;
		const LOCKED = linux_raw_sys::general::MAP_LOCKED;
		const NONBLOCK = linux_raw_sys::general::MAP_NONBLOCK;
		const NORESERVE = linux_raw_sys::general::MAP_NORESERVE;
		const POPULATE = linux_raw_sys::general::MAP_POPULATE;
		const STACK = linux_raw_sys::general::MAP_STACK;
		const SYNC = linux_raw_sys::general::MAP_SYNC;
		const UNINITIALIZED = linux_raw_sys::general::MAP_UNINITIALIZED;
	}
}

/// `void *mmap(void *addr, size_t len, int prot, int flags, int fildes, off_t off);`
#[inline]
pub unsafe fn mmap(
	addr: Option<NonNull<c_void>>,
	len: NonZero<usize>,
	prot: MMapProt,
	flags: MMapFlags,
	fildes: Option<c_int>,
	off: linux_raw_sys::general::__kernel_off_t,
) -> Result<NonNull<c_void>, syscalls::Errno> {
	debug_assert!(MMapProt::all().contains(prot));
	debug_assert!(MMapFlags::all().contains(flags));

	syscalls::syscall!(
		syscalls::Sysno::mmap,
		addr.map(|ptr| ptr.as_ptr().cast_const()).unwrap_or(core::ptr::null()),
		len.get(),
		prot.bits(),
		flags.bits(),
		fildes.unwrap_or(-1),
		off
	)
	.map(|val| NonNull::new(val as *mut c_void).expect("Successfull mapping should always return nonnull pointer"))
}

#[cfg(test)]
mod test {
	use super::super::munmap;
	use super::*;

	#[test]
	fn map_and_unmap() {
		unsafe {
			let len = NonZero::new(4096).unwrap();
			let mapping = mmap(
				None,
				len,
				MMapProt::READ | MMapProt::WRITE,
				MMapFlags::PRIVATE | MMapFlags::ANONYMOUS,
				None,
				0,
			)
			.unwrap();

			{
				let first_word = mapping.cast::<u32>().as_mut();
				assert_eq!(*first_word, 0);
				*first_word = 42;
				assert_eq!(*first_word, 42);
			}

			munmap(mapping, len).unwrap();
		}
	}
}
