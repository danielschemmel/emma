#![allow(dead_code)]

mod syscalls;
use core::ffi::c_void;
use core::num::NonZero;
use core::ptr::NonNull;

pub use self::syscalls::*;

#[inline]
unsafe fn move_mapping_down(
	old_addr: NonNull<c_void>,
	size: NonZero<usize>,
	amount: NonZero<usize>,
	prot: MMapProt,
	flags: MMapFlags,
) -> Option<NonNull<c_void>> {
	unsafe {
		let target_addr = old_addr.byte_sub(amount.get());
		let filler = mmap(
			Some(target_addr),
			amount,
			prot,
			flags | MMapFlags::FIXED_NOREPLACE,
			None,
			0,
		)
		.ok()?;
		assert_eq!(
			filler, target_addr,
			"Emma is not compatible with linux kernels that do not recognize MAP_FIXED_NOREPLACE (pre 4.17)."
		);

		munmap(target_addr.byte_add(size.get()), amount).unwrap();

		Some(target_addr)
	}
}

#[inline]
unsafe fn move_mapping_up(
	old_addr: NonNull<c_void>,
	size: NonZero<usize>,
	amount: NonZero<usize>,
	prot: MMapProt,
	flags: MMapFlags,
) -> Option<NonNull<c_void>> {
	unsafe {
		let target_addr = old_addr.byte_add(size.get());
		let filler = mmap(
			Some(target_addr),
			amount,
			prot,
			flags | MMapFlags::FIXED_NOREPLACE,
			None,
			0,
		)
		.ok()?;
		assert_eq!(
			filler, target_addr,
			"Emma is not compatible with linux kernels that do not recognize MAP_FIXED_NOREPLACE (pre 4.17)."
		);

		munmap(old_addr, amount).unwrap();

		Some(old_addr.byte_add(amount.get()))
	}
}

unsafe fn mmap_aligned_rec(
	size: NonZero<usize>,
	alignment: NonZero<usize>,
	recursive_retries: usize,
) -> Option<NonNull<c_void>> {
	unsafe {
		let prot = MMapProt::READ | MMapProt::WRITE;
		let flags = MMapFlags::PRIVATE | MMapFlags::ANONYMOUS | MMapFlags::NORESERVE;
		let mapping = mmap(None, size, prot, flags, None, 0).ok()?;

		if let Some(misalignment) = NonZero::new(mapping.as_ptr() as usize & (alignment.get() - 1)) {
			if let Some(mapping) = move_mapping_up(
				mapping,
				size,
				NonZero::new(alignment.get() - misalignment.get()).unwrap(),
				prot,
				flags,
			) {
				debug_assert_eq!(mapping.as_ptr() as usize & (alignment.get() - 1), 0);
				return Some(mapping);
			}
			if mapping.as_ptr() as usize > misalignment.get() {
				if let Some(mapping) = move_mapping_down(mapping, size, misalignment, prot, flags) {
					debug_assert_eq!(mapping.as_ptr() as usize & (alignment.get() - 1), 0);
					return Some(mapping);
				}
			}

			if recursive_retries > 0 {
				let ret = mmap_aligned_rec(size, alignment, recursive_retries - 1);
				munmap(mapping, size).unwrap();
				ret
			} else {
				munmap(mapping, size).unwrap();

				None
			}
		} else {
			Some(mapping)
		}
	}
}

/// Tries to allocate suitably aligned storage from the OS. As this may fail initially, the function will retry up to
/// `recursive_retries` times.
///
/// This function allocates virtual memory, not physical memory.
pub unsafe fn alloc_aligned(
	size: NonZero<usize>,
	alignment: NonZero<usize>,
	recursive_retries: usize,
) -> Option<NonNull<c_void>> {
	debug_assert!(alignment.is_power_of_two());
	debug_assert_eq!(size.get() & (alignment.get() - 1), 0);

	unsafe { mmap_aligned_rec(size, alignment, recursive_retries) }
}

/// Tries to allocate storage at the exact location provided.
pub unsafe fn alloc_at(address: NonNull<c_void>, size: NonZero<usize>) -> Option<NonNull<c_void>> {
	let prot = MMapProt::READ | MMapProt::WRITE;
	let flags = MMapFlags::PRIVATE | MMapFlags::ANONYMOUS | MMapFlags::NORESERVE | MMapFlags::FIXED_NOREPLACE;
	let ret = unsafe { mmap(Some(address), size, prot, flags, None, 0).ok()? };
	assert_eq!(
		ret, address,
		"Emma is not compatible with linux kernels that do not recognize MAP_FIXED_NOREPLACE (pre 4.17)."
	);
	Some(ret)
}

#[cfg(test)]
mod test {
	use super::*;

	fn mmap_aligned_and_unmap(multiples: usize, alignment: usize) {
		let size = NonZero::new(multiples * alignment).unwrap();
		let alignment = NonZero::new(alignment).unwrap();

		unsafe {
			let region = alloc_aligned(size, alignment, 3).unwrap();
			munmap(region, size).unwrap();
		}
	}

	#[test]
	fn mmap_aligned_100x4k() {
		mmap_aligned_and_unmap(100, 4 * 1024);
	}

	#[test]
	fn mmap_aligned_100x2m() {
		mmap_aligned_and_unmap(100, 2 * 1024 * 1024);
	}

	#[test]
	fn mmap_aligned_100x1g() {
		mmap_aligned_and_unmap(100, 1 * 1024 * 1024 * 1024);
	}
}
