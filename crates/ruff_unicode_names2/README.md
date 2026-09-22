# unicode_names2

[![Build Status](https://travis-ci.org/ProgVal/unicode_names2.png)](https://travis-ci.org/ProgVal/unicode_names2)

Time and memory efficiently mapping characters to and from their
Unicode 17.0 names, at runtime and compile-time.

```rust
fn main() {
    println!("☃ is called {}", unicode_names2::name('☃')); // SNOWMAN
    println!("{} is happy", unicode_names2::character("white smiling face")); // ☺
    // (NB. case insensitivity)
}
```

The maps are compressed using similar tricks to Python's `unicodedata`
module, although those here are about 70KB (12%) smaller.

[**Documentation**](https://docs.rs/unicode_names2)
