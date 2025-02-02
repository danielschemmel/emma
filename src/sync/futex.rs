use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use lock_api::{GuardSend, RawMutex};

use super::syscalls::{FutexFlags, futex_wait, futex_wake};

#[derive(Debug)]
pub struct RawFutex {
	lock: AtomicU32,
}

unsafe impl RawMutex for RawFutex {
	#[allow(clippy::declare_interior_mutable_const)]
	const INIT: RawFutex = RawFutex {
		lock: AtomicU32::new(0),
	};

	type GuardMarker = GuardSend;

	#[inline]
	fn lock(&self) {
		while !self.try_lock() {
			let res = unsafe { futex_wait(&self.lock, FutexFlags::empty(), 1, None) };
			debug_assert!(matches!(res, Ok(())) || matches!(res, Err(syscalls::Errno::EAGAIN)));
		}
	}

	#[inline]
	fn try_lock(&self) -> bool {
		self
			.lock
			.compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed)
			.is_ok()
	}

	#[inline]
	unsafe fn unlock(&self) {
		debug_assert_eq!(self.lock.load(Ordering::SeqCst), 1);
		self.lock.store(0, Ordering::Release);
		let res = unsafe { futex_wake(&self.lock, FutexFlags::empty(), 1) };
		debug_assert!(matches!(res, Ok(0 | 1)));
	}

	fn is_locked(&self) -> bool {
		self.lock.load(Ordering::Acquire) != 0
	}
}

// 3. Export the wrappers. This are the types that your users will actually use.
pub type Futex<T> = lock_api::Mutex<RawFutex, T>;
pub type FutexGuard<'a, T> = lock_api::MutexGuard<'a, RawFutex, T>;

#[cfg(test)]
mod test {
	use super::*;

	#[test]
	fn lock_unlock() {
		let f = Futex::new(42);
		assert!(!f.is_locked());
		{
			let mut guard = f.lock();
			assert!(f.is_locked());
			assert_eq!(*guard, 42);
			*guard = 65535;
			assert_eq!(*guard, 65535);
		}
		assert!(!f.is_locked());
		{
			let guard = f.lock();
			assert!(f.is_locked());
			assert_eq!(*guard, 65535);
		}
		assert!(!f.is_locked());
	}
}
