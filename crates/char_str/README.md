# `CharStr`

`CharStr` is a compact, immutable, UTF-8 encoded string type.

It has the following properties:

- `size_of::<CharStr>() == size_of::<[usize; 2]>()` (two words).
- `size_of::<Option<CharStr>>() == size_of::<CharStr>()`.
- Strings up to two words long are stored inline.
- Longer strings use exactly-sized, reference-counted heap allocations for cheap clones.
- Construction from a long `&'static str` is `const`, allocation-free, and O(1).

```rust
use char_str::CharStr;

let inline = CharStr::from("hello");
assert!(!inline.is_heap_allocated());

let heap = CharStr::from("a string longer than the inline limit");
let shared = heap.clone();
assert!(core::ptr::eq(heap.as_ptr(), shared.as_ptr()));

const STATIC: CharStr = CharStr::from_static_str("a long string stored in static memory");
assert!(!STATIC.is_heap_allocated());
```

## Acknowledgements

This crate is a fork of [`lean_string`](https://github.com/ryota2357/lean_string) by ryota2357,
based on the immutable `LeanStr` implementation proposed in
[`lean_string#6`](https://github.com/ryota2357/lean_string/pull/6), and trimmed down to only that
immutable string type. `LeanStr` is not part of upstream `lean_string`. `lean_string` is available
under the MIT license; its original license is included in
[`LICENSE-lean_string`](LICENSE-lean_string).
