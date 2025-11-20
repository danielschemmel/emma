use core::alloc::Layout;
use core::num::NonZero;
use core::ptr::{self, NonNull};
#[cfg(feature = "tls")]
use core::sync::atomic::AtomicU64;

use arena::{large_objects, medium_objects, small_objects};
use const_format::assertc_eq;

use crate::mmap::{alloc_aligned, munmap};
#[cfg(not(feature = "tls"))]
use crate::sync::Futex;

mod arena;

#[cfg(feature = "tls")]
mod heap_manager;

pub type DefaultEmma = Emma;

/// The main allocator struct. Instantiate to interface with Emma.
#[derive(Debug)]
pub struct Emma {
	#[cfg(not(feature = "tls"))]
	heap: Futex<Heap>,

	/// TODO: make static!
	#[cfg(feature = "tls")]
	heap_manager: heap_manager::HeapManager,
}

impl Emma {
	/// Create a new [`Emma`] instance.
	#[allow(clippy::new_without_default)] // Sadly, default is not const.
	pub const fn new() -> Self {
		Self {
			#[cfg(not(feature = "tls"))]
			heap: Futex::new(Heap::new()),

			#[cfg(feature = "tls")]
			heap_manager: heap_manager::HeapManager::new(),
		}
	}

	/// Print internals of the [`Emma`] type. This is probably not interesting for consumers of this library.
	pub const fn print_internals() -> impl core::fmt::Debug {
		struct F(fn(&mut core::fmt::Formatter) -> core::fmt::Result);

		impl core::fmt::Debug for F {
			fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
				self.0(f)
			}
		}

		F(Self::print_internals_impl)
	}

	fn print_internals_impl(f: &mut core::fmt::Formatter) -> core::fmt::Result {
		writeln!(f, "{} v{}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"))?;

		#[cfg(not(feature = "tls"))]
		const TLS_ENABLED: &str = "disabled";
		#[cfg(feature = "tls")]
		const TLS_ENABLED: &str = "enabled";
		writeln!(f, "tls {TLS_ENABLED}")?;

		#[cfg(not(feature = "boundary-checks"))]
		const BOUNDARY_CHECKS_ENABLED: &str = "disabled";
		#[cfg(feature = "boundary-checks")]
		const BOUNDARY_CHECKS_ENABLED: &str = "enabled";
		writeln!(f, "boundary checks {BOUNDARY_CHECKS_ENABLED}")?;

		#[cfg(not(debug_assertions))]
		const DEBUG_ASSERTIONS_ENABLED: &str = "disabled";
		#[cfg(debug_assertions)]
		const DEBUG_ASSERTIONS_ENABLED: &str = "enabled";
		writeln!(f, "debug assertions {DEBUG_ASSERTIONS_ENABLED}")?;
		writeln!(f)?;

		writeln!(f, "Object Sizes")?;
		writeln!(f, "Emma: size {} align {}", size_of::<Self>(), align_of::<Self>())?;
		writeln!(f, "Heap: size {} align {}", size_of::<Heap>(), align_of::<Heap>())?;

		Ok(())
	}
}

/// The per-thread heap. Can be accessed without locking, but may not be sent between threads.
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
				debug_assert_ne!(thread_heap.as_ref().id, 0);
				THREAD_HEAP = Some(thread_heap);
				Some(thread_heap)
			} else {
				None
			}
		}
	}
}

const NUM_SMALL_OBJECT_BINS: usize = ((2 * small_objects::MAXIMUM_OBJECT_ALIGNMENT - 8) / 8) as usize;
const NUM_MEDIUM_OBJECT_BINS: usize = ((u32::ilog2(medium_objects::MAXIMUM_OBJECT_ALIGNMENT)
	- u32::ilog2(small_objects::MAXIMUM_OBJECT_ALIGNMENT))
	* 4) as usize;
