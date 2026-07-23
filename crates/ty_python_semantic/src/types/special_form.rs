//! An enumeration of special forms in the Python type system.
//! Each of these is considered to inhabit a unique type in our model of the type system.

use super::{ClassType, Type, TypeFormType, class::KnownClass};
use crate::SemanticEnvironment;
use crate::db::Db;
use crate::types::IntersectionType;
use crate::types::infer::InferenceFlags;
use crate::types::{
    CallableType, FunctionDecorators, InvalidTypeExpression, TypeDefinition, TypeQualifiers,
    generics::typing_self,
    infer::{function_known_decorator_flags, nearest_enclosing_class},
};
use strum_macros::EnumString;
use ty_module_resolver::{ImportingFile, KnownModule, file_to_module, resolve_module_confident};
use ty_python_core::ProgramFile;
use ty_python_core::{
    FileScopeId,
    definition::{Definition, DefinitionKind},
    place::ScopedPlaceId,
    place_table,
    scope::ScopeId,
    semantic_index, use_def_map,
};

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

    /// The special form `typing.TypeForm` (which can also be found as
    /// `typing_extensions.TypeForm`).
    TypeForm,

    /// The special form `typing.Callable`.
    ///
    /// This is distinct from the `Callable` exported by the `collections.abc` module.
    TypingCallable,

    /// The symbol `collections.abc.Callable`
    CollectionsAbcCallable,

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
    /// The symbol `ty_extensions._internal.Divergent`
    Divergent,
    /// The symbol `ty_extensions._internal.Todo`
    Todo,
    /// The symbol `ty_extensions.AlwaysTruthy`
    AlwaysTruthy,
    /// The symbol `ty_extensions.AlwaysFalsy`
    AlwaysFalsy,
    /// The symbol `ty_extensions.Not`
    Not,
    /// The symbol `ty_extensions.Intersection`
    Intersection,
    /// The symbol `ty_extensions._internal.TypeOf`
    TypeOf,
    /// The symbol `ty_extensions._internal.CallableTypeOf`
    CallableTypeOf,
    /// The symbol `ty_extensions._internal.RegularCallableTypeOf`
    RegularCallableTypeOf,
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
    /// The symbol `typing.TypedDict` or `typing_extensions.TypedDict`.
    TypedDict(TypedDictModule),
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

/// The module or modules from which `TypedDict` may have been imported.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, get_size2::GetSize)]
pub enum TypedDictModule {
    /// `typing.TypedDict`.
    Typing,
    /// `typing_extensions.TypedDict`.
    TypingExtensions,
}

