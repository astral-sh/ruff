//! The `KnownInstance` type.
//!
//! Despite its name, this is quite a different type from [`super::NominalInstanceType`].
//! For the vast majority of instance-types in Python, we cannot say how many possible
//! inhabitants there are or could be of that type at runtime. Each variant of the
//! [`KnownInstanceType`] enum, however, represents a specific runtime symbol
//! that requires heavy special-casing in the type system. Thus any one `KnownInstance`
//! variant can only be inhabited by one or two specific objects at runtime with
//! locations that are known in advance.

use super::{class::KnownClass, ClassType, Truthiness, Type, TypeAliasType, TypeVarInstance};
use crate::db::Db;
use crate::module_resolver::{file_to_module, KnownModule};
use ruff_db::files::File;

/// Enumeration of specific runtime symbols that are special enough
/// that they can each be considered to inhabit a unique type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, salsa::Update)]
pub enum KnownInstanceType<'db> {
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
    /// The symbol `typing.Any` (which can also be found as `typing_extensions.Any`)
    /// This is not used since typeshed switched to representing `Any` as a class; now we use
    /// `KnownClass::Any` instead. But we still support the old `Any = object()` representation, at
    /// least for now. TODO maybe remove?
    Any,
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
    /// The symbol `typing.Protocol` (which can also be found as `typing_extensions.Protocol`)
    Protocol,
    /// The symbol `typing.Generic` (which can also be found as `typing_extensions.Generic`)
    Generic,
    /// The symbol `typing.Type` (which can also be found as `typing_extensions.Type`)
    Type,
    /// A single instance of `typing.TypeVar`
    TypeVar(TypeVarInstance<'db>),
    /// A single instance of `typing.TypeAliasType` (PEP 695 type alias)
    TypeAliasType(TypeAliasType<'db>),
    /// The symbol `knot_extensions.Unknown`
    Unknown,
    /// The symbol `knot_extensions.AlwaysTruthy`
    AlwaysTruthy,
    /// The symbol `knot_extensions.AlwaysFalsy`
    AlwaysFalsy,
    /// The symbol `knot_extensions.Not`
    Not,
    /// The symbol `knot_extensions.Intersection`
    Intersection,
    /// The symbol `knot_extensions.TypeOf`
    TypeOf,
    /// The symbol `knot_extensions.CallableTypeOf`
    CallableTypeOf,
    /// The symbol `typing.Callable`
    /// (which can also be found as `typing_extensions.Callable` or as `collections.abc.Callable`)
    Callable,

    // Various special forms, special aliases and type qualifiers that we don't yet understand
    // (all currently inferred as TODO in most contexts):
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
    // TODO: fill this enum out with more special forms, etc.
}

impl<'db> KnownInstanceType<'db> {
    /// Evaluate the known instance in boolean context
    pub(crate) const fn bool(self) -> Truthiness {
        match self {
            Self::Annotated
            | Self::Literal
            | Self::LiteralString
            | Self::Optional
            // This is a legacy `TypeVar` _outside_ of any generic class or function, so it's
            // AlwaysTrue. The truthiness of a typevar inside of a generic class or function
            // depends on its bounds and constraints; but that's represented by `Type::TypeVar` and
            // handled in elsewhere.
            | Self::TypeVar(_)
            | Self::Union
            | Self::NoReturn
            | Self::Never
            | Self::Any
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
            | Self::List
            | Self::Dict
            | Self::DefaultDict
            | Self::Set
            | Self::FrozenSet
            | Self::Counter
            | Self::Deque
            | Self::ChainMap
            | Self::OrderedDict
            | Self::Protocol
            | Self::Generic
            | Self::ReadOnly
            | Self::TypeAliasType(_)
            | Self::Unknown
            | Self::AlwaysTruthy
            | Self::AlwaysFalsy
            | Self::Not
            | Self::Intersection
            | Self::TypeOf
            | Self::CallableTypeOf => Truthiness::AlwaysTrue,
        }
    }

    /// Return the repr of the symbol at runtime
    pub(crate) fn repr(self) -> &'db str {
        match self {
            Self::Annotated => "typing.Annotated",
            Self::Literal => "typing.Literal",
            Self::LiteralString => "typing.LiteralString",
            Self::Optional => "typing.Optional",
            Self::Union => "typing.Union",
            Self::NoReturn => "typing.NoReturn",
            Self::Never => "typing.Never",
            Self::Any => "typing.Any",
            Self::Tuple => "typing.Tuple",
            Self::Type => "typing.Type",
            Self::TypingSelf => "typing.Self",
            Self::Final => "typing.Final",
            Self::ClassVar => "typing.ClassVar",
            Self::Callable => "typing.Callable",
            Self::Concatenate => "typing.Concatenate",
            Self::Unpack => "typing.Unpack",
            Self::Required => "typing.Required",
            Self::NotRequired => "typing.NotRequired",
            Self::TypeAlias => "typing.TypeAlias",
            Self::TypeGuard => "typing.TypeGuard",
            Self::TypedDict => "typing.TypedDict",
            Self::TypeIs => "typing.TypeIs",
            Self::List => "typing.List",
            Self::Dict => "typing.Dict",
            Self::DefaultDict => "typing.DefaultDict",
            Self::Set => "typing.Set",
            Self::FrozenSet => "typing.FrozenSet",
            Self::Counter => "typing.Counter",
            Self::Deque => "typing.Deque",
            Self::ChainMap => "typing.ChainMap",
            Self::OrderedDict => "typing.OrderedDict",
            Self::Protocol => "typing.Protocol",
            Self::Generic => "typing.Generic",
            Self::ReadOnly => "typing.ReadOnly",
            // This is a legacy `TypeVar` _outside_ of any generic class or function, so we render
            // it as an instance of `typing.TypeVar`. Inside of a generic class or function, we'll
            // have a `Type::TypeVar(_)`, which is rendered as the typevar's name.
            Self::TypeVar(_) => "typing.TypeVar",
            Self::TypeAliasType(_) => "typing.TypeAliasType",
            Self::Unknown => "knot_extensions.Unknown",
            Self::AlwaysTruthy => "knot_extensions.AlwaysTruthy",
            Self::AlwaysFalsy => "knot_extensions.AlwaysFalsy",
            Self::Not => "knot_extensions.Not",
            Self::Intersection => "knot_extensions.Intersection",
            Self::TypeOf => "knot_extensions.TypeOf",
            Self::CallableTypeOf => "knot_extensions.CallableTypeOf",
        }
    }

    /// Return the [`KnownClass`] which this symbol is an instance of
    pub(crate) const fn class(self) -> KnownClass {
        match self {
            Self::Annotated => KnownClass::SpecialForm,
            Self::Literal => KnownClass::SpecialForm,
            Self::LiteralString => KnownClass::SpecialForm,
            Self::Optional => KnownClass::SpecialForm,
            Self::Union => KnownClass::SpecialForm,
            Self::NoReturn => KnownClass::SpecialForm,
            Self::Never => KnownClass::SpecialForm,
            Self::Any => KnownClass::Object,
            Self::Tuple => KnownClass::SpecialForm,
            Self::Type => KnownClass::SpecialForm,
            Self::TypingSelf => KnownClass::SpecialForm,
            Self::Final => KnownClass::SpecialForm,
            Self::ClassVar => KnownClass::SpecialForm,
            Self::Callable => KnownClass::SpecialForm,
            Self::Concatenate => KnownClass::SpecialForm,
            Self::Unpack => KnownClass::SpecialForm,
            Self::Required => KnownClass::SpecialForm,
            Self::NotRequired => KnownClass::SpecialForm,
            Self::TypeAlias => KnownClass::SpecialForm,
            Self::TypeGuard => KnownClass::SpecialForm,
            Self::TypedDict => KnownClass::SpecialForm,
            Self::TypeIs => KnownClass::SpecialForm,
            Self::ReadOnly => KnownClass::SpecialForm,
            Self::List => KnownClass::StdlibAlias,
            Self::Dict => KnownClass::StdlibAlias,
            Self::DefaultDict => KnownClass::StdlibAlias,
            Self::Set => KnownClass::StdlibAlias,
            Self::FrozenSet => KnownClass::StdlibAlias,
            Self::Counter => KnownClass::StdlibAlias,
            Self::Deque => KnownClass::StdlibAlias,
            Self::ChainMap => KnownClass::StdlibAlias,
            Self::OrderedDict => KnownClass::StdlibAlias,
            Self::Protocol => KnownClass::SpecialForm, // actually `_ProtocolMeta` at runtime but this is what typeshed says
            Self::Generic => KnownClass::SpecialForm, // actually `type` at runtime but this is what typeshed says
            Self::TypeVar(_) => KnownClass::TypeVar,
            Self::TypeAliasType(_) => KnownClass::TypeAliasType,
            Self::TypeOf => KnownClass::SpecialForm,
            Self::Not => KnownClass::SpecialForm,
            Self::Intersection => KnownClass::SpecialForm,
            Self::CallableTypeOf => KnownClass::SpecialForm,
            Self::Unknown => KnownClass::Object,
            Self::AlwaysTruthy => KnownClass::Object,
            Self::AlwaysFalsy => KnownClass::Object,
        }
    }

    /// Return the instance type which this type is a subtype of.
    ///
    /// For example, the symbol `typing.Literal` is an instance of `typing._SpecialForm`,
    /// so `KnownInstanceType::Literal.instance_fallback(db)`
    /// returns `Type::NominalInstance(NominalInstanceType { class: <typing._SpecialForm> })`.
    pub(super) fn instance_fallback(self, db: &dyn Db) -> Type {
        self.class().to_instance(db)
    }

    /// Return `true` if this symbol is an instance of `class`.
    pub(super) fn is_instance_of(self, db: &'db dyn Db, class: ClassType<'db>) -> bool {
        self.class().is_subclass_of(db, class)
    }

    pub(super) fn try_from_file_and_name(
        db: &'db dyn Db,
        file: File,
        symbol_name: &str,
    ) -> Option<Self> {
        let candidate = match symbol_name {
            "Any" => Self::Any,
            "ClassVar" => Self::ClassVar,
            "Deque" => Self::Deque,
            "List" => Self::List,
            "Dict" => Self::Dict,
            "DefaultDict" => Self::DefaultDict,
            "Set" => Self::Set,
            "FrozenSet" => Self::FrozenSet,
            "Counter" => Self::Counter,
            "ChainMap" => Self::ChainMap,
            "OrderedDict" => Self::OrderedDict,
            "Generic" => Self::Generic,
            "Protocol" => Self::Protocol,
            "Optional" => Self::Optional,
            "Union" => Self::Union,
            "NoReturn" => Self::NoReturn,
            "Tuple" => Self::Tuple,
            "Type" => Self::Type,
            "Callable" => Self::Callable,
            "Annotated" => Self::Annotated,
            "Literal" => Self::Literal,
            "Never" => Self::Never,
            "Self" => Self::TypingSelf,
            "Final" => Self::Final,
            "Unpack" => Self::Unpack,
            "Required" => Self::Required,
            "TypeAlias" => Self::TypeAlias,
            "TypeGuard" => Self::TypeGuard,
            "TypedDict" => Self::TypedDict,
            "TypeIs" => Self::TypeIs,
            "ReadOnly" => Self::ReadOnly,
            "Concatenate" => Self::Concatenate,
            "NotRequired" => Self::NotRequired,
            "LiteralString" => Self::LiteralString,
            "Unknown" => Self::Unknown,
            "AlwaysTruthy" => Self::AlwaysTruthy,
            "AlwaysFalsy" => Self::AlwaysFalsy,
            "Not" => Self::Not,
            "Intersection" => Self::Intersection,
            "TypeOf" => Self::TypeOf,
            "CallableTypeOf" => Self::CallableTypeOf,
            _ => return None,
        };

        candidate
            .check_module(file_to_module(db, file)?.known()?)
            .then_some(candidate)
    }

    /// Return `true` if `module` is a module from which this `KnownInstance` variant can validly originate.
    ///
    /// Most variants can only exist in one module, which is the same as `self.class().canonical_module()`.
    /// Some variants could validly be defined in either `typing` or `typing_extensions`, however.
    pub(super) fn check_module(self, module: KnownModule) -> bool {
        match self {
            Self::Any
            | Self::ClassVar
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
            | Self::Protocol
            | Self::Literal
            | Self::LiteralString
            | Self::Never
            | Self::TypingSelf
            | Self::Final
            | Self::Concatenate
            | Self::Unpack
            | Self::Required
            | Self::NotRequired
            | Self::TypeAlias
            | Self::TypeGuard
            | Self::TypedDict
            | Self::TypeIs
            | Self::ReadOnly
            | Self::TypeAliasType(_)
            | Self::TypeVar(_) => {
                matches!(module, KnownModule::Typing | KnownModule::TypingExtensions)
            }
            Self::Unknown
            | Self::AlwaysTruthy
            | Self::AlwaysFalsy
            | Self::Not
            | Self::Intersection
            | Self::TypeOf
            | Self::CallableTypeOf => module.is_knot_extensions(),
        }
    }

    pub(super) fn to_meta_type(self, db: &'db dyn Db) -> Type<'db> {
        self.class().to_class_literal(db)
    }
}
