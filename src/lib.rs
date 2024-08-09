#![cfg_attr(not(test), no_std)]
#![cfg_attr(feature = "tls", feature(thread_local))]

extern crate alloc;

mod mmap;
mod sync;
mod sys;

mod emma;
pub use emma::{DefaultEmma, Emma};
