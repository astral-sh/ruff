use crate::{
    Db, Program,
    place::{DefinedPlace, Definedness, Place, known_module_symbol},
    types::{
        Binding, ClassLiteral, ClassType, GenericContext, KnownInstanceType, StaticClassLiteral,
        SubclassOfType, Type, binding_type,
        bound_super::{BoundSuperError, BoundSuperType},
        class::CodeGeneratorKind,
        constraints::{ConstraintSet, ConstraintSetBuilder},
        context::InferContext,
        diagnostic::SUPER_CALL_IN_NAMED_TUPLE_METHOD,
        infer::nearest_enclosing_class,
        known_instance::DeprecatedInstance,
    },
};
use ruff_db::files::File;
use ruff_python_ast as ast;
use ruff_python_ast::PythonVersion;
use rustc_hash::FxHashSet;
use std::{
    borrow::Cow,
    sync::{LazyLock, Mutex},
};
use ty_module_resolver::{KnownModule, file_to_module};
use ty_python_core::{SemanticIndex, Truthiness, scope::NodeWithScopeKind};

/// Non-exhaustive enumeration of known classes (e.g. `builtins.int`, `typing.Any`, ...) to allow
/// for easier syntax when interacting with very common classes.
///
/// Feel free to expand this enum if you ever find yourself using the same class in multiple
/// places.
/// Note: good candidates are any classes in `[ty_module_resolver::module::KnownModule]`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, get_size2::GetSize)]
#[cfg_attr(test, derive(strum_macros::EnumIter))]
pub enum KnownClass {
    // To figure out where an stdlib symbol is defined, you can go into `crates/ty_vendored`
    // and grep for the symbol name in any `.pyi` file.

    // Builtins
    Bool,
    Object,
    Bytes,
    Bytearray,
    Type,
    Int,
    Float,
    Complex,
    Str,
    List,
    Tuple,
    Set,
    FrozenSet,
    Dict,
    Slice,
    Property,
    BaseException,
    Exception,
    BaseExceptionGroup,
    ExceptionGroup,
    Staticmethod,
    Classmethod,
    Super,
    NotImplementedError,
    // enum
    Enum,
    EnumType,
    Auto,
    Member,
    Nonmember,
    StrEnum,
    IntEnum,
    Flag,
    IntFlag,
    // abc
    ABCMeta,
    // Types
    GenericAlias,
    ModuleType,
    FunctionType,
    MethodType,
    MethodWrapperType,
    WrapperDescriptorType,
    UnionType,
    GeneratorType,
    AsyncGeneratorType,
    CoroutineType,
    NotImplementedType,
    BuiltinFunctionType,
    // Exposed as `types.EllipsisType` on Python >=3.10;
    // backported as `builtins.ellipsis` by typeshed on Python <=3.9
    EllipsisType,
    // Typeshed
    NoneType, // Part of `types` for Python >= 3.10
    // Typing
    Awaitable,
    Generator,
    AsyncGenerator,
    Deprecated,
    StdlibAlias,
    SpecialForm,
    TypeVar,
    ParamSpec,
    // typing_extensions.ParamSpec
    ExtensionsParamSpec, // must be distinct from typing.ParamSpec, backports new features
    ParamSpecArgs,
    ParamSpecKwargs,
    ProtocolMeta,
    TypeVarTuple,
    TypeAliasType,
    NoDefaultType,
    NewType,
    SupportsIndex,
    Iterable,
    Iterator,
    AsyncIterator,
    Sequence,
    Mapping,
    MutableMapping,
    // typing_extensions
    ExtensionsTypeVar, // must be distinct from typing.TypeVar, backports new features
    // Collections
    ChainMap,
    Counter,
    DefaultDict,
    Deque,
    OrderedDict,
    // sys
    VersionInfo,
    // dataclasses
    Field,
    KwOnly,
    // _typeshed._type_checker_internals
    NamedTupleFallback,
    NamedTupleLike,
    TypedDictFallback,
    // string.templatelib
    Template,
    // pathlib
    Path,
    // ty_extensions
    ConstraintSet,
    GenericContext,
    Specialization,
}

impl KnownClass {
    pub(crate) const fn is_bool(self) -> bool {
        matches!(self, Self::Bool)
    }

    pub(crate) const fn is_special_form(self) -> bool {
        matches!(self, Self::SpecialForm)
    }

    /// Determine whether instances of this class are always truthy, always falsy,
    /// or have an ambiguous truthiness.
    ///
    /// Returns `None` for `KnownClass::Tuple`, since the truthiness of a tuple
    /// depends on its spec.
    pub(crate) const fn bool(self) -> Option<Truthiness> {
        match self {
            // N.B. It's only generally safe to infer `Truthiness::AlwaysTrue` for a `KnownClass`
            // variant if the class's `__bool__` method always returns the same thing *and* the
            // class is `@final`.
            //
            // E.g. `ModuleType.__bool__` always returns `True`, but `ModuleType` is not `@final`.
            // Equally, `range` is `@final`, but its `__bool__` method can return `False`.
            Self::EllipsisType
            | Self::NoDefaultType
            | Self::MethodType
            | Self::Slice
            | Self::FunctionType
            | Self::VersionInfo
            | Self::TypeAliasType
            | Self::TypeVar
            | Self::ExtensionsTypeVar
            | Self::ParamSpec
            | Self::ExtensionsParamSpec
            | Self::ParamSpecArgs
            | Self::ParamSpecKwargs
            | Self::TypeVarTuple
            | Self::Super
            | Self::WrapperDescriptorType
            | Self::UnionType
            | Self::GeneratorType
            | Self::AsyncGeneratorType
            | Self::MethodWrapperType
            | Self::CoroutineType
            | Self::BuiltinFunctionType
            | Self::Template
            | Self::Path => Some(Truthiness::AlwaysTrue),

            Self::NoneType => Some(Truthiness::AlwaysFalse),

            Self::BaseException
            | Self::Exception
            | Self::NotImplementedError
            | Self::ExceptionGroup
            | Self::Object
            | Self::OrderedDict
            | Self::BaseExceptionGroup
            | Self::Bool
            | Self::Str
            | Self::List
            | Self::GenericAlias
            | Self::NewType
            | Self::StdlibAlias
            | Self::SupportsIndex
            | Self::Set
            | Self::Int
            | Self::Type
            | Self::Bytes
            | Self::Bytearray
            | Self::FrozenSet
            | Self::Property
            | Self::SpecialForm
            | Self::Dict
            | Self::ModuleType
            | Self::ChainMap
            | Self::Complex
            | Self::Counter
            | Self::DefaultDict
            | Self::Deque
            | Self::Float
            | Self::Enum
            | Self::EnumType
            | Self::Auto
            | Self::Member
            | Self::Nonmember
            | Self::StrEnum
            | Self::IntEnum
            | Self::Flag
            | Self::IntFlag
            | Self::ABCMeta
            | Self::Iterable
            | Self::Iterator
            | Self::AsyncIterator
            | Self::Sequence
            | Self::Mapping
            | Self::MutableMapping
            // Evaluating `NotImplementedType` in a boolean context was deprecated in Python 3.9
            // and raises a `TypeError` in Python >=3.14
            // (see https://docs.python.org/3/library/constants.html#NotImplemented)
            | Self::NotImplementedType
            | Self::Staticmethod
            | Self::Classmethod
            | Self::Awaitable
            | Self::Generator
            | Self::AsyncGenerator
            | Self::Deprecated
            | Self::Field
            | Self::KwOnly
            | Self::NamedTupleFallback
            | Self::NamedTupleLike
            | Self::ConstraintSet
            | Self::GenericContext
            | Self::Specialization
            | Self::ProtocolMeta
            | Self::TypedDictFallback => Some(Truthiness::Ambiguous),

            Self::Tuple => None,
        }
    }

