`emma` is a full-fledged allocator written in pure rust that is intended to not rely on anything in the target process. This means:

- Fully `no_std` compatible.
- No direct or indirect usage of `libc` (or any other non-rust library): `emma` uses raw syscalls instead.
- No usage of shared resources: Instead of `brk`/`sbrk` to modify the _shared_ data segment, `emma` only ever maps its own segment(s) using `mmap`.

This means that multiple `emma` instances can coexist with other allocators the same process.
If its symbols are renamed, `emma` can even coexist with other copies of itself!

# Performance
`emma` seems not far behind (other) state-of-the-art allocators.

# Target Architecture
At the moment, `emma` exclusively targets linux on `x86_64`.

# Known Issues
- It is not clear how `emma` behaves when a process is forked. (Note: [child processes of multi-threaded processes may only access async-signal-safe functions until they call `execve`](https://www.man7.org/linux/man-pages/man2/fork.2.html) anyway.)
- Calling `fork` (or performing an equivalent `clone` call) is safe as long as no thread is currently de-/allocating memory. However, if the forking process ever allocated memory on more than one thread, memory usage will be suboptimal until the new main thread terminates.
- Embedding multiple `emma`s into one process should work, but unless their symbols are renamed they may share data structures behind the scenes.
- An `emma` instance does not return all its resources, even if all allocations are returned. This is reasonable for a global allocator, but makes it not as useful as a temporary allocator.
- `emma` is not async-signal-safe, i.e., you may not de-/allocate memory in a signal handler. (The same probably holds true for your default memory allocator; POSIX does not list `malloc` or `free` as async-signal safe either.)

# Features
- `tls` enabling thread-local-storage requires a nightly compiler. Enabling `tls` massively increases performance.
- `boundary-checks` enables assertions at the library boundary. These assertions cost a small amount of performance.

# License
Licensed under either of:

 * Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or https://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or https://opensource.org/licenses/MIT)

at your option.

## Contributions
Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
