use std::str::FromStr;

use bitflags::bitflags;

use crate::Db;
use crate::module_resolver::{KnownModule, file_to_module};
use crate::semantic_index::definition::Definition;
use crate::types::narrow::ClassInfoConstraintFunction;

bitflags! {
    #[derive(Copy, Clone, Debug, Eq, PartialEq, Default, Hash)]
    pub struct FunctionDecorators: u8 {
        /// `@classmethod`
        const CLASSMETHOD = 1 << 0;
        /// `@typing.no_type_check`
        const NO_TYPE_CHECK = 1 << 1;
        /// `@typing.overload`
        const OVERLOAD = 1 << 2;
        /// `@abc.abstractmethod`
        const ABSTRACT_METHOD = 1 << 3;
        /// `@typing.final`
        const FINAL = 1 << 4;
        /// `@typing.override`
        const OVERRIDE = 1 << 6;
    }
}

bitflags! {
    /// Used for the return type of `dataclass_transform(…)` calls. Keeps track of the
    /// arguments that were passed in. For the precise meaning of the fields, see [1].
    ///
    /// [1]: https://docs.python.org/3/library/typing.html#typing.dataclass_transform
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, salsa::Update)]
    pub struct DataclassTransformerParams: u8 {
        const EQ_DEFAULT = 0b0000_0001;
        const ORDER_DEFAULT = 0b0000_0010;
        const KW_ONLY_DEFAULT = 0b0000_0100;
        const FROZEN_DEFAULT = 0b0000_1000;
    }
}

impl Default for DataclassTransformerParams {
    fn default() -> Self {
        Self::EQ_DEFAULT
    }
}

/// Non-exhaustive enumeration of known functions (e.g. `builtins.reveal_type`, ...) that might
/// have special behavior.
#[derive(
    Debug, Copy, Clone, PartialEq, Eq, Hash, strum_macros::EnumString, strum_macros::IntoStaticStr,
)]
#[strum(serialize_all = "snake_case")]
#[cfg_attr(test, derive(strum_macros::EnumIter))]
pub enum KnownFunction {
    /// `builtins.isinstance`
    #[strum(serialize = "isinstance")]
    IsInstance,
    /// `builtins.issubclass`
    #[strum(serialize = "issubclass")]
    IsSubclass,
    /// `builtins.hasattr`
    #[strum(serialize = "hasattr")]
    HasAttr,
    /// `builtins.reveal_type`, `typing.reveal_type` or `typing_extensions.reveal_type`
    RevealType,
    /// `builtins.len`
    Len,
    /// `builtins.repr`
    Repr,
    /// `typing(_extensions).final`
    Final,

    /// [`typing(_extensions).no_type_check`](https://typing.python.org/en/latest/spec/directives.html#no-type-check)
    NoTypeCheck,

    /// `typing(_extensions).assert_type`
    AssertType,
    /// `typing(_extensions).assert_never`
    AssertNever,
    /// `typing(_extensions).cast`
    Cast,
    /// `typing(_extensions).overload`
    Overload,
    /// `typing(_extensions).override`
    Override,
    /// `typing(_extensions).is_protocol`
    IsProtocol,
    /// `typing(_extensions).get_protocol_members`
    GetProtocolMembers,
    /// `typing(_extensions).runtime_checkable`
    RuntimeCheckable,
    /// `typing(_extensions).dataclass_transform`
    DataclassTransform,

    /// `abc.abstractmethod`
    #[strum(serialize = "abstractmethod")]
    AbstractMethod,

    /// `dataclasses.dataclass`
    Dataclass,

    /// `inspect.getattr_static`
    GetattrStatic,

    /// `ty_extensions.static_assert`
    StaticAssert,
    /// `ty_extensions.is_equivalent_to`
    IsEquivalentTo,
    /// `ty_extensions.is_subtype_of`
    IsSubtypeOf,
    /// `ty_extensions.is_assignable_to`
    IsAssignableTo,
    /// `ty_extensions.is_disjoint_from`
    IsDisjointFrom,
    /// `ty_extensions.is_gradual_equivalent_to`
    IsGradualEquivalentTo,
    /// `ty_extensions.is_fully_static`
    IsFullyStatic,
    /// `ty_extensions.is_singleton`
    IsSingleton,
    /// `ty_extensions.is_single_valued`
    IsSingleValued,
    /// `ty_extensions.generic_context`
    GenericContext,
    /// `ty_extensions.dunder_all_names`
    DunderAllNames,
}

impl KnownFunction {
    pub fn into_classinfo_constraint_function(self) -> Option<ClassInfoConstraintFunction> {
        match self {
            Self::IsInstance => Some(ClassInfoConstraintFunction::IsInstance),
            Self::IsSubclass => Some(ClassInfoConstraintFunction::IsSubclass),
            _ => None,
        }
    }

    pub(crate) fn try_from_definition_and_name<'db>(
        db: &'db dyn Db,
        definition: Definition<'db>,
        name: &str,
    ) -> Option<Self> {
        let candidate = Self::from_str(name).ok()?;
        candidate
            .check_module(file_to_module(db, definition.file(db))?.known()?)
            .then_some(candidate)
    }

    /// Return `true` if `self` is defined in `module` at runtime.
    const fn check_module(self, module: KnownModule) -> bool {
        match self {
            Self::IsInstance | Self::IsSubclass | Self::HasAttr | Self::Len | Self::Repr => {
                module.is_builtins()
            }
            Self::AssertType
            | Self::AssertNever
            | Self::Cast
            | Self::Overload
            | Self::Override
            | Self::RevealType
            | Self::Final
            | Self::IsProtocol
            | Self::GetProtocolMembers
            | Self::RuntimeCheckable
            | Self::DataclassTransform
            | Self::NoTypeCheck => {
                matches!(module, KnownModule::Typing | KnownModule::TypingExtensions)
            }
            Self::AbstractMethod => {
                matches!(module, KnownModule::Abc)
            }
            Self::Dataclass => {
                matches!(module, KnownModule::Dataclasses)
            }
            Self::GetattrStatic => module.is_inspect(),
            Self::IsAssignableTo
            | Self::IsDisjointFrom
            | Self::IsEquivalentTo
            | Self::IsGradualEquivalentTo
            | Self::IsFullyStatic
            | Self::IsSingleValued
            | Self::IsSingleton
            | Self::IsSubtypeOf
            | Self::GenericContext
            | Self::DunderAllNames
            | Self::StaticAssert => module.is_ty_extensions(),
        }
    }
}
