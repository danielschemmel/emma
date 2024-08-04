use core::mem::offset_of;
use core::num::NonZero;
use core::ptr::{self, NonNull};
use core::sync::atomic::{AtomicPtr, AtomicU32, Ordering};

use static_assertions::const_assert_eq;

use super::Heap;
use crate::mmap::mmap_aligned;

#[derive(Debug, Default)]
pub(crate) struct HeapManager {
	heaps: AtomicPtr<ThreadHeap>,
}

#[derive(Debug)]
struct ThreadHeap {
	next: AtomicPtr<ThreadHeap>,
	thread_lock: AtomicU32,
	heap: Heap,
}

impl ThreadHeap {
	fn new(next: *mut ThreadHeap) -> Self {
		Self {
			next: AtomicPtr::new(next),
			thread_lock: AtomicU32::new(0),
			heap: Heap::new(),
		}
	}
}

impl HeapManager {
	pub(crate) const fn new() -> Self {
		Self {
			heaps: AtomicPtr::new(ptr::null_mut()),
		}
	}

	pub unsafe fn acquire_thread_heap(&self) -> Option<NonNull<Heap>> {
		// let mut p = self.heaps.load(Ordering::Relaxed);
		// while let Some(thread_heap) = NonNull::new(p) {
		// 	let thread_lock = thread_heap
		// 		.byte_add(offset_of!(ThreadHeap, thread_lock))
		// 		.cast::<AtomicU32>();

		// 	match crate::sync::syscalls::futex_trylock_pi(thread_lock.as_ref(), crate::sync::syscalls::FutexFlags::empty())
		// { 		Ok(true) => return Some(thread_heap.byte_add(offset_of!(ThreadHeap, heap)).cast::<Heap>()),
		// 		Ok(false) | Err(syscalls::Errno::EAGAIN) => (),
		// 		// FIXME: panics can not be processed in the memory allocator....
		// 		Err(_err) => return None,
		// 	}

		// 	p = thread_heap
		// 		.byte_add(offset_of!(ThreadHeap, next))
		// 		.cast::<AtomicPtr<ThreadHeap>>()
		// 		.as_ref()
		// 		.load(Ordering::Relaxed);
		// }

		const_assert_eq!(size_of::<ThreadHeap>() % align_of::<ThreadHeap>(), 0);
		let size = (size_of::<ThreadHeap>() + 4095) & !4095;
		let mut thread_heap = mmap_aligned(
			NonZero::new(size).unwrap(),
			NonZero::new(align_of::<ThreadHeap>()).unwrap(),
			3,
		)
		.map(|ptr| ptr.cast::<ThreadHeap>())?;

		let mut start = self.heaps.load(Ordering::Acquire);
		thread_heap.write(ThreadHeap::new(start));
		let locked = crate::sync::syscalls::futex_trylock_pi(
			&thread_heap.as_ref().thread_lock,
			crate::sync::syscalls::FutexFlags::empty(),
		);
		debug_assert_eq!(locked, Ok(true));
		debug_assert_ne!(thread_heap.as_ref().thread_lock.load(Ordering::Acquire), 0);

		while self
			.heaps
			.compare_exchange(start, thread_heap.as_ptr(), Ordering::AcqRel, Ordering::Relaxed)
			.is_err()
		{
			start = self.heaps.load(Ordering::Acquire);
			thread_heap.as_mut().next.store(start, Ordering::Release);
		}

		Some(thread_heap.byte_add(offset_of!(ThreadHeap, heap)).cast::<Heap>())
	}
}
