use std::{
    borrow::Cow,
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
};

use char_str::CharStr;

const INLINE_LIMIT: usize = size_of::<CharStr>();

#[test]
fn layout() {
    assert_eq!(size_of::<CharStr>(), 2 * size_of::<usize>());
    assert_eq!(size_of::<Option<CharStr>>(), size_of::<CharStr>());
    assert_eq!(align_of::<CharStr>(), align_of::<usize>());
}

#[test]
fn storage_kinds() {
    const STATIC: CharStr =
        CharStr::from_static_str("a static string longer than the inline limit");

    let inline = CharStr::from("x".repeat(INLINE_LIMIT));
    let heap = CharStr::from("x".repeat(INLINE_LIMIT + 1));

    assert!(!inline.is_heap_allocated());
    assert!(heap.is_heap_allocated());
    assert!(!STATIC.is_heap_allocated());
}

#[test]
fn full_inline_storage_accepts_every_valid_last_byte_kind() {
    let nul = CharStr::from("\0".repeat(INLINE_LIMIT));
    let continuation = CharStr::from(format!("{}é", "x".repeat(INLINE_LIMIT - 2)));

    assert_eq!(nul.len(), INLINE_LIMIT);
    assert_eq!(continuation.len(), INLINE_LIMIT);
    assert!(!nul.is_heap_allocated());
    assert!(!continuation.is_heap_allocated());
}

#[test]
fn clone_shares_heap_storage() {
    let one = CharStr::from("a string longer than the inline limit");
    let two = one.clone();

    assert!(core::ptr::eq(one.as_ptr(), two.as_ptr()));
}

#[test]
fn clone_from_replaces_each_storage_kind() {
    let source = CharStr::from("a source string longer than the inline limit");
    let mut heap = CharStr::from("a different heap string that will be released");
    let mut inline = CharStr::from("inline");

    heap.clone_from(&source);
    inline.clone_from(&source);

    assert_eq!(heap, source);
    assert_eq!(inline, source);
    assert!(core::ptr::eq(heap.as_ptr(), source.as_ptr()));
    assert!(core::ptr::eq(inline.as_ptr(), source.as_ptr()));
}

#[test]
fn concat_uses_smallest_storage_kind() {
    let empty = CharStr::concat(&[]);
    let inline_text = "x".repeat(INLINE_LIMIT);
    let heap_text = "x".repeat(INLINE_LIMIT + 1);
    let inline = CharStr::concat(&[&inline_text[..1], &inline_text[1..]]);
    let heap = CharStr::try_concat(&[&heap_text[..1], &heap_text[1..]]).unwrap();

    assert!(empty.is_empty());
    assert!(!empty.is_heap_allocated());
    assert_eq!(inline, inline_text);
    assert!(!inline.is_heap_allocated());
    assert_eq!(heap, heap_text);
    assert!(heap.is_heap_allocated());
}

#[test]
fn join_uses_smallest_storage_kind() {
    let empty = CharStr::join::<&str>(&[], ".");
    let inline = CharStr::join(&["foo", "bar"], ".");
    let heap = CharStr::try_join(&["a long first component", "bar"], ".").unwrap();

    assert!(empty.is_empty());
    assert!(!empty.is_heap_allocated());
    assert_eq!(inline, "foo.bar");
    assert!(!inline.is_heap_allocated());
    assert_eq!(heap, "a long first component.bar");
    assert!(heap.is_heap_allocated());
}

#[test]
fn common_string_traits() {
    let text = "a string longer than the inline limit";
    let value = CharStr::from(text);
    let parsed: CharStr = text.parse().unwrap();
    let cow = CharStr::from(Cow::Borrowed(text));
    let boxed = CharStr::from(Box::<str>::from(text));
    let collected: CharStr = text.chars().collect();

    assert_eq!(value.as_bytes(), text.as_bytes());
    assert_eq!(value.to_string(), text);
    assert_eq!(String::from(&value), text);
    assert_eq!(String::from(value.clone()), text);
    assert_eq!(parsed, value);
    assert_eq!(cow, value);
    assert_eq!(boxed, value);
    assert_eq!(collected, value);
    assert_eq!(format!("{value:?}"), format!("{text:?}"));

    let mut value_hasher = DefaultHasher::new();
    value.hash(&mut value_hasher);
    let mut text_hasher = DefaultHasher::new();
    text.hash(&mut text_hasher);
    assert_eq!(value_hasher.finish(), text_hasher.finish());
}

#[test]
fn thread_safety() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<CharStr>();
}
