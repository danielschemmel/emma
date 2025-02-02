#![allow(dead_code)]
#![allow(unused_imports)]

mod madvise;
mod mmap;
mod mremap;
mod munmap;

pub use madvise::{MAdviseAdvice, madvise};
pub use mmap::{MMapFlags, MMapProt, mmap};
pub use mremap::mremap_resize;
pub use munmap::munmap;
