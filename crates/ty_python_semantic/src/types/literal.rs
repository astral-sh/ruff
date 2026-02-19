use compact_str::CompactString;
use ruff_python_ast::name::Name;

use crate::Db;
use crate::types::{ClassLiteral, KnownClass, Type};

/// A literal value. See [`LiteralValueTypeKind`] for details.
#[derive(
    PartialOrd, Ord, Copy, Clone, Debug, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize,
)]
pub struct LiteralValueType<'db>(LiteralValueTypeInner<'db>);

// This enum effectively contains two variants, `Promotable(LiteralValueKind)` and `Unpromotable(LiteralValueKind)`,
// but flattened to reduce the size of the type.
#[derive(
    PartialOrd, Ord, Copy, Clone, Debug, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize,
)]
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
}

#[derive(
    PartialOrd, Ord, Copy, Clone, Debug, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize,
)]
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

    /// Creates a literal value that may be promoted during literal promotion.
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

    /// Creates a literal value that should not be promoted during literal promotion.
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
        let repr = match self.0 {
            LiteralValueTypeInner::PromotableInt(int) => {
                LiteralValueTypeInner::UnpromotableInt(int)
            }
            LiteralValueTypeInner::PromotableBool(bool) => {
                LiteralValueTypeInner::UnpromotableBool(bool)
            }
            LiteralValueTypeInner::PromotableString(string) => {
                LiteralValueTypeInner::UnpromotableString(string)
            }
            LiteralValueTypeInner::PromotableEnum(e) => LiteralValueTypeInner::UnpromotableEnum(e),
            LiteralValueTypeInner::PromotableBytes(bytes) => {
                LiteralValueTypeInner::UnpromotableBytes(bytes)
            }
            LiteralValueTypeInner::PromotableLiteralString => {
                LiteralValueTypeInner::UnpromotableLiteralString
            }
            LiteralValueTypeInner::UnpromotableInt(_)
            | LiteralValueTypeInner::UnpromotableBool(_)
            | LiteralValueTypeInner::UnpromotableString(_)
            | LiteralValueTypeInner::UnpromotableEnum(_)
            | LiteralValueTypeInner::UnpromotableBytes(_)
            | LiteralValueTypeInner::UnpromotableLiteralString => self.0,
        };

        Self(repr)
    }

    /// Returns `true` if this literal value should be eagerly promoted to its instance type.
    pub(crate) fn is_promotable(self) -> bool {
        match self.0 {
            LiteralValueTypeInner::PromotableInt(_)
            | LiteralValueTypeInner::PromotableBool(_)
            | LiteralValueTypeInner::PromotableString(_)
            | LiteralValueTypeInner::PromotableEnum(_)
            | LiteralValueTypeInner::PromotableBytes(_)
            | LiteralValueTypeInner::PromotableLiteralString => true,

            LiteralValueTypeInner::UnpromotableInt(_)
            | LiteralValueTypeInner::UnpromotableBool(_)
            | LiteralValueTypeInner::UnpromotableString(_)
            | LiteralValueTypeInner::UnpromotableEnum(_)
            | LiteralValueTypeInner::UnpromotableBytes(_)
            | LiteralValueTypeInner::UnpromotableLiteralString => false,
        }
    }

    pub(crate) fn kind(self) -> LiteralValueTypeKind<'db> {
        match self.0 {
            LiteralValueTypeInner::UnpromotableInt(int)
            | LiteralValueTypeInner::PromotableInt(int) => LiteralValueTypeKind::Int(int),
            LiteralValueTypeInner::UnpromotableBool(bool)
            | LiteralValueTypeInner::PromotableBool(bool) => LiteralValueTypeKind::Bool(bool),
            LiteralValueTypeInner::UnpromotableString(string)
            | LiteralValueTypeInner::PromotableString(string) => {
                LiteralValueTypeKind::String(string)
            }
            LiteralValueTypeInner::UnpromotableEnum(e)
            | LiteralValueTypeInner::PromotableEnum(e) => LiteralValueTypeKind::Enum(e),
            LiteralValueTypeInner::UnpromotableBytes(bytes)
            | LiteralValueTypeInner::PromotableBytes(bytes) => LiteralValueTypeKind::Bytes(bytes),
            LiteralValueTypeInner::UnpromotableLiteralString
            | LiteralValueTypeInner::PromotableLiteralString => LiteralValueTypeKind::LiteralString,
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

/// # Ordering
/// Ordering is based on the string literal's salsa-assigned id and not on its value.
/// The id may change between runs, or when the string literal was garbage collected and recreated.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
#[derive(PartialOrd, Ord)]
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

/// # Ordering
/// Ordering is based on the byte literal's salsa-assigned id and not on its value.
/// The id may change between runs, or when the byte literal was garbage collected and recreated.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
#[derive(PartialOrd, Ord)]
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
#[derive(PartialOrd, Ord)]
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