impl TypedDictModule {
    /// Return the module for a `TypedDict` special form, including a union of the special forms
    /// exported by `typing` and `typing_extensions`.
    pub(super) fn from_type<'db>(db: &'db dyn Db, ty: Type<'db>) -> Option<Self> {
        match ty {
            Type::SpecialForm(SpecialFormType::TypedDict(module)) => Some(module),
            Type::Union(union) => {
                let mut elements = union.elements(db).iter();
                let Type::SpecialForm(SpecialFormType::TypedDict(module)) = elements.next()? else {
                    return None;
                };
                elements.try_fold(*module, |module, element| {
                    let Type::SpecialForm(SpecialFormType::TypedDict(element_module)) = element
                    else {
                        return None;
                    };
                    // `typing_extensions.TypedDict` always offers strictly more functionality than `typing.TypedDict`.
                    // If any element is from `typing`, we therefore infer that the type is a `typing.TypedDict`,
                    // since an operation on a union is only valid if the operation is valid on all elements in the
                    // union.
                    Some(match (module, element_module) {
                        (TypedDictModule::TypingExtensions, TypedDictModule::TypingExtensions) => {
                            TypedDictModule::TypingExtensions
                        }
                        _ => TypedDictModule::Typing,
                    })
                })
            }
            _ => None,
        }
    }
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
            | Self::TypeForm
            | Self::TypingSelf
            | Self::TypingCallable
            | Self::CollectionsAbcCallable
            | Self::Concatenate
            | Self::Unpack
            | Self::TypeAlias
            | Self::TypeGuard
            | Self::TypedDict(_)
            | Self::TypeIs
            | Self::TypeOf
            | Self::Not
            | Self::Top
            | Self::Bottom
            | Self::Intersection
            | Self::CallableTypeOf
            | Self::RegularCallableTypeOf
            | Self::Unknown
            | Self::Divergent
            | Self::Todo
            | Self::AlwaysTruthy
            | Self::AlwaysFalsy => KnownClass::SpecialForm,

            Self::TypeQualifier(qualifier) => qualifier.class(),

            // Typeshed says it's an instance of `_SpecialForm`,
            // but then we wouldn't recognise things like `issubclass(`X, Protocol)`
            // as being valid.
            Self::Protocol => KnownClass::ProtocolMeta,

            Self::Generic | Self::Any => KnownClass::Type,

            Self::LegacyStdlibAlias(_) => KnownClass::StdlibAlias,

            Self::NamedTuple => KnownClass::FunctionType,
        }
    }

    /// Return the instance type which this type is a subtype of.
    ///
    /// For example, the symbol `typing.Literal` is an instance of `typing._SpecialForm`,
    /// so `SpecialFormType::Literal.instance_fallback(db, python_version)`
    /// returns `Type::NominalInstance(NominalInstanceType { class: <typing._SpecialForm> })`.
    pub(super) fn instance_fallback<'db>(self, env: &SemanticEnvironment<'db>) -> Type<'db> {
        self.class().to_instance(env)
    }

    /// Return the type denoted by this retained special-form value when it is valid without
    /// parameters or a surrounding inference scope.
    pub(crate) fn type_form_argument<'db>(
        self,
        env: &SemanticEnvironment<'db>,
    ) -> Option<Type<'db>> {
        let db = env.db();
        match self {
            Self::Never | Self::NoReturn => Some(Type::Never),
            Self::LiteralString => Some(Type::literal_string()),
            Self::Any => Some(Type::any()),
            Self::Unknown => Some(Type::unknown()),
            Self::AlwaysTruthy => Some(Type::AlwaysTruthy),
            Self::AlwaysFalsy => Some(Type::AlwaysFalsy),
            Self::NamedTuple => Some(IntersectionType::from_two_elements(
                env,
                Type::homogeneous_tuple(db, Type::object()),
                KnownClass::NamedTupleLike.to_instance(env),
            )),
            Self::Type => Some(KnownClass::Type.to_instance(env)),
            Self::TypeForm => Some(TypeFormType::from_type_expression(db, Type::any())),
            Self::Tuple => Some(Type::homogeneous_tuple(db, Type::unknown())),
            Self::TypingCallable | Self::CollectionsAbcCallable => {
                Some(Type::Callable(CallableType::unknown(db)))
            }
            Self::LegacyStdlibAlias(alias) => Some(alias.aliased_class().to_instance(env)),
            _ => None,
        }
    }

    /// Return `true` if this symbol is an instance of `class`.
    pub(super) fn is_instance_of(self, env: &SemanticEnvironment<'_>, class: ClassType) -> bool {
        self.class().is_subclass_of(env, class)
    }

    pub(super) fn try_from_file_and_name(
        db: &dyn Db,
        file: ImportingFile<'_>,
        symbol_name: &str,
    ) -> Option<Self> {
        let candidates = Self::candidates_from_name(symbol_name);
        if candidates.is_empty() {
            return None;
        }

        let known_module =
            file_to_module(db, file.resolver_file(db)).and_then(|module| module.known(db))?;
        candidates
            .iter()
            .find(|candidate| candidate.check_module(known_module))
            .copied()
    }

    /// Given the special form we resolved (`self`) and the module the user actually
    /// imported the symbol from (`import_module`), return the variant that matches the
    /// import path — or `self` if there's no better match.
    ///
    /// This exists because typeshed defines some special forms as re-exports from other modules.
    /// Following the alias chain to the definition site would otherwise erase distinctions such as
    /// `typing.Callable` versus `collections.abc.Callable`.
    ///
    /// Called at module-attribute resolution — the one boundary where the import-path
    /// module is still observable.
    pub(super) fn rewrap_for_import_module(self, name: &str, import_module: KnownModule) -> Self {
        match (self, name, import_module) {
            (Self::TypingCallable, "Callable", KnownModule::CollectionsAbcInternal) => {
                Self::CollectionsAbcCallable
            }
            _ => self,
        }
    }

    /// Parse a `SpecialFormType` from its runtime symbol name.
    fn candidates_from_name(name: &str) -> &'static [Self] {
        /// An enum that maps 1:1 with `SpecialFormType`, but which holds no associated data
        /// (and therefore can have `EnumString` derived on it).
        /// This is much more robust than having a manual `from_string` method that matches
        /// on string literals, because experience has shown it's very easy to forget to
        /// update such a method when adding new variants.
        #[derive(EnumString)]
        enum SpecialFormTypeBuilder {
            Tuple,
            Type,
            TypeForm,
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
            Divergent,
            Todo,
            AlwaysTruthy,
            AlwaysFalsy,
            Not,
            Intersection,
            TypeOf,
            CallableTypeOf,
            RegularCallableTypeOf,
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
            InitVar,
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
                    SpecialFormType::TypingCallable => Self::Callable,
                    SpecialFormType::CollectionsAbcCallable => Self::Callable,
                    SpecialFormType::CallableTypeOf => Self::CallableTypeOf,
                    SpecialFormType::RegularCallableTypeOf => Self::RegularCallableTypeOf,
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
                    SpecialFormType::TypeForm => Self::TypeForm,
                    SpecialFormType::TypeAlias => Self::TypeAlias,
                    SpecialFormType::TypeGuard => Self::TypeGuard,
                    SpecialFormType::TypeIs => Self::TypeIs,
                    SpecialFormType::TypingSelf => Self::TypingSelf,
                    SpecialFormType::Union => Self::Union,
                    SpecialFormType::Unknown => Self::Unknown,
                    SpecialFormType::Divergent => Self::Divergent,
                    SpecialFormType::Todo => Self::Todo,
                    SpecialFormType::Generic => Self::Generic,
                    SpecialFormType::NamedTuple => Self::NamedTuple,
                    SpecialFormType::Any => Self::Any,
                    SpecialFormType::Bottom => Self::Bottom,
                    SpecialFormType::Top => Self::Top,
                    SpecialFormType::Unpack => Self::Unpack,
                    SpecialFormType::Tuple => Self::Tuple,
                    SpecialFormType::TypedDict(_) => Self::TypedDict,
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
                        TypeQualifier::InitVar => Self::InitVar,
                    },
                }
            }
        }

        SpecialFormTypeBuilder::try_from(name)
            .ok()
            .map(|form| -> &'static [Self] {
                match form {
                    SpecialFormTypeBuilder::AlwaysFalsy => &[Self::AlwaysFalsy],
                    SpecialFormTypeBuilder::AlwaysTruthy => &[Self::AlwaysTruthy],
                    SpecialFormTypeBuilder::Annotated => &[Self::Annotated],
                    SpecialFormTypeBuilder::Callable => &[Self::TypingCallable],
                    SpecialFormTypeBuilder::CallableTypeOf => &[Self::CallableTypeOf],
                    SpecialFormTypeBuilder::RegularCallableTypeOf => &[Self::RegularCallableTypeOf],
                    SpecialFormTypeBuilder::Concatenate => &[Self::Concatenate],
                    SpecialFormTypeBuilder::Intersection => &[Self::Intersection],
                    SpecialFormTypeBuilder::Literal => &[Self::Literal],
                    SpecialFormTypeBuilder::LiteralString => &[Self::LiteralString],
                    SpecialFormTypeBuilder::Never => &[Self::Never],
                    SpecialFormTypeBuilder::NoReturn => &[Self::NoReturn],
                    SpecialFormTypeBuilder::Not => &[Self::Not],
                    SpecialFormTypeBuilder::Optional => &[Self::Optional],
                    SpecialFormTypeBuilder::Protocol => &[Self::Protocol],
                    SpecialFormTypeBuilder::Type => &[Self::Type],
                    SpecialFormTypeBuilder::TypeForm => &[Self::TypeForm],
                    SpecialFormTypeBuilder::TypeAlias => &[Self::TypeAlias],
                    SpecialFormTypeBuilder::TypeGuard => &[Self::TypeGuard],
                    SpecialFormTypeBuilder::TypeIs => &[Self::TypeIs],
                    SpecialFormTypeBuilder::TypingSelf => &[Self::TypingSelf],
                    SpecialFormTypeBuilder::Union => &[Self::Union],
                    SpecialFormTypeBuilder::Unknown => &[Self::Unknown],
                    SpecialFormTypeBuilder::Divergent => &[Self::Divergent],
                    SpecialFormTypeBuilder::Todo => &[Self::Todo],
                    SpecialFormTypeBuilder::Generic => &[Self::Generic],
                    SpecialFormTypeBuilder::NamedTuple => &[Self::NamedTuple],
                    SpecialFormTypeBuilder::Any => &[Self::Any],
                    SpecialFormTypeBuilder::Bottom => &[Self::Bottom],
                    SpecialFormTypeBuilder::Top => &[Self::Top],
                    SpecialFormTypeBuilder::Unpack => &[Self::Unpack],
                    SpecialFormTypeBuilder::Tuple => &[Self::Tuple],
                    SpecialFormTypeBuilder::TypedDict => &[
                        Self::TypedDict(TypedDictModule::Typing),
                        Self::TypedDict(TypedDictModule::TypingExtensions),
                    ],
                    SpecialFormTypeBuilder::TypeOf => &[Self::TypeOf],
                    SpecialFormTypeBuilder::List => {
                        &[Self::LegacyStdlibAlias(LegacyStdlibAlias::List)]
                    }
                    SpecialFormTypeBuilder::Dict => {
                        &[Self::LegacyStdlibAlias(LegacyStdlibAlias::Dict)]
                    }
                    SpecialFormTypeBuilder::Set => {
                        &[Self::LegacyStdlibAlias(LegacyStdlibAlias::Set)]
                    }
                    SpecialFormTypeBuilder::FrozenSet => {
                        &[Self::LegacyStdlibAlias(LegacyStdlibAlias::FrozenSet)]
                    }
                    SpecialFormTypeBuilder::ChainMap => {
                        &[Self::LegacyStdlibAlias(LegacyStdlibAlias::ChainMap)]
                    }
                    SpecialFormTypeBuilder::Counter => {
                        &[Self::LegacyStdlibAlias(LegacyStdlibAlias::Counter)]
                    }
                    SpecialFormTypeBuilder::DefaultDict => {
                        &[Self::LegacyStdlibAlias(LegacyStdlibAlias::DefaultDict)]
                    }
                    SpecialFormTypeBuilder::Deque => {
                        &[Self::LegacyStdlibAlias(LegacyStdlibAlias::Deque)]
                    }
                    SpecialFormTypeBuilder::OrderedDict => {
                        &[Self::LegacyStdlibAlias(LegacyStdlibAlias::OrderedDict)]
                    }
                    SpecialFormTypeBuilder::Final => &[Self::TypeQualifier(TypeQualifier::Final)],
                    SpecialFormTypeBuilder::ClassVar => {
                        &[Self::TypeQualifier(TypeQualifier::ClassVar)]
                    }
                    SpecialFormTypeBuilder::ReadOnly => {
                        &[Self::TypeQualifier(TypeQualifier::ReadOnly)]
                    }
                    SpecialFormTypeBuilder::Required => {
                        &[Self::TypeQualifier(TypeQualifier::Required)]
                    }
                    SpecialFormTypeBuilder::NotRequired => {
                        &[Self::TypeQualifier(TypeQualifier::NotRequired)]
                    }
                    SpecialFormTypeBuilder::InitVar => {
                        &[Self::TypeQualifier(TypeQualifier::InitVar)]
                    }
                }
            })
            .unwrap_or_default()
    }

    /// Return `true` if `module` is a module from which this `SpecialFormType` variant can validly originate.
    ///
    /// Most variants can only exist in one module, which is the same as `self.class().canonical_module(db)`.
    /// Some variants could validly be defined in either `typing` or `typing_extensions`, however.
    pub(super) fn check_module(self, module: KnownModule) -> bool {
        match self {
            Self::TypeQualifier(qualifier) => qualifier.check_module(module),
            Self::LegacyStdlibAlias(_)
            | Self::Optional
            | Self::Union
            | Self::NoReturn
            | Self::Tuple
            | Self::Type
            | Self::Generic
            | Self::TypedDict(TypedDictModule::Typing)
            | Self::TypingCallable => module.is_typing(),

            Self::Annotated
            | Self::Literal
            | Self::LiteralString
            | Self::Never
            | Self::Concatenate
            | Self::Unpack
            | Self::TypeAlias
            | Self::TypeGuard
            | Self::TypeIs
            | Self::TypeForm
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
            | Self::Intersection => module.is_ty_extensions(),

            Self::Divergent
            | Self::Todo
            | Self::TypeOf
            | Self::CallableTypeOf
            | Self::RegularCallableTypeOf => module.is_ty_extensions_internal(),

            Self::CollectionsAbcCallable => matches!(
                module,
                KnownModule::CollectionsAbc | KnownModule::CollectionsAbcInternal
            ),

            Self::TypedDict(TypedDictModule::TypingExtensions) => module.is_typing_extensions(),
        }
    }

    pub(super) fn to_meta_type<'db>(self, env: &SemanticEnvironment<'db>) -> Type<'db> {
        self.class().to_class_literal(env)
    }

    /// Return true if this special form is callable at runtime.
    /// Most special forms are not callable (they are type constructors that are subscripted),
    /// but some like `TypedDict` and collection constructors can be called.
    pub(super) const fn is_callable(self) -> bool {
        match self {
            // TypedDict can be called as a constructor to create TypedDict types
            Self::TypedDict(_)

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
            Self::TypeForm => true,

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

            Self::TypeQualifier(qualifier) => qualifier.is_callable(),

            // All other special forms are also not callable
            Self::Annotated
            | Self::Literal
            | Self::LiteralString
            | Self::Optional
            | Self::Union
            | Self::NoReturn
            | Self::Never
            | Self::Unknown
            | Self::Divergent
            | Self::Todo
            | Self::AlwaysTruthy
            | Self::AlwaysFalsy
            | Self::Not
            | Self::Top
            | Self::Bottom
            | Self::Intersection
            | Self::TypeOf
            | Self::CallableTypeOf
            | Self::RegularCallableTypeOf
            | Self::TypingCallable
            | Self::CollectionsAbcCallable
            | Self::TypingSelf
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
            Self::TypeQualifier(qualifier) => qualifier.is_valid_isinstance_target(),

            Self::TypingCallable
            | Self::CollectionsAbcCallable
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
            | Self::RegularCallableTypeOf
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
            | Self::TypedDict(_)
            | Self::TypingSelf
            | Self::Union
            | Self::Unknown
            | Self::Divergent
            | Self::Todo
            | Self::TypeOf
            | Self::Any  // can be used in `issubclass()` but not `isinstance()`.
            | Self::Unpack => false,
            Self::TypeForm => false,
        }
    }

    /// Return the name of the symbol at runtime
    pub(super) const fn name(self) -> &'static str {
        match self {
            SpecialFormType::TypeQualifier(qualifier) => qualifier.name(),
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
            SpecialFormType::TypeForm => "TypeForm",
            SpecialFormType::TypingSelf => "Self",
            SpecialFormType::TypingCallable | SpecialFormType::CollectionsAbcCallable => "Callable",
            SpecialFormType::Concatenate => "Concatenate",
            SpecialFormType::Unpack => "Unpack",
            SpecialFormType::TypeAlias => "TypeAlias",
            SpecialFormType::TypeGuard => "TypeGuard",
            SpecialFormType::TypedDict(_) => "TypedDict",
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
            SpecialFormType::Unknown => "Unknown",
            SpecialFormType::Divergent => "Divergent",
            SpecialFormType::Todo => "Todo",
            SpecialFormType::AlwaysTruthy => "AlwaysTruthy",
            SpecialFormType::AlwaysFalsy => "AlwaysFalsy",
            SpecialFormType::Not => "Not",
            SpecialFormType::Intersection => "Intersection",
            SpecialFormType::TypeOf => "TypeOf",
            SpecialFormType::CallableTypeOf => "CallableTypeOf",
            SpecialFormType::RegularCallableTypeOf => "RegularCallableTypeOf",
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
            | SpecialFormType::TypeForm
            | SpecialFormType::TypingSelf
            | SpecialFormType::TypingCallable
            | SpecialFormType::Concatenate
            | SpecialFormType::Unpack
            | SpecialFormType::TypeAlias
            | SpecialFormType::TypeGuard
            | SpecialFormType::TypeIs
            | SpecialFormType::Protocol
            | SpecialFormType::Generic
            | SpecialFormType::NamedTuple
            | SpecialFormType::LegacyStdlibAlias(_) => {
                &[KnownModule::Typing, KnownModule::TypingExtensions]
            }

            SpecialFormType::TypedDict(TypedDictModule::Typing) => &[KnownModule::Typing],
            SpecialFormType::TypedDict(TypedDictModule::TypingExtensions) => {
                &[KnownModule::TypingExtensions]
            }

            SpecialFormType::TypeQualifier(qualifier) => qualifier.definition_modules(),

            SpecialFormType::CollectionsAbcCallable => &[KnownModule::CollectionsAbc],

            SpecialFormType::Unknown
            | SpecialFormType::AlwaysTruthy
            | SpecialFormType::AlwaysFalsy
            | SpecialFormType::Not
            | SpecialFormType::Intersection
            | SpecialFormType::Top
            | SpecialFormType::Bottom => &[KnownModule::TyExtensions],

            SpecialFormType::Divergent
            | SpecialFormType::Todo
            | SpecialFormType::TypeOf
            | SpecialFormType::CallableTypeOf
            | SpecialFormType::RegularCallableTypeOf => &[KnownModule::TyExtensionsInternal],
        }
    }

    pub(super) fn definition<'db>(
        self,
        env: &SemanticEnvironment<'db>,
    ) -> Option<TypeDefinition<'db>> {
        let db = env.db();
        self.definition_modules()
            .iter()
            .find_map(|module| {
                let module =
                    resolve_module_confident(db, env.resolver_environment(), &module.name())?;
                let file = ProgramFile::new(db, module.file(db)?, env.program());
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
        env: &SemanticEnvironment<'db>,
        scope_id: ScopeId<'db>,
        typevar_binding_context: Option<Definition<'db>>,
        inference_flags: InferenceFlags,
    ) -> Result<Type<'db>, InvalidTypeExpression<'db>> {
        let db = env.db();
        match self {
            Self::Never | Self::NoReturn => Ok(Type::Never),
            Self::LiteralString => Ok(Type::literal_string()),
            Self::Any => Ok(Type::any()),
            Self::Unknown => Ok(Type::unknown()),
            Self::Divergent | Self::Todo => Err(InvalidTypeExpression::InvalidType(
                Type::SpecialForm(self),
                scope_id,
            )),
            Self::AlwaysTruthy => Ok(Type::AlwaysTruthy),
            Self::AlwaysFalsy => Ok(Type::AlwaysFalsy),

            // Special case: `NamedTuple` in a type expression is understood to describe the type
            // `tuple[object, ...] & <a protocol that any `NamedTuple` class would satisfy>`.
            // This isn't very principled (since at runtime, `NamedTuple` is just a function),
            // but it appears to be what users often expect, and it improves compatibility with
            // other type checkers such as mypy.
            // See conversation in https://github.com/astral-sh/ruff/pull/19915.
            Self::NamedTuple => Ok(IntersectionType::from_two_elements(
                env,
                Type::homogeneous_tuple(db, Type::object()),
                KnownClass::NamedTupleLike.to_instance(env),
            )),

            Self::TypingSelf => {
                if inference_flags.contains(InferenceFlags::IN_TYPE_ALIAS) {
                    return Err(InvalidTypeExpression::TypingSelfInTypeAlias);
                }

                let program_file = scope_id.program_file(db);
                let index = semantic_index(db, program_file);
                let Some(class) = nearest_enclosing_class(db, index, scope_id) else {
                    return Err(InvalidTypeExpression::InvalidType(
                        Type::SpecialForm(self),
                        scope_id,
                    ));
                };

                let typing_self = typing_self(env, scope_id, typevar_binding_context, class.into());

                let in_staticmethod = typing_self.is_some_and(|typing_self| {
                    let Some(binding_definition) = typing_self.binding_context(db).definition()
                    else {
                        return false;
                    };

                    if !matches!(binding_definition.kind(db), DefinitionKind::Function(_)) {
                        return false;
                    }

                    binding_definition.name(db).as_deref() != Some("__new__")
                        && function_known_decorator_flags(env, binding_definition)
                            .contains(FunctionDecorators::STATICMETHOD)
                });
                if in_staticmethod {
                    return Err(InvalidTypeExpression::TypingSelfInStaticMethod);
                }

                let is_in_metaclass = KnownClass::Type
                    .to_class_literal(env)
                    .to_class_type(env)
                    .is_some_and(|type_class| {
                        class
                            .default_specialization(env)
                            .is_subclass_of(env, type_class)
                    });
                if is_in_metaclass {
                    return Err(InvalidTypeExpression::TypingSelfInMetaclass);
                }

                Ok(typing_self
                    .map(Type::TypeVar)
                    .unwrap_or(Type::SpecialForm(self)))
            }
            // We ensure that `typing.TypeAlias` used in the expected position (annotating an
            // annotated assignment statement) doesn't reach here. Using it in any other type
            // expression is an error.
            Self::TypeAlias => Err(InvalidTypeExpression::TypeAlias),
            Self::TypedDict(_) => Err(InvalidTypeExpression::TypedDict),

            Self::Literal | Self::Union | Self::Intersection => {
                Err(InvalidTypeExpression::RequiresArguments(self))
            }

            Self::Protocol => Err(InvalidTypeExpression::Protocol),
            Self::Generic => Err(InvalidTypeExpression::Generic),

            // `Concatenate` is just always invalid in this context in a type expression
            Self::Concatenate
                if !inference_flags.contains(InferenceFlags::IN_VALID_CONCATENATE_CONTEXT) =>
            {
                Err(InvalidTypeExpression::Concatenate)
            }

            Self::Concatenate | Self::Annotated => {
                Err(InvalidTypeExpression::RequiresTwoArguments(self))
            }

            Self::Optional
            | Self::Not
            | Self::Top
            | Self::Bottom
            | Self::TypeOf
            | Self::TypeIs
            | Self::TypeGuard
            | Self::Unpack
            | Self::CallableTypeOf
            | Self::RegularCallableTypeOf => Err(InvalidTypeExpression::RequiresOneArgument(self)),

            // We treat `typing.Type` exactly the same as `builtins.type`:
            SpecialFormType::Type => Ok(KnownClass::Type.to_instance(env)),
            SpecialFormType::TypeForm => Ok(TypeFormType::from_type_expression(db, Type::any())),
            SpecialFormType::Tuple => Ok(Type::homogeneous_tuple(db, Type::unknown())),
            SpecialFormType::TypingCallable | SpecialFormType::CollectionsAbcCallable => {
                Ok(Type::Callable(CallableType::unknown(db)))
            }
            SpecialFormType::LegacyStdlibAlias(alias) => Ok(alias.aliased_class().to_instance(env)),
            SpecialFormType::TypeQualifier(qualifier) => {
                Err(InvalidTypeExpression::TypeQualifier(qualifier))
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

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, get_size2::GetSize, strum_macros::EnumIter)]
pub enum TypeQualifier {
    ReadOnly,
    Final,
    ClassVar,
    Required,
    NotRequired,
    /// The symbol `dataclasses.InitVar`.
    ///
    /// Typeshed defines this symbol as a class, which is accurate, but we represent it as a
    /// special form internally as it's more similar semantically to `ClassVar`/`Final` etc.
    /// than to a regular generic class.
    InitVar,
}

impl TypeQualifier {
    const fn is_callable(self) -> bool {
        match self {
            Self::InitVar => true,
            Self::ReadOnly | Self::Final | Self::ClassVar | Self::Required | Self::NotRequired => {
                false
            }
        }
    }

    const fn check_module(self, module: KnownModule) -> bool {
        match self {
            Self::InitVar => module.is_dataclasses(),
            Self::ClassVar => module.is_typing(),
            Self::ReadOnly | Self::Final | Self::Required | Self::NotRequired => {
                matches!(module, KnownModule::Typing | KnownModule::TypingExtensions)
            }
        }
    }

    const fn is_valid_isinstance_target(self) -> bool {
        match self {
            Self::InitVar => true,
            Self::ReadOnly | Self::Final | Self::ClassVar | Self::Required | Self::NotRequired => {
                false
            }
        }
    }

    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::ReadOnly => "ReadOnly",
            Self::Final => "Final",
            Self::ClassVar => "ClassVar",
            Self::Required => "Required",
            Self::NotRequired => "NotRequired",
            Self::InitVar => "InitVar",
        }
    }

    const fn definition_modules(self) -> &'static [KnownModule] {
        match self {
            Self::InitVar => &[KnownModule::Dataclasses],
            Self::ClassVar | Self::ReadOnly | Self::Final | Self::Required | Self::NotRequired => {
                &[KnownModule::Typing, KnownModule::TypingExtensions]
            }
        }
    }

    const fn class(self) -> KnownClass {
        match self {
            Self::ReadOnly | Self::Final | Self::ClassVar | Self::Required | Self::NotRequired => {
                KnownClass::SpecialForm
            }
            Self::InitVar => KnownClass::Type,
        }
    }

    /// Return `true` if this type qualifier requires exactly one argument
    /// when used in a type expression.
    pub(super) const fn requires_one_argument(self) -> bool {
        match self {
            Self::Final | Self::ClassVar => false,
            Self::Required | Self::NotRequired | Self::InitVar | Self::ReadOnly => true,
        }
    }
    pub(crate) const fn is_valid_for_non_name_targets(self) -> bool {
        match self {
            TypeQualifier::ReadOnly
            | TypeQualifier::Required
            | TypeQualifier::NotRequired
            | TypeQualifier::ClassVar
            | TypeQualifier::InitVar => false,
            TypeQualifier::Final => true,
        }
    }

    pub(crate) const fn is_valid_in_typeddict_field(self) -> bool {
        match self {
            TypeQualifier::ReadOnly | TypeQualifier::Required | TypeQualifier::NotRequired => true,
            TypeQualifier::ClassVar | TypeQualifier::Final | TypeQualifier::InitVar => false,
        }
    }
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
            TypeQualifier::InitVar => TypeQualifiers::INIT_VAR,
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
