//! An enumeration of special forms in the Python type system.
//! Each of these is considered to inhabit a unique type in our model of the type system.

use super::{ClassType, Type, class::KnownClass};
use crate::db::Db;
use crate::module_resolver::{KnownModule, file_to_module};
use crate::semantic_index::{definition::Definition, scope::ScopeId, semantic_index};
use crate::types::{
    DynamicType, IntersectionBuilder, InvalidTypeExpression, InvalidTypeExpressionError,
    generics::typing_self, infer::nearest_enclosing_class,
};
use ruff_db::files::File;
use std::str::FromStr;

/// Enumeration of specific runtime symbols that are special enough
/// that they can each be considered to inhabit a unique type.
///
/// # Ordering
///
/// Ordering is stable and should be the same between runs.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    salsa::Update,
    PartialOrd,
    Ord,
    strum_macros::EnumString,
    get_size2::GetSize,
)]
pub enum SpecialFormType {
    Any,
    /// The symbol `typing.Annotated` (which can also be found as `typing_extensions.Annotated`)
    Annotated,
    /// The symbol `typing.Literal` (which can also be found as `typing_extensions.Literal`)
    Literal,
    /// The symbol `typing.LiteralString` (which can also be found as `typing_extensions.LiteralString`)
    LiteralString,
    /// The symbol `typing.Optional` (which can also be found as `typing_extensions.Optional`)
    Optional,
    /// The symbol `typing.Union` (which can also be found as `typing_extensions.Union`)
    Union,
    /// The symbol `typing.NoReturn` (which can also be found as `typing_extensions.NoReturn`)
    NoReturn,
    /// The symbol `typing.Never` available since 3.11 (which can also be found as `typing_extensions.Never`)
    Never,
    /// The symbol `typing.Tuple` (which can also be found as `typing_extensions.Tuple`)
    Tuple,
    /// The symbol `typing.List` (which can also be found as `typing_extensions.List`)
    List,
    /// The symbol `typing.Dict` (which can also be found as `typing_extensions.Dict`)
    Dict,
    /// The symbol `typing.Set` (which can also be found as `typing_extensions.Set`)
    Set,
    /// The symbol `typing.FrozenSet` (which can also be found as `typing_extensions.FrozenSet`)
    FrozenSet,
    /// The symbol `typing.ChainMap` (which can also be found as `typing_extensions.ChainMap`)
    ChainMap,
    /// The symbol `typing.Counter` (which can also be found as `typing_extensions.Counter`)
    Counter,
    /// The symbol `typing.DefaultDict` (which can also be found as `typing_extensions.DefaultDict`)
    DefaultDict,
    /// The symbol `typing.Deque` (which can also be found as `typing_extensions.Deque`)
    Deque,
    /// The symbol `typing.OrderedDict` (which can also be found as `typing_extensions.OrderedDict`)
    OrderedDict,
    /// The symbol `typing.Type` (which can also be found as `typing_extensions.Type`)
    Type,
    /// The symbol `ty_extensions.Unknown`
    Unknown,
    /// The symbol `ty_extensions.AlwaysTruthy`
    AlwaysTruthy,
    /// The symbol `ty_extensions.AlwaysFalsy`
    AlwaysFalsy,
    /// The symbol `ty_extensions.Not`
    Not,
    /// The symbol `ty_extensions.Intersection`
    Intersection,
    /// The symbol `ty_extensions.TypeOf`
    TypeOf,
    /// The symbol `ty_extensions.CallableTypeOf`
    CallableTypeOf,
    /// The symbol `ty_extensions.Top`
    Top,
    /// The symbol `ty_extensions.Bottom`
    Bottom,
    /// The symbol `typing.Callable`
    /// (which can also be found as `typing_extensions.Callable` or as `collections.abc.Callable`)
    Callable,
    /// The symbol `typing.Self` (which can also be found as `typing_extensions.Self`)
    #[strum(serialize = "Self")]
    TypingSelf,
    /// The symbol `typing.Final` (which can also be found as `typing_extensions.Final`)
    Final,
    /// The symbol `typing.ClassVar` (which can also be found as `typing_extensions.ClassVar`)
    ClassVar,
    /// The symbol `typing.Concatenate` (which can also be found as `typing_extensions.Concatenate`)
    Concatenate,
    /// The symbol `typing.Unpack` (which can also be found as `typing_extensions.Unpack`)
    Unpack,
    /// The symbol `typing.Required` (which can also be found as `typing_extensions.Required`)
    Required,
    /// The symbol `typing.NotRequired` (which can also be found as `typing_extensions.NotRequired`)
    NotRequired,
    /// The symbol `typing.TypeAlias` (which can also be found as `typing_extensions.TypeAlias`)
    TypeAlias,
    /// The symbol `typing.TypeGuard` (which can also be found as `typing_extensions.TypeGuard`)
    TypeGuard,
    /// The symbol `typing.TypedDict` (which can also be found as `typing_extensions.TypedDict`)
    TypedDict,
    /// The symbol `typing.TypeIs` (which can also be found as `typing_extensions.TypeIs`)
    TypeIs,
    /// The symbol `typing.ReadOnly` (which can also be found as `typing_extensions.ReadOnly`)
    ReadOnly,

