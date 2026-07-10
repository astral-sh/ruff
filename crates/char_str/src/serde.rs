use core::{fmt, str};

use ::serde::{
    de::{Deserialize, Deserializer, Error, Unexpected, Visitor},
    ser::{Serialize, Serializer},
};

use crate::CharStr;

impl Serialize for CharStr {
    #[inline]
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.as_str().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for CharStr {
    #[inline]
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct CharStrVisitor;

        impl<'de> Visitor<'de> for CharStrVisitor {
            type Value = CharStr;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a string")
            }

            fn visit_str<E: Error>(self, value: &str) -> Result<Self::Value, E> {
                Ok(CharStr::from(value))
            }

            fn visit_borrowed_str<E: Error>(self, value: &'de str) -> Result<Self::Value, E> {
                Ok(CharStr::from(value))
            }

            fn visit_bytes<E: Error>(self, value: &[u8]) -> Result<Self::Value, E> {
                str::from_utf8(value)
                    .map(CharStr::from)
                    .map_err(|_| Error::invalid_value(Unexpected::Bytes(value), &self))
            }

            fn visit_borrowed_bytes<E: Error>(self, value: &'de [u8]) -> Result<Self::Value, E> {
                self.visit_bytes(value)
            }
        }

        deserializer.deserialize_string(CharStrVisitor)
    }
}
