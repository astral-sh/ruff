//! An enumeration of special forms in the Python type system.
//! Each of these is considered to inhabit a unique type in our model of the type system.

use itertools::Itertools;

use super::{ClassType, Type, class::KnownClass};
use crate::db::Db;
use crate::semantic_index::place::ScopedPlaceId;
use crate::semantic_index::scope::NodeWithScopeKind;
use crate::semantic_index::{
    FileScopeId, definition::Definition, place_table, scope::ScopeId, semantic_index, use_def_map,
};
use crate::types::IntersectionType;
use crate::types::{
    CallableType, InvalidTypeExpression, InvalidTypeExpressionError, TypeDefinition,
    TypeQualifiers, generics::typing_self, infer::nearest_enclosing_class,
};
use ruff_db::files::File;
use ruff_db::parsed::parsed_module;
use ruff_python_ast::{self as ast, Expr};
use strum_macros::EnumString;
use ty_module_resolver::{KnownModule, file_to_module, resolve_module_confident};

/// Enumeration of specific runtime symbols that are special enough
/// that they can each be considered to inhabit a unique type.
///
/// The enum uses a nested structure: variants that fall into well-defined subcategories
/// (legacy stdlib aliases and type qualifiers) are represented as nested enums,
/// while other special forms that each require unique handling remain as direct variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, get_size2::GetSize)]
pub enum SpecialFormType {
    /// Special forms that are simple aliases to classes elsewhere in the standard library.
    LegacyStdlibAlias(LegacyStdlibAlias),

    /// Special forms that are type qualifiers
    TypeQualifier(TypeQualifier),

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
    /// The symbol `typing.Self` (which can also be found as `typing_extensions.Self`)
    TypingSelf,
    /// The symbol `typing.Concatenate` (which can also be found as `typing_extensions.Concatenate`)
    Concatenate,
    /// The symbol `typing.Unpack` (which can also be found as `typing_extensions.Unpack`)
    Unpack,
    /// The symbol `typing.TypeAlias` (which can also be found as `typing_extensions.TypeAlias`)
    TypeAlias,
    /// The symbol `typing.TypeGuard` (which can also be found as `typing_extensions.TypeGuard`)
    TypeGuard,
    /// The symbol `typing.TypedDict` (which can also be found as `typing_extensions.TypedDict`)
    TypedDict,
    /// The symbol `typing.TypeIs` (which can also be found as `typing_extensions.TypeIs`)
    TypeIs,

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
            | Self::Callable
            | Self::Concatenate
            | Self::Unpack
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
            | Self::TypeQualifier(_) => KnownClass::SpecialForm,

            // Typeshed says it's an instance of `_SpecialForm`,
            // but then we wouldn't recognise things like `issubclass(`X, Protocol)`
            // as being valid.
            Self::Protocol => KnownClass::ProtocolMeta,

            Self::Generic | Self::Any => KnownClass::Type,

