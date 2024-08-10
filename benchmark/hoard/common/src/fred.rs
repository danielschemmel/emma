pub mod hl {
	use std::ffi::c_int;
	use std::mem::MaybeUninit;
	use std::thread;

	pub struct Fred {
		t: MaybeUninit<thread::JoinHandle<()>>,
	}

	impl Fred {
		pub fn new() -> Self {
			// pthread_attr_setscope has no meaning on linux
			Self {
				t: MaybeUninit::uninit(),
			}
		}

		pub fn create<T: Send + 'static, F: FnOnce(T) + Send + 'static>(&mut self, f: F, arg: T) {
			unsafe {
				self
					.t
					.write(thread::Builder::new().spawn(move || f(arg)).unwrap_unchecked());
			}
		}

		pub fn join(&mut self) {
			unsafe {
				self.t.assume_init_read().join().unwrap_unchecked();
			}
		}

		pub fn r#yield() {
			thread::yield_now();
		}

		pub fn set_concurrency(_n: c_int) {
			// `pthread_setconcurrency` has no meaning on linux
		}
	}
}

pub use hl::*;
