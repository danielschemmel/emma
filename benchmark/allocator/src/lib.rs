#[cfg(feature = "emma")]
pub type Allocator = emma::DefaultEmma;
#[cfg(feature = "emma")]
pub const fn create_allocator() -> Allocator { emma::DefaultEmma::new() }

#[cfg(feature = "libc")]
pub type Allocator = libc_alloc::LibcAlloc;
#[cfg(feature = "libc")]
pub const fn create_allocator() -> Allocator { libc_alloc::LibcAlloc }

#[cfg(feature = "std")]
pub type Allocator = std::alloc::System;
#[cfg(feature = "std")]
pub const fn create_allocator() -> Allocator { std::alloc::System }

#[cfg(feature = "jemalloc")]
pub type Allocator = tikv_jemallocator::Jemalloc;
#[cfg(feature = "jemalloc")]
pub const fn create_allocator() -> Allocator { tikv_jemallocator::Jemalloc }

#[cfg(feature = "mimalloc")]
pub type Allocator = mimalloc::MiMalloc;
#[cfg(feature = "mimalloc")]
pub const fn create_allocator() -> Allocator { mimalloc::MiMalloc }