    /// The symbol `typing.Protocol` (which can also be found as `typing_extensions.Protocol`)
    ///
    /// Note that instances of subscripted `typing.Protocol` are not represented by this type;
    /// see also [`super::KnownInstanceType::SubscriptedProtocol`].
    Protocol,

    /// The symbol `typing.Generic` (which can also be found as `typing_extensions.Generic`).
    ///
    /// Note that instances of subscripted `typing.Generic` are not represented by this type;
    /// see also [`super::KnownInstanceType::SubscriptedGeneric`].
    Generic,

    /// The symbol `typing.NamedTuple` (which can also be found as `typing_extensions.NamedTuple`).
    /// Typeshed defines this symbol as a class, but this isn't accurate: it's actually a factory function
    /// at runtime. We therefore represent it as a special form internally.
    NamedTuple,
}

impl SpecialFormType {
    /// Return the [`KnownClass`] which this symbol is an instance of
    pub(crate) const fn class(self) -> KnownClass {
        match self {
            Self::Annotated
            | Self::Literal
            | Self::LiteralString
            | Self::Optional
            | Self::Union
            | Self::NoReturn
            | Self::Never
            | Self::Tuple
            | Self::Type
            | Self::TypingSelf
            | Self::Final
            | Self::ClassVar
            | Self::Callable
            | Self::Concatenate
            | Self::Unpack
            | Self::Required
            | Self::NotRequired
            | Self::TypeAlias
            | Self::TypeGuard
            | Self::TypedDict
            | Self::TypeIs
            | Self::TypeOf
            | Self::Not
            | Self::Top
            | Self::Bottom
            | Self::Intersection
            | Self::CallableTypeOf
            | Self::ReadOnly => KnownClass::SpecialForm,

            // Typeshed says it's an instance of `_SpecialForm`,
            // but then we wouldn't recognise things like `issubclass(`X, Protocol)`
            // as being valid.
            Self::Protocol => KnownClass::ProtocolMeta,

            Self::Generic | Self::Any => KnownClass::Type,

            Self::List
            | Self::Dict
            | Self::DefaultDict
            | Self::Set
            | Self::FrozenSet
            | Self::Counter
            | Self::Deque
            | Self::ChainMap
            | Self::OrderedDict => KnownClass::StdlibAlias,

            Self::Unknown | Self::AlwaysTruthy | Self::AlwaysFalsy => KnownClass::Object,

            Self::NamedTuple => KnownClass::FunctionType,
        }
    }

