use core::mem::offset_of;
use core::num::NonZero;
use core::ptr::NonNull;
use core::sync::atomic::{AtomicU32, Ordering};

use const_format::assertc;
use syscalls::Errno;

use super::Heap;
use crate::mmap::alloc_aligned;
use crate::sync::Futex;
use crate::sync::syscalls::FUTEX_OWNER_DIED;
use crate::sys::Pid;

#[derive(Debug, Default)]
pub(crate) struct HeapManager;

impl HeapManager {
	pub(crate) const fn new() -> Self {
		Self
	}

	pub unsafe fn acquire_thread_heap(&self) -> Option<NonNull<Heap>> {
		unsafe { THREAD_HEAPS.lock().acquire_thread_heap() }
	}
}

/// This is not a member of [`HeapManager`], as we use thread-local storage to remember the heap per thread, which
/// causes them to be shared across different [`Emma`](super::Emma) instances. This means that the [`ThreadManager`]
/// also needs to be (effectively) shared across [`Emma`](super::Emma) instances.
static THREAD_HEAPS: Futex<ThreadHeaps> = Futex::new(ThreadHeaps::new());

struct ThreadHeaps {
	last_pid: Pid,
	heaps: Option<NonNull<ThreadHeap>>,
}
unsafe impl core::marker::Send for ThreadHeaps {}

impl ThreadHeaps {
	pub const fn new() -> Self {
		Self {
			last_pid: 0,
			heaps: None,
		}
	}

	pub unsafe fn acquire_thread_heap(&mut self) -> Option<NonNull<Heap>> {
		if let Some(already_owned) = unsafe { self.fixup_fork() } {
			return Some(already_owned);
		}

		let mut p = self.heaps;
		while let Some(thread_heap) = p {
			let thread_lock = unsafe {
				thread_heap
					.byte_add(offset_of!(ThreadHeap, thread_lock))
					.cast::<AtomicU32>()
					.as_ref()
			};

			// a robust futex sadly requires a global resource: https://www.man7.org/linux/man-pages/man2/set_robust_list.2.html
			let tid = thread_lock.load(Ordering::Relaxed);
			match unsafe { crate::sync::syscalls::futex_trylock_pi(thread_lock, crate::sync::syscalls::FutexFlags::PRIVATE) }
			{
				Ok(true) => {
					thread_lock.fetch_nand(FUTEX_OWNER_DIED, Ordering::Release);
					return Some(unsafe { thread_heap.byte_add(offset_of!(ThreadHeap, heap)).cast::<Heap>() });
				}
				Ok(false) | Err(Errno::EAGAIN) => (),
				Err(Errno::ESRCH) => {
					if thread_lock
						.compare_exchange(tid, crate::sys::gettid(), Ordering::Acquire, Ordering::Relaxed)
						.is_ok()
					{
						return Some(unsafe { thread_heap.byte_add(offset_of!(ThreadHeap, heap)).cast::<Heap>() });
					}
				}
				Err(Errno::EDEADLK) => {
					// This may happen in some rare post-`fork` circumstances:
					//
					// 1. Original process must have allocated memory on a thread that is _not_ the one doing the `fork`
					// 2. The process doing the `fork` must not have allocated memory previously
					// 3. The first thread allocating memory in the new process is not the main thread
					// 4. The main thread now allocates memory, which causes it to look for an available heap. It will find a heap
					//    that it has already locked due to the fixup done previously.
					return Some(unsafe { thread_heap.byte_add(offset_of!(ThreadHeap, heap)).cast::<Heap>() });
				}
				Err(Errno::ENOMEM) => panic!("ENOMEM"),
				Err(Errno::EINVAL) => panic!("EINVAL"),
				Err(Errno::ENOSYS) => panic!("ENOSYS"),
				Err(Errno::EPERM) => panic!("EPERM"),
				Err(err) => panic!("{}", err),
			}

			p = unsafe {
				*thread_heap
					.byte_add(offset_of!(ThreadHeap, next))
					.cast::<Option<NonNull<ThreadHeap>>>()
					.as_ref()
			};
		}

		assertc!(
			size_of::<ThreadHeap>().is_multiple_of(align_of::<ThreadHeap>()),
			"The ThreadHeap should have a size ({}) that is a multiple of its alignment ({}).",
			size_of::<ThreadHeap>(),
			align_of::<ThreadHeap>()
		);
		let size = (size_of::<ThreadHeap>() + 4095) & !4095;
		let thread_heap = unsafe {
			alloc_aligned(
				NonZero::new(size).unwrap(),
				NonZero::new(align_of::<ThreadHeap>()).unwrap(),
				3,
			)?
			.cast::<ThreadHeap>()
		};

		unsafe { thread_heap.write(ThreadHeap::new(self.heaps)) };
		self.heaps = Some(thread_heap);

		Some(unsafe { thread_heap.byte_add(offset_of!(ThreadHeap, heap)).cast::<Heap>() })
	}

	/// Fixes up locks on existing threads post fork. Potentially returns an already owned heap.
	#[inline]
	unsafe fn fixup_fork(&mut self) -> Option<NonNull<Heap>> {
		let pid = crate::sys::getpid();
		debug_assert_ne!(pid, 0);
		if self.last_pid != pid {
			self.last_pid = pid;

			// We don't know which heap is owned by the exactly one remaining thread post-fork, so we just mark all as owned
			// by it. Once that thread terminates, we can be assured that none of them remain owned by anyone.
			//
			// The one thread remaining after the fork has a TID that is equal to the (new) PID, as it is the new main thread
			// of the process.
			if let Some(mut heap) = self.heaps {
				loop {
					unsafe {
						heap
							.byte_add(offset_of!(ThreadHeap, thread_lock))
							.cast::<AtomicU32>()
							.as_mut()
							.store(pid, Ordering::Relaxed)
					};
					if let Some(next) = unsafe {
						*heap
							.byte_add(offset_of!(ThreadHeap, next))
							.cast::<Option<NonNull<ThreadHeap>>>()
							.as_ref()
					} {
						heap = next;
					} else {
						break;
					}
				}

				if crate::sys::gettid() == pid {
					return Some(unsafe {
						self
							.heaps
							.unwrap()
							.byte_add(offset_of!(ThreadHeap, heap))
							.cast::<Heap>()
					});
				}
			}
		}

		None
	}
}

#[derive(Debug)]
struct ThreadHeap {
	next: Option<NonNull<ThreadHeap>>,
	thread_lock: AtomicU32,
	heap: Heap,
}

impl ThreadHeap {
	fn new(next: Option<NonNull<ThreadHeap>>) -> Self {
		Self {
			next,
			thread_lock: AtomicU32::new(crate::sys::gettid()),
			heap: Heap::new(),
		}
	}
}
