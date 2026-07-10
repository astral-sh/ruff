#![cfg(feature = "serde")]

use char_str::CharStr;

#[test]
fn roundtrip() {
    let value = CharStr::from("a string longer than the inline limit");
    let serialized = serde_json::to_string(&value).unwrap();
    let deserialized: CharStr = serde_json::from_str(&serialized).unwrap();

    assert_eq!(deserialized, value);
}

#[test]
fn invalid_utf8_bytes_are_rejected() {
    use serde::de::{DeserializeSeed, value::BorrowedBytesDeserializer};

    let deserializer = BorrowedBytesDeserializer::<serde::de::value::Error>::new(&[0xff]);
    let result = std::marker::PhantomData::<CharStr>.deserialize(deserializer);

    assert!(result.is_err());
}
