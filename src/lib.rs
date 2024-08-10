//! Emma is an EMbeddable Memory Allocator. It is `no_std` and "no-libc"" safe, and has zero binary dependencies.
//!
//! Use emma as you would any other allocator:
//!
//! ```rust
//! #[global_allocator]
//! static EMMA: emma::DefaultEmma = emma::DefaultEmma::new();
//! ```

#![cfg_attr(not(test), no_std)]
#![cfg_attr(feature = "tls", feature(thread_local))]

extern crate alloc;

mod mmap;
mod sync;
mod sys;

mod emma;
pub use emma::{DefaultEmma, Emma};
