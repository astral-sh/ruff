//! `builtin_types` in asdl.py and Attributed

use num_bigint::BigInt;
use rustpython_parser_core::text_size::{TextRange, TextSize};

pub type String = std::string::String;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Identifier(String);

impl Identifier {
    #[inline]
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
    #[inline]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl std::string::ToString for Identifier {
    #[inline]
    fn to_string(&self) -> String {
        self.0.clone()
    }
}

impl From<Identifier> for String {
    #[inline]
    fn from(id: Identifier) -> String {
        id.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Int(u32);

impl Int {
    pub fn new(i: u32) -> Self {
        Self(i)
    }
    pub fn new_bool(i: bool) -> Self {
        Self(i as u32)
    }
    pub fn to_u32(&self) -> u32 {
        self.0
    }
    pub fn to_usize(&self) -> usize {
        self.0 as _
    }
    pub fn to_bool(&self) -> bool {
        self.0 > 0
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Constant {
    None,
    Bool(bool),
    Str(String),
    Bytes(Vec<u8>),
    Int(BigInt),
    Tuple(Vec<Constant>),
    Float(f64),
    Complex { real: f64, imag: f64 },
    Ellipsis,
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
            Constant::Tuple(tup) => {
                if let [elt] = &**tup {
                    write!(f, "({elt},)")
                } else {
                    f.write_str("(")?;
                    for (i, elt) in tup.iter().enumerate() {
                        if i != 0 {
                            f.write_str(", ")?;
                        }
                        elt.fmt(f)?;
                    }
                    f.write_str(")")
                }
            }
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

#[derive(Clone, Debug, PartialEq)]
pub struct Attributed<T, U = ()> {
    pub range: TextRange,
    pub custom: U,
    pub node: T,
}

impl<T, U> Attributed<T, U> {
    /// Returns the node
    #[inline]
    pub fn node(&self) -> &T {
        &self.node
    }

    /// Returns the `range` of the node. The range offsets are absolute to the start of the document.
    #[inline]
    pub const fn range(&self) -> TextRange {
        self.range
    }

    /// Returns the absolute start position of the node from the beginning of the document.
    #[inline]
    pub const fn start(&self) -> TextSize {
        self.range.start()
    }

    /// Returns the absolute position at which the node ends in the source document.
    #[inline]
    pub const fn end(&self) -> TextSize {
        self.range.end()
    }
}

impl<T> Attributed<T, ()> {
    /// Creates a new node that spans the position specified by `range`.
    pub fn new(range: impl Into<TextRange>, node: T) -> Self {
        Self {
            range: range.into(),
            custom: (),
            node,
        }
    }

    /// Consumes self and returns the node.
    #[inline]
    pub fn into_node(self) -> T {
        self.node
    }
}

impl<T, U> std::ops::Deref for Attributed<T, U> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.node
    }
}
