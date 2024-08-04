use core::num::NonZero;
use core::ptr::{self, NonNull};
#[cfg(feature = "tls")]
use core::sync::atomic::AtomicU64;

use arena::SmallObjectPage;

use crate::mmap::{mmap_aligned, munmap};
#[cfg(not(feature = "tls"))]
use crate::sync::Futex;

mod arena;

#[cfg(feature = "tls")]
mod heap_manager;

pub type DefaultEmma = Emma;

#[derive(Debug)]
pub struct Emma {
	#[cfg(not(feature = "tls"))]
	heap: Futex<Heap>,

	#[cfg(feature = "tls")]
	heap_manager: heap_manager::HeapManager,
}

impl Emma {
	/// Create a new [`Emma`] instance.
	pub const fn new() -> Self {
		Self {
			#[cfg(not(feature = "tls"))]
			heap: Futex::new(Heap::new()),

			#[cfg(feature = "tls")]
			heap_manager: heap_manager::HeapManager::new(),
		}
	}
}

#[cfg(feature = "tls")]
#[thread_local]
static mut THREAD_HEAP: Option<NonNull<Heap>> = None;

#[cfg(feature = "tls")]
impl Emma {
	fn thread_heap(&self) -> Option<NonNull<Heap>> {
		unsafe {
			if let Some(thread_heap) = THREAD_HEAP {
				Some(thread_heap)
			} else if let Some(thread_heap) = self.heap_manager.acquire_thread_heap() {
				THREAD_HEAP = Some(thread_heap);
				Some(thread_heap)
			} else {
				None
			}
		}
	}

	unsafe fn thread_heap_unchecked() -> NonNull<Heap> {
		debug_assert_ne!(THREAD_HEAP, None);
		THREAD_HEAP.unwrap_unchecked()
	}
}

#[derive(Debug)]
struct Heap {
	#[cfg(feature = "tls")]
	id: HeapId,
	small_object_pages: [Option<NonNull<arena::SmallObjectPage>>; 2048 / 8],
}

#[cfg(feature = "tls")]
type HeapId = u64;

#[cfg(feature = "tls")]
type AtomicHeapId = AtomicU64;

unsafe impl Send for Heap {}

#[cfg(not(feature = "tls"))]
impl Heap {
	const fn new() -> Self {
		Self {
			small_object_pages: [None; 2048 / 8],
		}
	}
}

#[cfg(feature = "tls")]
impl Heap {
	fn new() -> Self {
		use core::sync::atomic::AtomicU64;

		static HEAP_IDS: AtomicU64 = AtomicU64::new(0);

		Self {
			id: HEAP_IDS.fetch_add(1, core::sync::atomic::Ordering::Relaxed),
			small_object_pages: [None; 2048 / 8],
		}
	}
}

impl Heap {
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

			#[cfg(not(feature = "tls"))]
			let pages_from_new_arena = SmallObjectPage::from_new_arena();
			#[cfg(feature = "tls")]
			let pages_from_new_arena = SmallObjectPage::from_new_arena(self.id);
			if let Some((mut page, first_additional_page, mut last_additional_page)) = pages_from_new_arena {
				debug_assert_eq!(last_additional_page.as_ref().next_page, None);
				last_additional_page.as_mut().next_page = self.small_object_pages[0];
				self.small_object_pages[0] = Some(first_additional_page);

				page.as_mut().object_size = (bin * 8) as u32;
				page.as_mut().next_page = self.small_object_pages[bin];
				self.small_object_pages[bin] = Some(page);

				let ret = page.as_mut().alloc();
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
			#[cfg(not(feature = "tls"))]
			{
				SmallObjectPage::dealloc(NonNull::new_unchecked(ptr));
			}
			#[cfg(feature = "tls")]
			{
				SmallObjectPage::dealloc(self.id, NonNull::new_unchecked(ptr));
			}
		} else {
			munmap(NonNull::new(ptr.cast()).unwrap(), size).unwrap();
		}
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
			#[cfg(not(feature = "tls"))]
			{
				self.heap.lock().alloc(
					NonZero::new(aligned_layout.size()).unwrap(),
					NonZero::new(aligned_layout.align()).unwrap(),
				)
			}
			#[cfg(feature = "tls")]
			if let Some(mut thread_heap) = self.thread_heap() {
				thread_heap.as_mut().alloc(
					NonZero::new(aligned_layout.size()).unwrap(),
					NonZero::new(aligned_layout.align()).unwrap(),
				)
			} else {
				ptr::null_mut()
			}
		}
	}

	unsafe fn dealloc(&self, ptr: *mut u8, layout: core::alloc::Layout) {
		assert_ne!(ptr, core::ptr::null_mut());

		let aligned_layout = layout.pad_to_align();
		if aligned_layout.size() > MMAP_CUTOFF {
			let size = (layout.size() + 4095) & !4095;
			munmap(NonNull::new(ptr.cast()).unwrap(), NonZero::new(size).unwrap()).unwrap();
		} else {
			#[cfg(not(feature = "tls"))]
			{
				self.heap.lock().dealloc(
					ptr,
					NonZero::new(aligned_layout.size()).unwrap(),
					NonZero::new(aligned_layout.align()).unwrap(),
				)
			}
			#[cfg(feature = "tls")]
			{
				Self::thread_heap_unchecked().as_mut().dealloc(
					ptr,
					NonZero::new(aligned_layout.size()).unwrap(),
					NonZero::new(aligned_layout.align()).unwrap(),
				)
			}
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
