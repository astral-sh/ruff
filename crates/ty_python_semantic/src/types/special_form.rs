//! An enumeration of special forms in the Python type system.
//! Each of these is considered to inhabit a unique type in our model of the type system.

use super::{ClassType, Type, class::KnownClass};
use crate::db::Db;
use crate::module_resolver::{KnownModule, file_to_module};
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
)]
pub enum SpecialFormType {
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
            | Self::Intersection
            | Self::CallableTypeOf
            | Self::Protocol  // actually `_ProtocolMeta` at runtime but this is what typeshed says
            | Self::Generic  // actually `type` at runtime but this is what typeshed says
            | Self::ReadOnly => KnownClass::SpecialForm,

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
        }
    }

    /// Return the instance type which this type is a subtype of.
    ///
    /// For example, the symbol `typing.Literal` is an instance of `typing._SpecialForm`,
    /// so `SpecialFormType::Literal.instance_fallback(db)`
    /// returns `Type::NominalInstance(NominalInstanceType { class: <typing._SpecialForm> })`.
    pub(super) fn instance_fallback(self, db: &dyn Db) -> Type {
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
            .check_module(file_to_module(db, file)?.known()?)
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
            | Self::ReadOnly => {
                matches!(module, KnownModule::Typing | KnownModule::TypingExtensions)
            }

            Self::Unknown
            | Self::AlwaysTruthy
            | Self::AlwaysFalsy
            | Self::Not
            | Self::Intersection
            | Self::TypeOf
            | Self::CallableTypeOf => module.is_ty_extensions(),
        }
    }

    pub(super) fn to_meta_type(self, db: &dyn Db) -> Type {
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
            | Self::Generic => false,
        }
    }

    /// Return the repr of the symbol at runtime
    pub(super) const fn repr(self) -> &'static str {
        match self {
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
            SpecialFormType::Protocol => "typing.Protocol",
            SpecialFormType::Generic => "typing.Generic",
        }
    }
}

impl std::fmt::Display for SpecialFormType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.repr())
    }
}
