use core::num::NonZero;
use core::ptr::{self, NonNull};

use crate::mmap::{mmap_aligned, munmap};
use crate::sync::Futex;

pub type DefaultEmma = Emma;

#[derive(Debug)]
pub struct Emma {
	heap: Futex<Heap>,
}

impl Emma {
	pub const fn new() -> Self {
		Self {
			heap: Futex::new(Heap::new()),
		}
	}

	/// # Safety
	/// Only safe if all objects have been deallocated!
	pub unsafe fn reset(&self) {
		self.heap.lock().reset()
	}
}

#[derive(Debug)]
struct Heap {}

impl Heap {
	pub const fn new() -> Self {
		Self {}
	}

	unsafe fn alloc(&mut self, size: NonZero<usize>, alignment: NonZero<usize>) -> *mut u8 {
		mmap_aligned(size, alignment, 3)
			.map(|ptr| ptr.as_ptr().cast())
			.unwrap_or(ptr::null_mut())
	}

	unsafe fn dealloc(&mut self, ptr: *mut u8, size: NonZero<usize>) {
		munmap(NonNull::new(ptr.cast()).unwrap(), size).unwrap();
	}

	unsafe fn reset(&mut self) {
		todo!()
	}
}

unsafe impl alloc::alloc::GlobalAlloc for Emma {
	unsafe fn alloc(&self, layout: core::alloc::Layout) -> *mut u8 {
		let aligned_layout = layout.pad_to_align();
		self.heap.lock().alloc(
			NonZero::new(aligned_layout.size()).unwrap(),
			NonZero::new(aligned_layout.align()).unwrap(),
		)
	}

	unsafe fn dealloc(&self, ptr: *mut u8, layout: core::alloc::Layout) {
		let aligned_layout = layout.pad_to_align();
		self
			.heap
			.lock()
			.dealloc(ptr, NonZero::new(aligned_layout.size()).unwrap())
	}
}

#[cfg(test)]
mod test {
	use core::alloc::GlobalAlloc;
	use core::ptr;

	use super::*;

	#[test]
	fn alloc_dealloc() {
		let emma = DefaultEmma::new();
		let layout = core::alloc::Layout::new::<u64>();

		let p = unsafe { emma.alloc(layout) };
		assert_ne!(p, ptr::null_mut());
		let q = p as *mut u64;
		assert_ne!(q, ptr::null_mut());
		unsafe {
			*q = 42;
			assert_eq!(*q, 42);
		}
		unsafe { emma.dealloc(p, layout) };

		// unsafe { emma.reset() };
	}
}
