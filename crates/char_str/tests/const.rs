use char_str::CharStr;

static INLINE: CharStr = CharStr::from_static_str("hello world");
static LONG: CharStr = CharStr::from_static_str("a static string longer than the inline limit");
static LENGTH_VALUE: CharStr = CharStr::from_static_str("hello");

const INLINE_TEXT: &str = INLINE.as_str();
const LONG_TEXT: &str = LONG.as_str();
const LENGTH: usize = LENGTH_VALUE.len();

#[test]
fn static_strings_are_const() {
    assert_eq!(std::hint::black_box(INLINE_TEXT), "hello world");
    assert_eq!(
        std::hint::black_box(LONG_TEXT),
        "a static string longer than the inline limit"
    );
    assert!(!INLINE.is_heap_allocated());
    assert!(!LONG.is_heap_allocated());
}

#[test]
fn length_is_const() {
    assert_eq!(std::hint::black_box(LENGTH), 5);
}
