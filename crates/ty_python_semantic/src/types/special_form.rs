//! An enumeration of special forms in the Python type system.
//! Each of these is considered to inhabit a unique type in our model of the type system.

use super::{ClassType, Type, class::KnownClass};
use crate::db::Db;
use crate::semantic_index::place::ScopedPlaceId;
use crate::semantic_index::{FileScopeId, place_table, use_def_map};
use crate::types::TypeDefinition;
use ruff_db::files::File;
use std::str::FromStr;
use ty_module_resolver::{KnownModule, file_to_module, resolve_module_confident};

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

    /// Return `Some(KnownClass)` if this special form is an alias
    /// to a standard library class.
    pub(super) const fn aliased_stdlib_class(self) -> Option<KnownClass> {
        match self {
            Self::List => Some(KnownClass::List),
            Self::Dict => Some(KnownClass::Dict),
            Self::Set => Some(KnownClass::Set),
            Self::FrozenSet => Some(KnownClass::FrozenSet),
            Self::ChainMap => Some(KnownClass::ChainMap),
            Self::Counter => Some(KnownClass::Counter),
            Self::DefaultDict => Some(KnownClass::DefaultDict),
            Self::Deque => Some(KnownClass::Deque),
            Self::OrderedDict => Some(KnownClass::OrderedDict),
            Self::Tuple => Some(KnownClass::Tuple),
            Self::Type => Some(KnownClass::Type),

            Self::AlwaysFalsy
            | Self::AlwaysTruthy
            | Self::Annotated
            | Self::Bottom
            | Self::CallableTypeOf
            | Self::ClassVar
            | Self::Concatenate
            | Self::Final
            | Self::Intersection
            | Self::Literal
            | Self::LiteralString
            | Self::Never
            | Self::NoReturn
            | Self::Not
            | Self::ReadOnly
            | Self::Required
            | Self::TypeAlias
            | Self::TypeGuard
            | Self::NamedTuple
            | Self::NotRequired
            | Self::Optional
            | Self::Top
            | Self::TypeIs
            | Self::TypedDict
            | Self::TypingSelf
            | Self::Union
            | Self::Unknown
            | Self::TypeOf
            | Self::Any
            // `typing.Callable` is an alias to `collections.abc.Callable`,
            // but they're both the same `SpecialFormType` in our model,
            // and neither is a class in typeshed (even though the `collections.abc` one is at runtime)
            | Self::Callable
            | Self::Protocol
            | Self::Generic
            | Self::Unpack => None,
        }
    }

    /// Return `true` if this special form is valid as the second argument
    /// to `issubclass()` and `isinstance()` calls.
    pub(super) const fn is_valid_isinstance_target(self) -> bool {
        match self {
            Self::Callable
            | Self::ChainMap
            | Self::Counter
            | Self::DefaultDict
            | Self::Deque
            | Self::FrozenSet
            | Self::Dict
            | Self::List
            | Self::OrderedDict
            | Self::Set
            | Self::Tuple
            | Self::Type
            | Self::Protocol
            | Self::Generic => true,

            Self::AlwaysFalsy
            | Self::AlwaysTruthy
            | Self::Annotated
            | Self::Bottom
            | Self::CallableTypeOf
            | Self::ClassVar
            | Self::Concatenate
            | Self::Final
            | Self::Intersection
            | Self::Literal
            | Self::LiteralString
            | Self::Never
            | Self::NoReturn
            | Self::Not
            | Self::ReadOnly
            | Self::Required
            | Self::TypeAlias
            | Self::TypeGuard
            | Self::NamedTuple
            | Self::NotRequired
            | Self::Optional
            | Self::Top
            | Self::TypeIs
            | Self::TypedDict
            | Self::TypingSelf
            | Self::Union
            | Self::Unknown
            | Self::TypeOf
            | Self::Any  // can be used in `issubclass()` but not `isinstance()`.
            | Self::Unpack => false,
        }
    }

    /// Return the name of the symbol at runtime
    pub(super) const fn name(self) -> &'static str {
        match self {
            SpecialFormType::Any => "Any",
            SpecialFormType::Annotated => "Annotated",
            SpecialFormType::Literal => "Literal",
            SpecialFormType::LiteralString => "LiteralString",
            SpecialFormType::Optional => "Optional",
            SpecialFormType::Union => "Union",
            SpecialFormType::NoReturn => "NoReturn",
            SpecialFormType::Never => "Never",
            SpecialFormType::Tuple => "Tuple",
            SpecialFormType::Type => "Type",
            SpecialFormType::TypingSelf => "Self",
            SpecialFormType::Final => "Final",
            SpecialFormType::ClassVar => "ClassVar",
            SpecialFormType::Callable => "Callable",
            SpecialFormType::Concatenate => "Concatenate",
            SpecialFormType::Unpack => "Unpack",
            SpecialFormType::Required => "Required",
            SpecialFormType::NotRequired => "NotRequired",
            SpecialFormType::TypeAlias => "TypeAlias",
            SpecialFormType::TypeGuard => "TypeGuard",
            SpecialFormType::TypedDict => "TypedDict",
            SpecialFormType::TypeIs => "TypeIs",
            SpecialFormType::List => "List",
            SpecialFormType::Dict => "Dict",
            SpecialFormType::DefaultDict => "DefaultDict",
            SpecialFormType::Set => "Set",
            SpecialFormType::FrozenSet => "FrozenSet",
            SpecialFormType::Counter => "Counter",
            SpecialFormType::Deque => "Deque",
            SpecialFormType::ChainMap => "ChainMap",
            SpecialFormType::OrderedDict => "OrderedDict",
            SpecialFormType::ReadOnly => "ReadOnly",
            SpecialFormType::Unknown => "Unknown",
            SpecialFormType::AlwaysTruthy => "AlwaysTruthy",
            SpecialFormType::AlwaysFalsy => "AlwaysFalsy",
            SpecialFormType::Not => "Not",
            SpecialFormType::Intersection => "Intersection",
            SpecialFormType::TypeOf => "TypeOf",
            SpecialFormType::CallableTypeOf => "CallableTypeOf",
            SpecialFormType::Top => "Top",
            SpecialFormType::Bottom => "Bottom",
            SpecialFormType::Protocol => "Protocol",
            SpecialFormType::Generic => "Generic",
            SpecialFormType::NamedTuple => "NamedTuple",
        }
    }

    /// Return the module(s) in which this special form could be defined
    fn definition_modules(self) -> &'static [KnownModule] {
        match self {
            SpecialFormType::Any
            | SpecialFormType::Annotated
            | SpecialFormType::Literal
            | SpecialFormType::LiteralString
            | SpecialFormType::Optional
            | SpecialFormType::Union
            | SpecialFormType::NoReturn
            | SpecialFormType::Never
            | SpecialFormType::Tuple
            | SpecialFormType::Type
            | SpecialFormType::TypingSelf
            | SpecialFormType::Final
            | SpecialFormType::ClassVar
            | SpecialFormType::Callable
            | SpecialFormType::Concatenate
            | SpecialFormType::Unpack
            | SpecialFormType::Required
            | SpecialFormType::NotRequired
            | SpecialFormType::TypeAlias
            | SpecialFormType::TypeGuard
            | SpecialFormType::TypedDict
            | SpecialFormType::TypeIs
            | SpecialFormType::ReadOnly
            | SpecialFormType::Protocol
            | SpecialFormType::Generic
            | SpecialFormType::NamedTuple
            | SpecialFormType::List
            | SpecialFormType::Dict
            | SpecialFormType::DefaultDict
            | SpecialFormType::Set
            | SpecialFormType::FrozenSet
            | SpecialFormType::Counter
            | SpecialFormType::Deque
            | SpecialFormType::ChainMap
            | SpecialFormType::OrderedDict => &[KnownModule::Typing, KnownModule::TypingExtensions],

            SpecialFormType::Unknown
            | SpecialFormType::AlwaysTruthy
            | SpecialFormType::AlwaysFalsy
            | SpecialFormType::Not
            | SpecialFormType::Intersection
            | SpecialFormType::TypeOf
            | SpecialFormType::CallableTypeOf
            | SpecialFormType::Top
            | SpecialFormType::Bottom => &[KnownModule::TyExtensions],
        }
    }

    pub(super) fn definition(self, db: &dyn Db) -> Option<TypeDefinition<'_>> {
        self.definition_modules()
            .iter()
            .find_map(|module| {
                let file = resolve_module_confident(db, &module.name())?.file(db)?;
                let scope = FileScopeId::global().to_scope_id(db, file);
                let symbol_id = place_table(db, scope).symbol_id(self.name())?;

                use_def_map(db, scope)
                    .end_of_scope_bindings(ScopedPlaceId::Symbol(symbol_id))
                    .next()?
                    .binding
                    .definition()
            })
            .map(TypeDefinition::SpecialForm)
    }
}

impl std::fmt::Display for SpecialFormType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}.{}",
            self.definition_modules()[0].as_str(),
            self.name()
        )
    }
}