            Self::LegacyStdlibAlias(_) => KnownClass::StdlibAlias,

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
        let candidate = Self::from_name(symbol_name)?;
        candidate
            .check_module(file_to_module(db, file)?.known(db)?)
            .then_some(candidate)
    }

    /// Parse a `SpecialFormType` from its runtime symbol name.
    fn from_name(name: &str) -> Option<Self> {
        /// An enum that maps 1:1 with `SpecialFormType`, but which holds no associated data
        /// (and therefore can have `EnumString` derived on it).
        /// This is much more robust than having a manual `from_string` method that matches
        /// on string literals, because experience has shown it's very easy to forget to
        /// update such a method when adding new variants.
        #[derive(EnumString)]
        enum SpecialFormTypeBuilder {
            Tuple,
            Type,
            Callable,
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
            #[strum(serialize = "Self")]
            TypingSelf,
            Concatenate,
            Unpack,
            TypeAlias,
            TypeGuard,
            TypedDict,
            TypeIs,
            Protocol,
            Generic,
            NamedTuple,
            List,
            Dict,
            FrozenSet,
            Set,
            ChainMap,
            Counter,
            DefaultDict,
            Deque,
            OrderedDict,
            Final,
            ClassVar,
            ReadOnly,
            Required,
            NotRequired,
        }

        // This implementation exists purely to enforce that every variant of `SpecialFormType`
        // is included in the `SpecialFormTypeBuilder` enum
        #[cfg(test)]
        impl From<SpecialFormType> for SpecialFormTypeBuilder {
            fn from(value: SpecialFormType) -> Self {
                match value {
                    SpecialFormType::AlwaysFalsy => Self::AlwaysFalsy,
                    SpecialFormType::AlwaysTruthy => Self::AlwaysTruthy,
                    SpecialFormType::Annotated => Self::Annotated,
                    SpecialFormType::Callable => Self::Callable,
                    SpecialFormType::CallableTypeOf => Self::CallableTypeOf,
                    SpecialFormType::Concatenate => Self::Concatenate,
                    SpecialFormType::Intersection => Self::Intersection,
                    SpecialFormType::Literal => Self::Literal,
                    SpecialFormType::LiteralString => Self::LiteralString,
                    SpecialFormType::Never => Self::Never,
                    SpecialFormType::NoReturn => Self::NoReturn,
                    SpecialFormType::Not => Self::Not,
                    SpecialFormType::Optional => Self::Optional,
                    SpecialFormType::Protocol => Self::Protocol,
                    SpecialFormType::Type => Self::Type,
                    SpecialFormType::TypeAlias => Self::TypeAlias,
                    SpecialFormType::TypeGuard => Self::TypeGuard,
                    SpecialFormType::TypeIs => Self::TypeIs,
                    SpecialFormType::TypingSelf => Self::TypingSelf,
                    SpecialFormType::Union => Self::Union,
                    SpecialFormType::Unknown => Self::Unknown,
                    SpecialFormType::Generic => Self::Generic,
                    SpecialFormType::NamedTuple => Self::NamedTuple,
                    SpecialFormType::Any => Self::Any,
                    SpecialFormType::Bottom => Self::Bottom,
                    SpecialFormType::Top => Self::Top,
                    SpecialFormType::Unpack => Self::Unpack,
                    SpecialFormType::Tuple => Self::Tuple,
                    SpecialFormType::TypedDict => Self::TypedDict,
                    SpecialFormType::TypeOf => Self::TypeOf,
                    SpecialFormType::LegacyStdlibAlias(alias) => match alias {
                        LegacyStdlibAlias::List => Self::List,
                        LegacyStdlibAlias::Dict => Self::Dict,
                        LegacyStdlibAlias::Set => Self::Set,
                        LegacyStdlibAlias::FrozenSet => Self::FrozenSet,
                        LegacyStdlibAlias::ChainMap => Self::ChainMap,
                        LegacyStdlibAlias::Counter => Self::Counter,
                        LegacyStdlibAlias::DefaultDict => Self::DefaultDict,
                        LegacyStdlibAlias::Deque => Self::Deque,
                        LegacyStdlibAlias::OrderedDict => Self::OrderedDict,
                    },
                    SpecialFormType::TypeQualifier(qualifier) => match qualifier {
                        TypeQualifier::Final => Self::Final,
                        TypeQualifier::ClassVar => Self::ClassVar,
                        TypeQualifier::ReadOnly => Self::ReadOnly,
                        TypeQualifier::Required => Self::Required,
                        TypeQualifier::NotRequired => Self::NotRequired,
                    },
                }
            }
        }

        SpecialFormTypeBuilder::try_from(name)
            .ok()
            .map(|form| match form {
                SpecialFormTypeBuilder::AlwaysFalsy => Self::AlwaysFalsy,
                SpecialFormTypeBuilder::AlwaysTruthy => Self::AlwaysTruthy,
                SpecialFormTypeBuilder::Annotated => Self::Annotated,
                SpecialFormTypeBuilder::Callable => Self::Callable,
                SpecialFormTypeBuilder::CallableTypeOf => Self::CallableTypeOf,
                SpecialFormTypeBuilder::Concatenate => Self::Concatenate,
                SpecialFormTypeBuilder::Intersection => Self::Intersection,
                SpecialFormTypeBuilder::Literal => Self::Literal,
                SpecialFormTypeBuilder::LiteralString => Self::LiteralString,
                SpecialFormTypeBuilder::Never => Self::Never,
                SpecialFormTypeBuilder::NoReturn => Self::NoReturn,
                SpecialFormTypeBuilder::Not => Self::Not,
                SpecialFormTypeBuilder::Optional => Self::Optional,
                SpecialFormTypeBuilder::Protocol => Self::Protocol,
                SpecialFormTypeBuilder::Type => Self::Type,
                SpecialFormTypeBuilder::TypeAlias => Self::TypeAlias,
                SpecialFormTypeBuilder::TypeGuard => Self::TypeGuard,
                SpecialFormTypeBuilder::TypeIs => Self::TypeIs,
                SpecialFormTypeBuilder::TypingSelf => Self::TypingSelf,
                SpecialFormTypeBuilder::Union => Self::Union,
                SpecialFormTypeBuilder::Unknown => Self::Unknown,
                SpecialFormTypeBuilder::Generic => Self::Generic,
                SpecialFormTypeBuilder::NamedTuple => Self::NamedTuple,
                SpecialFormTypeBuilder::Any => Self::Any,
                SpecialFormTypeBuilder::Bottom => Self::Bottom,
                SpecialFormTypeBuilder::Top => Self::Top,
                SpecialFormTypeBuilder::Unpack => Self::Unpack,
                SpecialFormTypeBuilder::Tuple => Self::Tuple,
                SpecialFormTypeBuilder::TypedDict => Self::TypedDict,
                SpecialFormTypeBuilder::TypeOf => Self::TypeOf,
                SpecialFormTypeBuilder::List => Self::LegacyStdlibAlias(LegacyStdlibAlias::List),
                SpecialFormTypeBuilder::Dict => Self::LegacyStdlibAlias(LegacyStdlibAlias::Dict),
                SpecialFormTypeBuilder::Set => Self::LegacyStdlibAlias(LegacyStdlibAlias::Set),
                SpecialFormTypeBuilder::FrozenSet => {
                    Self::LegacyStdlibAlias(LegacyStdlibAlias::FrozenSet)
                }
                SpecialFormTypeBuilder::ChainMap => {
                    Self::LegacyStdlibAlias(LegacyStdlibAlias::ChainMap)
                }
                SpecialFormTypeBuilder::Counter => {
                    Self::LegacyStdlibAlias(LegacyStdlibAlias::Counter)
                }
                SpecialFormTypeBuilder::DefaultDict => {
                    Self::LegacyStdlibAlias(LegacyStdlibAlias::DefaultDict)
                }
                SpecialFormTypeBuilder::Deque => Self::LegacyStdlibAlias(LegacyStdlibAlias::Deque),
                SpecialFormTypeBuilder::OrderedDict => {
                    Self::LegacyStdlibAlias(LegacyStdlibAlias::OrderedDict)
                }
                SpecialFormTypeBuilder::Final => Self::TypeQualifier(TypeQualifier::Final),
                SpecialFormTypeBuilder::ClassVar => Self::TypeQualifier(TypeQualifier::ClassVar),
                SpecialFormTypeBuilder::ReadOnly => Self::TypeQualifier(TypeQualifier::ReadOnly),
                SpecialFormTypeBuilder::Required => Self::TypeQualifier(TypeQualifier::Required),
                SpecialFormTypeBuilder::NotRequired => {
                    Self::TypeQualifier(TypeQualifier::NotRequired)
                }
            })
    }

    /// Return `true` if `module` is a module from which this `SpecialFormType` variant can validly originate.
    ///
    /// Most variants can only exist in one module, which is the same as `self.class().canonical_module(db)`.
    /// Some variants could validly be defined in either `typing` or `typing_extensions`, however.
    pub(super) fn check_module(self, module: KnownModule) -> bool {
        match self {
            Self::TypeQualifier(TypeQualifier::ClassVar)
            | Self::LegacyStdlibAlias(_)
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
            | Self::TypeQualifier(
                TypeQualifier::Final
                | TypeQualifier::Required
                | TypeQualifier::NotRequired
                | TypeQualifier::ReadOnly,
            )
            | Self::Concatenate
            | Self::Unpack
            | Self::TypeAlias
            | Self::TypeGuard
            | Self::TypedDict
            | Self::TypeIs
            | Self::TypingSelf
            | Self::Protocol
            | Self::NamedTuple
            | Self::Any => {
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
            | Self::LegacyStdlibAlias(
                LegacyStdlibAlias::ChainMap
                | LegacyStdlibAlias::Counter
                | LegacyStdlibAlias::DefaultDict
                | LegacyStdlibAlias::Deque
                | LegacyStdlibAlias::OrderedDict
            )
            | Self::NamedTuple => true,

            // Unlike the aliases to `collections` classes,
            // the aliases to builtin classes are *not* callable...
            Self::LegacyStdlibAlias(
                LegacyStdlibAlias::List
                | LegacyStdlibAlias::Dict
                | LegacyStdlibAlias::Set
                | LegacyStdlibAlias::FrozenSet
            )
            | Self::Tuple
            | Self::Type => false,

            // All other special forms are also not callable
            Self::Annotated
            | Self::Literal
            | Self::LiteralString
            | Self::Optional
            | Self::Union
            | Self::NoReturn
            | Self::Never
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
            | Self::TypeQualifier(_)
            | Self::Concatenate
            | Self::Unpack
            | Self::TypeAlias
            | Self::TypeGuard
            | Self::TypeIs
            | Self::Protocol
            | Self::Any
            | Self::Generic => false,
        }
    }

    /// Return `true` if this special form is valid as the second argument
    /// to `issubclass()` and `isinstance()` calls.
    pub(super) const fn is_valid_isinstance_target(self) -> bool {
        match self {
            Self::Callable
            | Self::LegacyStdlibAlias(_)
            | Self::Tuple
            | Self::Type
            | Self::Protocol
            | Self::Generic => true,

            Self::AlwaysFalsy
            | Self::AlwaysTruthy
            | Self::Annotated
            | Self::Bottom
            | Self::CallableTypeOf
            | Self::TypeQualifier(_)
            | Self::Concatenate
            | Self::Intersection
            | Self::Literal
            | Self::LiteralString
            | Self::Never
            | Self::NoReturn
            | Self::Not
            | Self::TypeAlias
            | Self::TypeGuard
            | Self::NamedTuple
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
            SpecialFormType::TypeQualifier(TypeQualifier::Final) => "Final",
            SpecialFormType::TypeQualifier(TypeQualifier::ClassVar) => "ClassVar",
            SpecialFormType::Callable => "Callable",
            SpecialFormType::Concatenate => "Concatenate",
            SpecialFormType::Unpack => "Unpack",
            SpecialFormType::TypeQualifier(TypeQualifier::Required) => "Required",
            SpecialFormType::TypeQualifier(TypeQualifier::NotRequired) => "NotRequired",
            SpecialFormType::TypeAlias => "TypeAlias",
            SpecialFormType::TypeGuard => "TypeGuard",
            SpecialFormType::TypedDict => "TypedDict",
            SpecialFormType::TypeIs => "TypeIs",
            SpecialFormType::LegacyStdlibAlias(LegacyStdlibAlias::List) => "List",
            SpecialFormType::LegacyStdlibAlias(LegacyStdlibAlias::Dict) => "Dict",
            SpecialFormType::LegacyStdlibAlias(LegacyStdlibAlias::DefaultDict) => "DefaultDict",
            SpecialFormType::LegacyStdlibAlias(LegacyStdlibAlias::Set) => "Set",
            SpecialFormType::LegacyStdlibAlias(LegacyStdlibAlias::FrozenSet) => "FrozenSet",
            SpecialFormType::LegacyStdlibAlias(LegacyStdlibAlias::Counter) => "Counter",
            SpecialFormType::LegacyStdlibAlias(LegacyStdlibAlias::Deque) => "Deque",
            SpecialFormType::LegacyStdlibAlias(LegacyStdlibAlias::ChainMap) => "ChainMap",
            SpecialFormType::LegacyStdlibAlias(LegacyStdlibAlias::OrderedDict) => "OrderedDict",
            SpecialFormType::TypeQualifier(TypeQualifier::ReadOnly) => "ReadOnly",
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
            | SpecialFormType::TypeQualifier(_)
            | SpecialFormType::Callable
            | SpecialFormType::Concatenate
            | SpecialFormType::Unpack
            | SpecialFormType::TypeAlias
            | SpecialFormType::TypeGuard
            | SpecialFormType::TypedDict
            | SpecialFormType::TypeIs
            | SpecialFormType::Protocol
            | SpecialFormType::Generic
            | SpecialFormType::NamedTuple
            | SpecialFormType::LegacyStdlibAlias(_) => {
                &[KnownModule::Typing, KnownModule::TypingExtensions]
            }

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

    /// Interpret this special form as an unparameterized type in a type-expression context.
    ///
    /// This is called for the "misc" special forms that are not aliases, type qualifiers,
    /// `Tuple`, `Type`, or `Callable` (those are handled by their respective call sites).
    pub(super) fn in_type_expression<'db>(
        self,
        db: &'db dyn Db,
        scope_id: ScopeId<'db>,
        typevar_binding_context: Option<Definition<'db>>,
    ) -> Result<Type<'db>, InvalidTypeExpressionError<'db>> {
        match self {
            Self::Never | Self::NoReturn => Ok(Type::Never),
            Self::LiteralString => Ok(Type::literal_string()),
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
            Self::NamedTuple => Ok(IntersectionType::from_two_elements(
                db,
                Type::homogeneous_tuple(db, Type::object()),
                KnownClass::NamedTupleLike.to_instance(db),
            )),

            Self::TypingSelf => {
                let file = scope_id.file(db);
                let index = semantic_index(db, file);
                let Some(class) = nearest_enclosing_class(db, index, scope_id) else {
                    return Err(InvalidTypeExpressionError {
                        fallback_type: Type::unknown(),
                        invalid_expressions: smallvec::smallvec_inline![
                            InvalidTypeExpression::InvalidType(Type::SpecialForm(self), scope_id)
                        ],
                    });
                };

                // Check if the enclosing method's `self`/`cls` parameter is annotated with a
                // type variable other than `Self`. If it is, `Self` cannot be used elsewhere
                // in the method signature.
                if enclosing_method_has_typevar_self_annotation(
                    db,
                    index,
                    scope_id,
                    typevar_binding_context,
                ) {
                    return Err(InvalidTypeExpressionError {
                        fallback_type: Type::unknown(),
                        invalid_expressions: smallvec::smallvec_inline![
                            InvalidTypeExpression::SelfWithNonSelfParam
                        ],
                    });
                }

                Ok(
                    typing_self(db, scope_id, typevar_binding_context, class.into())
                        .map(Type::TypeVar)
                        .unwrap_or(Type::SpecialForm(self)),
                )
            }
            // We ensure that `typing.TypeAlias` used in the expected position (annotating an
            // annotated assignment statement) doesn't reach here. Using it in any other type
            // expression is an error.
            Self::TypeAlias => Err(InvalidTypeExpressionError {
                invalid_expressions: smallvec::smallvec_inline![InvalidTypeExpression::TypeAlias],
                fallback_type: Type::unknown(),
            }),
            Self::TypedDict => Err(InvalidTypeExpressionError {
                invalid_expressions: smallvec::smallvec_inline![InvalidTypeExpression::TypedDict],
                fallback_type: Type::unknown(),
            }),

            Self::Literal | Self::Union | Self::Intersection => Err(InvalidTypeExpressionError {
                invalid_expressions: smallvec::smallvec_inline![
                    InvalidTypeExpression::RequiresArguments(self)
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
                    InvalidTypeExpression::RequiresOneArgument(self)
                ],
                fallback_type: Type::unknown(),
            }),

            Self::Annotated | Self::Concatenate => Err(InvalidTypeExpressionError {
                invalid_expressions: smallvec::smallvec_inline![
                    InvalidTypeExpression::RequiresTwoArguments(self)
                ],
                fallback_type: Type::unknown(),
            }),

            // We treat `typing.Type` exactly the same as `builtins.type`:
            SpecialFormType::Type => Ok(KnownClass::Type.to_instance(db)),
            SpecialFormType::Tuple => Ok(Type::homogeneous_tuple(db, Type::unknown())),
            SpecialFormType::Callable => Ok(Type::Callable(CallableType::unknown(db))),
            SpecialFormType::LegacyStdlibAlias(alias) => Ok(alias.aliased_class().to_instance(db)),
            SpecialFormType::TypeQualifier(qualifier) => {
                let err = match qualifier {
                    TypeQualifier::Final | TypeQualifier::ClassVar => {
                        InvalidTypeExpression::TypeQualifier(qualifier)
                    }
                    TypeQualifier::ReadOnly
                    | TypeQualifier::NotRequired
                    | TypeQualifier::Required => {
                        InvalidTypeExpression::TypeQualifierRequiresOneArgument(qualifier)
                    }
                };
                Err(InvalidTypeExpressionError {
                    invalid_expressions: smallvec::smallvec_inline![err],
                    fallback_type: Type::unknown(),
                })
            }
        }
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

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, get_size2::GetSize)]
pub enum LegacyStdlibAlias {
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
        SpecialFormType::LegacyStdlibAlias(value)
    }
}