    /// Return the instance type which this type is a subtype of.
    ///
    /// For example, the symbol `typing.Literal` is an instance of `typing._SpecialForm`,
    /// so `SpecialFormType::Literal.instance_fallback(db)`
    /// returns `Type::NominalInstance(NominalInstanceType { class: <typing._SpecialForm> })`.
    pub(super) fn instance_fallback(self, db: &dyn Db) -> Type<'_> {
        self.class().to_instance(db)
    }

    /// Return `true` if this symbol is an instance of `class`.
    pub(super) fn is_instance_of(self, db: &dyn Db, class: ClassType) -> bool {
        self.class().is_subclass_of(db, class)
    }

    pub(super) fn try_from_file_and_name(
        db: &dyn Db,
        file: File,
        symbol_name: &str,
    ) -> Option<Self> {
        let candidate = Self::from_str(symbol_name).ok()?;
        candidate
            .check_module(file_to_module(db, file)?.known(db)?)
            .then_some(candidate)
    }

    /// Return `true` if `module` is a module from which this `SpecialFormType` variant can validly originate.
    ///
    /// Most variants can only exist in one module, which is the same as `self.class().canonical_module(db)`.
    /// Some variants could validly be defined in either `typing` or `typing_extensions`, however.
    pub(super) fn check_module(self, module: KnownModule) -> bool {
        match self {
            Self::ClassVar
            | Self::Deque
            | Self::List
            | Self::Dict
            | Self::DefaultDict
            | Self::Set
            | Self::FrozenSet
            | Self::Counter
            | Self::ChainMap
            | Self::OrderedDict
            | Self::Optional
            | Self::Union
            | Self::NoReturn
            | Self::Tuple
            | Self::Type
            | Self::Generic
            | Self::Callable => module.is_typing(),

            Self::Annotated
            | Self::Literal
            | Self::LiteralString
            | Self::Never
            | Self::Final
            | Self::Concatenate
            | Self::Unpack
            | Self::Required
            | Self::NotRequired
            | Self::TypeAlias
            | Self::TypeGuard
            | Self::TypedDict
            | Self::TypeIs
            | Self::TypingSelf
            | Self::Protocol
            | Self::NamedTuple
            | Self::Any
            | Self::ReadOnly => {
                matches!(module, KnownModule::Typing | KnownModule::TypingExtensions)
            }

            Self::Unknown
            | Self::AlwaysTruthy
            | Self::AlwaysFalsy
            | Self::Not
            | Self::Top
            | Self::Bottom
            | Self::Intersection
            | Self::TypeOf
            | Self::CallableTypeOf => module.is_ty_extensions(),
        }
    }

    pub(super) fn to_meta_type(self, db: &dyn Db) -> Type<'_> {
        self.class().to_class_literal(db)
    }

    /// Return true if this special form is callable at runtime.
    /// Most special forms are not callable (they are type constructors that are subscripted),
    /// but some like `TypedDict` and collection constructors can be called.
    pub(super) const fn is_callable(self) -> bool {
        match self {
            // TypedDict can be called as a constructor to create TypedDict types
            Self::TypedDict
            // Collection constructors are callable
            // TODO actually implement support for calling them
            | Self::ChainMap
            | Self::Counter
            | Self::DefaultDict
            | Self::Deque
            | Self::NamedTuple
            | Self::OrderedDict => true,

            // All other special forms are not callable
            Self::Annotated
            | Self::Literal
            | Self::LiteralString
            | Self::Optional
            | Self::Union
            | Self::NoReturn
            | Self::Never
            | Self::Tuple
            | Self::List
            | Self::Dict
            | Self::Set
            | Self::FrozenSet
            | Self::Type
            | Self::Unknown
            | Self::AlwaysTruthy
            | Self::AlwaysFalsy
            | Self::Not
            | Self::Top
            | Self::Bottom
            | Self::Intersection
            | Self::TypeOf
            | Self::CallableTypeOf
            | Self::Callable
            | Self::TypingSelf
            | Self::Final
            | Self::ClassVar
            | Self::Concatenate
            | Self::Unpack
            | Self::Required
            | Self::NotRequired
            | Self::TypeAlias
            | Self::TypeGuard
            | Self::TypeIs
            | Self::ReadOnly
            | Self::Protocol
            | Self::Any
            | Self::Generic => false,
        }
    }

    /// Return the repr of the symbol at runtime
    pub(super) const fn repr(self) -> &'static str {
        match self {
            SpecialFormType::Any => "typing.Any",
            SpecialFormType::Annotated => "typing.Annotated",
            SpecialFormType::Literal => "typing.Literal",
            SpecialFormType::LiteralString => "typing.LiteralString",
            SpecialFormType::Optional => "typing.Optional",
            SpecialFormType::Union => "typing.Union",
            SpecialFormType::NoReturn => "typing.NoReturn",
            SpecialFormType::Never => "typing.Never",
            SpecialFormType::Tuple => "typing.Tuple",
            SpecialFormType::Type => "typing.Type",
            SpecialFormType::TypingSelf => "typing.Self",
            SpecialFormType::Final => "typing.Final",
            SpecialFormType::ClassVar => "typing.ClassVar",
            SpecialFormType::Callable => "typing.Callable",
            SpecialFormType::Concatenate => "typing.Concatenate",
            SpecialFormType::Unpack => "typing.Unpack",
            SpecialFormType::Required => "typing.Required",
            SpecialFormType::NotRequired => "typing.NotRequired",
            SpecialFormType::TypeAlias => "typing.TypeAlias",
            SpecialFormType::TypeGuard => "typing.TypeGuard",
            SpecialFormType::TypedDict => "typing.TypedDict",
            SpecialFormType::TypeIs => "typing.TypeIs",
            SpecialFormType::List => "typing.List",
            SpecialFormType::Dict => "typing.Dict",
            SpecialFormType::DefaultDict => "typing.DefaultDict",
            SpecialFormType::Set => "typing.Set",
            SpecialFormType::FrozenSet => "typing.FrozenSet",
            SpecialFormType::Counter => "typing.Counter",
            SpecialFormType::Deque => "typing.Deque",
            SpecialFormType::ChainMap => "typing.ChainMap",
            SpecialFormType::OrderedDict => "typing.OrderedDict",
            SpecialFormType::ReadOnly => "typing.ReadOnly",
            SpecialFormType::Unknown => "ty_extensions.Unknown",
            SpecialFormType::AlwaysTruthy => "ty_extensions.AlwaysTruthy",
            SpecialFormType::AlwaysFalsy => "ty_extensions.AlwaysFalsy",
            SpecialFormType::Not => "ty_extensions.Not",
            SpecialFormType::Intersection => "ty_extensions.Intersection",
            SpecialFormType::TypeOf => "ty_extensions.TypeOf",
            SpecialFormType::CallableTypeOf => "ty_extensions.CallableTypeOf",
            SpecialFormType::Top => "ty_extensions.Top",
            SpecialFormType::Bottom => "ty_extensions.Bottom",
            SpecialFormType::Protocol => "typing.Protocol",
            SpecialFormType::Generic => "typing.Generic",
            SpecialFormType::NamedTuple => "typing.NamedTuple",
        }
    }

    pub(super) const fn kind(self) -> SpecialFormCategory {
        match self {
            // See the `SpecialFormCategory` doc-comment for why these three are
            // treated as their own category.
            Self::Callable => SpecialFormCategory::Callable,
            Self::Tuple => SpecialFormCategory::Tuple,
            Self::Type => SpecialFormCategory::Type,

            // Legacy standard library aliases
            Self::List => SpecialFormCategory::LegacyStdlibAlias(LegacyStdlibAlias::List),
            Self::Dict => SpecialFormCategory::LegacyStdlibAlias(LegacyStdlibAlias::Dict),
            Self::Set => SpecialFormCategory::LegacyStdlibAlias(LegacyStdlibAlias::Set),
            Self::FrozenSet => SpecialFormCategory::LegacyStdlibAlias(LegacyStdlibAlias::FrozenSet),
            Self::ChainMap => SpecialFormCategory::LegacyStdlibAlias(LegacyStdlibAlias::ChainMap),
            Self::Counter => SpecialFormCategory::LegacyStdlibAlias(LegacyStdlibAlias::Counter),
            Self::Deque => SpecialFormCategory::LegacyStdlibAlias(LegacyStdlibAlias::Deque),
            Self::DefaultDict => {
                SpecialFormCategory::LegacyStdlibAlias(LegacyStdlibAlias::DefaultDict)
            }
            Self::OrderedDict => {
                SpecialFormCategory::LegacyStdlibAlias(LegacyStdlibAlias::OrderedDict)
            }

            // Non-standard-library aliases
            Self::AlwaysFalsy => SpecialFormCategory::NonStdlibAlias(NonStdlibAlias::AlwaysFalsy),
            Self::Unknown => SpecialFormCategory::NonStdlibAlias(NonStdlibAlias::Unknown),
            Self::Not => SpecialFormCategory::NonStdlibAlias(NonStdlibAlias::Not),
            Self::TypeOf => SpecialFormCategory::NonStdlibAlias(NonStdlibAlias::TypeOf),
            Self::Top => SpecialFormCategory::NonStdlibAlias(NonStdlibAlias::Top),
            Self::Bottom => SpecialFormCategory::NonStdlibAlias(NonStdlibAlias::Bottom),
            Self::Annotated => SpecialFormCategory::NonStdlibAlias(NonStdlibAlias::Annotated),
            Self::Any => SpecialFormCategory::NonStdlibAlias(NonStdlibAlias::Any),
            Self::Literal => SpecialFormCategory::NonStdlibAlias(NonStdlibAlias::Literal),
            Self::Optional => SpecialFormCategory::NonStdlibAlias(NonStdlibAlias::Optional),
            Self::Union => SpecialFormCategory::NonStdlibAlias(NonStdlibAlias::Union),
            Self::NoReturn => SpecialFormCategory::NonStdlibAlias(NonStdlibAlias::NoReturn),
            Self::Never => SpecialFormCategory::NonStdlibAlias(NonStdlibAlias::Never),
            Self::Final => SpecialFormCategory::NonStdlibAlias(NonStdlibAlias::Final),
            Self::ClassVar => SpecialFormCategory::NonStdlibAlias(NonStdlibAlias::ClassVar),
            Self::Concatenate => SpecialFormCategory::NonStdlibAlias(NonStdlibAlias::Concatenate),
            Self::Unpack => SpecialFormCategory::NonStdlibAlias(NonStdlibAlias::Unpack),
            Self::Required => SpecialFormCategory::NonStdlibAlias(NonStdlibAlias::Required),
            Self::NotRequired => SpecialFormCategory::NonStdlibAlias(NonStdlibAlias::NotRequired),
            Self::TypeAlias => SpecialFormCategory::NonStdlibAlias(NonStdlibAlias::TypeAlias),
            Self::TypeGuard => SpecialFormCategory::NonStdlibAlias(NonStdlibAlias::TypeGuard),
            Self::TypedDict => SpecialFormCategory::NonStdlibAlias(NonStdlibAlias::TypedDict),
            Self::TypeIs => SpecialFormCategory::NonStdlibAlias(NonStdlibAlias::TypeIs),
            Self::ReadOnly => SpecialFormCategory::NonStdlibAlias(NonStdlibAlias::ReadOnly),
            Self::Protocol => SpecialFormCategory::NonStdlibAlias(NonStdlibAlias::Protocol),
            Self::Generic => SpecialFormCategory::NonStdlibAlias(NonStdlibAlias::Generic),
            Self::NamedTuple => SpecialFormCategory::NonStdlibAlias(NonStdlibAlias::NamedTuple),
            Self::AlwaysTruthy => SpecialFormCategory::NonStdlibAlias(NonStdlibAlias::AlwaysTruthy),
            Self::Intersection => SpecialFormCategory::NonStdlibAlias(NonStdlibAlias::Intersection),
            Self::TypingSelf => SpecialFormCategory::NonStdlibAlias(NonStdlibAlias::TypingSelf),
            Self::LiteralString => {
                SpecialFormCategory::NonStdlibAlias(NonStdlibAlias::LiteralString)
            }
            Self::CallableTypeOf => {
                SpecialFormCategory::NonStdlibAlias(NonStdlibAlias::CallableTypeOf)
            }
        }
    }
}

