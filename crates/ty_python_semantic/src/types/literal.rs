use compact_str::CompactString;
use ruff_python_ast::name::Name;

use crate::Db;
use crate::types::set_theoretic::RecursivelyDefined;
use crate::types::{ClassLiteral, KnownClass, Type};

/// A literal value. See [`LiteralValueTypeKind`] for details.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize)]
pub struct LiteralValueType<'db>(LiteralValueTypeInner<'db>);

/// This enum encodes three orthogonal properties in its discriminant:
///   - kind (`Int, Bool, String, Enum, Bytes, LiteralString`)
///   - promotability (Promotable vs Unpromotable)
///   - whether it is recursively defined (Yes vs No)
///
/// Resulting in 6 x 2 x 2 = 24 variants, all fitting in a single u8 discriminant
/// while keeping the maximum payload at 8 bytes (`IntLiteralType`).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize)]
enum LiteralValueTypeInner<'db> {
    PromotableInt(IntLiteralType),
    PromotableBool(bool),
    PromotableString(StringLiteralType<'db>),
    PromotableEnum(EnumLiteralType<'db>),
    PromotableBytes(BytesLiteralType<'db>),
    PromotableLiteralString,
    UnpromotableInt(IntLiteralType),
    UnpromotableBool(bool),
    UnpromotableString(StringLiteralType<'db>),
    UnpromotableEnum(EnumLiteralType<'db>),
    UnpromotableBytes(BytesLiteralType<'db>),
    UnpromotableLiteralString,
    RecPromotableInt(IntLiteralType),
    RecPromotableBool(bool),
    RecPromotableString(StringLiteralType<'db>),
    RecPromotableEnum(EnumLiteralType<'db>),
    RecPromotableBytes(BytesLiteralType<'db>),
    RecPromotableLiteralString,
    RecUnpromotableInt(IntLiteralType),
    RecUnpromotableBool(bool),
    RecUnpromotableString(StringLiteralType<'db>),
    RecUnpromotableEnum(EnumLiteralType<'db>),
    RecUnpromotableBytes(BytesLiteralType<'db>),
    RecUnpromotableLiteralString,
}

use LiteralValueTypeInner::{
    PromotableBool, PromotableBytes, PromotableEnum, PromotableInt, PromotableLiteralString,
    PromotableString, RecPromotableBool, RecPromotableBytes, RecPromotableEnum, RecPromotableInt,
    RecPromotableLiteralString, RecPromotableString, RecUnpromotableBool, RecUnpromotableBytes,
    RecUnpromotableEnum, RecUnpromotableInt, RecUnpromotableLiteralString, RecUnpromotableString,
    UnpromotableBool, UnpromotableBytes, UnpromotableEnum, UnpromotableInt,
    UnpromotableLiteralString, UnpromotableString,
};

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize)]
pub(crate) enum LiteralValueTypeKind<'db> {
    /// An integer literal
    Int(IntLiteralType),
    /// A boolean literal, either `True` or `False`.
    Bool(bool),
    /// A string literal whose value is known
    String(StringLiteralType<'db>),
    /// A singleton type that represents a specific enum member
    Enum(EnumLiteralType<'db>),
    /// A string known to originate only from literal values, but whose value is not known,
    /// unlike `String` above.
    LiteralString,
    /// A bytes literal
    Bytes(BytesLiteralType<'db>),
}

impl<'db> LiteralValueType<'db> {
    pub(crate) fn new(
        kind: impl Into<LiteralValueTypeKind<'db>>,
        is_promotable: bool,
    ) -> LiteralValueType<'db> {
        if is_promotable {
            Self::promotable(kind.into())
        } else {
            Self::unpromotable(kind.into())
        }
    }

    pub(crate) fn with_recursively_defined(self, value: RecursivelyDefined) -> Self {
        Self(match value {
            RecursivelyDefined::Yes => match self.0 {
                PromotableInt(v) => RecPromotableInt(v),
                PromotableBool(v) => RecPromotableBool(v),
                PromotableString(v) => RecPromotableString(v),
                PromotableEnum(v) => RecPromotableEnum(v),
                PromotableBytes(v) => RecPromotableBytes(v),
                PromotableLiteralString => RecPromotableLiteralString,
                UnpromotableInt(v) => RecUnpromotableInt(v),
                UnpromotableBool(v) => RecUnpromotableBool(v),
                UnpromotableString(v) => RecUnpromotableString(v),
                UnpromotableEnum(v) => RecUnpromotableEnum(v),
                UnpromotableBytes(v) => RecUnpromotableBytes(v),
                UnpromotableLiteralString => RecUnpromotableLiteralString,
                already_rec => already_rec,
            },
            RecursivelyDefined::No => match self.0 {
                RecPromotableInt(v) => PromotableInt(v),
                RecPromotableBool(v) => PromotableBool(v),
                RecPromotableString(v) => PromotableString(v),
                RecPromotableEnum(v) => PromotableEnum(v),
                RecPromotableBytes(v) => PromotableBytes(v),
                RecPromotableLiteralString => PromotableLiteralString,
                RecUnpromotableInt(v) => UnpromotableInt(v),
                RecUnpromotableBool(v) => UnpromotableBool(v),
                RecUnpromotableString(v) => UnpromotableString(v),
                RecUnpromotableEnum(v) => UnpromotableEnum(v),
                RecUnpromotableBytes(v) => UnpromotableBytes(v),
                RecUnpromotableLiteralString => UnpromotableLiteralString,
                already_normal => already_normal,
            },
        })
    }

    pub(crate) fn recursively_defined(self) -> RecursivelyDefined {
        match self.0 {
            RecPromotableInt(_)
            | RecPromotableBool(_)
            | RecPromotableString(_)
            | RecPromotableEnum(_)
            | RecPromotableBytes(_)
            | RecPromotableLiteralString
            | RecUnpromotableInt(_)
            | RecUnpromotableBool(_)
            | RecUnpromotableString(_)
            | RecUnpromotableEnum(_)
            | RecUnpromotableBytes(_)
            | RecUnpromotableLiteralString => RecursivelyDefined::Yes,
            _ => RecursivelyDefined::No,
        }
    }

    /// Creates a literal value that may be promoted.
    pub(crate) fn promotable(kind: impl Into<LiteralValueTypeKind<'db>>) -> LiteralValueType<'db> {
        let repr = match kind.into() {
            LiteralValueTypeKind::Int(int) => LiteralValueTypeInner::PromotableInt(int),
            LiteralValueTypeKind::Bool(bool) => LiteralValueTypeInner::PromotableBool(bool),
            LiteralValueTypeKind::String(string) => LiteralValueTypeInner::PromotableString(string),
            LiteralValueTypeKind::Enum(e) => LiteralValueTypeInner::PromotableEnum(e),
            LiteralValueTypeKind::Bytes(bytes) => LiteralValueTypeInner::PromotableBytes(bytes),
            LiteralValueTypeKind::LiteralString => LiteralValueTypeInner::PromotableLiteralString,
        };

        Self(repr)
    }

    /// Creates a literal value that should not be promoted.
    pub(crate) fn unpromotable(
        kind: impl Into<LiteralValueTypeKind<'db>>,
    ) -> LiteralValueType<'db> {
        let repr = match kind.into() {
            LiteralValueTypeKind::Int(int) => LiteralValueTypeInner::UnpromotableInt(int),
            LiteralValueTypeKind::Bool(bool) => LiteralValueTypeInner::UnpromotableBool(bool),
            LiteralValueTypeKind::String(string) => {
                LiteralValueTypeInner::UnpromotableString(string)
            }
            LiteralValueTypeKind::Enum(e) => LiteralValueTypeInner::UnpromotableEnum(e),
            LiteralValueTypeKind::Bytes(bytes) => LiteralValueTypeInner::UnpromotableBytes(bytes),
            LiteralValueTypeKind::LiteralString => LiteralValueTypeInner::UnpromotableLiteralString,
        };

        Self(repr)
    }

    /// Returns the unpromotable form of this literal value.
    #[must_use]
    pub(crate) fn to_unpromotable(self) -> Self {
        Self(match self.0 {
            PromotableInt(v) => UnpromotableInt(v),
            PromotableBool(v) => UnpromotableBool(v),
            PromotableString(v) => UnpromotableString(v),
            PromotableEnum(v) => UnpromotableEnum(v),
            PromotableBytes(v) => UnpromotableBytes(v),
            PromotableLiteralString => UnpromotableLiteralString,
            RecPromotableInt(v) => RecUnpromotableInt(v),
            RecPromotableBool(v) => RecUnpromotableBool(v),
            RecPromotableString(v) => RecUnpromotableString(v),
            RecPromotableEnum(v) => RecUnpromotableEnum(v),
            RecPromotableBytes(v) => RecUnpromotableBytes(v),
            RecPromotableLiteralString => RecUnpromotableLiteralString,
            already_unpromotable => already_unpromotable,
        })
    }

    /// Returns `true` if this literal value should be eagerly promoted to its instance type.
    pub(crate) fn is_promotable(self) -> bool {
        matches!(
            self.0,
            PromotableInt(_)
                | PromotableBool(_)
                | PromotableString(_)
                | PromotableEnum(_)
                | PromotableBytes(_)
                | PromotableLiteralString
                | RecPromotableInt(_)
                | RecPromotableBool(_)
                | RecPromotableString(_)
                | RecPromotableEnum(_)
                | RecPromotableBytes(_)
                | RecPromotableLiteralString
        )
    }

    pub(crate) fn kind(self) -> LiteralValueTypeKind<'db> {
        match self.0 {
            PromotableInt(v) | UnpromotableInt(v) | RecPromotableInt(v) | RecUnpromotableInt(v) => {
                LiteralValueTypeKind::Int(v)
            }
            PromotableBool(v)
            | UnpromotableBool(v)
            | RecPromotableBool(v)
            | RecUnpromotableBool(v) => LiteralValueTypeKind::Bool(v),
            PromotableString(v)
            | UnpromotableString(v)
            | RecPromotableString(v)
            | RecUnpromotableString(v) => LiteralValueTypeKind::String(v),
            PromotableEnum(v)
            | UnpromotableEnum(v)
            | RecPromotableEnum(v)
            | RecUnpromotableEnum(v) => LiteralValueTypeKind::Enum(v),
            PromotableBytes(v)
            | UnpromotableBytes(v)
            | RecPromotableBytes(v)
            | RecUnpromotableBytes(v) => LiteralValueTypeKind::Bytes(v),
            PromotableLiteralString
            | UnpromotableLiteralString
            | RecPromotableLiteralString
            | RecUnpromotableLiteralString => LiteralValueTypeKind::LiteralString,
        }
    }

    pub(crate) fn as_bytes(self) -> Option<BytesLiteralType<'db>> {
        if let LiteralValueTypeKind::Bytes(v) = self.kind() {
            Some(v)
        } else {
            None
        }
    }

    pub(crate) fn as_enum(self) -> Option<EnumLiteralType<'db>> {
        if let LiteralValueTypeKind::Enum(v) = self.kind() {
            Some(v)
        } else {
            None
        }
    }

    pub(crate) fn as_string(self) -> Option<StringLiteralType<'db>> {
        if let LiteralValueTypeKind::String(v) = self.kind() {
            Some(v)
        } else {
            None
        }
    }

    pub(crate) fn as_bool(self) -> Option<bool> {
        if let LiteralValueTypeKind::Bool(v) = self.kind() {
            Some(v)
        } else {
            None
        }
    }

    pub(crate) fn as_int(self) -> Option<i64> {
        if let LiteralValueTypeKind::Int(v) = self.kind() {
            Some(v.as_i64())
        } else {
            None
        }
    }

    pub(crate) fn is_int(self) -> bool {
        matches!(self.kind(), LiteralValueTypeKind::Int(..))
    }

    pub(crate) fn is_bool(self) -> bool {
        matches!(self.kind(), LiteralValueTypeKind::Bool(..))
    }

    pub(crate) fn is_literal_string(self) -> bool {
        matches!(self.kind(), LiteralValueTypeKind::LiteralString)
    }

    pub(crate) fn is_string(self) -> bool {
        matches!(self.kind(), LiteralValueTypeKind::String(..))
    }

    pub fn is_enum(self) -> bool {
        matches!(self.kind(), LiteralValueTypeKind::Enum(..))
    }

    pub(crate) fn is_bytes(self) -> bool {
        matches!(self.kind(), LiteralValueTypeKind::Bytes(..))
    }

    pub(crate) fn fallback_instance(self, db: &'db dyn Db) -> Type<'db> {
        match self.kind() {
            LiteralValueTypeKind::String(_) | LiteralValueTypeKind::LiteralString => {
                KnownClass::Str.to_instance(db)
            }
            LiteralValueTypeKind::Bool(_) => KnownClass::Bool.to_instance(db),
            LiteralValueTypeKind::Int(_) => KnownClass::Int.to_instance(db),
            LiteralValueTypeKind::Bytes(_) => KnownClass::Bytes.to_instance(db),
            LiteralValueTypeKind::Enum(literal) => literal.enum_class_instance(db),
        }
    }
}

impl From<i64> for LiteralValueTypeKind<'_> {
    fn from(v: i64) -> Self {
        Self::Int(IntLiteralType::from_i64(v))
    }
}

impl From<bool> for LiteralValueTypeKind<'_> {
    fn from(v: bool) -> Self {
        Self::Bool(v)
    }
}

impl<'db> From<StringLiteralType<'db>> for LiteralValueTypeKind<'db> {
    fn from(v: StringLiteralType<'db>) -> Self {
        Self::String(v)
    }
}

impl<'db> From<BytesLiteralType<'db>> for LiteralValueTypeKind<'db> {
    fn from(v: BytesLiteralType<'db>) -> Self {
        Self::Bytes(v)
    }
}

impl<'db> From<EnumLiteralType<'db>> for LiteralValueTypeKind<'db> {
    fn from(v: EnumLiteralType<'db>) -> Self {
        Self::Enum(v)
    }
}

impl<'db> From<LiteralValueType<'db>> for Type<'db> {
    fn from(value: LiteralValueType<'db>) -> Self {
        Type::LiteralValue(value)
    }
}

// This type has the same alignment as `salsa::Id`, allowing `LiteralValueType` to use a smaller
// discriminant.
#[derive(PartialOrd, Ord, Copy, Clone, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize)]
pub(crate) struct IntLiteralType {
    high: u32,
    low: u32,
}

impl IntLiteralType {
    pub(crate) fn as_i64(self) -> i64 {
        (i64::from(self.high) << 32) | i64::from(self.low)
    }

    #[expect(clippy::cast_possible_truncation)]
    pub(crate) fn from_i64(value: i64) -> Self {
        let value = value.cast_unsigned();

        Self {
            high: (value >> 32) as u32,
            low: value as u32,
        }
    }
}

impl std::fmt::Display for IntLiteralType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.as_i64(), f)
    }
}

