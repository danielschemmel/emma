use core::num::NonZero;
use core::ptr::{self, NonNull};
#[cfg(feature = "tls")]
use core::sync::atomic::AtomicU64;

use arena::{large_object, medium_object, small_object};
use static_assertions::const_assert_eq;

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

const NUM_SMALL_OBJECT_BINS: usize = ((2 * small_object::MAXIMUM_OBJECT_ALIGNMENT - 8) / 8) as usize;
const NUM_MEDIUM_OBJECT_BINS: usize = ((u32::ilog2(medium_object::MAXIMUM_OBJECT_ALIGNMENT)
	- u32::ilog2(small_object::MAXIMUM_OBJECT_ALIGNMENT))
	* 4) as usize;
const NUM_LARGE_OBJECT_BINS: usize = ((u32::ilog2(large_object::MAXIMUM_OBJECT_ALIGNMENT)
	- u32::ilog2(medium_object::MAXIMUM_OBJECT_ALIGNMENT))
	* 4) as usize;

const_assert_eq!(
	(NUM_MEDIUM_OBJECT_BINS - 1) as u32,
	powerlaw_bin_from_size(
		(medium_object::MAXIMUM_OBJECT_ALIGNMENT
			+ medium_object::MAXIMUM_OBJECT_ALIGNMENT / 2
			+ medium_object::MAXIMUM_OBJECT_ALIGNMENT / 4) as usize
	) - powerlaw_bin_from_size((small_object::MAXIMUM_OBJECT_ALIGNMENT * 2) as usize)
);
const_assert_eq!(
	(NUM_LARGE_OBJECT_BINS - 1) as u32,
	powerlaw_bin_from_size(
		(large_object::MAXIMUM_OBJECT_ALIGNMENT
			+ large_object::MAXIMUM_OBJECT_ALIGNMENT / 2
			+ large_object::MAXIMUM_OBJECT_ALIGNMENT / 4) as usize
	) - powerlaw_bin_from_size((medium_object::MAXIMUM_OBJECT_ALIGNMENT * 2) as usize)
);

#[derive(Debug)]
struct Heap {
	#[cfg(feature = "tls")]
	id: HeapId,
	small_object_reserve: Option<NonNull<small_object::Page>>,
	small_object_pages: [Option<NonNull<small_object::Page>>; NUM_SMALL_OBJECT_BINS],
	medium_object_reserve: Option<NonNull<medium_object::Page>>,
	medium_object_pages: [Option<NonNull<medium_object::Page>>; NUM_MEDIUM_OBJECT_BINS],
	large_object_reserve: Option<NonNull<large_object::Page>>,
	large_object_pages: [Option<NonNull<large_object::Page>>; NUM_LARGE_OBJECT_BINS],
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
			small_object_reserve: None,
			small_object_pages: [None; NUM_SMALL_OBJECT_BINS],
			medium_object_reserve: None,
			medium_object_pages: [None; NUM_MEDIUM_OBJECT_BINS],
			large_object_reserve: None,
			large_object_pages: [None; NUM_LARGE_OBJECT_BINS],
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
			small_object_reserve: None,
			small_object_pages: [None; NUM_SMALL_OBJECT_BINS],
			medium_object_reserve: None,
			medium_object_pages: [None; NUM_MEDIUM_OBJECT_BINS],
			large_object_reserve: None,
			large_object_pages: [None; NUM_LARGE_OBJECT_BINS],
		}
	}
}

#[inline]
const fn powerlaw_bin_from_size(size: usize) -> u32 {
	debug_assert!(size >= 0b100);

	let lz = size.leading_zeros();
	(usize::BITS - lz - 3) * 4 + ((size >> (usize::BITS - lz - 3)) as u32 - 4)
}

const_assert_eq!(powerlaw_bin_from_size(0b100), 0);
const_assert_eq!(powerlaw_bin_from_size(0b101), 1);
const_assert_eq!(powerlaw_bin_from_size(0b110), 2);
const_assert_eq!(powerlaw_bin_from_size(0b111), 3);
const_assert_eq!(powerlaw_bin_from_size(0b1001), 4);
const_assert_eq!(powerlaw_bin_from_size(0b100000), 12);
const_assert_eq!(powerlaw_bin_from_size(0b101000), 13);
const_assert_eq!(powerlaw_bin_from_size(0b110000), 14);
const_assert_eq!(powerlaw_bin_from_size(0b111000), 15);
const_assert_eq!(powerlaw_bin_from_size(0b1001000), 16);
const_assert_eq!(powerlaw_bin_from_size(0b1011000), 17);
const_assert_eq!(powerlaw_bin_from_size(0b1101000), 18);
const_assert_eq!(powerlaw_bin_from_size(0b1111000), 19);
const_assert_eq!(powerlaw_bin_from_size(0b10010000), 20);

#[inline]
const fn powerlaw_bins_round_up_size(size: NonZero<usize>) -> NonZero<usize> {
	debug_assert!(size.get() >= 8);

	let lz = size.leading_zeros();
	unsafe {
		NonZero::new_unchecked(
			size.get()
				& (1usize << (usize::BITS - 1 - lz) | 1usize << (usize::BITS - 2 - lz) | 1usize << (usize::BITS - 3 - lz)),
		)
	}
}

// const_assert_eq!(powerlaw_bins_round_up_size(0b1001), 0b1000);
// const_assert_eq!(powerlaw_bins_round_up_size(0b10010), 0b10000);
// const_assert_eq!(powerlaw_bins_round_up_size(0b110100), 0b110000);
// const_assert_eq!(powerlaw_bins_round_up_size(0b1011000), 0b1010000);
// const_assert_eq!(powerlaw_bins_round_up_size(usize::MAX / 2 + 1), usize::MAX / 2 + 1);

