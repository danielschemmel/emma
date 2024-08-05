use core::mem::offset_of;
use core::num::NonZero;
use core::ptr::{self, NonNull};
use core::sync::atomic::{AtomicPtr, AtomicU32, Ordering};

use static_assertions::const_assert_eq;
use syscalls::Errno;

use super::Heap;
use crate::mmap::mmap_aligned;
use crate::sync::syscalls::FUTEX_OWNER_DIED;

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
			thread_lock: AtomicU32::new(crate::sys::gettid() as u32),
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
		let mut p = self.heaps.load(Ordering::Relaxed);
		while let Some(thread_heap) = NonNull::new(p) {
			let thread_lock = thread_heap
				.byte_add(offset_of!(ThreadHeap, thread_lock))
				.cast::<AtomicU32>();

			// a robust futex sadly requires a global resource: https://www.man7.org/linux/man-pages/man2/set_robust_list.2.html
			let tid = thread_lock.as_ref().load(Ordering::Relaxed);
			match crate::sync::syscalls::futex_trylock_pi(thread_lock.as_ref(), crate::sync::syscalls::FutexFlags::PRIVATE) {
				Ok(true) => {
					thread_lock.as_ref().fetch_nand(FUTEX_OWNER_DIED, Ordering::Release);
					return Some(thread_heap.byte_add(offset_of!(ThreadHeap, heap)).cast::<Heap>());
				}
				Ok(false) | Err(Errno::EAGAIN) => (),
				Err(Errno::ESRCH) => {
					if thread_lock
						.as_ref()
						.compare_exchange(tid, crate::sys::gettid() as u32, Ordering::Acquire, Ordering::Relaxed)
						.is_ok()
					{
						return Some(thread_heap.byte_add(offset_of!(ThreadHeap, heap)).cast::<Heap>());
					}
				}
				Err(_) => panic!(),
			}

			p = thread_heap
				.byte_add(offset_of!(ThreadHeap, next))
				.cast::<AtomicPtr<ThreadHeap>>()
				.as_ref()
				.load(Ordering::Relaxed);
		}

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
