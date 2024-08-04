`emma` is a full-fledged allocator written in pure rust that is intended to not rely on anything in the target process. This means:

- Fully `no_std` compatible.
- No direct or indirect usage of `libc` (or any other non-rust library): `emma` uses raw syscalls instead.
- No usage of shared resources: Instead of `brk`/`sbrk` to modify the _shared_ data segment, `emma` only ever maps its own segment(s) using `mmap`.

This means that multiple `emma` instances can be used in the same process without ever noticing one another.

# Performance
At the moment, `emma` is not very performant.

# Target Architecture
At the moment, `emma` exclusively targets linux on `x86_64`.

# Known Issues
- It is not clear how `emma` behaves when a process is forked (probably badly, especially if one of the other threads is currently de-/allocating memory).
- If a thread is terminated _during memory de-/allocation_, heap corruption may ensue. Terminating a thread at any other time in any way does not bother `emma`.
- Embedding multiple `emma`s into one process should work, but unless their symbols are renamed they may share data structures behind the scenes.
- `emma` cannot full clean up after itself, even if all allocations are returned.
- `emma` is not async-signal-safe, i.e., you may not de-/allocate memory in a signal handler. (The same probably holds true for your default memory allocator; POSIX does not list `malloc` or `free` as async-signal safe either.)

# Features
- `tls` enabling thread-local-storage requires a nightly compiler.
