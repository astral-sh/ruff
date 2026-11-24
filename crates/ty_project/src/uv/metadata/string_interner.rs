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
    // Serde field hooks cannot receive per-parse state. `InternerGuard` restores
    // an enclosing parse's table on return or unwinding, without retaining strings between parses.
    static STRINGS: RefCell<Option<FxHashSet<CharStr>>> = const { RefCell::new(None) };
}

pub(super) struct InternerGuard {
    previous: Option<FxHashSet<CharStr>>,
}

impl InternerGuard {
    pub(super) fn new() -> Self {
        Self {
            previous: STRINGS.replace(Some(FxHashSet::default())),
        }
    }
}

impl Drop for InternerGuard {
    fn drop(&mut self) {
        STRINGS.set(self.previous.take());
    }
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