impl std::fmt::Display for SpecialFormType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.repr())
    }
}

/// Various categories of special forms.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(super) enum SpecialFormCategory {
    /// Special forms that are simple aliases to classes elsewhere in the standard library.
    LegacyStdlibAlias(LegacyStdlibAlias),

    /// Special forms that are not aliases to classes elsewhere in the standard library.
    NonStdlibAlias(NonStdlibAlias),

    /// The special form `typing.Tuple`.
    ///
    /// While this is technically an alias to `builtins.tuple`, it requires special handling
    /// for type-expression parsing.
    Tuple,

    /// The special form `typing.Type`.
    ///
    /// While this is technically an alias to `builtins.type`, it requires special handling
    /// for type-expression parsing.
    Type,

    /// The special form `Callable`.
    ///
    /// While `typing.Callable` aliases `collections.abc.Callable`, we view both objects
    /// as inhabiting the same special form type internally. Moreover, `Callable` requires
    /// special handling for both type-expression parsing and `isinstance`/`issubclass`
    /// narrowing.
    Callable,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(super) enum LegacyStdlibAlias {
    List,
    Dict,
    Set,
    FrozenSet,
    ChainMap,
    Counter,
    DefaultDict,
    Deque,
    OrderedDict,
}

impl LegacyStdlibAlias {
    pub(super) const fn alias_spec(self) -> AliasSpec {
        let (class, expected_argument_number) = match self {
            LegacyStdlibAlias::List => (KnownClass::List, 1),
            LegacyStdlibAlias::Dict => (KnownClass::Dict, 2),
            LegacyStdlibAlias::Set => (KnownClass::Set, 1),
            LegacyStdlibAlias::FrozenSet => (KnownClass::FrozenSet, 1),
            LegacyStdlibAlias::ChainMap => (KnownClass::ChainMap, 2),
            LegacyStdlibAlias::Counter => (KnownClass::Counter, 1),
            LegacyStdlibAlias::DefaultDict => (KnownClass::DefaultDict, 2),
            LegacyStdlibAlias::Deque => (KnownClass::Deque, 1),
            LegacyStdlibAlias::OrderedDict => (KnownClass::OrderedDict, 2),
        };

        AliasSpec {
            class,
            expected_argument_number,
        }
    }