    /// Return `true` if this class is a subclass of `enum.Enum` *and* has enum members, i.e.
    /// if it is an "actual" enum, not `enum.Enum` itself or a similar custom enum class.
    pub(crate) const fn is_enum_subclass_with_members(self) -> bool {
        match self {
            KnownClass::Bool
            | KnownClass::Object
            | KnownClass::Bytes
            | KnownClass::Bytearray
            | KnownClass::Type
            | KnownClass::Int
            | KnownClass::Float
            | KnownClass::Complex
            | KnownClass::Str
            | KnownClass::List
            | KnownClass::Tuple
            | KnownClass::Set
            | KnownClass::FrozenSet
            | KnownClass::Dict
            | KnownClass::Slice
            | KnownClass::Property
            | KnownClass::BaseException
            | KnownClass::NotImplementedError
            | KnownClass::Exception
            | KnownClass::BaseExceptionGroup
            | KnownClass::ExceptionGroup
            | KnownClass::Staticmethod
            | KnownClass::Classmethod
            | KnownClass::Awaitable
            | KnownClass::Generator
            | KnownClass::AsyncGenerator
            | KnownClass::Deprecated
            | KnownClass::Super
            | KnownClass::Enum
            | KnownClass::EnumType
            | KnownClass::Auto
            | KnownClass::Member
            | KnownClass::Nonmember
            | KnownClass::StrEnum
            | KnownClass::IntEnum
            | KnownClass::Flag
            | KnownClass::IntFlag
            | KnownClass::ABCMeta
            | KnownClass::GenericAlias
            | KnownClass::ModuleType
            | KnownClass::FunctionType
            | KnownClass::MethodType
            | KnownClass::MethodWrapperType
            | KnownClass::WrapperDescriptorType
            | KnownClass::UnionType
            | KnownClass::GeneratorType
            | KnownClass::AsyncGeneratorType
            | KnownClass::CoroutineType
            | KnownClass::NoneType
            | KnownClass::StdlibAlias
            | KnownClass::SpecialForm
            | KnownClass::TypeVar
            | KnownClass::ExtensionsTypeVar
            | KnownClass::ParamSpec
            | KnownClass::ExtensionsParamSpec
            | KnownClass::ParamSpecArgs
            | KnownClass::ParamSpecKwargs
            | KnownClass::TypeVarTuple
            | KnownClass::TypeAliasType
            | KnownClass::NoDefaultType
            | KnownClass::NewType
            | KnownClass::SupportsIndex
            | KnownClass::Iterable
            | KnownClass::Iterator
            | KnownClass::AsyncIterator
            | KnownClass::Sequence
            | KnownClass::Mapping
            | KnownClass::MutableMapping
            | KnownClass::ChainMap
            | KnownClass::Counter
            | KnownClass::DefaultDict
            | KnownClass::Deque
            | KnownClass::OrderedDict
            | KnownClass::VersionInfo
            | KnownClass::EllipsisType
            | KnownClass::NotImplementedType
            | KnownClass::Field
            | KnownClass::KwOnly
            | KnownClass::NamedTupleFallback
            | KnownClass::NamedTupleLike
            | KnownClass::ConstraintSet
            | KnownClass::GenericContext
            | KnownClass::Specialization
            | KnownClass::TypedDictFallback
            | KnownClass::BuiltinFunctionType
            | KnownClass::ProtocolMeta
            | KnownClass::Template
            | KnownClass::Path => false,
        }
    }

    /// Return `true` if this class is a (true) subclass of `typing.TypedDict`.
    pub(crate) const fn is_typed_dict_subclass(self) -> bool {
        match self {
            KnownClass::Bool
            | KnownClass::Object
            | KnownClass::Bytes
            | KnownClass::Bytearray
            | KnownClass::Type
            | KnownClass::Int
            | KnownClass::Float
            | KnownClass::Complex
            | KnownClass::Str
            | KnownClass::List
            | KnownClass::Tuple
            | KnownClass::Set
            | KnownClass::FrozenSet
            | KnownClass::Dict
            | KnownClass::Slice
            | KnownClass::Property
            | KnownClass::BaseException
            | KnownClass::Exception
            | KnownClass::NotImplementedError
            | KnownClass::BaseExceptionGroup
            | KnownClass::ExceptionGroup
            | KnownClass::Staticmethod
            | KnownClass::Classmethod
            | KnownClass::Awaitable
            | KnownClass::Generator
            | KnownClass::AsyncGenerator
            | KnownClass::Deprecated
            | KnownClass::Super
            | KnownClass::Enum
            | KnownClass::EnumType
            | KnownClass::Auto
            | KnownClass::Member
            | KnownClass::Nonmember
            | KnownClass::StrEnum
            | KnownClass::IntEnum
            | KnownClass::Flag
            | KnownClass::IntFlag
            | KnownClass::ABCMeta
            | KnownClass::GenericAlias
            | KnownClass::ModuleType
            | KnownClass::FunctionType
            | KnownClass::MethodType
            | KnownClass::MethodWrapperType
            | KnownClass::WrapperDescriptorType
            | KnownClass::UnionType
            | KnownClass::GeneratorType
            | KnownClass::AsyncGeneratorType
            | KnownClass::CoroutineType
            | KnownClass::NoneType
            | KnownClass::StdlibAlias
            | KnownClass::SpecialForm
            | KnownClass::TypeVar
            | KnownClass::ExtensionsTypeVar
            | KnownClass::ParamSpec
            | KnownClass::ExtensionsParamSpec
            | KnownClass::ParamSpecArgs
            | KnownClass::ParamSpecKwargs
            | KnownClass::TypeVarTuple
            | KnownClass::TypeAliasType
            | KnownClass::NoDefaultType
            | KnownClass::NewType
            | KnownClass::SupportsIndex
            | KnownClass::Iterable
            | KnownClass::Iterator
            | KnownClass::AsyncIterator
            | KnownClass::Sequence
            | KnownClass::Mapping
            | KnownClass::MutableMapping
            | KnownClass::ChainMap
            | KnownClass::Counter
            | KnownClass::DefaultDict
            | KnownClass::Deque
            | KnownClass::OrderedDict
            | KnownClass::VersionInfo
            | KnownClass::EllipsisType
            | KnownClass::NotImplementedType
            | KnownClass::Field
            | KnownClass::KwOnly
            | KnownClass::NamedTupleFallback
            | KnownClass::NamedTupleLike
            | KnownClass::ConstraintSet
            | KnownClass::GenericContext
            | KnownClass::Specialization
            | KnownClass::TypedDictFallback
            | KnownClass::BuiltinFunctionType
            | KnownClass::ProtocolMeta
            | KnownClass::Template
            | KnownClass::Path => false,
        }
    }

    pub(crate) const fn is_tuple_subclass(self) -> bool {
        match self {
            KnownClass::Tuple | KnownClass::VersionInfo => true,

            KnownClass::Bool
            | KnownClass::Object
            | KnownClass::Bytes
            | KnownClass::Bytearray
            | KnownClass::Type
            | KnownClass::Int
            | KnownClass::Float
            | KnownClass::Complex
            | KnownClass::Str
            | KnownClass::List
            | KnownClass::Set
            | KnownClass::FrozenSet
            | KnownClass::Dict
            | KnownClass::Slice
            | KnownClass::Property
            | KnownClass::BaseException
            | KnownClass::Exception
            | KnownClass::NotImplementedError
            | KnownClass::BaseExceptionGroup
            | KnownClass::ExceptionGroup
            | KnownClass::Staticmethod
            | KnownClass::Classmethod
            | KnownClass::Awaitable
            | KnownClass::Generator
            | KnownClass::AsyncGenerator
            | KnownClass::Deprecated
            | KnownClass::Super
            | KnownClass::Enum
            | KnownClass::EnumType
            | KnownClass::Auto
            | KnownClass::Member
            | KnownClass::Nonmember
            | KnownClass::StrEnum
            | KnownClass::IntEnum
            | KnownClass::Flag
            | KnownClass::IntFlag
            | KnownClass::ABCMeta
            | KnownClass::GenericAlias
            | KnownClass::ModuleType
            | KnownClass::FunctionType
            | KnownClass::MethodType
            | KnownClass::MethodWrapperType
            | KnownClass::WrapperDescriptorType
            | KnownClass::UnionType
            | KnownClass::GeneratorType
            | KnownClass::AsyncGeneratorType
            | KnownClass::CoroutineType
            | KnownClass::NoneType
            | KnownClass::StdlibAlias
            | KnownClass::SpecialForm
            | KnownClass::TypeVar
            | KnownClass::ExtensionsTypeVar
            | KnownClass::ParamSpec
            | KnownClass::ExtensionsParamSpec
            | KnownClass::ParamSpecArgs
            | KnownClass::ParamSpecKwargs
            | KnownClass::TypeVarTuple
            | KnownClass::TypeAliasType
            | KnownClass::NoDefaultType
            | KnownClass::NewType
            | KnownClass::SupportsIndex
            | KnownClass::Iterable
            | KnownClass::Iterator
            | KnownClass::AsyncIterator
            | KnownClass::Sequence
            | KnownClass::Mapping
            | KnownClass::MutableMapping
            | KnownClass::ChainMap
            | KnownClass::Counter
            | KnownClass::DefaultDict
            | KnownClass::Deque
            | KnownClass::OrderedDict
            | KnownClass::EllipsisType
            | KnownClass::NotImplementedType
            | KnownClass::Field
            | KnownClass::KwOnly
            | KnownClass::TypedDictFallback
            | KnownClass::NamedTupleLike
            | KnownClass::NamedTupleFallback
            | KnownClass::ConstraintSet
            | KnownClass::GenericContext
            | KnownClass::Specialization
            | KnownClass::BuiltinFunctionType
            | KnownClass::ProtocolMeta
            | KnownClass::Template
            | KnownClass::Path => false,
        }
    }