impl std::fmt::Display for LegacyStdlibAlias {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        SpecialFormType::from(*self).fmt(f)
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, get_size2::GetSize)]
pub enum TypeQualifier {
    ReadOnly,
    Final,
    ClassVar,
    Required,
    NotRequired,
}

impl From<TypeQualifier> for SpecialFormType {
    fn from(value: TypeQualifier) -> Self {
        SpecialFormType::TypeQualifier(value)
    }
}

impl From<TypeQualifier> for TypeQualifiers {
    fn from(value: TypeQualifier) -> Self {
        match value {
            TypeQualifier::ReadOnly => TypeQualifiers::READ_ONLY,
            TypeQualifier::Final => TypeQualifiers::FINAL,
            TypeQualifier::ClassVar => TypeQualifiers::CLASS_VAR,
            TypeQualifier::Required => TypeQualifiers::REQUIRED,
            TypeQualifier::NotRequired => TypeQualifiers::NOT_REQUIRED,
        }
    }
}

impl std::fmt::Display for TypeQualifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        SpecialFormType::from(*self).fmt(f)
    }
}

/// Information regarding the [`KnownClass`] a [`LegacyStdlibAlias`] refers to.
#[derive(Debug)]
pub(super) struct AliasSpec {
    pub(super) class: KnownClass,
    pub(super) expected_argument_number: usize,
}

