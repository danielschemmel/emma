mod syscalls;
use core::ffi::c_void;
use core::num::NonZero;
use core::ptr::NonNull;

pub use self::syscalls::*;

unsafe fn move_mapping_down(
	old_mapping: NonNull<c_void>,
	size: NonZero<usize>,
	move_forwards: NonZero<usize>,
	prot: MMapProt,
	flags: MMapFlags,
) -> Option<NonNull<c_void>> {
	let target_addr = old_mapping.byte_sub(move_forwards.get());
	let filler = mmap(
		Some(target_addr),
		move_forwards,
		prot,
		flags | MMapFlags::FIXED_NOREPLACE,
		None,
		0,
	)
	.ok()?;
	assert_eq!(filler, target_addr);

	munmap(target_addr.byte_add(size.get()), move_forwards).unwrap();

	Some(target_addr)
}

unsafe fn move_mapping_up(
	old_mapping: NonNull<c_void>,
	size: NonZero<usize>,
	move_backwards: NonZero<usize>,
	prot: MMapProt,
	flags: MMapFlags,
) -> Option<NonNull<c_void>> {
	let target_addr = old_mapping.byte_add(size.get());
	let filler = mmap(
		Some(target_addr),
		move_backwards,
		prot,
		flags | MMapFlags::FIXED_NOREPLACE,
		None,
		0,
	)
	.ok()?;
	assert_eq!(filler, target_addr);

	munmap(old_mapping, move_backwards).unwrap();

	Some(old_mapping.byte_add(move_backwards.get()))
}

unsafe fn mmap_aligned_rec(
	size: NonZero<usize>,
	alignment: NonZero<usize>,
	recursive_retries: usize,
	leftover_mapping: Option<NonNull<c_void>>,
) -> Option<NonNull<c_void>> {
	let prot = MMapProt::READ | MMapProt::WRITE;
	let flags = MMapFlags::PRIVATE | MMapFlags::ANONYMOUS | MMapFlags::NORESERVE;
	let mapping = mmap(None, size, prot, flags, None, 0).ok()?;

	if let Some(leftover_mapping) = leftover_mapping {
		munmap(leftover_mapping, size).unwrap();
	}

	if let Some(misalignment) = NonZero::new(mapping.as_ptr() as usize & (alignment.get() - 1)) {
		if let Some(mapping) = move_mapping_up(
			mapping,
			size,
			NonZero::new(alignment.get() - misalignment.get()).unwrap(),
			prot,
			flags,
		) {
			return Some(mapping);
		}
		if mapping.as_ptr() as usize > misalignment.get() {
			if let Some(mapping) = move_mapping_down(mapping, size, misalignment, prot, flags) {
				return Some(mapping);
			}
		}

		if recursive_retries > 0 {
			let ret = mmap_aligned_rec(size, alignment, recursive_retries - 1, Some(mapping));
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

/// Tries to allocate suitably aligned storage from the OS. As this may fail initially, the function will retry up to `recursive_retries` times.
pub unsafe fn alloc_aligned(
	size: NonZero<usize>,
	alignment: NonZero<usize>,
	recursive_retries: usize,
) -> Option<NonNull<c_void>> {
	debug_assert!(alignment.is_power_of_two());
	debug_assert_eq!(size.get() & (alignment.get() - 1), 0);

	mmap_aligned_rec(size, alignment, recursive_retries, None)
}

/// Tries to allocate storage at the exact location provided.
pub unsafe fn alloc_at(
	address: NonNull<c_void>,
	size: NonZero<usize>,
) -> Option<NonNull<c_void>> {
	let prot = MMapProt::READ | MMapProt::WRITE;
	let flags = MMapFlags::PRIVATE | MMapFlags::ANONYMOUS | MMapFlags::NORESERVE;
	let ret = mmap(Some(address), size, prot, flags, None, 0).ok()?;
	debug_assert_eq!(ret, address);
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