const NUM_LARGE_OBJECT_BINS: usize = ((u32::ilog2(large_objects::MAXIMUM_OBJECT_ALIGNMENT)
	- u32::ilog2(medium_objects::MAXIMUM_OBJECT_ALIGNMENT))
	* 4) as usize;

assertc_eq!(
	(NUM_MEDIUM_OBJECT_BINS - 1) as u32,
	powerlaw_bin_from_size(
		(medium_objects::MAXIMUM_OBJECT_ALIGNMENT
			+ medium_objects::MAXIMUM_OBJECT_ALIGNMENT / 2
			+ medium_objects::MAXIMUM_OBJECT_ALIGNMENT / 4) as usize
	) - powerlaw_bin_from_size((small_objects::MAXIMUM_OBJECT_ALIGNMENT * 2) as usize)
);
assertc_eq!(
	(NUM_LARGE_OBJECT_BINS - 1) as u32,
	powerlaw_bin_from_size(
		(large_objects::MAXIMUM_OBJECT_ALIGNMENT
			+ large_objects::MAXIMUM_OBJECT_ALIGNMENT / 2
			+ large_objects::MAXIMUM_OBJECT_ALIGNMENT / 4) as usize
	) - powerlaw_bin_from_size((medium_objects::MAXIMUM_OBJECT_ALIGNMENT * 2) as usize)
);

/// Provides allocation and deallocation capabilities. The actual allocation/deallocation is dispatched, depending of
/// the size of the allocation.
///
/// - small objects are allocated using [`small_objects`]
/// - medium objects are allocated using [`medium_objects`]
/// - large objects are allocated using [`large_objects`]
/// - huge objects are allocated directly using [`crate::mmap`]
#[derive(Debug)]
struct Heap {
	/// An id to identify this heap. The id is guaranteed to not be zero (which means that the heap id zero can be used
	/// to indicate no heap).
	#[cfg(feature = "tls")]
	id: HeapId,
	/// A singly-linked list of free pages suitable for small objects. The next page is accessed via
	/// [`small_objects::Page::next_page`].
	small_object_reserve: Option<NonNull<small_objects::Page>>,
	/// Each element of this array contains a singly-linked list of pages suitable for allocation of small objects of one
	/// specific size. The next page is accessed via [`small_objects::Page::next_page`].
	small_object_pages: [Option<NonNull<small_objects::Page>>; NUM_SMALL_OBJECT_BINS],
	/// A singly-linked list of free pages suitable for medium objects. The next page is accessed via
	/// [`medium_objects::Page::next_page`].
	medium_object_reserve: Option<NonNull<medium_objects::Page>>,
	/// Each element of this array contains a singly-linked list of pages suitable for allocation of medium objects of
	/// one specific size. The next page is accessed via [`medium_objects::Page::next_page`].
	medium_object_pages: [Option<NonNull<medium_objects::Page>>; NUM_MEDIUM_OBJECT_BINS],
	/// Each element of this array contains a singly-linked list of pages suitable for allocation of large objects of one
	/// specific size. The next page is accessed via [`large_objects::Page::next_page`].
	large_object_pages: [Option<NonNull<large_objects::Page>>; NUM_LARGE_OBJECT_BINS],
}

#[cfg(feature = "tls")]
type HeapId = u64;

#[cfg(feature = "tls")]
type AtomicHeapId = AtomicU64;

unsafe impl Send for Heap {}

#[cfg(not(feature = "tls"))]
impl Heap {
	/// Creates a new heap
	const fn new() -> Self {
		Self {
			small_object_reserve: None,
			small_object_pages: [None; NUM_SMALL_OBJECT_BINS],
			medium_object_reserve: None,
			medium_object_pages: [None; NUM_MEDIUM_OBJECT_BINS],
			large_object_pages: [None; NUM_LARGE_OBJECT_BINS],
		}
	}
}