/// Check if the first parameter's annotation name is a type variable, either via
/// PEP 695 type parameters or via a legacy `TypeVar` definition.
fn first_param_annotation_is_typevar(
    db: &dyn Db,
    func_node: &ast::StmtFunctionDef,
    definition_scope: ScopeId<'_>,
) -> bool {
    use crate::types::{KnownInstanceType, binding_type};

    let Some(first_param) = func_node.parameters.iter_non_variadic_params().next() else {
        return false;
    };
    let Some(annotation) = first_param.annotation() else {
        return false;
    };

    // Get the annotation name. Handle plain names, `type[T]` subscripts, and skip
    // anything more complex (attributes, etc.).
    let annotation_name = match annotation {
        Expr::Name(ast::ExprName { id, .. }) => id.as_str(),
        Expr::Subscript(ast::ExprSubscript { slice, .. }) => {
            // Handle `type[T]` for classmethods
            match slice.as_ref() {
                Expr::Name(ast::ExprName { id, .. }) => id.as_str(),
                _ => return false,
            }
        }
        _ => return false,
    };

    // `Self` annotation is fine
    if annotation_name == "Self" {
        return false;
    }

    // Check PEP 695 type parameters
    if let Some(type_params) = &func_node.type_params {
        for param in &type_params.type_params {
            if param.name().as_str() == annotation_name {
                return true;
            }
        }
    }

    // Check if the name resolves to a legacy TypeVar/ParamSpec/TypeVarTuple by looking
    // up the name in the scope hierarchy and checking its inferred type.
    let file = definition_scope.file(db);
    let index = semantic_index(db, file);
    let mut scope = definition_scope.file_scope_id(db);
    loop {
        let scope_id = scope.to_scope_id(db, file);
        let pt = place_table(db, scope_id);
        if let Some(symbol_id) = pt.symbol_id(annotation_name) {
            let use_def = use_def_map(db, scope_id);
            for binding in use_def.end_of_scope_bindings(ScopedPlaceId::Symbol(symbol_id)) {
                if let Some(def) = binding.binding.definition() {
                    let ty = binding_type(db, def);
                    return matches!(ty, Type::KnownInstance(KnownInstanceType::TypeVar(_)));
                }
            }
            // Symbol exists in this scope but has no definition here;
            // continue to parent scope to find the actual definition.
        }
        let Some(parent) = index.scope(scope).parent() else {
            break;
        };
        scope = parent;
    }

    false
}

