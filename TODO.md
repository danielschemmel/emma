# TODO
Things that we really should do.

## Release Unused Physical Pages
Actually releasing the pages is easy (`MADV_FREE`), but we need to know which pages to release to begin with.

## Test More
A memory allocator always needs more and better tests!

## Benchmark Better
- The current set of benchmarks is not really comprehensive enough.
- Benchmarks should be executed interleaved rather than in large runs of the same allocator.
- Argument parsing should replace editing the global constants.

## Support Other OSs
While not currently the target, it would be nice to support other OSs.

# Ideas
Interesting ideas that may or may not help.

## Collapse Normal Pages into Huge Pages
We could use `MADV_COLLAPSE` to collapse multiple normal virtual pages into huge pages.

### Pro
Huge pages require less room in the address translation buffers, which reduces both actual memory usage and improves performance.

### Contra
Collapsing pages may require copying the whole data (takes time).
Memory overhead may increase, as releasing 4k pages may become harder or even unfeasible.
