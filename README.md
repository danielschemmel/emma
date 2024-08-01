`emma` is an allocator written in pure rust that is intended to not rely on anything in the target process. This means:

- Fully `no_std` compatible.
- No direct or indirect usage of `libc`: `emma` uses raw syscalls instead.
- No usage of shared resources: Instead of `brk`/`sbrk` to modify the _shared_ data segment, `emma` only ever maps its own segment(s) using `mmap`.

This means that multiple `emma` instances can be used in the same process without ever noticing one another.

# Performance
At the moment, `emma` is not very performant.

# Target Architecture
At the moment, `emma` exclusively targets linux on `x86_64`. Pull requests for reasonably common targets are welcome ;).