#[cfg(feature = "tls")]
impl Heap {
	/// Creates a new heap with a unique non-zero id
	fn new() -> Self {
		use core::sync::atomic::AtomicU64;

		// We use `0` as the id if we do not hold a heap, so we may not use it as a normal heap id.
		static HEAP_IDS: AtomicU64 = AtomicU64::new(1);

		Self {
			id: HEAP_IDS.fetch_add(1, core::sync::atomic::Ordering::Relaxed),
			small_object_reserve: None,
			small_object_pages: [None; NUM_SMALL_OBJECT_BINS],
			medium_object_reserve: None,
			medium_object_pages: [None; NUM_MEDIUM_OBJECT_BINS],
			large_object_pages: [None; NUM_LARGE_OBJECT_BINS],
		}
	}
}

#[inline]
const fn powerlaw_bin_from_size(size: usize) -> u32 {
	debug_assert!(size >= 0b100);

	let lz = size.leading_zeros();
	(usize::BITS - lz - 3) * 4 + (((size + (1 << (usize::BITS - lz - 3)) - 1) >> (usize::BITS - lz - 3)) as u32 - 4)
}

assertc_eq!(powerlaw_bin_from_size(0b100), 0u32);
assertc_eq!(powerlaw_bin_from_size(0b101), 1u32);
assertc_eq!(powerlaw_bin_from_size(0b110), 2u32);
assertc_eq!(powerlaw_bin_from_size(0b111), 3u32);
assertc_eq!(powerlaw_bin_from_size(0b1000), 4u32);
assertc_eq!(powerlaw_bin_from_size(0b1001), 5u32);
assertc_eq!(powerlaw_bin_from_size(0b1010), 5u32);
assertc_eq!(powerlaw_bin_from_size(0b100000), 12u32);
assertc_eq!(powerlaw_bin_from_size(0b101000), 13u32);
assertc_eq!(powerlaw_bin_from_size(0b110000), 14u32);
assertc_eq!(powerlaw_bin_from_size(0b111000), 15u32);
assertc_eq!(powerlaw_bin_from_size(0b1000000), 16u32);
assertc_eq!(powerlaw_bin_from_size(0b1001000), 17u32);
assertc_eq!(powerlaw_bin_from_size(0b1010000), 17u32);
assertc_eq!(powerlaw_bin_from_size(0b1011000), 18u32);
assertc_eq!(powerlaw_bin_from_size(0b1100000), 18u32);
assertc_eq!(powerlaw_bin_from_size(0b1101000), 19u32);
assertc_eq!(powerlaw_bin_from_size(0b1111000), 20u32);
assertc_eq!(powerlaw_bin_from_size(0b10010000), 21u32);

#[inline]
const fn powerlaw_bins_round_up_size(size: NonZero<usize>) -> NonZero<usize> {
	debug_assert!(size.get() >= 8);

	let lz = size.leading_zeros();
	let lowest_relevant_bit = 1usize << (usize::BITS - 3 - lz);
	unsafe { NonZero::new_unchecked((size.get() + (lowest_relevant_bit - 1)) & !(lowest_relevant_bit - 1)) }
}

/// ONLY FOR USE IN `const_assert` AND FRIENDS! DO NOT USE AT RUNTIME!
#[allow(dead_code)]
#[allow(unconditional_panic)]
#[allow(clippy::out_of_bounds_indexing)]
const fn const_non_zero_usize(x: usize) -> NonZero<usize> {
	match NonZero::new(x) {
		Some(val) => val,
		None => [][0],
	}
}