    /// Return `true` if this class is a protocol class.
    ///
    /// In an ideal world, perhaps we wouldn't hardcode this knowledge here;
    /// instead, we'd just look at the bases for these classes, as we do for
    /// all other classes. However, the special casing here helps us out in
    /// two important ways:
    ///
    /// 1. It helps us avoid Salsa cycles when creating types such as "instance of `str`"
    ///    and "instance of `sys._version_info`". These types are constructed very early
    ///    on, but it causes problems if we attempt to infer the types of their bases
    ///    too soon.
    /// 2. It's probably more performant.
    pub(crate) const fn is_protocol(self) -> bool {
        match self {
            Self::SupportsIndex
            | Self::Iterable
            | Self::Iterator
            | Self::AsyncIterator
            | Self::Awaitable
            | Self::NamedTupleLike
            | Self::AsyncGenerator
            | Self::Generator => true,

            Self::Bool
            | Self::Object
            | Self::Bytes
            | Self::Bytearray
            | Self::Tuple
            | Self::Int
            | Self::Float
            | Self::Complex
            | Self::FrozenSet
            | Self::Str
            | Self::Set
            | Self::Dict
            | Self::List
            | Self::Type
            | Self::Slice
            | Self::Property
            | Self::BaseException
            | Self::BaseExceptionGroup
            | Self::Exception
            | Self::NotImplementedError
            | Self::ExceptionGroup
            | Self::Staticmethod
            | Self::Classmethod
            | Self::Deprecated
            | Self::GenericAlias
            | Self::GeneratorType
            | Self::AsyncGeneratorType
            | Self::CoroutineType
            | Self::ModuleType
            | Self::FunctionType
            | Self::MethodType
            | Self::MethodWrapperType
            | Self::WrapperDescriptorType
            | Self::NoneType
            | Self::SpecialForm
            | Self::TypeVar
            | Self::ExtensionsTypeVar
            | Self::ParamSpec
            | Self::ExtensionsParamSpec
            | Self::ParamSpecArgs
            | Self::ParamSpecKwargs
            | Self::TypeVarTuple
            | Self::TypeAliasType
            | Self::NoDefaultType
            | Self::NewType
            | Self::ChainMap
            | Self::Counter
            | Self::DefaultDict
            | Self::Deque
            | Self::OrderedDict
            | Self::Enum
            | Self::EnumType
            | Self::Auto
            | Self::Member
            | Self::Nonmember
            | Self::StrEnum
            | Self::IntEnum
            | Self::Flag
            | Self::IntFlag
            | Self::ABCMeta
            | Self::Super
            | Self::StdlibAlias
            | Self::VersionInfo
            | Self::EllipsisType
            | Self::NotImplementedType
            | Self::UnionType
            | Self::Field
            | Self::KwOnly
            | Self::NamedTupleFallback
            | Self::ConstraintSet
            | Self::GenericContext
            | Self::Specialization
            | Self::TypedDictFallback
            | Self::BuiltinFunctionType
            | Self::ProtocolMeta
            | Self::Template
            | Self::Path
            | Self::Mapping
            | Self::MutableMapping
            | Self::Sequence => false,
        }
    }

    /// Return `true` if this class is a typeshed fallback class which is used to provide attributes and
    /// methods for another type (e.g. `NamedTupleFallback` for actual `NamedTuple`s). These fallback
    /// classes need special treatment in some places. For example, implicit usages of `Self` should not
    /// be eagerly replaced with the fallback class itself. Instead, `Self` should eventually be treated
    /// as referring to the destination type (e.g. the actual `NamedTuple`).
    pub(crate) const fn is_fallback_class(self) -> bool {
        match self {
            KnownClass::Bool
            | KnownClass::Object
            | KnownClass::Bytes
            | KnownClass::Bytearray
            | KnownClass::Type
            | KnownClass::Int
            | KnownClass::Float
            | KnownClass::Complex
            | KnownClass::Str
            | KnownClass::List
            | KnownClass::Tuple
            | KnownClass::Set
            | KnownClass::FrozenSet
            | KnownClass::Dict
            | KnownClass::Slice
            | KnownClass::Property
            | KnownClass::BaseException
            | KnownClass::Exception
            | KnownClass::NotImplementedError
            | KnownClass::BaseExceptionGroup
            | KnownClass::ExceptionGroup
            | KnownClass::Staticmethod
            | KnownClass::Classmethod
            | KnownClass::Super
            | KnownClass::Enum
            | KnownClass::EnumType
            | KnownClass::Auto
            | KnownClass::Member
            | KnownClass::Nonmember
            | KnownClass::StrEnum
            | KnownClass::IntEnum
            | KnownClass::Flag
            | KnownClass::IntFlag
            | KnownClass::ABCMeta
            | KnownClass::GenericAlias
            | KnownClass::ModuleType
            | KnownClass::FunctionType
            | KnownClass::MethodType
            | KnownClass::MethodWrapperType
            | KnownClass::WrapperDescriptorType
            | KnownClass::UnionType
            | KnownClass::GeneratorType
            | KnownClass::AsyncGeneratorType
            | KnownClass::CoroutineType
            | KnownClass::NotImplementedType
            | KnownClass::BuiltinFunctionType
            | KnownClass::EllipsisType
            | KnownClass::NoneType
            | KnownClass::Awaitable
            | KnownClass::Generator
            | KnownClass::AsyncGenerator
            | KnownClass::Deprecated
            | KnownClass::StdlibAlias
            | KnownClass::SpecialForm
            | KnownClass::TypeVar
            | KnownClass::ExtensionsTypeVar
            | KnownClass::ParamSpec
            | KnownClass::ExtensionsParamSpec
            | KnownClass::ParamSpecArgs
            | KnownClass::ParamSpecKwargs
            | KnownClass::ProtocolMeta
            | KnownClass::TypeVarTuple
            | KnownClass::TypeAliasType
            | KnownClass::NoDefaultType
            | KnownClass::NewType
            | KnownClass::SupportsIndex
            | KnownClass::Iterable
            | KnownClass::Iterator
            | KnownClass::AsyncIterator
            | KnownClass::Sequence
            | KnownClass::Mapping
            | KnownClass::MutableMapping
            | KnownClass::ChainMap
            | KnownClass::Counter
            | KnownClass::DefaultDict
            | KnownClass::Deque
            | KnownClass::OrderedDict
            | KnownClass::VersionInfo
            | KnownClass::Field
            | KnownClass::KwOnly
            | KnownClass::NamedTupleLike
            | KnownClass::Template
            | KnownClass::Path
            | KnownClass::ConstraintSet
            | KnownClass::GenericContext
            | KnownClass::Specialization => false,
            KnownClass::NamedTupleFallback | KnownClass::TypedDictFallback => true,
        }
    }

