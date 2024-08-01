#![no_std]

extern crate alloc;

mod mmap;
mod sync;

mod emma;
pub use emma::{DefaultEmma, Emma};