assertc_eq!(
	powerlaw_bins_round_up_size(const_non_zero_usize(0b1000)).get(),
	0b1000usize
);
assertc_eq!(
	powerlaw_bins_round_up_size(const_non_zero_usize(0b1001)).get(),
	0b1010usize
);
assertc_eq!(
	powerlaw_bins_round_up_size(const_non_zero_usize(0b1010)).get(),
	0b1010usize
);
assertc_eq!(
	powerlaw_bins_round_up_size(const_non_zero_usize(0b10010)).get(),
	0b10100usize
);
assertc_eq!(
	powerlaw_bins_round_up_size(const_non_zero_usize(0b110100)).get(),
	0b111000usize
);
assertc_eq!(
	powerlaw_bins_round_up_size(const_non_zero_usize(0b1011000)).get(),
	0b1100000usize
);
assertc_eq!(
	powerlaw_bins_round_up_size(const_non_zero_usize(usize::MAX / 2 + 1)).get(),
	usize::MAX / 2 + 1
);
assertc_eq!(
	powerlaw_bins_round_up_size(const_non_zero_usize(usize::MAX / 2 + 2)).get(),
	0b101usize << (usize::BITS - 3)
);
assertc_eq!(powerlaw_bins_round_up_size(const_non_zero_usize(4080)).get(), 4096usize);

impl Heap {
	unsafe fn alloc(&mut self, size: NonZero<usize>, alignment: NonZero<usize>) -> *mut u8 {
		let bin = size.get().div_ceil(8);
		debug_assert!(bin > 0);
		if bin <= self.small_object_pages.len() {
			unsafe {
				small_objects::alloc(
					&mut self.small_object_pages[bin - 1],
					&mut self.small_object_reserve,
					(bin * 8) as u32,
					#[cfg(feature = "tls")]
					self.id,
				)
			}
		} else {
			let bin = powerlaw_bin_from_size(size.get());
			if bin
				<= powerlaw_bin_from_size(
					(medium_objects::MAXIMUM_OBJECT_ALIGNMENT
						+ medium_objects::MAXIMUM_OBJECT_ALIGNMENT / 2
						+ medium_objects::MAXIMUM_OBJECT_ALIGNMENT / 4) as usize,
				) {
				if (powerlaw_bins_round_up_size(size).get() as u32 as usize) < size.get() {
					panic!("{}|{}", powerlaw_bins_round_up_size(size).get() as u32, size.get());
				}
				if bin != powerlaw_bin_from_size(powerlaw_bins_round_up_size(size).get() as u32 as usize) {
					panic!(
						"{}({}) {}|{}",
						powerlaw_bins_round_up_size(size).get() as u32,
						size.get(),
						powerlaw_bin_from_size(powerlaw_bins_round_up_size(size).get() as u32 as usize),
						bin
					);
				}
				unsafe {
					medium_objects::alloc(
						&mut self.medium_object_pages
							[(bin - powerlaw_bin_from_size((small_objects::MAXIMUM_OBJECT_ALIGNMENT * 2) as usize)) as usize],
						&mut self.medium_object_reserve,
						powerlaw_bins_round_up_size(size).get() as u32,
						#[cfg(feature = "tls")]
						self.id,
					)
				}
			} else if bin
				<= powerlaw_bin_from_size(
					(large_objects::MAXIMUM_OBJECT_ALIGNMENT
						+ large_objects::MAXIMUM_OBJECT_ALIGNMENT / 2
						+ large_objects::MAXIMUM_OBJECT_ALIGNMENT / 4) as usize,
				) {
				debug_assert!(powerlaw_bins_round_up_size(size).get() as u32 as usize >= size.get());
				debug_assert_eq!(
					bin,
					powerlaw_bin_from_size(powerlaw_bins_round_up_size(size).get() as u32 as usize)
				);
				unsafe {
					large_objects::alloc(
						&mut self.large_object_pages
							[(bin - powerlaw_bin_from_size((medium_objects::MAXIMUM_OBJECT_ALIGNMENT * 2) as usize)) as usize],
						powerlaw_bins_round_up_size(size).get() as u32,
						#[cfg(feature = "tls")]
						self.id,
					)
				}
			} else {
				let size = (size.get() + 4095) & !4095;
				unsafe { alloc_aligned(NonZero::new(size).unwrap(), alignment, 3) }
					.map(|ptr| ptr.as_ptr().cast())
					.unwrap_or(ptr::null_mut())
			}
		}
	}