    pub(super) const fn aliased_class(self) -> KnownClass {
        self.alias_spec().class
    }
}

impl From<LegacyStdlibAlias> for SpecialFormType {
    fn from(value: LegacyStdlibAlias) -> Self {
        match value {
            LegacyStdlibAlias::List => SpecialFormType::List,
            LegacyStdlibAlias::Dict => SpecialFormType::Dict,
            LegacyStdlibAlias::Set => SpecialFormType::Set,
            LegacyStdlibAlias::FrozenSet => SpecialFormType::FrozenSet,
            LegacyStdlibAlias::ChainMap => SpecialFormType::ChainMap,
            LegacyStdlibAlias::Counter => SpecialFormType::Counter,
            LegacyStdlibAlias::DefaultDict => SpecialFormType::DefaultDict,
            LegacyStdlibAlias::Deque => SpecialFormType::Deque,
            LegacyStdlibAlias::OrderedDict => SpecialFormType::OrderedDict,
        }
    }
}

impl std::fmt::Display for LegacyStdlibAlias {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        SpecialFormType::from(*self).fmt(f)
    }
}

/// Information regarding the [`KnownClass`] a [`LegacyStdlibAlias`] refers to.
pub(super) struct AliasSpec {
    pub(super) class: KnownClass,
    pub(super) expected_argument_number: usize,
}

