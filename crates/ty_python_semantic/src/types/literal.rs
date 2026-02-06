use compact_str::CompactString;
use ruff_python_ast::name::Name;

use crate::Db;
use crate::types::{ClassLiteral, NormalizedVisitor, Type};

/// A literal value. See [`LiteralValueTypeKind`] for details.
#[derive(
    PartialOrd, Ord, Copy, Clone, Debug, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize,
)]
pub struct LiteralValueType<'db>(LiteralValueTypeRepr<'db>);

/// Literal values are structured such that promotable values, i.e., the common case, are stored
/// inline, while unpromotable values require an extra allocation.
#[derive(
    PartialOrd, Ord, Copy, Clone, Debug, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize,
)]
enum LiteralValueTypeRepr<'db> {
    Promotable(LiteralValueTypeKind<'db>),
    Unpromotable(UnpromotableLiteralValueType<'db>),
}

#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
#[derive(PartialOrd, Ord)]
pub struct UnpromotableLiteralValueType<'db> {
    kind: LiteralValueTypeKind<'db>,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for UnpromotableLiteralValueType<'_> {}

#[derive(
    PartialOrd, Ord, Copy, Clone, Debug, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize,
)]
pub enum LiteralValueTypeKind<'db> {
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
        db: &'db dyn Db,
        kind: impl Into<LiteralValueTypeKind<'db>>,
        is_promotable: bool,
    ) -> LiteralValueType<'db> {
        if is_promotable {
            Self::promotable(db, kind.into())
        } else {
            Self::unpromotable(db, kind.into())
        }
    }

    /// Creates a literal value that may be promoted during literal promotion.
    pub(crate) fn promotable(
        _db: &'db dyn Db,
        kind: impl Into<LiteralValueTypeKind<'db>>,
    ) -> LiteralValueType<'db> {
        Self(LiteralValueTypeRepr::Promotable(kind.into()))
    }

    /// Creates a literal value that should not be promoted during literal promotion.
    pub(crate) fn unpromotable(
        db: &'db dyn Db,
        kind: impl Into<LiteralValueTypeKind<'db>>,
    ) -> LiteralValueType<'db> {
        Self(LiteralValueTypeRepr::Unpromotable(
            UnpromotableLiteralValueType::new(db, kind.into()),
        ))
    }

    /// Returns the promotable form of this literal value.
    #[must_use]
    pub(crate) fn to_promotable(self, db: &'db dyn Db) -> Self {
        match self.0 {
            LiteralValueTypeRepr::Promotable(_) => self,
            LiteralValueTypeRepr::Unpromotable(literal) => {
                Self(LiteralValueTypeRepr::Promotable(literal.kind(db)))
            }
        }
    }

    /// Returns the unpromotable form of this literal value.
    #[must_use]
    pub(crate) fn to_unpromotable(self, db: &'db dyn Db) -> Self {
        match self.0 {
            LiteralValueTypeRepr::Unpromotable(_) => self,
            LiteralValueTypeRepr::Promotable(kind) => Self(LiteralValueTypeRepr::Unpromotable(
                UnpromotableLiteralValueType::new(db, kind),
            )),
        }
    }

    /// Returns `true` if this literal value should be eagerly promoted to its instance type.
    pub(crate) fn is_promotable(self, _db: &'db dyn Db) -> bool {
        match self.0 {
            LiteralValueTypeRepr::Promotable(_) => true,
            LiteralValueTypeRepr::Unpromotable(_) => false,
        }
    }

    pub(crate) fn kind(self, db: &'db dyn Db) -> LiteralValueTypeKind<'db> {
        match self.0 {
            LiteralValueTypeRepr::Promotable(kind) => kind,
            LiteralValueTypeRepr::Unpromotable(literal) => literal.kind(db),
        }
    }

    pub(crate) fn as_bytes(self, db: &'db dyn Db) -> Option<BytesLiteralType<'db>> {
        if let LiteralValueTypeKind::Bytes(v) = self.kind(db) {
            Some(v)
        } else {
            None
        }
    }

    pub(crate) fn as_enum(self, db: &'db dyn Db) -> Option<EnumLiteralType<'db>> {
        if let LiteralValueTypeKind::Enum(v) = self.kind(db) {
            Some(v)
        } else {
            None
        }
    }

    pub(crate) fn as_string(self, db: &'db dyn Db) -> Option<StringLiteralType<'db>> {
        if let LiteralValueTypeKind::String(v) = self.kind(db) {
            Some(v)
        } else {
            None
        }
    }

    pub(crate) fn as_bool(self, db: &'db dyn Db) -> Option<bool> {
        if let LiteralValueTypeKind::Bool(v) = self.kind(db) {
            Some(v)
        } else {
            None
        }
    }

    pub(crate) fn as_int(self, db: &'db dyn Db) -> Option<i64> {
        if let LiteralValueTypeKind::Int(v) = self.kind(db) {
            Some(v.as_i64())
        } else {
            None
        }
    }

    pub(crate) fn is_int(self, db: &'db dyn Db) -> bool {
        matches!(self.kind(db), LiteralValueTypeKind::Int(..))
    }

    pub(crate) fn is_bool(self, db: &'db dyn Db) -> bool {
        matches!(self.kind(db), LiteralValueTypeKind::Bool(..))
    }

    pub(crate) fn is_literal_string(self, db: &'db dyn Db) -> bool {
        matches!(self.kind(db), LiteralValueTypeKind::LiteralString)
    }

    pub(crate) fn is_string(self, db: &'db dyn Db) -> bool {
        matches!(self.kind(db), LiteralValueTypeKind::String(..))
    }

    pub fn is_enum(self, db: &'db dyn Db) -> bool {
        matches!(self.kind(db), LiteralValueTypeKind::Enum(..))
    }

    pub(crate) fn is_bytes(self, db: &'db dyn Db) -> bool {
        matches!(self.kind(db), LiteralValueTypeKind::Bytes(..))
    }

    pub(crate) fn normalized_impl(
        self,
        db: &'db dyn Db,
        _visitor: &NormalizedVisitor<'db>,
    ) -> Self {
        self.to_promotable(db)
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
pub struct IntLiteralType {
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