impl Heap {
	unsafe fn alloc(&mut self, size: NonZero<usize>, alignment: NonZero<usize>) -> *mut u8 {
		let bin = (size.get() + 7) / 8;
		debug_assert!(bin > 0);
		if bin <= self.small_object_pages.len() {
			small_object::alloc(
				&mut self.small_object_pages[bin - 1],
				&mut self.small_object_reserve,
				(bin * 8) as u32,
				#[cfg(feature = "tls")]
				self.id,
			)
		} else {
			let bin = powerlaw_bin_from_size(size.get());
			if bin
				<= powerlaw_bin_from_size(
					(medium_object::MAXIMUM_OBJECT_ALIGNMENT
						+ medium_object::MAXIMUM_OBJECT_ALIGNMENT / 2
						+ medium_object::MAXIMUM_OBJECT_ALIGNMENT / 4) as usize,
				) {
				medium_object::alloc(
					&mut self.medium_object_pages
						[(bin - powerlaw_bin_from_size((small_object::MAXIMUM_OBJECT_ALIGNMENT * 2) as usize)) as usize],
					&mut self.medium_object_reserve,
					powerlaw_bins_round_up_size(size).get() as u32,
					#[cfg(feature = "tls")]
					self.id,
				)
			} else if bin
				<= powerlaw_bin_from_size(
					(large_object::MAXIMUM_OBJECT_ALIGNMENT
						+ large_object::MAXIMUM_OBJECT_ALIGNMENT / 2
						+ large_object::MAXIMUM_OBJECT_ALIGNMENT / 4) as usize,
				) {
				large_object::alloc(
					&mut self.large_object_pages
						[(bin - powerlaw_bin_from_size((medium_object::MAXIMUM_OBJECT_ALIGNMENT * 2) as usize)) as usize],
					&mut self.large_object_reserve,
					powerlaw_bins_round_up_size(size).get() as u32,
					#[cfg(feature = "tls")]
					self.id,
				)
			} else {
				let size = (size.get() + 4095) & !4095;
				mmap_aligned(NonZero::new(size).unwrap(), alignment, 3)
					.map(|ptr| ptr.as_ptr().cast())
					.unwrap_or(ptr::null_mut())
			}
		}
	}

	unsafe fn dealloc(&mut self, ptr: *mut u8, size: NonZero<usize>, _alignment: NonZero<usize>) {
		let bin = (size.get() + 7) / 8;
		debug_assert!(bin > 0);
		if bin <= self.small_object_pages.len() {
			debug_assert!(!ptr.is_null());
			small_object::Page::dealloc(
				#[cfg(feature = "tls")]
				self.id,
				NonNull::new_unchecked(ptr),
			);
		} else {
			let bin = powerlaw_bin_from_size(size.get());
			if bin
				<= powerlaw_bin_from_size(
					(medium_object::MAXIMUM_OBJECT_ALIGNMENT
						+ medium_object::MAXIMUM_OBJECT_ALIGNMENT / 2
						+ medium_object::MAXIMUM_OBJECT_ALIGNMENT / 4) as usize,
				) {
				medium_object::Page::dealloc(
					#[cfg(feature = "tls")]
					self.id,
					NonNull::new_unchecked(ptr),
				);
			} else if bin
				<= powerlaw_bin_from_size(
					(large_object::MAXIMUM_OBJECT_ALIGNMENT
						+ large_object::MAXIMUM_OBJECT_ALIGNMENT / 2
						+ large_object::MAXIMUM_OBJECT_ALIGNMENT / 4) as usize,
				) {
				large_object::Page::dealloc(
					#[cfg(feature = "tls")]
					self.id,
					NonNull::new_unchecked(ptr),
				);
			} else {
				let size = (size.get() + 4095) & !4095;
				munmap(NonNull::new(ptr.cast()).unwrap(), NonZero::new(size).unwrap()).unwrap();
			}
		}
	}
}

unsafe impl alloc::alloc::GlobalAlloc for Emma {
	unsafe fn alloc(&self, layout: core::alloc::Layout) -> *mut u8 {
		assert!(layout.align().is_power_of_two());
		assert_eq!(layout.size() & (layout.align() - 1), 0);

		#[cfg(not(feature = "tls"))]
		{
			self.heap.lock().alloc(
				NonZero::new(layout.size()).unwrap(),
				NonZero::new(layout.align()).unwrap(),
			)
		}
		#[cfg(feature = "tls")]
		if let Some(mut thread_heap) = self.thread_heap() {
			thread_heap.as_mut().alloc(
				NonZero::new(layout.size()).unwrap(),
				NonZero::new(layout.align()).unwrap(),
			)
		} else {
			ptr::null_mut()
		}
	}

	unsafe fn dealloc(&self, ptr: *mut u8, layout: core::alloc::Layout) {
		assert_ne!(ptr, core::ptr::null_mut());
		assert!(layout.align().is_power_of_two());
		assert_eq!(layout.size() & (layout.align() - 1), 0);

		#[cfg(not(feature = "tls"))]
		{
			self.heap.lock().dealloc(
				ptr,
				NonZero::new(layout.size()).unwrap(),
				NonZero::new(layout.align()).unwrap(),
			)
		}
		#[cfg(feature = "tls")]
		{
			Self::thread_heap_unchecked().as_mut().dealloc(
				ptr,
				NonZero::new(layout.size()).unwrap(),
				NonZero::new(layout.align()).unwrap(),
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