/// Enumeration of special forms that are not aliases to classes or special constructs
/// elsewhere in the Python standard library.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(super) enum NonStdlibAlias {
    Any,
    Annotated,
    Literal,
    LiteralString,
    Optional,
    Union,
    NoReturn,
    Never,
    Unknown,
    AlwaysTruthy,
    AlwaysFalsy,
    Not,
    Intersection,
    TypeOf,
    CallableTypeOf,
    Top,
    Bottom,
    TypingSelf,
    Final,
    ClassVar,
    Concatenate,
    Unpack,
    Required,
    NotRequired,
    TypeAlias,
    TypeGuard,
    TypedDict,
    TypeIs,
    ReadOnly,
    Protocol,
    Generic,
    NamedTuple,
}

impl NonStdlibAlias {
    pub(super) fn in_type_expression<'db>(
        self,
        db: &'db dyn Db,
        scope_id: ScopeId<'db>,
        typevar_binding_context: Option<Definition<'db>>,
    ) -> Result<Type<'db>, InvalidTypeExpressionError<'db>> {
        match self {
            Self::Never | Self::NoReturn => Ok(Type::Never),
            Self::LiteralString => Ok(Type::LiteralString),
            Self::Any => Ok(Type::any()),
            Self::Unknown => Ok(Type::unknown()),
            Self::AlwaysTruthy => Ok(Type::AlwaysTruthy),
            Self::AlwaysFalsy => Ok(Type::AlwaysFalsy),

            // Special case: `NamedTuple` in a type expression is understood to describe the type
            // `tuple[object, ...] & <a protocol that any `NamedTuple` class would satisfy>`.
            // This isn't very principled (since at runtime, `NamedTuple` is just a function),
            // but it appears to be what users often expect, and it improves compatibility with
            // other type checkers such as mypy.
            // See conversation in https://github.com/astral-sh/ruff/pull/19915.
            Self::NamedTuple => Ok(IntersectionBuilder::new(db)
                .positive_elements([
                    Type::homogeneous_tuple(db, Type::object()),
                    KnownClass::NamedTupleLike.to_instance(db),
                ])
                .build()),

            Self::TypingSelf => {
                let index = semantic_index(db, scope_id.file(db));
                let Some(class) = nearest_enclosing_class(db, index, scope_id) else {
                    return Err(InvalidTypeExpressionError {
                        fallback_type: Type::unknown(),
                        invalid_expressions: smallvec::smallvec_inline![
                            InvalidTypeExpression::InvalidType(self.into(), scope_id)
                        ],
                    });
                };

                Ok(
                    typing_self(db, scope_id, typevar_binding_context, class)
                        .unwrap_or(self.into()),
                )
            }
            Self::TypeAlias => Ok(Type::Dynamic(DynamicType::TodoTypeAlias)),
            Self::TypedDict => Err(InvalidTypeExpressionError {
                invalid_expressions: smallvec::smallvec_inline![InvalidTypeExpression::TypedDict],
                fallback_type: Type::unknown(),
            }),

            Self::Literal | Self::Union | Self::Intersection => Err(InvalidTypeExpressionError {
                invalid_expressions: smallvec::smallvec_inline![
                    InvalidTypeExpression::RequiresArguments(self.into())
                ],
                fallback_type: Type::unknown(),
            }),

            Self::Protocol => Err(InvalidTypeExpressionError {
                invalid_expressions: smallvec::smallvec_inline![InvalidTypeExpression::Protocol],
                fallback_type: Type::unknown(),
            }),
            Self::Generic => Err(InvalidTypeExpressionError {
                invalid_expressions: smallvec::smallvec_inline![InvalidTypeExpression::Generic],
                fallback_type: Type::unknown(),
            }),

            Self::Optional
            | Self::Not
            | Self::Top
            | Self::Bottom
            | Self::TypeOf
            | Self::TypeIs
            | Self::TypeGuard
            | Self::Unpack
            | Self::CallableTypeOf => Err(InvalidTypeExpressionError {
                invalid_expressions: smallvec::smallvec_inline![
                    InvalidTypeExpression::RequiresOneArgument(self.into())
                ],
                fallback_type: Type::unknown(),
            }),

            Self::Annotated | Self::Concatenate => Err(InvalidTypeExpressionError {
                invalid_expressions: smallvec::smallvec_inline![
                    InvalidTypeExpression::RequiresTwoArguments(self.into())
                ],
                fallback_type: Type::unknown(),
            }),

            Self::ClassVar | Self::Final => Err(InvalidTypeExpressionError {
                invalid_expressions: smallvec::smallvec_inline![
                    InvalidTypeExpression::TypeQualifier(self)
                ],
                fallback_type: Type::unknown(),
            }),

            Self::ReadOnly | Self::NotRequired | Self::Required => {
                Err(InvalidTypeExpressionError {
                    invalid_expressions: smallvec::smallvec_inline![
                        InvalidTypeExpression::TypeQualifierRequiresOneArgument(self)
                    ],
                    fallback_type: Type::unknown(),
                })
            }
        }
    }
}

