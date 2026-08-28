//! Shares repeated dependency strings while deserializing a uv metadata response.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fmt;
use std::marker::PhantomData;

use char_str::CharStr;
use rustc_hash::FxHashSet;
use serde::de::{MapAccess, Visitor};
use serde::{Deserialize, Deserializer};

thread_local! {
    // Serde field hooks cannot receive per-parse state. The guard in `with_interner` restores
    // an enclosing parse's table on return or unwinding, without retaining strings between parses.
    static STRINGS: RefCell<Option<FxHashSet<CharStr>>> = const { RefCell::new(None) };
}

struct InternerGuard {
    previous: Option<FxHashSet<CharStr>>,
}

impl Drop for InternerGuard {
    fn drop(&mut self) {
        STRINGS.set(self.previous.take());
    }
}

fn with_interner<T>(parse: impl FnOnce() -> T) -> T {
    let _guard = InternerGuard {
        previous: STRINGS.replace(Some(FxHashSet::default())),
    };
    parse()
}

pub(super) fn from_slice<'de, T: Deserialize<'de>>(
    input: &'de [u8],
) -> Result<T, serde_json::Error> {
    with_interner(|| serde_json::from_slice(input))
}

pub(super) fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<CharStr, D::Error> {
    struct StringVisitor;

    impl Visitor<'_> for StringVisitor {
        type Value = CharStr;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a string")
        }

        fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
            if let Some(inline) = CharStr::new_inline(value) {
                return Ok(inline);
            }
            Ok(STRINGS.with_borrow_mut(|strings| {
                let Some(strings) = strings else {
                    return CharStr::from(value);
                };
                if let Some(shared) = strings.get(value) {
                    return shared.clone();
                }
                let shared = CharStr::from(value);
                strings.insert(shared.clone());
                shared
            }))
        }
    }

    deserializer.deserialize_str(StringVisitor)
}

struct Interned(CharStr);

impl<'de> Deserialize<'de> for Interned {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserialize(deserializer).map(Self)
    }
}

pub(super) fn deserialize_optional<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<CharStr>, D::Error> {
    Option::<Interned>::deserialize(deserializer).map(|value| value.map(|value| value.0))
}

pub(super) fn deserialize_map<'de, D: Deserializer<'de>, V: Deserialize<'de>>(
    deserializer: D,
) -> Result<BTreeMap<CharStr, V>, D::Error> {
    struct MapVisitor<V>(PhantomData<V>);

    impl<'de, V: Deserialize<'de>> Visitor<'de> for MapVisitor<V> {
        type Value = BTreeMap<CharStr, V>;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a map")
        }

        fn visit_map<A: MapAccess<'de>>(self, mut access: A) -> Result<Self::Value, A::Error> {
            let mut values = BTreeMap::new();
            while let Some((key, value)) = access.next_entry::<Interned, V>()? {
                values.insert(key.0, value);
            }
            Ok(values)
        }
    }

    deserializer.deserialize_map(MapVisitor(PhantomData))
}

#[cfg(test)]
mod tests {
    use std::panic::catch_unwind;

    use rustc_hash::FxHashSet;

    use super::{Interned, STRINGS, from_slice, with_interner};

    const LONG_STRING: &[u8] = br#""a-string-longer-than-sixteen-bytes""#;

    #[test]
    fn skips_inline_strings() -> serde_json::Result<()> {
        with_interner(|| {
            let value: Interned = serde_json::from_slice(br#""short""#)?;
            assert!(!value.0.is_heap_allocated());
            assert!(
                STRINGS.with_borrow(|strings| strings.as_ref().is_some_and(FxHashSet::is_empty))
            );
            Ok(())
        })
    }

    #[test]
    fn restores_enclosing_interner() -> serde_json::Result<()> {
        with_interner(|| {
            let first: Interned = serde_json::from_slice(LONG_STRING)?;
            let nested: Interned = from_slice(LONG_STRING)?;
            assert_ne!(first.0.as_str().as_ptr(), nested.0.as_str().as_ptr());
            let second: Interned = serde_json::from_slice(LONG_STRING)?;
            assert_eq!(first.0.as_str().as_ptr(), second.0.as_str().as_ptr());
            Ok(())
        })
    }

    #[test]
    fn clears_interner_after_errors_and_unwinding() {
        assert!(
            from_slice::<Interned>(br#""a-string-longer-than-sixteen-bytes" trailing"#).is_err()
        );
        assert!(STRINGS.with_borrow(Option::is_none));
        let result = catch_unwind(|| {
            with_interner(|| {
                let _value = serde_json::from_slice::<Interned>(LONG_STRING);
                panic!("exercise interner cleanup during unwinding");
            });
        });
        assert!(result.is_err());
        assert!(STRINGS.with_borrow(Option::is_none));
    }
}