	unsafe fn dealloc(
		#[cfg(not(feature = "tls"))] &mut self,
		#[cfg(feature = "tls")] id: HeapId,
		ptr: *mut u8,
		size: NonZero<usize>,
		_alignment: NonZero<usize>,
	) {
		unsafe {
			let bin = size.get().div_ceil(8);
			debug_assert!(bin > 0);
			if bin <= NUM_SMALL_OBJECT_BINS {
				debug_assert!(!ptr.is_null());
				small_objects::Page::dealloc(
					#[cfg(feature = "tls")]
					id,
					NonNull::new_unchecked(ptr),
				);
			} else {
				let bin = powerlaw_bin_from_size(size.get());
				if bin
					<= powerlaw_bin_from_size(
						(medium_objects::MAXIMUM_OBJECT_ALIGNMENT
							+ medium_objects::MAXIMUM_OBJECT_ALIGNMENT / 2
							+ medium_objects::MAXIMUM_OBJECT_ALIGNMENT / 4) as usize,
					) {
					medium_objects::Page::dealloc(
						#[cfg(feature = "tls")]
						id,
						NonNull::new_unchecked(ptr),
					);
				} else if bin
					<= powerlaw_bin_from_size(
						(large_objects::MAXIMUM_OBJECT_ALIGNMENT
							+ large_objects::MAXIMUM_OBJECT_ALIGNMENT / 2
							+ large_objects::MAXIMUM_OBJECT_ALIGNMENT / 4) as usize,
					) {
					large_objects::Page::dealloc(
						#[cfg(feature = "tls")]
						id,
						NonNull::new_unchecked(ptr),
					);
				} else {
					let size = (size.get() + 4095) & !4095;
					munmap(NonNull::new(ptr.cast()).unwrap(), NonZero::new(size).unwrap()).unwrap();
				}
			}
		}
	}
}

unsafe impl alloc::alloc::GlobalAlloc for Emma {
	unsafe fn alloc(&self, layout: core::alloc::Layout) -> *mut u8 {
		#[cfg(any(feature = "boundary-checks", debug_assertions))]
		{
			debug_assert!(layout.size() > 0);
			debug_assert!(layout.align().is_power_of_two());
		}

		let layout = layout.pad_to_align();

		#[cfg(not(feature = "tls"))]
		unsafe {
			self.heap.lock().alloc(
				NonZero::new(layout.size()).unwrap(),
				NonZero::new(layout.align()).unwrap(),
			)
		}
		#[cfg(feature = "tls")]
		if let Some(mut thread_heap) = self.thread_heap() {
			let ret = unsafe {
				thread_heap.as_mut().alloc(
					NonZero::new(layout.size()).unwrap(),
					NonZero::new(layout.align()).unwrap(),
				)
			};
			debug_assert!(
				ret.is_null() || ret as usize > 4096,
				"We should return a proper null-pointer"
			);
			ret
		} else {
			ptr::null_mut()
		}
	}

