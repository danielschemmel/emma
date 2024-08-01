use core::sync::atomic::{AtomicBool, Ordering};

use lock_api::{GuardSend, RawMutex};

use super::syscalls::sched_yield;

#[derive(Debug)]
pub struct RawSpinlock {
	lock: AtomicBool,
}

unsafe impl RawMutex for RawSpinlock {
	#[allow(clippy::declare_interior_mutable_const)]
	const INIT: RawSpinlock = RawSpinlock {
		lock: AtomicBool::new(false),
	};

	type GuardMarker = GuardSend;

	#[inline]
	fn lock(&self) {
		while !self.try_lock() {
			sched_yield();
		}
	}

	#[inline]
	fn try_lock(&self) -> bool {
		self
			.lock
			.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
			.is_ok()
	}

	#[inline]
	unsafe fn unlock(&self) {
		self.lock.store(false, Ordering::Release);
	}
}

// 3. Export the wrappers. This are the types that your users will actually use.
pub type Spinlock<T> = lock_api::Mutex<RawSpinlock, T>;
pub type SpinlockGuard<'a, T> = lock_api::MutexGuard<'a, RawSpinlock, T>;
