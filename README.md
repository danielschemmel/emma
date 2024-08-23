# Emma

[![docs.rs](https://img.shields.io/docsrs/emma)](https://docs.rs/emma)
[![Crates.io Version](https://img.shields.io/crates/v/emma)](https://crates.io/crates/emma)
[![Crates.io License](https://img.shields.io/crates/l/emma)](https://github.com/danielschemmel/emma?tab=readme-ov-file#license)
[![GitHub branch check runs](https://img.shields.io/github/check-runs/danielschemmel/emma/main)](https://github.com/danielschemmel/emma/actions?query=branch%3Amain)

Emma is an EMbeddable Memory Allocator. This means:

- Fully `no_std` compatible.
- No direct or indirect binary dependencies, including on `libc`: emma uses raw syscalls instead. Note that your rustc target may depend on `libc` - use the [`x86_64-unknown-linux-unknown` target](https://doc.rust-lang.org/rustc/platform-support/x86_64-unknown-linux-none.html), with which emma is compatible, if you want to avoid this.
- No usage of any shared resources: Instead of `brk`/`sbrk` to modify the _shared_ data segment, emma only ever maps its own segments using `mmap`.

Emma can coexist with other allocators in the same process.
If its symbols are renamed, emma can even coexist with other copies and/or versions of itself without interference!

## Usage
Use emma as you would any other allocator:

```rust
#[global_allocator]
static EMMA: emma::DefaultEmma = emma::DefaultEmma::new();
```

## Performance
Emma seems not far behind (other) state-of-the-art allocators.

## Target Architecture
At the moment, emma exclusively targets linux on `x86_64`.

## `fork`
Calling `fork` (or performing an equivalent `clone` call) is safe as long as no thread is currently de-/allocating memory.
If a thread was de-/allocating memory when `fork` was called, no further de-/allocation of memory may take place until such a time as the process calls `execve`.

## Cargo Features
- `tls` enabling thread-local-storage requires a nightly compiler. Enabling `tls` massively increases performance.
- `boundary-checks` enables assertions at the library boundary. These assertions cost a small amount of performance.

## License
Licensed under either of:

 * Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or https://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or https://opensource.org/licenses/MIT)

at your option.

### Contributions
Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