    pub(crate) fn name(self, db: &dyn Db) -> &'static str {
        match self {
            Self::Bool => "bool",
            Self::Object => "object",
            Self::Bytes => "bytes",
            Self::Bytearray => "bytearray",
            Self::Tuple => "tuple",
            Self::Int => "int",
            Self::Float => "float",
            Self::Complex => "complex",
            Self::FrozenSet => "frozenset",
            Self::Str => "str",
            Self::Set => "set",
            Self::Dict => "dict",
            Self::List => "list",
            Self::Type => "type",
            Self::Slice => "slice",
            Self::Property => "property",
            Self::BaseException => "BaseException",
            Self::BaseExceptionGroup => "BaseExceptionGroup",
            Self::Exception => "Exception",
            Self::NotImplementedError => "NotImplementedError",
            Self::ExceptionGroup => "ExceptionGroup",
            Self::Staticmethod => "staticmethod",
            Self::Classmethod => "classmethod",
            Self::Awaitable => "Awaitable",
            Self::Generator => "Generator",
            Self::AsyncGenerator => "AsyncGenerator",
            Self::Deprecated => "deprecated",
            Self::GenericAlias => "GenericAlias",
            Self::ModuleType => "ModuleType",
            Self::FunctionType => "FunctionType",
            Self::MethodType => "MethodType",
            Self::UnionType => "UnionType",
            Self::MethodWrapperType => "MethodWrapperType",
            Self::WrapperDescriptorType => "WrapperDescriptorType",
            Self::BuiltinFunctionType => "BuiltinFunctionType",
            Self::GeneratorType => "GeneratorType",
            Self::AsyncGeneratorType => "AsyncGeneratorType",
            Self::CoroutineType => "CoroutineType",
            Self::NoneType => "NoneType",
            Self::SpecialForm => "_SpecialForm",
            Self::TypeVar => "TypeVar",
            Self::ExtensionsTypeVar => "TypeVar",
            Self::ParamSpec => "ParamSpec",
            Self::ExtensionsParamSpec => "ParamSpec",
            Self::ParamSpecArgs => "ParamSpecArgs",
            Self::ParamSpecKwargs => "ParamSpecKwargs",
            Self::TypeVarTuple => "TypeVarTuple",
            Self::TypeAliasType => "TypeAliasType",
            Self::NoDefaultType => "_NoDefaultType",
            Self::NewType => "NewType",
            Self::SupportsIndex => "SupportsIndex",
            Self::ChainMap => "ChainMap",
            Self::Counter => "Counter",
            Self::DefaultDict => "defaultdict",
            Self::Deque => "deque",
            Self::OrderedDict => "OrderedDict",
            Self::Enum => "Enum",
            Self::EnumType => {
                if Program::get(db).python_version(db) >= PythonVersion::PY311 {
                    "EnumType"
                } else {
                    "EnumMeta"
                }
            }
            Self::Auto => "auto",
            Self::Member => "member",
            Self::Nonmember => "nonmember",
            Self::StrEnum => "StrEnum",
            Self::IntEnum => "IntEnum",
            Self::Flag => "Flag",
            Self::IntFlag => "IntFlag",
            Self::ABCMeta => "ABCMeta",
            Self::Super => "super",
            Self::Iterable => "Iterable",
            Self::Iterator => "Iterator",
            Self::AsyncIterator => "AsyncIterator",
            Self::Sequence => "Sequence",
            Self::Mapping => "Mapping",
            Self::MutableMapping => "MutableMapping",
            // For example, `typing.List` is defined as `List = _Alias()` in typeshed
            Self::StdlibAlias => "_Alias",
            // This is the name the type of `sys.version_info` has in typeshed,
            // which is different to what `type(sys.version_info).__name__` is at runtime.
            // (At runtime, `type(sys.version_info).__name__ == "version_info"`,
            // which is impossible to replicate in the stubs since the sole instance of the class
            // also has that name in the `sys` module.)
            Self::VersionInfo => "_version_info",
            Self::EllipsisType => {
                // Exposed as `types.EllipsisType` on Python >=3.10;
                // backported as `builtins.ellipsis` by typeshed on Python <=3.9
                if Program::get(db).python_version(db) >= PythonVersion::PY310 {
                    "EllipsisType"
                } else {
                    "ellipsis"
                }
            }
            Self::NotImplementedType => {
                // Exposed as `types.NotImplementedType` on Python >=3.10;
                // backported as `builtins._NotImplementedType` by typeshed on Python <=3.9
                if Program::get(db).python_version(db) >= PythonVersion::PY310 {
                    "NotImplementedType"
                } else {
                    "_NotImplementedType"
                }
            }
            Self::Field => "Field",
            Self::KwOnly => "KW_ONLY",
            Self::NamedTupleFallback => "NamedTupleFallback",
            Self::NamedTupleLike => "NamedTupleLike",
            Self::ConstraintSet => "ConstraintSet",
            Self::GenericContext => "GenericContext",
            Self::Specialization => "Specialization",
            Self::TypedDictFallback => "TypedDictFallback",
            Self::Template => "Template",
            Self::Path => "Path",
            Self::ProtocolMeta => "_ProtocolMeta",
        }
    }

    pub(crate) fn display(self, db: &dyn Db) -> impl std::fmt::Display + '_ {
        struct KnownClassDisplay<'db> {
            db: &'db dyn Db,
            class: KnownClass,
        }

        impl std::fmt::Display for KnownClassDisplay<'_> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                let KnownClassDisplay {
                    class: known_class,
                    db,
                } = *self;
                write!(
                    f,
                    "{module}.{class}",
                    module = known_class.canonical_module(db),
                    class = known_class.name(db)
                )
            }
        }

        KnownClassDisplay { db, class: self }
    }

    /// Lookup a [`KnownClass`] in typeshed and return a [`Type`] representing all possible instances of
    /// the class. If this class is generic, this will use the default specialization.
    ///
    /// If the class cannot be found in typeshed, a debug-level log message will be emitted stating this.
    #[track_caller]
    pub fn to_instance(self, db: &dyn Db) -> Type<'_> {
        debug_assert_ne!(
            self,
            KnownClass::Tuple,
            "Use `Type::heterogeneous_tuple` or `Type::homogeneous_tuple` to create `tuple` instances"
        );
        self.to_class_literal(db)
            .to_class_type(db)
            .map(|class| Type::instance(db, class))
            .unwrap_or_else(Type::unknown)
    }

    /// Similar to [`KnownClass::to_instance`], but returns the Unknown-specialization where each type
    /// parameter is specialized to `Unknown`.
    #[track_caller]
    pub(crate) fn to_instance_unknown(self, db: &dyn Db) -> Type<'_> {
        debug_assert_ne!(
            self,
            KnownClass::Tuple,
            "Use `Type::heterogeneous_tuple` or `Type::homogeneous_tuple` to create `tuple` instances"
        );
        self.try_to_class_literal(db)
            .map(|literal| Type::instance(db, literal.unknown_specialization(db)))
            .unwrap_or_else(Type::unknown)
    }

    /// Lookup a generic [`KnownClass`] in typeshed and return a [`Type`]
    /// representing a specialization of that class.
    ///
    /// If the class cannot be found in typeshed, or if you provide a specialization with the wrong
    /// number of types, a debug-level log message will be emitted stating this.
    pub(crate) fn to_specialized_class_type<'t, 'db, T>(
        self,
        db: &'db dyn Db,
        specialization: T,
    ) -> Option<ClassType<'db>>
    where
        T: Into<Cow<'t, [Type<'db>]>>,
        'db: 't,
    {
        fn to_specialized_class_type_impl<'db>(
            db: &'db dyn Db,
            class: KnownClass,
            class_literal: StaticClassLiteral<'db>,
            specialization: Cow<[Type<'db>]>,
            generic_context: GenericContext<'db>,
        ) -> ClassType<'db> {
            if specialization.len() != generic_context.len(db) {
                // a cache of the `KnownClass`es that we have already seen mismatched-arity
                // specializations for (and therefore that we've already logged a warning for)
                static MESSAGES: LazyLock<Mutex<FxHashSet<KnownClass>>> =
                    LazyLock::new(Mutex::default);
                if MESSAGES.lock().unwrap().insert(class) {
                    tracing::info!(
                        "Wrong number of types when specializing {}. \
                 Falling back to default specialization for the symbol instead.",
                        class.display(db)
                    );
                }
                return class_literal.default_specialization(db);
            }

            class_literal
                .apply_specialization(db, |_| generic_context.specialize(db, specialization))
        }

        let class_literal = self.to_class_literal(db).as_class_literal()?.as_static()?;
        let generic_context = class_literal.generic_context(db)?;
        let specialization = specialization.into();

        Some(to_specialized_class_type_impl(
            db,
            self,
            class_literal,
            specialization,
            generic_context,
        ))
    }

    /// Lookup a [`KnownClass`] in typeshed and return a [`Type`]
    /// representing all possible instances of the generic class with a specialization.
    ///
    /// If the class cannot be found in typeshed, or if you provide a specialization with the wrong
    /// number of types, a debug-level log message will be emitted stating this.
    #[track_caller]
    pub(crate) fn to_specialized_instance<'t, 'db, T>(
        self,
        db: &'db dyn Db,
        specialization: T,
    ) -> Type<'db>
    where
        T: Into<Cow<'t, [Type<'db>]>>,
        'db: 't,
    {
        debug_assert_ne!(
            self,
            KnownClass::Tuple,
            "Use `Type::heterogeneous_tuple` or `Type::homogeneous_tuple` to create `tuple` instances"
        );
        self.to_specialized_class_type(db, specialization)
            .and_then(|class_type| Type::from(class_type).to_instance(db))
            .unwrap_or_else(Type::unknown)
    }

    /// Attempt to lookup a [`KnownClass`] in typeshed and return a [`Type`] representing that class-literal.
    ///
    /// Return an error if the symbol cannot be found in the expected typeshed module,
    /// or if the symbol is not a class definition, or if the symbol is possibly unbound.
    fn try_to_class_literal_without_logging(
        self,
        db: &dyn Db,
    ) -> Result<StaticClassLiteral<'_>, KnownClassLookupError<'_>> {
        let symbol = known_module_symbol(db, self.canonical_module(db), self.name(db)).place;
        match symbol {
            Place::Defined(DefinedPlace {
                ty: Type::ClassLiteral(ClassLiteral::Static(class_literal)),
                definedness: Definedness::AlwaysDefined,
                ..
            }) => Ok(class_literal),
            Place::Defined(DefinedPlace {
                ty: Type::ClassLiteral(ClassLiteral::Static(class_literal)),
                definedness: Definedness::PossiblyUndefined,
                ..
            }) => Err(KnownClassLookupError::ClassPossiblyUnbound { class_literal }),
            Place::Defined(DefinedPlace { ty: found_type, .. }) => {
                Err(KnownClassLookupError::SymbolNotAClass { found_type })
            }
            Place::Undefined => Err(KnownClassLookupError::ClassNotFound),
        }
    }

    /// Lookup a [`KnownClass`] in typeshed and return a [`Type`] representing that class-literal.
    ///
    /// If the class cannot be found in typeshed, a debug-level log message will be emitted stating this.
    pub(crate) fn try_to_class_literal(self, db: &dyn Db) -> Option<StaticClassLiteral<'_>> {
        #[salsa::interned(heap_size=ruff_memory_usage::heap_size)]
        struct KnownClassArgument {
            class: KnownClass,
        }

        fn known_class_to_class_literal_initial<'db>(
            _db: &'db dyn Db,
            _id: salsa::Id,
            _class: KnownClassArgument<'db>,
        ) -> Option<StaticClassLiteral<'db>> {
            None
        }

        #[salsa::tracked(cycle_initial=known_class_to_class_literal_initial, heap_size=ruff_memory_usage::heap_size)]
        fn known_class_to_class_literal<'db>(
            db: &'db dyn Db,
            class: KnownClassArgument<'db>,
        ) -> Option<StaticClassLiteral<'db>> {
            let class = class.class(db);
            class
                .try_to_class_literal_without_logging(db)
                .or_else(|lookup_error| {
                    if matches!(
                        lookup_error,
                        KnownClassLookupError::ClassPossiblyUnbound { .. }
                    ) {
                        tracing::info!("{}", lookup_error.display(db, class));
                    } else {
                        tracing::info!(
                            "{}. Falling back to `Unknown` for the symbol instead.",
                            lookup_error.display(db, class)
                        );
                    }

                    match lookup_error {
                        KnownClassLookupError::ClassPossiblyUnbound { class_literal, .. } => {
                            Ok(class_literal)
                        }
                        KnownClassLookupError::ClassNotFound { .. }
                        | KnownClassLookupError::SymbolNotAClass { .. } => Err(()),
                    }
                })
                .ok()
        }

        known_class_to_class_literal(db, KnownClassArgument::new(db, self))
    }

    /// Lookup a [`KnownClass`] in typeshed and return a [`Type`] representing that class-literal.
    ///
    /// If the class cannot be found in typeshed, a debug-level log message will be emitted stating this.
    pub(crate) fn to_class_literal(self, db: &dyn Db) -> Type<'_> {
        self.try_to_class_literal(db)
            .map(|class| Type::ClassLiteral(ClassLiteral::Static(class)))
            .unwrap_or_else(Type::unknown)
    }

    /// Lookup a [`KnownClass`] in typeshed and return a [`Type`]
    /// representing that class and all possible subclasses of the class.
    ///
    /// If the class cannot be found in typeshed, a debug-level log message will be emitted stating this.
    pub fn to_subclass_of(self, db: &dyn Db) -> Type<'_> {
        self.to_class_literal(db)
            .to_class_type(db)
            .map(|class| SubclassOfType::from(db, class))
            .unwrap_or_else(SubclassOfType::subclass_of_unknown)
    }

    pub(crate) fn to_specialized_subclass_of<'db>(
        self,
        db: &'db dyn Db,
        specialization: &[Type<'db>],
    ) -> Type<'db> {
        self.to_specialized_class_type(db, specialization)
            .map(|class_type| SubclassOfType::from(db, class_type))
            .unwrap_or_else(SubclassOfType::subclass_of_unknown)
    }

    /// Return `true` if this symbol can be resolved to a class definition `class` in typeshed,
    /// *and* `class` is a subclass of `other`.
    pub(crate) fn is_subclass_of<'db>(self, db: &'db dyn Db, other: ClassType<'db>) -> bool {
        self.try_to_class_literal_without_logging(db)
            .is_ok_and(|class| class.is_subclass_of(db, None, other))
    }

    pub(crate) fn when_subclass_of<'db, 'c>(
        self,
        db: &'db dyn Db,
        other: ClassType<'db>,
        constraints: &'c ConstraintSetBuilder<'db>,
    ) -> ConstraintSet<'db, 'c> {
        ConstraintSet::from_bool(constraints, self.is_subclass_of(db, other))
    }

    /// Return the module in which we should look up the definition for this class
    pub(super) fn canonical_module(self, db: &dyn Db) -> KnownModule {
        match self {
            Self::Bool
            | Self::Object
            | Self::Bytes
            | Self::Bytearray
            | Self::Type
            | Self::Int
            | Self::Float
            | Self::Complex
            | Self::Str
            | Self::List
            | Self::Tuple
            | Self::Set
            | Self::FrozenSet
            | Self::Dict
            | Self::BaseException
            | Self::BaseExceptionGroup
            | Self::Exception
            | Self::NotImplementedError
            | Self::ExceptionGroup
            | Self::Staticmethod
            | Self::Classmethod
            | Self::Slice
            | Self::Super
            | Self::Property => KnownModule::Builtins,
            Self::VersionInfo => KnownModule::Sys,
            Self::ABCMeta => KnownModule::Abc,
            Self::Enum
            | Self::EnumType
            | Self::Auto
            | Self::Member
            | Self::Nonmember
            | Self::StrEnum
            | Self::IntEnum
            | Self::Flag
            | Self::IntFlag => KnownModule::Enum,
            Self::GenericAlias
            | Self::ModuleType
            | Self::FunctionType
            | Self::MethodType
            | Self::GeneratorType
            | Self::AsyncGeneratorType
            | Self::CoroutineType
            | Self::MethodWrapperType
            | Self::UnionType
            | Self::BuiltinFunctionType
            | Self::WrapperDescriptorType => KnownModule::Types,
            Self::NoneType => KnownModule::Typeshed,
            Self::Awaitable
            | Self::Generator
            | Self::AsyncGenerator
            | Self::SpecialForm
            | Self::TypeVar
            | Self::StdlibAlias
            | Self::Iterable
            | Self::Iterator
            | Self::AsyncIterator
            | Self::Sequence
            | Self::Mapping
            | Self::MutableMapping
            | Self::ProtocolMeta
            | Self::SupportsIndex => KnownModule::Typing,
            Self::TypeAliasType
            | Self::ExtensionsTypeVar
            | Self::TypeVarTuple
            | Self::ExtensionsParamSpec
            | Self::ParamSpecArgs
            | Self::ParamSpecKwargs
            | Self::Deprecated
            | Self::NewType => KnownModule::TypingExtensions,
            Self::ParamSpec => {
                if Program::get(db).python_version(db) >= PythonVersion::PY310 {
                    KnownModule::Typing
                } else {
                    KnownModule::TypingExtensions
                }
            }
            Self::NoDefaultType => {
                let python_version = Program::get(db).python_version(db);

                // typing_extensions has a 3.13+ re-export for the `typing.NoDefault`
                // singleton, but not for `typing._NoDefaultType`. So we need to switch
                // to `typing._NoDefaultType` for newer versions:
                if python_version >= PythonVersion::PY313 {
                    KnownModule::Typing
                } else {
                    KnownModule::TypingExtensions
                }
            }
            Self::EllipsisType => {
                // Exposed as `types.EllipsisType` on Python >=3.10;
                // backported as `builtins.ellipsis` by typeshed on Python <=3.9
                if Program::get(db).python_version(db) >= PythonVersion::PY310 {
                    KnownModule::Types
                } else {
                    KnownModule::Builtins
                }
            }
            Self::NotImplementedType => {
                // Exposed as `types.NotImplementedType` on Python >=3.10;
                // backported as `builtins._NotImplementedType` by typeshed on Python <=3.9
                if Program::get(db).python_version(db) >= PythonVersion::PY310 {
                    KnownModule::Types
                } else {
                    KnownModule::Builtins
                }
            }
            Self::ChainMap
            | Self::Counter
            | Self::DefaultDict
            | Self::Deque
            | Self::OrderedDict => KnownModule::Collections,
            Self::Field | Self::KwOnly => KnownModule::Dataclasses,
            Self::NamedTupleFallback | Self::TypedDictFallback => KnownModule::TypeCheckerInternals,
            Self::NamedTupleLike
            | Self::ConstraintSet
            | Self::GenericContext
            | Self::Specialization => KnownModule::TyExtensions,
            Self::Template => KnownModule::Templatelib,
            Self::Path => KnownModule::Pathlib,
        }
    }

    /// Returns `Some(true)` if all instances of this `KnownClass` compare equal.
    /// Returns `None` for `KnownClass::Tuple`, since whether or not a tuple type
    /// is single-valued depends on the tuple spec.
    pub(crate) const fn is_single_valued(self) -> Option<bool> {
        match self {
            Self::NoneType
            | Self::NoDefaultType
            | Self::VersionInfo
            | Self::EllipsisType
            | Self::TypeAliasType
            | Self::UnionType
            | Self::NotImplementedType => Some(true),

            Self::Bool
            | Self::Object
            | Self::Bytes
            | Self::Bytearray
            | Self::Type
            | Self::Int
            | Self::Float
            | Self::Complex
            | Self::Str
            | Self::List
            | Self::Set
            | Self::FrozenSet
            | Self::Dict
            | Self::Slice
            | Self::Property
            | Self::BaseException
            | Self::BaseExceptionGroup
            | Self::Exception
            | Self::NotImplementedError
            | Self::ExceptionGroup
            | Self::Staticmethod
            | Self::Classmethod
            | Self::Awaitable
            | Self::Generator
            | Self::AsyncGenerator
            | Self::Deprecated
            | Self::GenericAlias
            | Self::ModuleType
            | Self::FunctionType
            | Self::GeneratorType
            | Self::AsyncGeneratorType
            | Self::CoroutineType
            | Self::MethodType
            | Self::MethodWrapperType
            | Self::WrapperDescriptorType
            | Self::SpecialForm
            | Self::ChainMap
            | Self::Counter
            | Self::DefaultDict
            | Self::Deque
            | Self::OrderedDict
            | Self::SupportsIndex
            | Self::StdlibAlias
            | Self::TypeVar
            | Self::ExtensionsTypeVar
            | Self::ParamSpec
            | Self::ExtensionsParamSpec
            | Self::ParamSpecArgs
            | Self::ParamSpecKwargs
            | Self::TypeVarTuple
            | Self::Enum
            | Self::EnumType
            | Self::Auto
            | Self::Member
            | Self::Nonmember
            | Self::StrEnum
            | Self::IntEnum
            | Self::Flag
            | Self::IntFlag
            | Self::ABCMeta
            | Self::Super
            | Self::NewType
            | Self::Field
            | Self::KwOnly
            | Self::Iterable
            | Self::Iterator
            | Self::AsyncIterator
            | Self::Sequence
            | Self::Mapping
            | Self::MutableMapping
            | Self::NamedTupleFallback
            | Self::NamedTupleLike
            | Self::ConstraintSet
            | Self::GenericContext
            | Self::Specialization
            | Self::TypedDictFallback
            | Self::BuiltinFunctionType
            | Self::ProtocolMeta
            | Self::Template
            | Self::Path => Some(false),

            Self::Tuple => None,
        }
    }

    /// Is this class a singleton class?
    ///
    /// A singleton class is a class where it is known that only one instance can ever exist at runtime.
    pub(crate) const fn is_singleton(self) -> bool {
        match self {
            Self::NoneType
            | Self::EllipsisType
            | Self::NoDefaultType
            | Self::VersionInfo
            | Self::TypeAliasType
            | Self::NotImplementedType => true,

            Self::Bool
            | Self::Object
            | Self::Bytes
            | Self::Bytearray
            | Self::Tuple
            | Self::Int
            | Self::Float
            | Self::Complex
            | Self::Str
            | Self::Set
            | Self::FrozenSet
            | Self::Dict
            | Self::List
            | Self::Type
            | Self::Slice
            | Self::Property
            | Self::GenericAlias
            | Self::ModuleType
            | Self::FunctionType
            | Self::MethodType
            | Self::MethodWrapperType
            | Self::WrapperDescriptorType
            | Self::GeneratorType
            | Self::AsyncGeneratorType
            | Self::CoroutineType
            | Self::SpecialForm
            | Self::ChainMap
            | Self::Counter
            | Self::DefaultDict
            | Self::Deque
            | Self::OrderedDict
            | Self::StdlibAlias
            | Self::SupportsIndex
            | Self::BaseException
            | Self::BaseExceptionGroup
            | Self::Exception
            | Self::NotImplementedError
            | Self::ExceptionGroup
            | Self::Staticmethod
            | Self::Classmethod
            | Self::Awaitable
            | Self::Generator
            | Self::AsyncGenerator
            | Self::Deprecated
            | Self::TypeVar
            | Self::ExtensionsTypeVar
            | Self::ParamSpec
            | Self::ExtensionsParamSpec
            | Self::ParamSpecArgs
            | Self::ParamSpecKwargs
            | Self::TypeVarTuple
            | Self::Enum
            | Self::EnumType
            | Self::Auto
            | Self::Member
            | Self::Nonmember
            | Self::StrEnum
            | Self::IntEnum
            | Self::Flag
            | Self::IntFlag
            | Self::ABCMeta
            | Self::Super
            | Self::UnionType
            | Self::NewType
            | Self::Field
            | Self::KwOnly
            | Self::Iterable
            | Self::Iterator
            | Self::AsyncIterator
            | Self::Sequence
            | Self::Mapping
            | Self::MutableMapping
            | Self::NamedTupleFallback
            | Self::NamedTupleLike
            | Self::ConstraintSet
            | Self::GenericContext
            | Self::Specialization
            | Self::TypedDictFallback
            | Self::BuiltinFunctionType
            | Self::ProtocolMeta
            | Self::Template
            | Self::Path => false,
        }
    }

    pub(crate) fn try_from_file_and_name(
        db: &dyn Db,
        file: File,
        class_name: &str,
    ) -> Option<Self> {
        // We assert that this match is exhaustive over the right-hand side in the unit test
        // `known_class_roundtrip_from_str()`
        let candidates: &[Self] = match class_name {
            "bool" => &[Self::Bool],
            "object" => &[Self::Object],
            "bytes" => &[Self::Bytes],
            "bytearray" => &[Self::Bytearray],
            "tuple" => &[Self::Tuple],
            "type" => &[Self::Type],
            "int" => &[Self::Int],
            "float" => &[Self::Float],
            "complex" => &[Self::Complex],
            "str" => &[Self::Str],
            "set" => &[Self::Set],
            "frozenset" => &[Self::FrozenSet],
            "dict" => &[Self::Dict],
            "list" => &[Self::List],
            "slice" => &[Self::Slice],
            "property" => &[Self::Property],
            "BaseException" => &[Self::BaseException],
            "BaseExceptionGroup" => &[Self::BaseExceptionGroup],
            "Exception" => &[Self::Exception],
            "NotImplementedError" => &[Self::NotImplementedError],
            "ExceptionGroup" => &[Self::ExceptionGroup],
            "staticmethod" => &[Self::Staticmethod],
            "classmethod" => &[Self::Classmethod],
            "Awaitable" => &[Self::Awaitable],
            "Generator" => &[Self::Generator],
            "AsyncGenerator" => &[Self::AsyncGenerator],
            "deprecated" => &[Self::Deprecated],
            "GenericAlias" => &[Self::GenericAlias],
            "NoneType" => &[Self::NoneType],
            "ModuleType" => &[Self::ModuleType],
            "GeneratorType" => &[Self::GeneratorType],
            "AsyncGeneratorType" => &[Self::AsyncGeneratorType],
            "CoroutineType" => &[Self::CoroutineType],
            "FunctionType" => &[Self::FunctionType],
            "MethodType" => &[Self::MethodType],
            "UnionType" => &[Self::UnionType],
            "MethodWrapperType" => &[Self::MethodWrapperType],
            "WrapperDescriptorType" => &[Self::WrapperDescriptorType],
            "BuiltinFunctionType" => &[Self::BuiltinFunctionType],
            "NewType" => &[Self::NewType],
            "TypeAliasType" => &[Self::TypeAliasType],
            "TypeVar" => &[Self::TypeVar, Self::ExtensionsTypeVar],
            "Iterable" => &[Self::Iterable],
            "Iterator" => &[Self::Iterator],
            "AsyncIterator" => &[Self::AsyncIterator],
            "Sequence" => &[Self::Sequence],
            "Mapping" => &[Self::Mapping],
            "MutableMapping" => &[Self::MutableMapping],
            "ParamSpec" => &[Self::ParamSpec, Self::ExtensionsParamSpec],
            "ParamSpecArgs" => &[Self::ParamSpecArgs],
            "ParamSpecKwargs" => &[Self::ParamSpecKwargs],
            "TypeVarTuple" => &[Self::TypeVarTuple],
            "ChainMap" => &[Self::ChainMap],
            "Counter" => &[Self::Counter],
            "defaultdict" => &[Self::DefaultDict],
            "deque" => &[Self::Deque],
            "OrderedDict" => &[Self::OrderedDict],
            "_Alias" => &[Self::StdlibAlias],
            "_SpecialForm" => &[Self::SpecialForm],
            "_NoDefaultType" => &[Self::NoDefaultType],
            "SupportsIndex" => &[Self::SupportsIndex],
            "Enum" => &[Self::Enum],
            "EnumMeta" => &[Self::EnumType],
            "EnumType" if Program::get(db).python_version(db) >= PythonVersion::PY311 => {
                &[Self::EnumType]
            }
            "StrEnum" if Program::get(db).python_version(db) >= PythonVersion::PY311 => {
                &[Self::StrEnum]
            }
            "IntEnum" => &[Self::IntEnum],
            "Flag" => &[Self::Flag],
            "IntFlag" => &[Self::IntFlag],
            "auto" => &[Self::Auto],
            "member" => &[Self::Member],
            "nonmember" => &[Self::Nonmember],
            "ABCMeta" => &[Self::ABCMeta],
            "super" => &[Self::Super],
            "_version_info" => &[Self::VersionInfo],
            "ellipsis" if Program::get(db).python_version(db) <= PythonVersion::PY39 => {
                &[Self::EllipsisType]
            }
            "EllipsisType" if Program::get(db).python_version(db) >= PythonVersion::PY310 => {
                &[Self::EllipsisType]
            }
            "_NotImplementedType" if Program::get(db).python_version(db) <= PythonVersion::PY39 => {
                &[Self::NotImplementedType]
            }
            "NotImplementedType" if Program::get(db).python_version(db) >= PythonVersion::PY310 => {
                &[Self::NotImplementedType]
            }
            "Field" => &[Self::Field],
            "KW_ONLY" => &[Self::KwOnly],
            "NamedTupleFallback" => &[Self::NamedTupleFallback],
            "NamedTupleLike" => &[Self::NamedTupleLike],
            "ConstraintSet" => &[Self::ConstraintSet],
            "GenericContext" => &[Self::GenericContext],
            "Specialization" => &[Self::Specialization],
            "TypedDictFallback" => &[Self::TypedDictFallback],
            "Template" => &[Self::Template],
            "Path" => &[Self::Path],
            "_ProtocolMeta" => &[Self::ProtocolMeta],
            _ => return None,
        };

        let module = file_to_module(db, file)?.known(db)?;

        candidates
            .iter()
            .copied()
            .find(|&candidate| candidate.check_module(db, module))
    }

    /// Return `true` if the module of `self` matches `module`
    fn check_module(self, db: &dyn Db, module: KnownModule) -> bool {
        match self {
            Self::Bool
            | Self::Object
            | Self::Bytes
            | Self::Bytearray
            | Self::Type
            | Self::Int
            | Self::Float
            | Self::Complex
            | Self::Str
            | Self::List
            | Self::Tuple
            | Self::Set
            | Self::FrozenSet
            | Self::Dict
            | Self::Slice
            | Self::Property
            | Self::GenericAlias
            | Self::ChainMap
            | Self::Counter
            | Self::DefaultDict
            | Self::Deque
            | Self::OrderedDict
            | Self::StdlibAlias  // no equivalent class exists in typing_extensions, nor ever will
            | Self::ModuleType
            | Self::VersionInfo
            | Self::BaseException
            | Self::Exception
            | Self::NotImplementedError
            | Self::ExceptionGroup
            | Self::EllipsisType
            | Self::BaseExceptionGroup
            | Self::Staticmethod
            | Self::Classmethod
            | Self::FunctionType
            | Self::MethodType
            | Self::MethodWrapperType
            | Self::Enum
            | Self::EnumType
            | Self::Auto
            | Self::Member
            | Self::Nonmember
            | Self::StrEnum
            | Self::IntEnum
            | Self::Flag
            | Self::IntFlag
            | Self::ABCMeta
            | Self::Super
            | Self::NotImplementedType
            | Self::UnionType
            | Self::GeneratorType
            | Self::AsyncGeneratorType
            | Self::CoroutineType
            | Self::WrapperDescriptorType
            | Self::BuiltinFunctionType
            | Self::Field
            | Self::KwOnly
            | Self::NamedTupleFallback
            | Self::TypedDictFallback
            | Self::TypeVar
            | Self::ExtensionsTypeVar
            | Self::ParamSpec
            | Self::ExtensionsParamSpec
            | Self::NamedTupleLike
            | Self::ConstraintSet
            | Self::GenericContext
            | Self::Specialization
            | Self::Awaitable
            | Self::Generator
            | Self::AsyncGenerator
            | Self::Template
            | Self::Path => module == self.canonical_module(db),
            Self::NoneType => matches!(module, KnownModule::Typeshed | KnownModule::Types),
            Self::SpecialForm
            | Self::TypeAliasType
            | Self::NoDefaultType
            | Self::SupportsIndex
            | Self::ParamSpecArgs
            | Self::ParamSpecKwargs
            | Self::TypeVarTuple
            | Self::Iterable
            | Self::Iterator
            | Self::AsyncIterator
            | Self::Sequence
            | Self::Mapping
            | Self::MutableMapping
            | Self::ProtocolMeta
            | Self::NewType => matches!(module, KnownModule::Typing | KnownModule::TypingExtensions),
            Self::Deprecated => matches!(module, KnownModule::Warnings | KnownModule::TypingExtensions),
        }
    }

    /// Evaluate a call to this known class, emit any diagnostics that are necessary
    /// as a result of the call, and return the type that results from the call.
    pub(crate) fn check_call<'db>(
        self,
        context: &InferContext<'db, '_>,
        index: &SemanticIndex<'db>,
        overload: &mut Binding<'db>,
        call_expression: &ast::ExprCall,
    ) {
        let db = context.db();
        let scope = context.scope();
        let module = context.module();

        match self {
            KnownClass::Super => {
                // Handle the case where `super()` is called with no arguments.
                // In this case, we need to infer the two arguments:
                //   1. The nearest enclosing class
                //   2. The first parameter of the current function (typically `self` or `cls`)
                match overload.parameter_types() {
                    [] => {
                        let Some(enclosing_class) = nearest_enclosing_class(db, index, scope)
                        else {
                            BoundSuperError::UnavailableImplicitArguments
                                .report_diagnostic(context, call_expression.into());
                            overload.set_return_type(Type::unknown());
                            return;
                        };

                        // Check if the enclosing class is a `NamedTuple`, which forbids the use of `super()`.
                        if CodeGeneratorKind::NamedTuple.matches(db, enclosing_class.into(), None) {
                            if let Some(builder) = context
                                .report_lint(&SUPER_CALL_IN_NAMED_TUPLE_METHOD, call_expression)
                            {
                                builder.into_diagnostic(format_args!(
                                    "Cannot use `super()` in a method of NamedTuple class `{}`",
                                    enclosing_class.name(db)
                                ));
                            }
                            overload.set_return_type(Type::unknown());
                            return;
                        }

                        // The type of the first parameter if the given scope is function-like (i.e. function or lambda).
                        // `None` if the scope is not function-like, or has no parameters.
                        let first_param = match scope.node(db) {
                            NodeWithScopeKind::Function(f) => {
                                f.node(module).parameters.iter().next()
                            }
                            NodeWithScopeKind::Lambda(l) => l
                                .node(module)
                                .parameters
                                .as_ref()
                                .into_iter()
                                .flatten()
                                .next(),
                            _ => None,
                        };

                        let Some(first_param) = first_param else {
                            BoundSuperError::UnavailableImplicitArguments
                                .report_diagnostic(context, call_expression.into());
                            overload.set_return_type(Type::unknown());
                            return;
                        };

                        let definition = index.expect_single_definition(first_param);
                        let first_param = binding_type(db, definition);

                        let bound_super = BoundSuperType::build(
                            db,
                            Type::ClassLiteral(ClassLiteral::Static(enclosing_class)),
                            first_param,
                        )
                        .unwrap_or_else(|err| {
                            err.report_diagnostic(context, call_expression.into());
                            Type::unknown()
                        });

                        overload.set_return_type(bound_super);
                    }
                    [Some(pivot_class_type), Some(owner_type)] => {
                        // Check if the enclosing class is a `NamedTuple`, which forbids the use of `super()`.
                        if let Some(enclosing_class) = nearest_enclosing_class(db, index, scope) {
                            if CodeGeneratorKind::NamedTuple.matches(
                                db,
                                enclosing_class.into(),
                                None,
                            ) {
                                if let Some(builder) = context
                                    .report_lint(&SUPER_CALL_IN_NAMED_TUPLE_METHOD, call_expression)
                                {
                                    builder.into_diagnostic(format_args!(
                                        "Cannot use `super()` in a method of NamedTuple class `{}`",
                                        enclosing_class.name(db)
                                    ));
                                }
                                overload.set_return_type(Type::unknown());
                                return;
                            }
                        }

                        let bound_super = BoundSuperType::build(db, *pivot_class_type, *owner_type)
                            .unwrap_or_else(|err| {
                                err.report_diagnostic(context, call_expression.into());
                                Type::unknown()
                            });
                        overload.set_return_type(bound_super);
                    }
                    _ => {}
                }
            }

            KnownClass::Deprecated => {
                // Parsing something of the form:
                //
                // @deprecated("message")
                // @deprecated("message", category = DeprecationWarning, stacklevel = 1)
                //
                // "Static type checker behavior is not affected by the category and stacklevel arguments"
                // so we only need the message and can ignore everything else. The message is mandatory,
                // must be a LiteralString, and always comes first.
                //
                // We aren't guaranteed to know the static value of a LiteralString, so we need to
                // accept that sometimes we will fail to include the message.
                //
                // We don't do any serious validation/diagnostics here, as the signature for this
                // is included in `Type::bindings`.
                //
                // See: <https://typing.python.org/en/latest/spec/directives.html#deprecated>
                let [Some(message), ..] = overload.parameter_types() else {
                    // Checking in Type::bindings will complain about this for us
                    return;
                };

                overload.set_return_type(Type::KnownInstance(KnownInstanceType::Deprecated(
                    DeprecatedInstance {
                        message: message.as_string_literal(),
                    },
                )));
            }

            _ => {}
        }
    }
}

