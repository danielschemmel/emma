use core::num::NonZero;
use core::ptr::{self, NonNull};

use arena::{SmallObjectArena, SmallObjectPage};

use crate::mmap::{mmap_aligned, munmap};
use crate::sync::Futex;

mod arena;

pub type DefaultEmma = Emma;

#[derive(Debug)]
pub struct Emma {
	heap: Futex<Heap>,
}

impl Emma {
	/// Create a new [`Emma`] instance.
	pub const fn new() -> Self {
		Self {
			heap: Futex::new(Heap::new()),
		}
	}

	/// Release all internal metadata and return to initial state.
	///
	/// # Safety
	/// Only safe if all objects have been deallocated!
	#[allow(dead_code)]
	unsafe fn reset(&self) {
		self.heap.lock().reset()
	}
}

#[derive(Debug)]
struct Heap {
	small_object_pages: [Option<NonNull<arena::SmallObjectPage>>; 2048 / 8],
}

unsafe impl Send for Heap {}

impl Heap {
	const fn new() -> Self {
		Self {
			small_object_pages: [None; 2048 / 8],
		}
	}

	unsafe fn alloc(&mut self, size: NonZero<usize>, alignment: NonZero<usize>) -> *mut u8 {
		let bin = (size.get() + 7) / 8;
		debug_assert!(bin > 0);
		if bin < self.small_object_pages.len() {
			{
				let mut pp: *mut Option<NonNull<SmallObjectPage>> = &mut self.small_object_pages[bin];
				let mut p = self.small_object_pages[bin];
				loop {
					if let Some(mut q) = p {
						let page = q.as_mut();
						debug_assert_eq!(page.object_size as usize, bin * 8);

						if let Some(ret) = page.alloc() {
							if p != self.small_object_pages[bin] {
								*pp.as_mut().unwrap_unchecked() = page.next_page;
								page.next_page = self.small_object_pages[bin];
								self.small_object_pages[bin] = p;
							}
							return ret.as_ptr();
						}
						pp = &mut page.next_page;
						p = page.next_page;
					} else {
						break;
					}
				}
			}

			if let Some(mut p) = self.small_object_pages[0] {
				let page = p.as_mut();

				self.small_object_pages[0] = page.next_page;
				page.next_page = self.small_object_pages[bin];
				self.small_object_pages[bin] = Some(p);

				page.object_size = (bin * 8) as u32;
				let ret = page.alloc();
				debug_assert!(ret.is_some());
				return unsafe { ret.unwrap_unchecked() }.as_ptr();
			}

			if let Some(mut p_arena) = SmallObjectArena::new() {
				let arena = p_arena.as_mut();
				for i in 2..arena.pages.len() {
					arena.pages[i - 1].next_page = Some(NonNull::new_unchecked(&mut arena.pages[i]));
				}
				// the last page now has no successor, which matches the free page list in bin 0:
				debug_assert_eq!(self.small_object_pages[0], None);
				debug_assert_eq!(arena.pages.last().unwrap().next_page, None);
				self.small_object_pages[0] = Some(NonNull::new_unchecked(&mut arena.pages[1]));

				let page = &mut arena.pages[0];
				page.object_size = (bin * 8) as u32;
				page.next_page = self.small_object_pages[bin];
				self.small_object_pages[bin] = Some(NonNull::new_unchecked(page));

				let ret = page.alloc();
				debug_assert!(ret.is_some());
				return unsafe { ret.unwrap_unchecked() }.as_ptr();
			} else {
				// OOM?
				ptr::null_mut()
			}
		} else {
			mmap_aligned(size, alignment, 3)
				.map(|ptr| ptr.as_ptr().cast())
				.unwrap_or(ptr::null_mut())
		}
	}

	unsafe fn dealloc(&mut self, ptr: *mut u8, size: NonZero<usize>, _alignment: NonZero<usize>) {
		let bin = (size.get() + 7) / 8;
		debug_assert!(bin > 0);
		if bin < self.small_object_pages.len() {
			debug_assert!(!ptr.is_null());
			SmallObjectPage::dealloc(NonNull::new_unchecked(ptr));
		} else {
			munmap(NonNull::new(ptr.cast()).unwrap(), size).unwrap();
		}
	}

	unsafe fn reset(&mut self) {
		todo!()
	}
}

const MMAP_CUTOFF: usize = 1024 * 1024;

unsafe impl alloc::alloc::GlobalAlloc for Emma {
	unsafe fn alloc(&self, layout: core::alloc::Layout) -> *mut u8 {
		let aligned_layout = layout.pad_to_align();
		if aligned_layout.size() > MMAP_CUTOFF {
			let size = (layout.size() + 4095) & !4095;
			mmap_aligned(NonZero::new(size).unwrap(), NonZero::new(layout.align()).unwrap(), 3)
				.map(|ptr| ptr.as_ptr().cast())
				.unwrap_or(ptr::null_mut())
		} else {
			self.heap.lock().alloc(
				NonZero::new(aligned_layout.size()).unwrap(),
				NonZero::new(aligned_layout.align()).unwrap(),
			)
		}
	}

	unsafe fn dealloc(&self, ptr: *mut u8, layout: core::alloc::Layout) {
		assert_ne!(ptr, core::ptr::null_mut());

		let aligned_layout = layout.pad_to_align();
		if aligned_layout.size() > MMAP_CUTOFF {
			let size = (layout.size() + 4095) & !4095;
			munmap(NonNull::new(ptr.cast()).unwrap(), NonZero::new(size).unwrap()).unwrap();
		} else {
			self.heap.lock().dealloc(
				ptr,
				NonZero::new(aligned_layout.size()).unwrap(),
				NonZero::new(aligned_layout.align()).unwrap(),
			)
		}
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
