use std::fmt::Debug;
use std::str::FromStr;

/// A Python integer literal. Values that fit in a `u64` are stored directly.
/// For larger integers, the parser owns the spelling and AST readers borrow it.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct Int<S = Box<str>>(pub(crate) IntValue<S>);

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

impl<S> Int<S> {
    pub const ZERO: Self = Int(IntValue::Small(0));
    pub const ONE: Self = Int(IntValue::Small(1));

    /// Create an [`Int`] from a value that fits in a `u64`.
    fn small(value: u64) -> Self {
        Self(IntValue::Small(value))
    }
}

impl Int {
    /// Borrow the spelling of an integer too large to fit in a `u64`.
    pub fn as_borrowed(&self) -> Int<&str> {
        match &self.0 {
            IntValue::Small(value) => Int::small(*value),
            IntValue::Big(text) => Int(IntValue::Big(text)),
        }
    }

    /// Create an [`Int`] from the spelling of a value larger than `u64::MAX`.
    fn big(value: impl Into<Box<str>>) -> Self {
        Self(IntValue::Big(value.into()))
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
}

impl<S> Int<S> {
    /// Return the [`Int`] as an u8, if it can be represented as that data type.
    pub fn as_u8(&self) -> Option<u8> {
        match &self.0 {
            IntValue::Small(small) => u8::try_from(*small).ok(),
            IntValue::Big(_) => None,
        }
    }

    /// Return the [`Int`] as an u16, if it can be represented as that data type.
    pub fn as_u16(&self) -> Option<u16> {
        match &self.0 {
            IntValue::Small(small) => u16::try_from(*small).ok(),
            IntValue::Big(_) => None,
        }
    }

    /// Return the [`Int`] as an u32, if it can be represented as that data type.
    fn as_u32(&self) -> Option<u32> {
        match &self.0 {
            IntValue::Small(small) => u32::try_from(*small).ok(),
            IntValue::Big(_) => None,
        }
    }

    /// Return the [`Int`] as an u64, if it can be represented as that data type.
    pub const fn as_u64(&self) -> Option<u64> {
        match &self.0 {
            IntValue::Small(small) => Some(*small),
            IntValue::Big(_) => None,
        }
    }

    /// Return the [`Int`] as an usize, if it can be represented as that data type.
    pub fn as_usize(&self) -> Option<usize> {
        match &self.0 {
            IntValue::Small(small) => usize::try_from(*small).ok(),
            IntValue::Big(_) => None,
        }
    }

    /// Return the [`Int`] as an i8, if it can be represented as that data type.
    pub fn as_i8(&self) -> Option<i8> {
        match &self.0 {
            IntValue::Small(small) => i8::try_from(*small).ok(),
            IntValue::Big(_) => None,
        }
    }

    /// Return the [`Int`] as an i16, if it can be represented as that data type.
    fn as_i16(&self) -> Option<i16> {
        match &self.0 {
            IntValue::Small(small) => i16::try_from(*small).ok(),
            IntValue::Big(_) => None,
        }
    }

    /// Return the [`Int`] as an i32, if it can be represented as that data type.
    pub fn as_i32(&self) -> Option<i32> {
        match &self.0 {
            IntValue::Small(small) => i32::try_from(*small).ok(),
            IntValue::Big(_) => None,
        }
    }

    /// Return the [`Int`] as an i64, if it can be represented as that data type.
    pub fn as_i64(&self) -> Option<i64> {
        match &self.0 {
            IntValue::Small(small) => i64::try_from(*small).ok(),
            IntValue::Big(_) => None,
        }
    }
}

impl<S: std::fmt::Display> std::fmt::Display for Int<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl<S: std::fmt::Display> Debug for Int<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}

impl<S> PartialEq<u8> for Int<S> {
    fn eq(&self, other: &u8) -> bool {
        self.as_u8() == Some(*other)
    }
}

impl<S> PartialEq<u16> for Int<S> {
    fn eq(&self, other: &u16) -> bool {
        self.as_u16() == Some(*other)
    }
}

impl<S> PartialEq<u32> for Int<S> {
    fn eq(&self, other: &u32) -> bool {
        self.as_u32() == Some(*other)
    }
}

impl<S> PartialEq<i8> for Int<S> {
    fn eq(&self, other: &i8) -> bool {
        self.as_i8() == Some(*other)
    }
}

impl<S> PartialEq<i16> for Int<S> {
    fn eq(&self, other: &i16) -> bool {
        self.as_i16() == Some(*other)
    }
}

impl<S> PartialEq<i32> for Int<S> {
    fn eq(&self, other: &i32) -> bool {
        self.as_i32() == Some(*other)
    }
}

impl<S> PartialEq<i64> for Int<S> {
    fn eq(&self, other: &i64) -> bool {
        self.as_i64() == Some(*other)
    }
}

impl<S> From<u8> for Int<S> {
    fn from(value: u8) -> Self {
        Self::small(u64::from(value))
    }
}

impl<S> From<u16> for Int<S> {
    fn from(value: u16) -> Self {
        Self::small(u64::from(value))
    }
}

impl<S> From<u32> for Int<S> {
    fn from(value: u32) -> Self {
        Self::small(u64::from(value))
    }
}

impl<S> From<u64> for Int<S> {
    fn from(value: u64) -> Self {
        Self::small(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub(crate) enum IntValue<S> {
    /// A "small" number that can be represented as a `u64`.
    Small(u64),
    /// A "large" number that cannot be represented as a `u64`.
    Big(S),
}

impl<S: std::fmt::Display> std::fmt::Display for IntValue<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IntValue::Small(value) => write!(f, "{value}"),
            IntValue::Big(value) => write!(f, "{value}"),
        }
    }
}
