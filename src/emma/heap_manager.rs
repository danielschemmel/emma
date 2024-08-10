use core::mem::offset_of;
use core::num::NonZero;
use core::ptr::{self, NonNull};
use core::sync::atomic::{AtomicPtr, AtomicU32, Ordering};

use static_assertions::const_assert_eq;
use syscalls::Errno;

use super::Heap;
use crate::mmap::alloc_aligned;
use crate::sync::syscalls::FUTEX_OWNER_DIED;

#[derive(Debug, Default)]
pub(crate) struct HeapManager;

/// A singly-linked list of all [`ThreadHeap`]s ever acquired. Each of these may or may not be currently owned by a
/// thread, which may or may not be the currently active thread.
///
/// This is not a member of [`HeapManager`], as we use thread-local storage to remember the heap per thread, which
/// causes them to be shared across different [`Emma`](super::Emma) instances. This means that the [`ThreadManager`]
/// also needs to be (effectively) shared across [`Emma`](super::Emma) instances.
static THREAD_HEAPS: AtomicPtr<ThreadHeap> = AtomicPtr::new(ptr::null_mut());

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
		Self
	}

	pub unsafe fn acquire_thread_heap(&self) -> Option<NonNull<Heap>> {
		let mut p = THREAD_HEAPS.load(Ordering::Relaxed);
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
				Err(Errno::ENOMEM) => panic!("ENOMEM"),
				Err(Errno::EDEADLK) => panic!("EDEADLK"),
				Err(Errno::EINVAL) => panic!("EINVAL"),
				Err(Errno::ENOSYS) => panic!("ENOSYS"),
				Err(Errno::EPERM) => panic!("EPERM"),
				Err(err) => panic!("{}", err),
			}

			p = thread_heap
				.byte_add(offset_of!(ThreadHeap, next))
				.cast::<AtomicPtr<ThreadHeap>>()
				.as_ref()
				.load(Ordering::Relaxed);
		}

		const_assert_eq!(size_of::<ThreadHeap>() % align_of::<ThreadHeap>(), 0);
		let size = (size_of::<ThreadHeap>() + 4095) & !4095;
		let thread_heap = alloc_aligned(
			NonZero::new(size).unwrap(),
			NonZero::new(align_of::<ThreadHeap>()).unwrap(),
			3,
		)
		.map(|ptr| ptr.cast::<ThreadHeap>())?;

		let mut start = THREAD_HEAPS.load(Ordering::Acquire);
		loop {
			thread_heap.write(ThreadHeap::new(start));
			match THREAD_HEAPS.compare_exchange(start, thread_heap.as_ptr(), Ordering::Relaxed, Ordering::Acquire) {
				Ok(_) => break,
				Err(new_start) => start = new_start,
			}
		}

		Some(thread_heap.byte_add(offset_of!(ThreadHeap, heap)).cast::<Heap>())
	}
}
