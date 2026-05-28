use bitflags::bitflags;
use compact_str::CompactString;
use ruff_python_ast::name::Name;

use crate::Db;
use crate::types::set_theoretic::RecursivelyDefined;
use crate::types::{ClassLiteral, KnownClass, Type};
use ty_python_core::definition::Definition;
use ty_python_core::{place_table, use_def_map};

/// A literal value. See [`LiteralValueTypeKind`] for details.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize)]
pub struct LiteralValueType<'db>(LiteralValueTypeInner<'db>);

/// Each variant carries a [`LiteralFlags`] byte alongside its payload.
/// Because the flags byte fits into the padding between the enum discriminant
/// (1 byte) and the 4-byte-aligned payload, the overall size of this enum
/// stays at 12 bytes, the same as the original flagless representation.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize)]
enum LiteralValueTypeInner<'db> {
    Int(IntLiteralType, LiteralFlags),
    Bool(bool, LiteralFlags),
    String(StringLiteralType<'db>, LiteralFlags),
    Enum(EnumLiteralType<'db>, LiteralFlags),
    Bytes(BytesLiteralType<'db>, LiteralFlags),
    LiteralString(LiteralFlags),
}

bitflags! {
    /// Bit-packed flags for promotability and recursive-definition status.
    ///
    /// Stored in each [`LiteralValueTypeInner`] variant, fitting into the
    /// discriminant's padding so that the enum size is unchanged.
    #[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, salsa::Update)]
    struct LiteralFlags: u8 {
        const PROMOTABLE = 1 << 0;
        const RECURSIVELY_DEFINED = 1 << 1;
    }
}

impl get_size2::GetSize for LiteralFlags {}

impl LiteralFlags {
    fn new(promotable: bool, recursively_defined: RecursivelyDefined) -> Self {
        let mut flags = Self::empty();
        flags.set(Self::PROMOTABLE, promotable);
        flags.set(Self::RECURSIVELY_DEFINED, recursively_defined.is_yes());
        flags
    }

    const fn is_promotable(self) -> bool {
        self.intersects(Self::PROMOTABLE)
    }

    const fn recursively_defined(self) -> RecursivelyDefined {
        if self.intersects(Self::RECURSIVELY_DEFINED) {
            RecursivelyDefined::Yes
        } else {
            RecursivelyDefined::No
        }
    }

    fn with_promotable(mut self, promotable: bool) -> Self {
        self.set(Self::PROMOTABLE, promotable);
        self
    }

    fn with_recursively_defined(mut self, value: RecursivelyDefined) -> Self {
        self.set(Self::RECURSIVELY_DEFINED, value.is_yes());
        self
    }
}

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

    fn flags(self) -> LiteralFlags {
        match self.0 {
            LiteralValueTypeInner::Int(_, f)
            | LiteralValueTypeInner::Bool(_, f)
            | LiteralValueTypeInner::String(_, f)
            | LiteralValueTypeInner::Enum(_, f)
            | LiteralValueTypeInner::Bytes(_, f)
            | LiteralValueTypeInner::LiteralString(f) => f,
        }
    }

    fn map_flags(self, func: impl FnOnce(LiteralFlags) -> LiteralFlags) -> Self {
        Self(match self.0 {
            LiteralValueTypeInner::Int(v, f) => LiteralValueTypeInner::Int(v, func(f)),
            LiteralValueTypeInner::Bool(v, f) => LiteralValueTypeInner::Bool(v, func(f)),
            LiteralValueTypeInner::String(v, f) => LiteralValueTypeInner::String(v, func(f)),
            LiteralValueTypeInner::Enum(v, f) => LiteralValueTypeInner::Enum(v, func(f)),
            LiteralValueTypeInner::Bytes(v, f) => LiteralValueTypeInner::Bytes(v, func(f)),
            LiteralValueTypeInner::LiteralString(f) => {
                LiteralValueTypeInner::LiteralString(func(f))
            }
        })
    }

    pub(crate) fn with_recursively_defined(self, value: RecursivelyDefined) -> Self {
        self.map_flags(|f| f.with_recursively_defined(value))
    }

    pub(crate) fn recursively_defined(self) -> RecursivelyDefined {
        self.flags().recursively_defined()
    }

    /// Creates a literal value that may be promoted.
    pub(crate) fn promotable(kind: impl Into<LiteralValueTypeKind<'db>>) -> LiteralValueType<'db> {
        let flags = LiteralFlags::new(true, RecursivelyDefined::No);
        Self(match kind.into() {
            LiteralValueTypeKind::Int(v) => LiteralValueTypeInner::Int(v, flags),
            LiteralValueTypeKind::Bool(v) => LiteralValueTypeInner::Bool(v, flags),
            LiteralValueTypeKind::String(v) => LiteralValueTypeInner::String(v, flags),
            LiteralValueTypeKind::Enum(v) => LiteralValueTypeInner::Enum(v, flags),
            LiteralValueTypeKind::Bytes(v) => LiteralValueTypeInner::Bytes(v, flags),
            LiteralValueTypeKind::LiteralString => LiteralValueTypeInner::LiteralString(flags),
        })
    }

    /// Creates a literal value that should not be promoted.
    pub(crate) fn unpromotable(
        kind: impl Into<LiteralValueTypeKind<'db>>,
    ) -> LiteralValueType<'db> {
        let flags = LiteralFlags::new(false, RecursivelyDefined::No);
        Self(match kind.into() {
            LiteralValueTypeKind::Int(v) => LiteralValueTypeInner::Int(v, flags),
            LiteralValueTypeKind::Bool(v) => LiteralValueTypeInner::Bool(v, flags),
            LiteralValueTypeKind::String(v) => LiteralValueTypeInner::String(v, flags),
            LiteralValueTypeKind::Enum(v) => LiteralValueTypeInner::Enum(v, flags),
            LiteralValueTypeKind::Bytes(v) => LiteralValueTypeInner::Bytes(v, flags),
            LiteralValueTypeKind::LiteralString => LiteralValueTypeInner::LiteralString(flags),
        })
    }

    /// Returns the unpromotable form of this literal value.
    #[must_use]
    pub(crate) fn to_unpromotable(self) -> Self {
        self.map_flags(|f| f.with_promotable(false))
    }

    /// Returns `true` if this literal value should be eagerly promoted to its instance type.
    pub(crate) fn is_promotable(self) -> bool {
        self.flags().is_promotable()
    }

    pub(crate) fn kind(self) -> LiteralValueTypeKind<'db> {
        match self.0 {
            LiteralValueTypeInner::Int(v, _) => LiteralValueTypeKind::Int(v),
            LiteralValueTypeInner::Bool(v, _) => LiteralValueTypeKind::Bool(v),
            LiteralValueTypeInner::String(v, _) => LiteralValueTypeKind::String(v),
            LiteralValueTypeInner::Enum(v, _) => LiteralValueTypeKind::Enum(v),
            LiteralValueTypeInner::Bytes(v, _) => LiteralValueTypeKind::Bytes(v),
            LiteralValueTypeInner::LiteralString(_) => LiteralValueTypeKind::LiteralString,
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
#[derive(Copy, Clone, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize)]
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

impl std::cmp::Ord for IntLiteralType {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.as_i64().cmp(&other.as_i64())
    }
}

impl std::cmp::PartialOrd for IntLiteralType {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
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

    pub(crate) fn definition(self, db: &'db dyn Db) -> Option<Definition<'db>> {
        let ClassLiteral::Static(class) = self.enum_class(db) else {
            return None;
        };

        let scope = class.body_scope(db);
        let symbol_id = place_table(db, scope).symbol_id(self.name(db))?;

        use_def_map(db, scope)
            .end_of_scope_symbol_bindings(symbol_id)
            .find_map(|binding| binding.binding.definition())
    }
}
