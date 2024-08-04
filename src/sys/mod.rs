#![allow(dead_code)]
#![allow(unused_imports)]

mod syscalls;

#[cfg(feature = "tls")]
mod tls {
	use core::ffi::c_int;

	pub type Tid = c_int;

	pub fn gettid() -> c_int {
		unsafe {
			#[thread_local]
			static mut TID: Tid = 0;

			if TID == 0 {
				TID = super::syscalls::gettid();
			}

			TID
		}
	}
}

#[cfg(feature = "tls")]
pub use tls::*;
