use std::fmt::Debug;
use std::str::FromStr;

/// A Python integer literal. Represents both small (fits in an `i64`) and large integers.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Int(Number);

impl FromStr for Int {
    type Err = std::num::ParseIntError;

    /// Parse an [`Int`] from a string.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.parse::<u64>() {
            Ok(value) => Ok(Int::small(value)),
            Err(err) => {
                if matches!(
                    err.kind(),
                    std::num::IntErrorKind::PosOverflow | std::num::IntErrorKind::NegOverflow
                ) {
                    Ok(Int::big(s))
                } else {
                    Err(err)
                }
            }
        }
    }
}

impl Int {
    pub const ZERO: Int = Int(Number::Small(0));
    pub const ONE: Int = Int(Number::Small(1));

    /// Create an [`Int`] to represent a value that can be represented as an `i64`.
    fn small(value: u64) -> Self {
        Self(Number::Small(value))
    }

    /// Create an [`Int`] to represent a value that cannot be represented as an `i64`.
    fn big(value: impl Into<Box<str>>) -> Self {
        Self(Number::Big(value.into()))
    }

    /// Parse an [`Int`] from a string with a given radix, like `0x95D`.
    ///
    /// Takes, as input, the numerical portion (`95D`), the parsed base (`16`), and the entire
    /// token (`0x95D`).
    pub fn from_str_radix(
        number: &str,
        radix: u32,
        token: &str,
    ) -> Result<Self, std::num::ParseIntError> {
        match u64::from_str_radix(number, radix) {
            Ok(value) => Ok(Int::small(value)),
            Err(err) => {
                if matches!(
                    err.kind(),
                    std::num::IntErrorKind::PosOverflow | std::num::IntErrorKind::NegOverflow
                ) {
                    Ok(Int::big(token))
                } else {
                    Err(err)
                }
            }
        }
    }

    /// Return the [`Int`] as an u8, if it can be represented as that data type.
    pub fn as_u8(&self) -> Option<u8> {
        match &self.0 {
            Number::Small(small) => u8::try_from(*small).ok(),
            Number::Big(_) => None,
        }
    }

    /// Return the [`Int`] as an u16, if it can be represented as that data type.
    pub fn as_u16(&self) -> Option<u16> {
        match &self.0 {
            Number::Small(small) => u16::try_from(*small).ok(),
            Number::Big(_) => None,
        }
    }

    /// Return the [`Int`] as an u32, if it can be represented as that data type.
    pub fn as_u32(&self) -> Option<u32> {
        match &self.0 {
            Number::Small(small) => u32::try_from(*small).ok(),
            Number::Big(_) => None,
        }
    }

    /// Return the [`Int`] as an u64, if it can be represented as that data type.
    pub const fn as_u64(&self) -> Option<u64> {
        match &self.0 {
            Number::Small(small) => Some(*small),
            Number::Big(_) => None,
        }
    }

    /// Return the [`Int`] as an i8, if it can be represented as that data type.
    pub fn as_i8(&self) -> Option<i8> {
        match &self.0 {
            Number::Small(small) => i8::try_from(*small).ok(),
            Number::Big(_) => None,
        }
    }

    /// Return the [`Int`] as an i16, if it can be represented as that data type.
    pub fn as_i16(&self) -> Option<i16> {
        match &self.0 {
            Number::Small(small) => i16::try_from(*small).ok(),
            Number::Big(_) => None,
        }
    }

    /// Return the [`Int`] as an i32, if it can be represented as that data type.
    pub fn as_i32(&self) -> Option<i32> {
        match &self.0 {
            Number::Small(small) => i32::try_from(*small).ok(),
            Number::Big(_) => None,
        }
    }

    /// Return the [`Int`] as an i64, if it can be represented as that data type.
    pub fn as_i64(&self) -> Option<i64> {
        match &self.0 {
            Number::Small(small) => i64::try_from(*small).ok(),
            Number::Big(_) => None,
        }
    }
}

impl std::fmt::Display for Int {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Debug for Int {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}

impl PartialEq<u8> for Int {
    fn eq(&self, other: &u8) -> bool {
        self.as_u8() == Some(*other)
    }
}

impl PartialEq<u16> for Int {
    fn eq(&self, other: &u16) -> bool {
        self.as_u16() == Some(*other)
    }
}

impl PartialEq<u32> for Int {
    fn eq(&self, other: &u32) -> bool {
        self.as_u32() == Some(*other)
    }
}

impl PartialEq<i8> for Int {
    fn eq(&self, other: &i8) -> bool {
        self.as_i8() == Some(*other)
    }
}

impl PartialEq<i16> for Int {
    fn eq(&self, other: &i16) -> bool {
        self.as_i16() == Some(*other)
    }
}

impl PartialEq<i32> for Int {
    fn eq(&self, other: &i32) -> bool {
        self.as_i32() == Some(*other)
    }
}

impl PartialEq<i64> for Int {
    fn eq(&self, other: &i64) -> bool {
        self.as_i64() == Some(*other)
    }
}

impl From<u8> for Int {
    fn from(value: u8) -> Self {
        Self::small(u64::from(value))
    }
}

impl From<u16> for Int {
    fn from(value: u16) -> Self {
        Self::small(u64::from(value))
    }
}

impl From<u32> for Int {
    fn from(value: u32) -> Self {
        Self::small(u64::from(value))
    }
}

impl From<u64> for Int {
    fn from(value: u64) -> Self {
        Self::small(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Number {
    /// A "small" number that can be represented as an `u64`.
    Small(u64),
    /// A "large" number that cannot be represented as an `u64`.
    Big(Box<str>),
}

impl std::fmt::Display for Number {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Number::Small(value) => write!(f, "{value}"),
            Number::Big(value) => write!(f, "{value}"),
        }
    }
}
