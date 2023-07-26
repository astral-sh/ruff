//! `builtin_types` in Attributed

use rustpython_parser_core::text_size::TextRange;

use crate::Ranged;
use num_bigint::BigInt;

pub type String = std::string::String;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Identifier {
    id: String,
    range: TextRange,
}

impl Identifier {
    #[inline]
    pub fn new(id: impl Into<String>, range: TextRange) -> Self {
        Self {
            id: id.into(),
            range,
        }
    }
}

impl Identifier {
    #[inline]
    pub fn as_str(&self) -> &str {
        self.id.as_str()
    }
}

impl PartialEq<str> for Identifier {
    #[inline]
    fn eq(&self, other: &str) -> bool {
        self.id == other
    }
}

impl PartialEq<String> for Identifier {
    #[inline]
    fn eq(&self, other: &String) -> bool {
        &self.id == other
    }
}

impl std::ops::Deref for Identifier {
    type Target = str;
    #[inline]
    fn deref(&self) -> &Self::Target {
        self.id.as_str()
    }
}

impl AsRef<str> for Identifier {
    #[inline]
    fn as_ref(&self) -> &str {
        self.id.as_str()
    }
}

impl AsRef<String> for Identifier {
    #[inline]
    fn as_ref(&self) -> &String {
        &self.id
    }
}

impl std::fmt::Display for Identifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.id.fmt(f)
    }
}

impl From<Identifier> for String {
    #[inline]
    fn from(identifier: Identifier) -> String {
        identifier.id
    }
}

impl Ranged for Identifier {
    fn range(&self) -> TextRange {
        self.range
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Int(u32);

impl Int {
    pub fn new(i: u32) -> Self {
        Self(i)
    }
    pub fn to_u32(&self) -> u32 {
        self.0
    }
    pub fn to_usize(&self) -> usize {
        self.0 as _
    }
}

impl std::cmp::PartialEq<u32> for Int {
    #[inline]
    fn eq(&self, other: &u32) -> bool {
        self.0 == *other
    }
}

impl std::cmp::PartialEq<usize> for Int {
    #[inline]
    fn eq(&self, other: &usize) -> bool {
        self.0 as usize == *other
    }
}

#[derive(Clone, Debug, PartialEq, is_macro::Is)]
pub enum Constant {
    None,
    Bool(bool),
    Str(String),
    Bytes(Vec<u8>),
    Int(BigInt),
    Float(f64),
    Complex { real: f64, imag: f64 },
    Ellipsis,
}

impl Constant {
    pub fn is_true(self) -> bool {
        self.bool().map_or(false, |b| b)
    }
    pub fn is_false(self) -> bool {
        self.bool().map_or(false, |b| !b)
    }
    pub fn complex(self) -> Option<(f64, f64)> {
        match self {
            Constant::Complex { real, imag } => Some((real, imag)),
            _ => None,
        }
    }
}

impl From<String> for Constant {
    fn from(s: String) -> Constant {
        Self::Str(s)
    }
}
impl From<Vec<u8>> for Constant {
    fn from(b: Vec<u8>) -> Constant {
        Self::Bytes(b)
    }
}
impl From<bool> for Constant {
    fn from(b: bool) -> Constant {
        Self::Bool(b)
    }
}
impl From<BigInt> for Constant {
    fn from(i: BigInt) -> Constant {
        Self::Int(i)
    }
}

#[cfg(feature = "rustpython-literal")]
impl std::fmt::Display for Constant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Constant::None => f.pad("None"),
            Constant::Bool(b) => f.pad(if *b { "True" } else { "False" }),
            Constant::Str(s) => rustpython_literal::escape::UnicodeEscape::new_repr(s.as_str())
                .str_repr()
                .write(f),
            Constant::Bytes(b) => {
                let escape = rustpython_literal::escape::AsciiEscape::new_repr(b);
                let repr = escape.bytes_repr().to_string().unwrap();
                f.pad(&repr)
            }
            Constant::Int(i) => i.fmt(f),
            Constant::Float(fp) => f.pad(&rustpython_literal::float::to_string(*fp)),
            Constant::Complex { real, imag } => {
                if *real == 0.0 {
                    write!(f, "{imag}j")
                } else {
                    write!(f, "({real}{imag:+}j)")
                }
            }
            Constant::Ellipsis => f.pad("..."),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_macro() {
        let none = Constant::None;
        assert!(none.is_none());
        assert!(!none.is_bool());
    }
}