impl From<NonStdlibAlias> for SpecialFormType {
    fn from(value: NonStdlibAlias) -> Self {
        match value {
            NonStdlibAlias::Any => SpecialFormType::Any,
            NonStdlibAlias::Annotated => SpecialFormType::Annotated,
            NonStdlibAlias::Literal => SpecialFormType::Literal,
            NonStdlibAlias::LiteralString => SpecialFormType::LiteralString,
            NonStdlibAlias::Optional => SpecialFormType::Optional,
            NonStdlibAlias::Union => SpecialFormType::Union,
            NonStdlibAlias::NoReturn => SpecialFormType::NoReturn,
            NonStdlibAlias::Never => SpecialFormType::Never,
            NonStdlibAlias::Unknown => SpecialFormType::Unknown,
            NonStdlibAlias::AlwaysTruthy => SpecialFormType::AlwaysTruthy,
            NonStdlibAlias::AlwaysFalsy => SpecialFormType::AlwaysFalsy,
            NonStdlibAlias::Not => SpecialFormType::Not,
            NonStdlibAlias::Intersection => SpecialFormType::Intersection,
            NonStdlibAlias::TypeOf => SpecialFormType::TypeOf,
            NonStdlibAlias::CallableTypeOf => SpecialFormType::CallableTypeOf,
            NonStdlibAlias::Top => SpecialFormType::Top,
            NonStdlibAlias::Bottom => SpecialFormType::Bottom,
            NonStdlibAlias::TypingSelf => SpecialFormType::TypingSelf,
            NonStdlibAlias::Final => SpecialFormType::Final,
            NonStdlibAlias::ClassVar => SpecialFormType::ClassVar,
            NonStdlibAlias::Concatenate => SpecialFormType::Concatenate,
            NonStdlibAlias::Unpack => SpecialFormType::Unpack,
            NonStdlibAlias::Required => SpecialFormType::Required,
            NonStdlibAlias::NotRequired => SpecialFormType::NotRequired,
            NonStdlibAlias::TypeAlias => SpecialFormType::TypeAlias,
            NonStdlibAlias::TypeGuard => SpecialFormType::TypeGuard,
            NonStdlibAlias::TypedDict => SpecialFormType::TypedDict,
            NonStdlibAlias::TypeIs => SpecialFormType::TypeIs,
            NonStdlibAlias::ReadOnly => SpecialFormType::ReadOnly,
            NonStdlibAlias::Protocol => SpecialFormType::Protocol,
            NonStdlibAlias::Generic => SpecialFormType::Generic,
            NonStdlibAlias::NamedTuple => SpecialFormType::NamedTuple,
        }
    }
}

impl From<NonStdlibAlias> for Type<'_> {
    fn from(value: NonStdlibAlias) -> Self {
        Type::SpecialForm(SpecialFormType::from(value))
    }
}

impl std::fmt::Display for NonStdlibAlias {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        SpecialFormType::from(*self).fmt(f)
    }
}
