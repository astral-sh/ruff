//! The `KnownInstance` type.
//!
//! Despite its name, this is quite a different type from [`super::NominalInstanceType`].
//! For the vast majority of instance-types in Python, we cannot say how many possible
//! inhabitants there are or could be of that type at runtime. Each variant of the
//! [`KnownInstanceType`] enum, however, represents a specific runtime symbol
//! that requires heavy special-casing in the type system. Thus any one `KnownInstance`
//! variant can only be inhabited by one or two specific objects at runtime with
//! locations that are known in advance.

use std::fmt::Display;

use super::generics::GenericContext;
use super::{ClassType, Truthiness, Type, TypeAliasType, TypeVarInstance, class::KnownClass};
use crate::db::Db;
use crate::module_resolver::{KnownModule, file_to_module};
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
    Protocol(Option<GenericContext<'db>>),
    /// The symbol `typing.Generic` (which can also be found as `typing_extensions.Generic`)
    Generic(Option<GenericContext<'db>>),
    /// The symbol `typing.Type` (which can also be found as `typing_extensions.Type`)
    Type,
    /// A single instance of `typing.TypeVar`
    TypeVar(TypeVarInstance<'db>),
    /// A single instance of `typing.TypeAliasType` (PEP 695 type alias)
    TypeAliasType(TypeAliasType<'db>),
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
    TypingSelf,

    // Various special forms, special aliases and type qualifiers that we don't yet understand
    // (all currently inferred as TODO in most contexts):
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
            | Self::Protocol(_)
            | Self::Generic(_)
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

    pub(crate) fn normalized(self, db: &'db dyn Db) -> Self {
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
            | Self::List
            | Self::Dict
            | Self::DefaultDict
            | Self::Set
            | Self::FrozenSet
            | Self::Counter
            | Self::Deque
            | Self::ChainMap
            | Self::OrderedDict
            | Self::ReadOnly
            | Self::Unknown
            | Self::AlwaysTruthy
            | Self::AlwaysFalsy
            | Self::Not
            | Self::Intersection
            | Self::TypeOf
            | Self::CallableTypeOf => self,
            Self::TypeVar(tvar) => Self::TypeVar(tvar.normalized(db)),
            Self::Protocol(ctx) => Self::Protocol(ctx.map(|ctx| ctx.normalized(db))),
            Self::Generic(ctx) => Self::Generic(ctx.map(|ctx| ctx.normalized(db))),
            Self::TypeAliasType(alias) => Self::TypeAliasType(alias.normalized(db)),
        }
    }

    /// Return the repr of the symbol at runtime
    pub(crate) fn repr(self, db: &'db dyn Db) -> impl Display + 'db {
        KnownInstanceRepr {
            known_instance: self,
            db,
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
            Self::Protocol(_) => KnownClass::SpecialForm, // actually `_ProtocolMeta` at runtime but this is what typeshed says
            Self::Generic(_) => KnownClass::SpecialForm, // actually `type` at runtime but this is what typeshed says
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
            "Generic" => Self::Generic(None),
            "Protocol" => Self::Protocol(None),
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
            | Self::Generic(_)
            | Self::Callable => module.is_typing(),
            Self::Annotated
            | Self::Protocol(_)
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
            | Self::CallableTypeOf => module.is_ty_extensions(),
        }
    }

    pub(super) fn to_meta_type(self, db: &'db dyn Db) -> Type<'db> {
        self.class().to_class_literal(db)
    }
}

struct KnownInstanceRepr<'db> {
    known_instance: KnownInstanceType<'db>,
    db: &'db dyn Db,
}

impl Display for KnownInstanceRepr<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.known_instance {
            KnownInstanceType::Annotated => f.write_str("typing.Annotated"),
            KnownInstanceType::Literal => f.write_str("typing.Literal"),
            KnownInstanceType::LiteralString => f.write_str("typing.LiteralString"),
            KnownInstanceType::Optional => f.write_str("typing.Optional"),
            KnownInstanceType::Union => f.write_str("typing.Union"),
            KnownInstanceType::NoReturn => f.write_str("typing.NoReturn"),
            KnownInstanceType::Never => f.write_str("typing.Never"),
            KnownInstanceType::Tuple => f.write_str("typing.Tuple"),
            KnownInstanceType::Type => f.write_str("typing.Type"),
            KnownInstanceType::TypingSelf => f.write_str("typing.Self"),
            KnownInstanceType::Final => f.write_str("typing.Final"),
            KnownInstanceType::ClassVar => f.write_str("typing.ClassVar"),
            KnownInstanceType::Callable => f.write_str("typing.Callable"),
            KnownInstanceType::Concatenate => f.write_str("typing.Concatenate"),
            KnownInstanceType::Unpack => f.write_str("typing.Unpack"),
            KnownInstanceType::Required => f.write_str("typing.Required"),
            KnownInstanceType::NotRequired => f.write_str("typing.NotRequired"),
            KnownInstanceType::TypeAlias => f.write_str("typing.TypeAlias"),
            KnownInstanceType::TypeGuard => f.write_str("typing.TypeGuard"),
            KnownInstanceType::TypedDict => f.write_str("typing.TypedDict"),
            KnownInstanceType::TypeIs => f.write_str("typing.TypeIs"),
            KnownInstanceType::List => f.write_str("typing.List"),
            KnownInstanceType::Dict => f.write_str("typing.Dict"),
            KnownInstanceType::DefaultDict => f.write_str("typing.DefaultDict"),
            KnownInstanceType::Set => f.write_str("typing.Set"),
            KnownInstanceType::FrozenSet => f.write_str("typing.FrozenSet"),
            KnownInstanceType::Counter => f.write_str("typing.Counter"),
            KnownInstanceType::Deque => f.write_str("typing.Deque"),
            KnownInstanceType::ChainMap => f.write_str("typing.ChainMap"),
            KnownInstanceType::OrderedDict => f.write_str("typing.OrderedDict"),
            KnownInstanceType::Protocol(generic_context) => {
                f.write_str("typing.Protocol")?;
                if let Some(generic_context) = generic_context {
                    generic_context.display(self.db).fmt(f)?;
                }
                Ok(())
            }
            KnownInstanceType::Generic(generic_context) => {
                f.write_str("typing.Generic")?;
                if let Some(generic_context) = generic_context {
                    generic_context.display(self.db).fmt(f)?;
                }
                Ok(())
            }
            KnownInstanceType::ReadOnly => f.write_str("typing.ReadOnly"),
            // This is a legacy `TypeVar` _outside_ of any generic class or function, so we render
            // it as an instance of `typing.TypeVar`. Inside of a generic class or function, we'll
            // have a `Type::TypeVar(_)`, which is rendered as the typevar's name.
            KnownInstanceType::TypeVar(_) => f.write_str("typing.TypeVar"),
            KnownInstanceType::TypeAliasType(_) => f.write_str("typing.TypeAliasType"),
            KnownInstanceType::Unknown => f.write_str("ty_extensions.Unknown"),
            KnownInstanceType::AlwaysTruthy => f.write_str("ty_extensions.AlwaysTruthy"),
            KnownInstanceType::AlwaysFalsy => f.write_str("ty_extensions.AlwaysFalsy"),
            KnownInstanceType::Not => f.write_str("ty_extensions.Not"),
            KnownInstanceType::Intersection => f.write_str("ty_extensions.Intersection"),
            KnownInstanceType::TypeOf => f.write_str("ty_extensions.TypeOf"),
            KnownInstanceType::CallableTypeOf => f.write_str("ty_extensions.CallableTypeOf"),
        }
    }
}