/// Returns `true` if the `Self` usage is directly in a method's signature (not in a nested
/// function) where the first parameter (`self` or `cls`) is annotated with a type variable
/// other than `Self`.
///
/// According to the typing spec, `Self` cannot be used in a method whose `self`/`cls` parameter
/// has a type annotation that is a type variable other than `Self`. This walks ancestor scopes
/// to find the innermost method (a function directly inside a class), matching the same logic
/// used by [`bind_typevar`][crate::types::generics::bind_typevar] for `typing.Self`.
fn enclosing_method_has_typevar_self_annotation<'db>(
    db: &'db dyn Db,
    index: &crate::semantic_index::SemanticIndex<'db>,
    scope_id: ScopeId<'db>,
    typevar_binding_context: Option<Definition<'db>>,
) -> bool {
    use crate::node_key::NodeKey;
    use crate::semantic_index::definition::DefinitionKind;
    use crate::semantic_index::scope::NodeWithScopeKey;

    // Use the same scope-walking start point as `typing_self` to find the enclosing method.
    let containing_scope = typevar_binding_context
        .and_then(|def| {
            let DefinitionKind::Function(func_ref) = def.kind(db) else {
                return None;
            };
            Some(
                index.node_scope_by_key(NodeWithScopeKey::Function(NodeKey::from_node_ref(
                    func_ref,
                ))),
            )
        })
        .unwrap_or_else(|| scope_id.file_scope_id(db));

    let file = scope_id.file(db);
    let module = parsed_module(db, file).load(db);

    for ((inner_id, inner), (outer_id, outer)) in
        index.ancestor_scopes(containing_scope).tuple_windows()
    {
        if outer.kind().is_class() {
            let (NodeWithScopeKind::Function(function_ref)
            | NodeWithScopeKind::FunctionTypeParameters(function_ref)) = inner.node()
            else {
                continue;
            };

            // Only apply the check if `Self` appears directly in this method's signature,
            // not in a nested function or class.
            if containing_scope != inner_id {
                return false;
            }

            let func_node = function_ref.node(&module);
            let class_scope = outer_id.to_scope_id(db, file);
            return first_param_annotation_is_typevar(db, func_node, class_scope);
        }
    }

    false
}