	unsafe fn realloc(&self, ptr: *mut u8, layout: core::alloc::Layout, new_size: usize) -> *mut u8 {
		#[cfg(any(feature = "boundary-checks", debug_assertions))]
		{
			assert_ne!(
				ptr,
				core::ptr::null_mut(),
				"Null pointers may not be passed to dealloc."
			);
			assert!(
				ptr as usize > 4096,
				"This looks like someone (slightly) indexed a null pointer and then tried to dealloc it."
			);
			assert!(layout.align().is_power_of_two());
			assert!(layout.size() > 0);
			assert!(new_size > 0);
			assert!(Layout::from_size_align(new_size, layout.align()).is_ok());
		}

		let layout = layout.pad_to_align();
		let new_layout = unsafe { Layout::from_size_align_unchecked(new_size, layout.align()).pad_to_align() };

		if layout.size() / 8 < NUM_SMALL_OBJECT_BINS {
			if layout.size() / 8 == new_layout.size() / 8 {
				return ptr;
			}
		} else {
			let old_bin = powerlaw_bin_from_size(layout.size());
			if old_bin
				<= powerlaw_bin_from_size(
					(large_objects::MAXIMUM_OBJECT_ALIGNMENT
						+ large_objects::MAXIMUM_OBJECT_ALIGNMENT / 2
						+ large_objects::MAXIMUM_OBJECT_ALIGNMENT / 4) as usize,
				) {
				let new_bin = powerlaw_bin_from_size(new_layout.size());
				if old_bin == new_bin {
					return ptr;
				}
			} else if new_layout.size()
				> (large_objects::MAXIMUM_OBJECT_ALIGNMENT
					+ large_objects::MAXIMUM_OBJECT_ALIGNMENT / 2
					+ large_objects::MAXIMUM_OBJECT_ALIGNMENT / 4) as usize
			{
				debug_assert!(
					powerlaw_bin_from_size(new_layout.size())
						> powerlaw_bin_from_size(
							(large_objects::MAXIMUM_OBJECT_ALIGNMENT
								+ large_objects::MAXIMUM_OBJECT_ALIGNMENT / 2
								+ large_objects::MAXIMUM_OBJECT_ALIGNMENT / 4) as usize,
						)
				);

				let old_size = (layout.size() + 4095) & !4095;
				let new_size = (new_layout.size() + 4095) & !4095;
				match old_size.cmp(&new_size) {
					core::cmp::Ordering::Less => {
						if unsafe {
							crate::mmap::mremap_resize(
								NonNull::new_unchecked(ptr).cast(),
								NonZero::new_unchecked(old_size),
								NonZero::new_unchecked(new_size),
							)
							.is_ok()
						} {
							return ptr;
						}
					}
					core::cmp::Ordering::Equal => return ptr,
					core::cmp::Ordering::Greater => {
						unsafe {
							crate::mmap::mremap_resize(
								NonNull::new_unchecked(ptr).cast(),
								NonZero::new_unchecked(old_size),
								NonZero::new_unchecked(new_size),
							)
							.unwrap()
						};
						return ptr;
					}
				}
			}
		}

		let new_ptr = unsafe { self.alloc(new_layout) };
		if !new_ptr.is_null() {
			unsafe {
				ptr::copy_nonoverlapping(ptr, new_ptr, core::cmp::min(layout.size(), new_size));
				self.dealloc(ptr, layout);
			}
		}
		new_ptr
	}

	unsafe fn dealloc(&self, ptr: *mut u8, layout: core::alloc::Layout) {
		#[cfg(any(feature = "boundary-checks", debug_assertions))]
		{
			assert_ne!(
				ptr,
				core::ptr::null_mut(),
				"Null pointers may not be passed to dealloc."
			);
			assert!(
				ptr as usize > 4096,
				"This looks like someone (slightly) indexed a null pointer and then tried to dealloc it."
			);
			assert!(layout.align().is_power_of_two());
			assert!(layout.size() > 0);
		}

		let layout = layout.pad_to_align();

		#[cfg(not(feature = "tls"))]
		unsafe {
			self.heap.lock().dealloc(
				ptr,
				NonZero::new(layout.size()).unwrap(),
				NonZero::new(layout.align()).unwrap(),
			)
		}
		#[cfg(feature = "tls")]
		unsafe {
			Heap::dealloc(
				// If we do not currently hold a heap, we can just use the NULL id that no allocated page should use.
				// This will end up using the foreign deallocation scheme - but as this thread does not have a heap, it could
				// not have allocated the object in the first place...
				THREAD_HEAP.map(|h| h.as_ref().id).unwrap_or(0),
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