impl std::fmt::Debug for IntLiteralType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.as_i64(), f)
    }
}

#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub struct StringLiteralType<'db> {
    #[returns(deref)]
    pub(crate) value: CompactString,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for StringLiteralType<'_> {}

impl<'db> StringLiteralType<'db> {
    /// The length of the string, as would be returned by Python's `len()`.
    pub(crate) fn python_len(self, db: &'db dyn Db) -> usize {
        self.value(db).chars().count()
    }
}

#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub struct BytesLiteralType<'db> {
    #[returns(deref)]
    pub(crate) value: Box<[u8]>,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for BytesLiteralType<'_> {}

impl<'db> BytesLiteralType<'db> {
    pub(crate) fn python_len(self, db: &'db dyn Db) -> usize {
        self.value(db).len()
    }
}

/// A singleton type corresponding to a specific enum member.
///
/// For the enum variant `Answer.YES` of the enum below, this type would store
/// a reference to `Answer` in `enum_class` and the name "YES" in `name`.
/// ```py
/// class Answer(Enum):
///     NO = 0
///     YES = 1
/// ```
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub struct EnumLiteralType<'db> {
    /// A reference to the enum class this literal belongs to
    pub(crate) enum_class: ClassLiteral<'db>,
    /// The name of the enum member
    #[returns(ref)]
    pub(crate) name: Name,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for EnumLiteralType<'_> {}

impl<'db> EnumLiteralType<'db> {
    pub(crate) fn enum_class_instance(self, db: &'db dyn Db) -> Type<'db> {
        self.enum_class(db).to_non_generic_instance(db)
    }
}
