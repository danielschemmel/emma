#![allow(dead_code)]
#![allow(unused_imports)]

mod madvise;
mod mmap;
mod mremap;
mod munmap;

pub use madvise::{madvise, MAdviseAdvice};
pub use mmap::{mmap, MMapFlags, MMapProt};
pub use mremap::mremap_resize;
pub use munmap::munmap;
