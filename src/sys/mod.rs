#![allow(dead_code)]
#![allow(unused_imports)]

mod syscalls;

#[cfg(not(feature = "tls"))]
mod tls {
	use core::ffi::c_int;

	pub type Tid = c_int;

	pub fn gettid() -> c_int {
		super::syscalls::gettid()
	}
}

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

pub use syscalls::kill;
pub use tls::*;