/// Enumeration of ways in which looking up a [`KnownClass`] in typeshed could fail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum KnownClassLookupError<'db> {
    /// There is no symbol by that name in the expected typeshed module.
    ClassNotFound,
    /// There is a symbol by that name in the expected typeshed module,
    /// but it's not a class.
    SymbolNotAClass { found_type: Type<'db> },
    /// There is a symbol by that name in the expected typeshed module,
    /// and it's a class definition, but it's possibly unbound.
    ClassPossiblyUnbound {
        class_literal: StaticClassLiteral<'db>,
    },
}

impl<'db> KnownClassLookupError<'db> {
    fn display(&self, db: &'db dyn Db, class: KnownClass) -> impl std::fmt::Display + 'db {
        struct ErrorDisplay<'db> {
            db: &'db dyn Db,
            class: KnownClass,
            error: KnownClassLookupError<'db>,
        }

        impl std::fmt::Display for ErrorDisplay<'_> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                let ErrorDisplay { db, class, error } = *self;

                let class = class.display(db);
                let python_version = Program::get(db).python_version(db);

                match error {
                    KnownClassLookupError::ClassNotFound => write!(
                        f,
                        "Could not find class `{class}` in typeshed on Python {python_version}",
                    ),
                    KnownClassLookupError::SymbolNotAClass { found_type } => write!(
                        f,
                        "Error looking up `{class}` in typeshed: expected to find a class definition \
                        on Python {python_version}, but found a symbol of type `{found_type}` instead",
                        found_type = found_type.display(db),
                    ),
                    KnownClassLookupError::ClassPossiblyUnbound { .. } => write!(
                        f,
                        "Error looking up `{class}` in typeshed on Python {python_version}: \
                        expected to find a fully bound symbol, but found one that is possibly unbound",
                    ),
                }
            }
        }

        ErrorDisplay {
            db,
            class,
            error: *self,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::tests::setup_db;
    use crate::{PythonVersionSource, PythonVersionWithSource};
    use salsa::Setter;
    use strum::IntoEnumIterator;
    use ty_module_resolver::resolve_module_confident;

    #[test]
    fn known_class_roundtrip_from_str() {
        let mut db = setup_db();
        Program::get(&db)
            .set_python_version_with_source(&mut db)
            .to(PythonVersionWithSource {
                version: PythonVersion::latest_preview(),
                source: PythonVersionSource::default(),
            });
        for class in KnownClass::iter() {
            let class_name = class.name(&db);
            let class_module =
                resolve_module_confident(&db, &class.canonical_module(&db).name()).unwrap();

            assert_eq!(
                KnownClass::try_from_file_and_name(
                    &db,
                    class_module.file(&db).unwrap(),
                    class_name
                ),
                Some(class),
                "`KnownClass::candidate_from_str` appears to be missing a case for `{class_name}`"
            );
        }
    }

    #[test]
    fn known_class_doesnt_fallback_to_unknown_unexpectedly_on_latest_version() {
        let mut db = setup_db();

        Program::get(&db)
            .set_python_version_with_source(&mut db)
            .to(PythonVersionWithSource {
                version: PythonVersion::latest_ty(),
                source: PythonVersionSource::default(),
            });

        for class in KnownClass::iter() {
            // Check the class can be looked up successfully
            class.try_to_class_literal_without_logging(&db).unwrap();

            // We can't call `KnownClass::Tuple.to_instance()`;
            // there are assertions to ensure that we always call `Type::homogeneous_tuple()`
            // or `Type::heterogeneous_tuple()` instead.`
            if class != KnownClass::Tuple {
                assert_ne!(
                    class.to_instance(&db),
                    Type::unknown(),
                    "Unexpectedly fell back to `Unknown` for `{class:?}`"
                );
            }
        }
    }

    #[test]
    fn known_class_doesnt_fallback_to_unknown_unexpectedly_on_low_python_version() {
        let mut db = setup_db();

        // First, collect the `KnownClass` variants
        // and sort them according to the version they were added in.
        // This makes the test far faster as it minimizes the number of times
        // we need to change the Python version in the loop.
        let mut classes: Vec<(KnownClass, PythonVersion)> = KnownClass::iter()
            .map(|class| {
                let version_added = match class {
                    KnownClass::Template => PythonVersion::PY314,
                    KnownClass::UnionType => PythonVersion::PY310,
                    KnownClass::BaseExceptionGroup | KnownClass::ExceptionGroup => {
                        PythonVersion::PY311
                    }
                    KnownClass::GenericAlias => PythonVersion::PY39,
                    KnownClass::KwOnly => PythonVersion::PY310,
                    KnownClass::Member | KnownClass::Nonmember | KnownClass::StrEnum => {
                        PythonVersion::PY311
                    }
                    KnownClass::ParamSpec => PythonVersion::PY310,
                    _ => PythonVersion::PY37,
                };
                (class, version_added)
            })
            .collect();

        classes.sort_unstable_by_key(|(_, version)| *version);

        let program = Program::get(&db);
        let mut current_version = program.python_version(&db);

        for (class, version_added) in classes {
            if version_added != current_version {
                program
                    .set_python_version_with_source(&mut db)
                    .to(PythonVersionWithSource {
                        version: version_added,
                        source: PythonVersionSource::default(),
                    });
                current_version = version_added;
            }

            // Check the class can be looked up successfully
            class.try_to_class_literal_without_logging(&db).unwrap();

            // We can't call `KnownClass::Tuple.to_instance()`;
            // there are assertions to ensure that we always call `Type::homogeneous_tuple()`
            // or `Type::heterogeneous_tuple()` instead.`
            if class != KnownClass::Tuple {
                assert_ne!(
                    class.to_instance(&db),
                    Type::unknown(),
                    "Unexpectedly fell back to `Unknown` for `{class:?}` on Python {version_added}"
                );
            }
        }
    }
}
