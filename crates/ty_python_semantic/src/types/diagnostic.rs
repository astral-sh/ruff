use super::call::CallErrorKind;
use super::context::InferContext;
use super::mro::DuplicateBaseError;
use super::{
    CallArguments, CallDunderError, ClassBase, ClassLiteral, GenericAlias, KnownClass,
    StaticClassLiteral, add_inferred_python_version_hint_to_diagnostic,
};
use crate::diagnostic::did_you_mean;
use crate::diagnostic::format_enumeration;
use crate::lint::{Level, LintRegistryBuilder, LintStatus};
use crate::place::{DefinedPlace, Place, place_from_bindings};
use crate::suppression::FileSuppressionId;
use crate::types::call::CallError;
use crate::types::class::{
    CodeGeneratorKind, DisjointBase, DisjointBaseKind, ExpandedClassBaseEntry, MethodDecorator,
};
use crate::types::function::{FunctionDecorators, FunctionType, KnownFunction, OverloadLiteral};
use crate::types::infer::UnsupportedComparisonError;
use crate::types::overrides::MethodKind;
use crate::types::protocol_class::ProtocolMember;
use crate::types::string_annotation::{
    ESCAPE_CHARACTER_IN_FORWARD_ANNOTATION, IMPLICIT_CONCATENATED_STRING_TYPE_ANNOTATION,
    INVALID_SYNTAX_IN_FORWARD_ANNOTATION, RAW_STRING_TYPE_ANNOTATION,
};
use crate::types::tuple::TupleSpec;
use crate::types::typed_dict::TypedDictSchema;
use crate::types::typevar::TypeVarInstance;
use crate::types::{
    BoundTypeVarInstance, ClassType, DynamicType, ErrorContextTree, LintDiagnosticGuard, Protocol,
    ProtocolInstanceType, SpecialFormType, SubclassOfInner, Type, TypeContext, TypeVarVariance,
    binding_type, protocol_class::ProtocolClass,
};
use crate::types::{KnownInstanceType, MemberLookupPolicy, TypeVarKind, TypedDictType, UnionType};
use crate::{Db, DisplaySettings, FxIndexMap, Program, declare_lint};
use itertools::Itertools;
use ruff_db::source::source_text;
use ruff_db::{
    diagnostic::{Annotation, Diagnostic, Span, SubDiagnostic, SubDiagnosticSeverity},
    parsed::parsed_module,
};
use ruff_diagnostics::{Edit, Fix, IsolationLevel};
use ruff_python_ast::name::Name;
use ruff_python_ast::token::parentheses_iterator;
use ruff_python_ast::{self as ast, AnyNodeRef, HasNodeIndex, StringFlags};
use ruff_source_file::LineRanges;
use ruff_text_size::{Ranged, TextRange};
use rustc_hash::{FxHashMap, FxHashSet};
use std::fmt::{self, Formatter};
use ty_module_resolver::{KnownModule, Module, ModuleName, file_to_module};
use ty_python_core::definition::{Definition, DefinitionKind};
use ty_python_core::place::{PlaceTable, ScopedPlaceId};
use ty_python_core::{global_scope, place_table, use_def_map};

const RUNTIME_CHECKABLE_DOCS_URL: &str =
    "https://docs.python.org/3/library/typing.html#typing.runtime_checkable";

/// Registers all known type check lints.
pub(crate) fn register_lints(registry: &mut LintRegistryBuilder) {
    registry.register_lint(&AMBIGUOUS_PROTOCOL_MEMBER);
    registry.register_lint(&CALL_NON_CALLABLE);
    registry.register_lint(&CALL_TOP_CALLABLE);
    registry.register_lint(&POSSIBLY_MISSING_IMPLICIT_CALL);
    registry.register_lint(&INVALID_DATACLASS_OVERRIDE);
    registry.register_lint(&INVALID_DATACLASS);
    registry.register_lint(&CONFLICTING_DECLARATIONS);
    registry.register_lint(&CONFLICTING_METACLASS);
    registry.register_lint(&CYCLIC_CLASS_DEFINITION);
    registry.register_lint(&CYCLIC_TYPE_ALIAS_DEFINITION);
    registry.register_lint(&DEPRECATED);
    registry.register_lint(&DIVISION_BY_ZERO);
    registry.register_lint(&DUPLICATE_BASE);
    registry.register_lint(&DUPLICATE_KW_ONLY);
    registry.register_lint(&DATACLASS_FIELD_ORDER);
    registry.register_lint(&EMPTY_BODY);
    registry.register_lint(&EXPERIMENTAL_SYNTAX);
    registry.register_lint(&INSTANCE_LAYOUT_CONFLICT);
    registry.register_lint(&INCONSISTENT_MRO);
    registry.register_lint(&INDEX_OUT_OF_BOUNDS);
    registry.register_lint(&INVALID_KEY);
    registry.register_lint(&ISINSTANCE_AGAINST_PROTOCOL);
    registry.register_lint(&ISINSTANCE_AGAINST_TYPED_DICT);
    registry.register_lint(&INVALID_ARGUMENT_TYPE);
    registry.register_lint(&INVALID_RETURN_TYPE);
    registry.register_lint(&INVALID_YIELD);
    registry.register_lint(&INVALID_ASSIGNMENT);
    registry.register_lint(&INVALID_AWAIT);
    registry.register_lint(&INVALID_BASE);
    registry.register_lint(&INVALID_CONTEXT_MANAGER);
    registry.register_lint(&INVALID_DECLARATION);
    registry.register_lint(&INVALID_EXCEPTION_CAUGHT);
    registry.register_lint(&INVALID_ENUM_MEMBER_ANNOTATION);
    registry.register_lint(&INVALID_GENERIC_ENUM);
    registry.register_lint(&INVALID_GENERIC_CLASS);
    registry.register_lint(&INVALID_LEGACY_TYPE_VARIABLE);
    registry.register_lint(&INVALID_PARAMSPEC);
    registry.register_lint(&INVALID_TYPE_ALIAS_TYPE);
    registry.register_lint(&INVALID_NEWTYPE);
    registry.register_lint(&MISMATCHED_TYPE_NAME);
    registry.register_lint(&INVALID_METACLASS);
    registry.register_lint(&INVALID_OVERLOAD);
    registry.register_lint(&USELESS_OVERLOAD_BODY);
    registry.register_lint(&INVALID_PARAMETER_DEFAULT);
    registry.register_lint(&INVALID_PROTOCOL);
    registry.register_lint(&INVALID_NAMED_TUPLE);
    registry.register_lint(&INVALID_NAMED_TUPLE_OVERRIDE);
    registry.register_lint(&INVALID_RAISE);
    registry.register_lint(&INVALID_SUPER_ARGUMENT);
    registry.register_lint(&INVALID_TYPE_ARGUMENTS);
    registry.register_lint(&INVALID_TYPE_CHECKING_CONSTANT);
    registry.register_lint(&INVALID_TYPE_FORM);
    registry.register_lint(&INVALID_MATCH_PATTERN);
    registry.register_lint(&INVALID_TYPE_GUARD_DEFINITION);
    registry.register_lint(&INVALID_TYPE_GUARD_CALL);
    registry.register_lint(&INVALID_TYPE_VARIABLE_CONSTRAINTS);
    registry.register_lint(&INVALID_TYPE_VARIABLE_BOUND);
    registry.register_lint(&INVALID_TYPE_VARIABLE_DEFAULT);
    registry.register_lint(&UNBOUND_TYPE_VARIABLE);
    registry.register_lint(&MISSING_ARGUMENT);
    registry.register_lint(&MISSING_TYPE_ARGUMENT);
    registry.register_lint(&NO_MATCHING_OVERLOAD);
    registry.register_lint(&NON_CALLABLE_INIT_SUBCLASS);
    registry.register_lint(&NOT_SUBSCRIPTABLE);
    registry.register_lint(&NOT_ITERABLE);
    registry.register_lint(&UNSUPPORTED_BOOL_CONVERSION);
    registry.register_lint(&PARAMETER_ALREADY_ASSIGNED);
    registry.register_lint(&POSSIBLY_MISSING_ATTRIBUTE);
    registry.register_lint(&POSSIBLY_MISSING_SUBMODULE);
    registry.register_lint(&POSSIBLY_MISSING_IMPORT);
    registry.register_lint(&POSSIBLY_UNRESOLVED_REFERENCE);
    registry.register_lint(&SHADOWED_TYPE_VARIABLE);
    registry.register_lint(&SUBCLASS_OF_FINAL_CLASS);
    registry.register_lint(&OVERRIDE_OF_FINAL_METHOD);
    registry.register_lint(&OVERRIDE_OF_FINAL_VARIABLE);
    registry.register_lint(&INEFFECTIVE_FINAL);
    registry.register_lint(&FINAL_ON_NON_METHOD);
    registry.register_lint(&FINAL_WITHOUT_VALUE);
    registry.register_lint(&ABSTRACT_METHOD_IN_FINAL_CLASS);
    registry.register_lint(&CALL_ABSTRACT_METHOD);
    registry.register_lint(&TYPE_ASSERTION_FAILURE);
    registry.register_lint(&ASSERT_TYPE_UNSPELLABLE_SUBTYPE);
    registry.register_lint(&TOO_MANY_POSITIONAL_ARGUMENTS);
    registry.register_lint(&UNAVAILABLE_IMPLICIT_SUPER_ARGUMENTS);
    registry.register_lint(&UNDEFINED_REVEAL);
    registry.register_lint(&UNKNOWN_ARGUMENT);
    registry.register_lint(&PYDANTIC_DISCARDED_EXTRA_ARGUMENT);
    registry.register_lint(&POSITIONAL_ONLY_PARAMETER_AS_KWARG);
    registry.register_lint(&UNRESOLVED_ATTRIBUTE);
    registry.register_lint(&UNRESOLVED_IMPORT);
    registry.register_lint(&UNRESOLVED_REFERENCE);
    registry.register_lint(&UNSUPPORTED_BASE);
    registry.register_lint(&UNSUPPORTED_DYNAMIC_BASE);
    registry.register_lint(&UNSUPPORTED_OPERATOR);
    registry.register_lint(&UNUSED_AWAITABLE);
    registry.register_lint(&ZERO_STEPSIZE_IN_SLICE);
    registry.register_lint(&STATIC_ASSERT_ERROR);
    registry.register_lint(&INVALID_ATTRIBUTE_ACCESS);
    registry.register_lint(&REDUNDANT_CAST);
    registry.register_lint(&REDUNDANT_FINAL_CLASSVAR);
    registry.register_lint(&UNRESOLVED_GLOBAL);
    registry.register_lint(&MISSING_TYPED_DICT_KEY);
    registry.register_lint(&INVALID_TYPED_DICT_STATEMENT);
    registry.register_lint(&INVALID_TYPED_DICT_FIELD);
    registry.register_lint(&INVALID_TYPED_DICT_HEADER);
    registry.register_lint(&INVALID_ATTRIBUTE_OVERRIDE);
    registry.register_lint(&INVALID_METHOD_OVERRIDE);
    registry.register_lint(&INVALID_EXPLICIT_OVERRIDE);
    registry.register_lint(&MISSING_OVERRIDE_DECORATOR);
    registry.register_lint(&SUPER_CALL_IN_NAMED_TUPLE_METHOD);
    registry.register_lint(&SUBCLASS_OF_DATACLASS_WITH_ORDER);
    registry.register_lint(&INVALID_FROZEN_DATACLASS_SUBCLASS);
    registry.register_lint(&INVALID_TOTAL_ORDERING);
    registry.register_lint(&INVALID_LEGACY_POSITIONAL_PARAMETER);

    // String annotations
    registry.register_lint(&ESCAPE_CHARACTER_IN_FORWARD_ANNOTATION);
    registry.register_lint(&IMPLICIT_CONCATENATED_STRING_TYPE_ANNOTATION);
    registry.register_lint(&INVALID_SYNTAX_IN_FORWARD_ANNOTATION);
    registry.register_lint(&RAW_STRING_TYPE_ANNOTATION);
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/experimental-syntax.md")]
    pub(crate) static EXPERIMENTAL_SYNTAX = {
        summary: "detects experimental syntax",
        status: LintStatus::stable("0.0.50"),
        default_level: Level::Warn,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/call-non-callable.md")]
    pub(crate) static CALL_NON_CALLABLE = {
        summary: "detects calls to non-callable objects",
        status: LintStatus::stable("0.0.1-alpha.1"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/call-top-callable.md")]
    pub(crate) static CALL_TOP_CALLABLE = {
        summary: "detects calls to the top callable type",
        status: LintStatus::stable("0.0.7"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/possibly-missing-implicit-call.md")]
    pub(crate) static POSSIBLY_MISSING_IMPLICIT_CALL = {
        summary: "detects implicit calls to possibly missing methods",
        status: LintStatus::stable("0.0.1-alpha.22"),
        default_level: Level::Warn,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/conflicting-declarations.md")]
    pub(crate) static CONFLICTING_DECLARATIONS = {
        summary: "detects conflicting declarations",
        status: LintStatus::stable("0.0.1-alpha.1"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/conflicting-metaclass.md")]
    pub(crate) static CONFLICTING_METACLASS = {
        summary: "detects conflicting metaclasses",
        status: LintStatus::stable("0.0.1-alpha.1"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/cyclic-class-definition.md")]
    pub(crate) static CYCLIC_CLASS_DEFINITION = {
        summary: "detects cyclic class definitions",
        status: LintStatus::stable("0.0.1-alpha.1"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/cyclic-type-alias-definition.md")]
    pub(crate) static CYCLIC_TYPE_ALIAS_DEFINITION = {
        summary: "detects cyclic type alias definitions",
        status: LintStatus::stable("0.0.1-alpha.29"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/division-by-zero.md")]
    pub(crate) static DIVISION_BY_ZERO = {
        summary: "detects division by zero",
        status: LintStatus::stable("0.0.1-alpha.1"),
        default_level: Level::Ignore,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/deprecated.md")]
    pub(crate) static DEPRECATED = {
        summary: "detects uses of deprecated items",
        status: LintStatus::stable("0.0.1-alpha.16"),
        default_level: Level::Warn,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/duplicate-base.md")]
    pub(crate) static DUPLICATE_BASE = {
        summary: "detects class definitions with duplicate bases",
        status: LintStatus::stable("0.0.1-alpha.1"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/duplicate-kw-only.md")]
    pub(crate) static DUPLICATE_KW_ONLY = {
        summary: "detects dataclass definitions with more than one usage of `KW_ONLY`",
        status: LintStatus::stable("0.0.1-alpha.12"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/dataclass-field-order.md")]
    pub(crate) static DATACLASS_FIELD_ORDER = {
        summary: "detects dataclass definitions with required fields after fields with default values",
        status: LintStatus::stable("0.0.15"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/invalid-dataclass-override.md")]
    pub(crate) static INVALID_DATACLASS_OVERRIDE = {
        summary: "detects dataclasses with `frozen=True` that have a custom `__setattr__` or `__delattr__` implementation",
        status: LintStatus::stable("0.0.13"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[expect(clippy::doc_overindented_list_items)]
    #[doc = include_str!("../../resources/lint_docs/invalid-dataclass.md")]
    pub(crate) static INVALID_DATACLASS = {
        summary: "detects invalid `@dataclass` applications",
        status: LintStatus::stable("0.0.12"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/instance-layout-conflict.md")]
    pub(crate) static INSTANCE_LAYOUT_CONFLICT = {
        summary: "detects class definitions that raise `TypeError` due to instance layout conflict",
        status: LintStatus::stable("0.0.1-alpha.12"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/invalid-protocol.md")]
    pub(crate) static INVALID_PROTOCOL = {
        summary: "detects invalid protocol class definitions",
        status: LintStatus::stable("0.0.1-alpha.1"),
        default_level: Level::Error,
    }
}

// Added in #17750.
declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/ambiguous-protocol-member.md")]
    pub(crate) static AMBIGUOUS_PROTOCOL_MEMBER = {
        summary: "detects protocol classes with ambiguous interfaces",
        status: LintStatus::stable("0.0.1-alpha.20"),
        default_level: Level::Warn,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/invalid-named-tuple.md")]
    pub(crate) static INVALID_NAMED_TUPLE = {
        summary: "detects invalid `NamedTuple` class definitions",
        status: LintStatus::stable("0.0.1-alpha.19"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/invalid-named-tuple-override.md")]
    pub(crate) static INVALID_NAMED_TUPLE_OVERRIDE = {
        summary: "detects subclass members that override inherited `NamedTuple` fields",
        status: LintStatus::stable("0.0.31"),
        default_level: Level::Warn,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/inconsistent-mro.md")]
    pub(crate) static INCONSISTENT_MRO = {
        summary: "detects class definitions with an inconsistent MRO",
        status: LintStatus::stable("0.0.1-alpha.1"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/index-out-of-bounds.md")]
    pub(crate) static INDEX_OUT_OF_BOUNDS = {
        summary: "detects index out of bounds errors",
        status: LintStatus::stable("0.0.1-alpha.1"),
        default_level: Level::Error,
    }
}

// Added in #19763.
declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/invalid-key.md")]
    pub(crate) static INVALID_KEY = {
        summary: "detects invalid subscript accesses or TypedDict literal keys",
        status: LintStatus::stable("0.0.1-alpha.17"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/isinstance-against-protocol.md")]
    pub(crate) static ISINSTANCE_AGAINST_PROTOCOL = {
        summary: "reports invalid runtime checks against protocol classes",
        status: LintStatus::stable("0.0.14"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/isinstance-against-typed-dict.md")]
    pub(crate) static ISINSTANCE_AGAINST_TYPED_DICT = {
        summary: "reports runtime checks against `TypedDict` classes",
        status: LintStatus::stable("0.0.15"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/invalid-argument-type.md")]
    pub(crate) static INVALID_ARGUMENT_TYPE = {
        summary: "detects call arguments whose type is not assignable to the corresponding typed parameter",
        status: LintStatus::stable("0.0.1-alpha.1"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/invalid-return-type.md")]
    pub(crate) static INVALID_RETURN_TYPE = {
        summary: "detects returned values that can't be assigned to the function's annotated return type",
        status: LintStatus::stable("0.0.1-alpha.1"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/invalid-yield.md")]
    pub(crate) static INVALID_YIELD = {
        summary: "detects yield expressions where the \"yield\" or \"send\" type is incompatible with the annotated return type",
        status: LintStatus::stable("0.0.25"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/empty-body.md")]
    pub(crate) static EMPTY_BODY = {
        summary: "detects functions with empty bodies that have a non-`None` return type annotation",
        status: LintStatus::stable("0.0.14"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/invalid-assignment.md")]
    pub(crate) static INVALID_ASSIGNMENT = {
        summary: "detects invalid assignments",
        status: LintStatus::stable("0.0.1-alpha.1"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/invalid-await.md")]
    pub(crate) static INVALID_AWAIT = {
        summary: "detects awaiting on types that don't support it",
        status: LintStatus::stable("0.0.1-alpha.19"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/invalid-base.md")]
    pub(crate) static INVALID_BASE = {
        summary: "detects class bases that will cause the class definition to raise an exception at runtime",
        status: LintStatus::stable("0.0.1-alpha.1"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/unsupported-base.md")]
    pub(crate) static UNSUPPORTED_BASE = {
        summary: "detects class bases that are unsupported as ty could not feasibly calculate the class's MRO",
        status: LintStatus::stable("0.0.1-alpha.7"),
        default_level: Level::Warn,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/unsupported-dynamic-base.md")]
    pub(crate) static UNSUPPORTED_DYNAMIC_BASE = {
        summary: "detects dynamic class bases that are unsupported as ty could not feasibly calculate the class's MRO",
        status: LintStatus::stable("0.0.12"),
        default_level: Level::Ignore,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/invalid-context-manager.md")]
    pub(crate) static INVALID_CONTEXT_MANAGER = {
        summary: "detects expressions used in with statements that don't implement the context manager protocol",
        status: LintStatus::stable("0.0.1-alpha.1"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/invalid-declaration.md")]
    pub(crate) static INVALID_DECLARATION = {
        summary: "detects invalid declarations",
        status: LintStatus::stable("0.0.1-alpha.1"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/invalid-exception-caught.md")]
    pub(crate) static INVALID_EXCEPTION_CAUGHT = {
        summary: "detects exception handlers that catch classes that do not inherit from `BaseException`",
        status: LintStatus::stable("0.0.1-alpha.1"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/invalid-enum-member-annotation.md")]
    pub(crate) static INVALID_ENUM_MEMBER_ANNOTATION = {
        summary: "detects type annotations on enum members",
        status: LintStatus::stable("0.0.20"),
        default_level: Level::Warn,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/invalid-generic-enum.md")]
    pub(crate) static INVALID_GENERIC_ENUM = {
        summary: "detects generic enum classes",
        status: LintStatus::stable("0.0.12"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/invalid-generic-class.md")]
    pub(crate) static INVALID_GENERIC_CLASS = {
        summary: "detects invalid generic classes",
        status: LintStatus::stable("0.0.1-alpha.1"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/non-callable-init-subclass.md")]
    pub(crate) static NON_CALLABLE_INIT_SUBCLASS = {
        summary: "detects class definitions that will fail due to non-callable `__init_subclass__`",
        status: LintStatus::stable("0.0.30"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/invalid-legacy-type-variable.md")]
    pub(crate) static INVALID_LEGACY_TYPE_VARIABLE = {
        summary: "detects invalid legacy type variables",
        status: LintStatus::stable("0.0.1-alpha.1"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/invalid-paramspec.md")]
    pub(crate) static INVALID_PARAMSPEC = {
        summary: "detects invalid ParamSpec usage",
        status: LintStatus::stable("0.0.1-alpha.1"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/invalid-type-alias-type.md")]
    pub(crate) static INVALID_TYPE_ALIAS_TYPE = {
        summary: "detects invalid TypeAliasType definitions",
        status: LintStatus::stable("0.0.1-alpha.6"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/invalid-newtype.md")]
    pub(crate) static INVALID_NEWTYPE = {
        summary: "detects invalid NewType definitions",
        status: LintStatus::stable("0.0.1-alpha.27"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/mismatched-type-name.md")]
    pub(crate) static MISMATCHED_TYPE_NAME = {
        summary: "detects functional typing definitions whose declared name does not match the assigned variable",
        status: LintStatus::stable("0.0.30"),
        default_level: Level::Warn,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/invalid-metaclass.md")]
    pub(crate) static INVALID_METACLASS = {
        summary: "detects invalid `metaclass=` arguments",
        status: LintStatus::stable("0.0.1-alpha.1"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/invalid-overload.md")]
    pub(crate) static INVALID_OVERLOAD = {
        summary: "detects invalid `@overload` usages",
        status: LintStatus::stable("0.0.1-alpha.1"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/useless-overload-body.md")]
    pub(crate) static USELESS_OVERLOAD_BODY = {
        summary: "detects `@overload`-decorated functions with non-stub bodies",
        status: LintStatus::stable("0.0.1-alpha.22"),
        default_level: Level::Warn,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/invalid-parameter-default.md")]
    pub(crate) static INVALID_PARAMETER_DEFAULT = {
        summary: "detects default values that can't be assigned to the parameter's annotated type",
        status: LintStatus::stable("0.0.1-alpha.1"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/invalid-raise.md")]
    pub(crate) static INVALID_RAISE = {
        summary: "detects `raise` statements that raise invalid exceptions or use invalid causes",
        status: LintStatus::stable("0.0.1-alpha.1"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/invalid-super-argument.md")]
    pub(crate) static INVALID_SUPER_ARGUMENT = {
        summary: "detects invalid arguments for `super()`",
        status: LintStatus::stable("0.0.1-alpha.1"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/invalid-type-checking-constant.md")]
    pub(crate) static INVALID_TYPE_CHECKING_CONSTANT = {
        summary: "detects invalid `TYPE_CHECKING` constant assignments",
        status: LintStatus::stable("0.0.1-alpha.1"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/invalid-type-form.md")]
    pub(crate) static INVALID_TYPE_FORM = {
        summary: "detects invalid type forms",
        status: LintStatus::stable("0.0.1-alpha.1"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/invalid-match-pattern.md")]
    pub(crate) static INVALID_MATCH_PATTERN = {
        summary: "detect invalid match patterns",
        status: LintStatus::stable("0.0.18"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/invalid-type-guard-definition.md")]
    pub(crate) static INVALID_TYPE_GUARD_DEFINITION = {
        summary: "detects malformed type guard functions",
        status: LintStatus::stable("0.0.1-alpha.11"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// Type guard calls without a narrowing target are valid and have no narrowing effect.
    pub(crate) static INVALID_TYPE_GUARD_CALL = {
        summary: "detects type guard function calls that have no narrowing effect",
        status: LintStatus::removed(
            "0.0.60",
            "Type guard calls without a narrowing target are valid and have no narrowing effect.",
        ),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/invalid-type-variable-constraints.md")]
    pub(crate) static INVALID_TYPE_VARIABLE_CONSTRAINTS = {
        summary: "detects invalid type variable constraints",
        status: LintStatus::stable("0.0.1-alpha.1"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/invalid-type-variable-bound.md")]
    pub(crate) static INVALID_TYPE_VARIABLE_BOUND = {
        summary: "detects invalid type variable bounds",
        status: LintStatus::stable("0.0.15"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/invalid-type-variable-default.md")]
    pub(crate) static INVALID_TYPE_VARIABLE_DEFAULT = {
        summary: "detects invalid type variable defaults",
        status: LintStatus::stable("0.0.16"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/unbound-type-variable.md")]
    pub(crate) static UNBOUND_TYPE_VARIABLE = {
        summary: "detects type variables used outside of their bound scope",
        status: LintStatus::stable("0.0.20"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/missing-argument.md")]
    pub(crate) static MISSING_ARGUMENT = {
        summary: "detects missing required arguments in a call",
        status: LintStatus::stable("0.0.1-alpha.1"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/missing-type-argument.md")]
    pub(crate) static MISSING_TYPE_ARGUMENT = {
        summary: "detects generic types used without explicit type parameters in type expressions",
        status: LintStatus::stable("0.0.45"),
        default_level: Level::Ignore,
    }
}

pub(super) fn report_missing_type_arguments<'db>(
    context: &InferContext<'db, '_>,
    ty: Type<'db>,
    annotation: &ast::Expr,
) {
    match ty {
        Type::ClassLiteral(class) => {
            let db = context.db();

            let Some(generic_context) = class.generic_context(db) else {
                return;
            };

            // Don't warn if all type parameters have defaults (PEP 696).
            if generic_context
                .variables(db)
                .all(|tv| tv.default_type(db).is_some())
            {
                return;
            }

            let required_count = generic_context
                .variables(db)
                .filter(|tv| tv.default_type(db).is_none())
                .count();

            if let Some(builder) = context.report_lint(&MISSING_TYPE_ARGUMENT, annotation) {
                let class_name = class.name(db);
                if required_count == 1 {
                    builder.into_diagnostic(format_args!(
                        "Missing type argument for generic class `{class_name}` \
                         (expected 1 type argument)"
                    ));
                } else {
                    builder.into_diagnostic(format_args!(
                        "Missing type arguments for generic class `{class_name}` \
                         (expected {required_count} type arguments)"
                    ));
                }
            }
        }
        Type::SpecialForm(
            SpecialFormType::TypingCallable | SpecialFormType::CollectionsAbcCallable,
        ) => {
            if let Some(builder) = context.report_lint(&MISSING_TYPE_ARGUMENT, annotation) {
                builder.into_diagnostic(format_args!(
                    "Missing type arguments for generic type `Callable` \
                     (expected 2 type arguments)"
                ));
            }
        }
        _ => {}
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/no-matching-overload.md")]
    pub(crate) static NO_MATCHING_OVERLOAD = {
        summary: "detects calls that do not match any overload",
        status: LintStatus::stable("0.0.1-alpha.1"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/not-subscriptable.md")]
    pub(crate) static NOT_SUBSCRIPTABLE = {
        summary: "detects subscripting objects that do not support subscripting",
        status: LintStatus::stable("0.0.1-alpha.1"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/invalid-type-arguments.md")]
    pub(crate) static INVALID_TYPE_ARGUMENTS = {
        summary: "detects invalid type arguments in generic specialization",
        status: LintStatus::stable("0.0.1-alpha.29"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/not-iterable.md")]
    pub(crate) static NOT_ITERABLE = {
        summary: "detects iteration over an object that is not iterable",
        status: LintStatus::stable("0.0.1-alpha.1"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/unsupported-bool-conversion.md")]
    pub(crate) static UNSUPPORTED_BOOL_CONVERSION = {
        summary: "detects boolean conversion where the object incorrectly implements `__bool__`",
        status: LintStatus::stable("0.0.1-alpha.1"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/parameter-already-assigned.md")]
    pub(crate) static PARAMETER_ALREADY_ASSIGNED = {
        summary: "detects multiple arguments for the same parameter",
        status: LintStatus::stable("0.0.1-alpha.1"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/possibly-missing-attribute.md")]
    pub(crate) static POSSIBLY_MISSING_ATTRIBUTE = {
        summary: "detects references to possibly missing attributes",
        status: LintStatus::stable("0.0.1-alpha.22"),
        default_level: Level::Ignore,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/possibly-missing-submodule.md")]
    pub(crate) static POSSIBLY_MISSING_SUBMODULE = {
        summary: "detects accesses of submodules that may not be available as attributes on their parent module",
        status: LintStatus::stable("0.0.23"),
        default_level: Level::Warn,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/possibly-missing-import.md")]
    pub(crate) static POSSIBLY_MISSING_IMPORT = {
        summary: "detects possibly missing imports",
        status: LintStatus::stable("0.0.1-alpha.22"),
        default_level: Level::Ignore,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/possibly-unresolved-reference.md")]
    pub(crate) static POSSIBLY_UNRESOLVED_REFERENCE = {
        summary: "detects references to possibly undefined names",
        status: LintStatus::stable("0.0.1-alpha.1"),
        default_level: Level::Ignore,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/subclass-of-final-class.md")]
    pub(crate) static SUBCLASS_OF_FINAL_CLASS = {
        summary: "detects subclasses of final classes",
        status: LintStatus::stable("0.0.1-alpha.1"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/subclass-of-dataclass-with-order.md")]
    pub(crate) static SUBCLASS_OF_DATACLASS_WITH_ORDER = {
        summary: "detects subclasses of dataclasses with `order=True`",
        status: LintStatus::stable("0.0.39"),
        default_level: Level::Warn,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/override-of-final-method.md")]
    pub(crate) static OVERRIDE_OF_FINAL_METHOD = {
        summary: "detects overrides of final methods",
        status: LintStatus::stable("0.0.1-alpha.29"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/override-of-final-variable.md")]
    pub(crate) static OVERRIDE_OF_FINAL_VARIABLE = {
        summary: "detects overrides of Final class variables",
        status: LintStatus::stable("0.0.16"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/ineffective-final.md")]
    pub(crate) static INEFFECTIVE_FINAL = {
        summary: "detects calls to `final()` that type checkers cannot interpret",
        status: LintStatus::stable("0.0.1-alpha.33"),
        default_level: Level::Warn,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/final-on-non-method.md")]
    pub(crate) static FINAL_ON_NON_METHOD = {
        summary: "detects `@final` applied to non-method functions",
        status: LintStatus::stable("0.0.20"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/final-without-value.md")]
    pub(crate) static FINAL_WITHOUT_VALUE = {
        summary: "detects `Final` declarations without a value",
        status: LintStatus::stable("0.0.15"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/abstract-method-in-final-class.md")]
    pub(crate) static ABSTRACT_METHOD_IN_FINAL_CLASS = {
        summary: "detects `@final` classes with unimplemented abstract methods",
        status: LintStatus::stable("0.0.13"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/call-abstract-method.md")]
    pub(crate) static CALL_ABSTRACT_METHOD = {
        summary: "detects calls to abstract methods with trivial bodies on class objects",
        status: LintStatus::stable("0.0.16"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/invalid-explicit-override.md")]
    pub(crate) static INVALID_EXPLICIT_OVERRIDE = {
        summary: "detects methods that are decorated with `@override` but do not override any method in a superclass",
        status: LintStatus::stable("0.0.1-alpha.28"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/missing-override-decorator.md")]
    pub(crate) static MISSING_OVERRIDE_DECORATOR = {
        summary: "detects methods that override a superclass member without an `@override` annotation",
        status: LintStatus::stable("0.0.41"),
        default_level: Level::Ignore,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/type-assertion-failure.md")]
    pub(crate) static TYPE_ASSERTION_FAILURE = {
        summary: "detects failed type assertions",
        status: LintStatus::stable("0.0.1-alpha.1"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/assert-type-unspellable-subtype.md")]
    pub(crate) static ASSERT_TYPE_UNSPELLABLE_SUBTYPE = {
        summary: "detects failed type assertions",
        status: LintStatus::stable("0.0.14"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/too-many-positional-arguments.md")]
    pub(crate) static TOO_MANY_POSITIONAL_ARGUMENTS = {
        summary: "detects calls passing too many positional arguments",
        status: LintStatus::stable("0.0.1-alpha.1"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/unavailable-implicit-super-arguments.md")]
    pub(crate) static UNAVAILABLE_IMPLICIT_SUPER_ARGUMENTS = {
        summary: "detects invalid `super()` calls where implicit arguments are unavailable.",
        status: LintStatus::stable("0.0.1-alpha.1"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/super-call-in-named-tuple-method.md")]
    pub(crate) static SUPER_CALL_IN_NAMED_TUPLE_METHOD = {
        summary: "detects `super()` calls in methods of `NamedTuple` classes",
        status: LintStatus::stable("0.0.1-alpha.30"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/undefined-reveal.md")]
    pub static UNDEFINED_REVEAL = {
        summary: "detects usages of `reveal_type` without importing it",
        status: LintStatus::stable("0.0.1-alpha.1"),
        default_level: Level::Warn,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/unknown-argument.md")]
    pub(crate) static UNKNOWN_ARGUMENT = {
        summary: "detects unknown keyword arguments in calls",
        status: LintStatus::stable("0.0.1-alpha.1"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[allow(
        rustdoc::invalid_codeblock_attributes,
        reason = "`data-mdtest` is an mdtest-specific code-block attribute"
    )]
    #[doc = include_str!("../../resources/lint_docs/pydantic-discarded-extra-argument.md")]
    pub(crate) static PYDANTIC_DISCARDED_EXTRA_ARGUMENT = {
        summary: "detects extra constructor arguments that Pydantic silently discards",
        status: LintStatus::stable("0.0.60"),
        default_level: Level::Warn,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/positional-only-parameter-as-kwarg.md")]
    pub(crate) static POSITIONAL_ONLY_PARAMETER_AS_KWARG = {
        summary: "detects positional-only parameters passed as keyword arguments",
        status: LintStatus::stable("0.0.1-alpha.22"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/unresolved-attribute.md")]
    pub(crate) static UNRESOLVED_ATTRIBUTE = {
        summary: "detects references to unresolved attributes",
        status: LintStatus::stable("0.0.1-alpha.1"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/unresolved-import.md")]
    pub(crate) static UNRESOLVED_IMPORT = {
        summary: "detects unresolved imports",
        status: LintStatus::stable("0.0.1-alpha.1"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/unresolved-reference.md")]
    pub static UNRESOLVED_REFERENCE = {
        summary: "detects references to names that are not defined",
        status: LintStatus::stable("0.0.1-alpha.1"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/unsupported-operator.md")]
    pub(crate) static UNSUPPORTED_OPERATOR = {
        summary: "detects binary, unary, or comparison expressions where the operands don't support the operator",
        status: LintStatus::stable("0.0.1-alpha.1"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/unused-awaitable.md")]
    pub(crate) static UNUSED_AWAITABLE = {
        summary: "detects awaitable objects that are used as expression statements without being awaited",
        status: LintStatus::stable("0.0.21"),
        default_level: Level::Warn,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/zero-stepsize-in-slice.md")]
    pub(crate) static ZERO_STEPSIZE_IN_SLICE = {
        summary: "detects a slice step size of zero",
        status: LintStatus::stable("0.0.1-alpha.1"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/static-assert-error.md")]
    pub(crate) static STATIC_ASSERT_ERROR = {
        summary: "Failed static assertion",
        status: LintStatus::stable("0.0.1-alpha.1"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/invalid-attribute-access.md")]
    pub(crate) static INVALID_ATTRIBUTE_ACCESS = {
        summary: "Invalid attribute access",
        status: LintStatus::stable("0.0.1-alpha.1"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/redundant-cast.md")]
    pub(crate) static REDUNDANT_CAST = {
        summary: "detects redundant `cast` calls",
        status: LintStatus::stable("0.0.1-alpha.1"),
        default_level: Level::Warn,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/redundant-final-classvar.md")]
    pub(crate) static REDUNDANT_FINAL_CLASSVAR = {
        summary: "detects redundant combinations of `ClassVar` and `Final`",
        status: LintStatus::stable("0.0.18"),
        default_level: Level::Warn,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/shadowed-type-variable.md")]
    pub(crate) static SHADOWED_TYPE_VARIABLE = {
        summary: "detects type variables that shadow type variables from outer scopes",
        status: LintStatus::stable("0.0.20"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/unresolved-global.md")]
    pub(crate) static UNRESOLVED_GLOBAL = {
        summary: "detects `global` statements with no definition in the global scope",
        status: LintStatus::stable("0.0.1-alpha.15"),
        default_level: Level::Warn,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/missing-typed-dict-key.md")]
    pub(crate) static MISSING_TYPED_DICT_KEY = {
        summary: "detects missing required keys in `TypedDict` constructors",
        status: LintStatus::stable("0.0.1-alpha.20"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/invalid-typed-dict-statement.md")]
    pub(crate) static INVALID_TYPED_DICT_STATEMENT = {
        summary: "detects invalid statements in `TypedDict` class bodies",
        status: LintStatus::stable("0.0.9"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/invalid-typed-dict-field.md")]
    pub(crate) static INVALID_TYPED_DICT_FIELD = {
        summary: "detects invalid `TypedDict` field declarations",
        status: LintStatus::stable("0.0.28"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/invalid-typed-dict-header.md")]
    pub(crate) static INVALID_TYPED_DICT_HEADER = {
        summary: "detects invalid statements in `TypedDict` class headers",
        status: LintStatus::stable("0.0.14"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/invalid-attribute-override.md")]
    pub(crate) static INVALID_ATTRIBUTE_OVERRIDE = {
        summary: "detects attribute overrides that change class-variable or instance-variable behavior",
        status: LintStatus::stable("0.0.33"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[expect(clippy::doc_overindented_list_items)]
    #[doc = include_str!("../../resources/lint_docs/invalid-method-override.md")]
    pub(crate) static INVALID_METHOD_OVERRIDE = {
        summary: "detects method definitions that violate the Liskov Substitution Principle",
        status: LintStatus::stable("0.0.1-alpha.20"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/invalid-frozen-dataclass-subclass.md")]
    pub(crate) static INVALID_FROZEN_DATACLASS_SUBCLASS = {
        summary: "detects dataclasses with invalid frozen/non-frozen subclassing",
        status: LintStatus::stable("0.0.1-alpha.35"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/invalid-total-ordering.md")]
    pub(crate) static INVALID_TOTAL_ORDERING = {
        summary: "detects `@total_ordering` classes without an ordering method",
        status: LintStatus::stable("0.0.10"),
        default_level: Level::Error,
    }
}

declare_lint! {
    #[doc = include_str!("../../resources/lint_docs/invalid-legacy-positional-parameter.md")]
    pub(crate) static INVALID_LEGACY_POSITIONAL_PARAMETER = {
        summary: "detects incorrect usage of the legacy convention for specifying positional-only parameters",
        status: LintStatus::stable("0.0.15"),
        default_level: Level::Warn,
    }
}

/// A collection of type check diagnostics.
#[derive(Default, Eq, PartialEq, get_size2::GetSize)]
pub struct TypeCheckDiagnostics {
    diagnostics: Vec<Diagnostic>,
    used_suppressions: FxHashSet<FileSuppressionId>,
}

pub(crate) fn report_mismatched_type_name<'db>(
    context: &InferContext<'db, '_>,
    node: impl Ranged,
    constructor: &str,
    expected_name: &str,
    actual_name: Option<&str>,
    actual_name_ty: Type<'db>,
) {
    if let Some(builder) = context.report_lint(&MISMATCHED_TYPE_NAME, node) {
        let mut diagnostic = builder.into_diagnostic(format_args!(
            "The name passed to `{constructor}` must match the variable it is assigned to"
        ));
        if let Some(actual_name) = actual_name {
            diagnostic.set_primary_message(format_args!(
                "Expected \"{expected_name}\", got \"{actual_name}\""
            ));
        } else {
            diagnostic.set_primary_message(format_args!(
                "Expected \"{expected_name}\", got variable of type `{}`",
                actual_name_ty.display(context.db())
            ));
        }
    }
}

impl TypeCheckDiagnostics {
    pub(crate) fn push(&mut self, diagnostic: Diagnostic) {
        self.diagnostics.push(diagnostic);
    }

    pub(super) fn extend(&mut self, other: &TypeCheckDiagnostics) {
        self.diagnostics.extend_from_slice(&other.diagnostics);
        self.used_suppressions.extend(&other.used_suppressions);
    }

    pub(super) fn extend_diagnostics(&mut self, diagnostics: impl IntoIterator<Item = Diagnostic>) {
        self.diagnostics.extend(diagnostics);
    }

    pub(crate) fn mark_used(&mut self, suppression_id: FileSuppressionId) {
        self.used_suppressions.insert(suppression_id);
    }

    pub(crate) fn is_used(&self, suppression_id: FileSuppressionId) -> bool {
        self.used_suppressions.contains(&suppression_id)
    }

    pub(crate) fn used_len(&self) -> usize {
        self.used_suppressions.len()
    }

    pub(crate) fn shrink_to_fit(&mut self) {
        self.used_suppressions.shrink_to_fit();
        self.diagnostics.shrink_to_fit();
    }

    pub(crate) fn into_diagnostics(self) -> Vec<Diagnostic> {
        self.diagnostics
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.diagnostics.is_empty() && self.used_suppressions.is_empty()
    }

    pub fn iter(&self) -> std::slice::Iter<'_, Diagnostic> {
        self.diagnostics().iter()
    }

    fn diagnostics(&self) -> &[Diagnostic] {
        self.diagnostics.as_slice()
    }
}

impl std::fmt::Debug for TypeCheckDiagnostics {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.diagnostics().fmt(f)
    }
}

impl IntoIterator for TypeCheckDiagnostics {
    type Item = Diagnostic;
    type IntoIter = std::vec::IntoIter<Diagnostic>;

    fn into_iter(self) -> Self::IntoIter {
        self.into_diagnostics().into_iter()
    }
}

impl<'a> IntoIterator for &'a TypeCheckDiagnostics {
    type Item = &'a Diagnostic;
    type IntoIter = std::slice::Iter<'a, Diagnostic>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// Emit a diagnostic declaring that an index is out of bounds for a tuple.
pub(super) fn report_index_out_of_bounds(
    context: &InferContext,
    kind: &'static str,
    node: AnyNodeRef,
    tuple_ty: Type,
    length: impl std::fmt::Display,
    index: i64,
) {
    let Some(builder) = context.report_lint(&INDEX_OUT_OF_BOUNDS, node) else {
        return;
    };
    builder.into_diagnostic(format_args!(
        "Index {index} is out of bounds for {kind} `{}` with length {length}",
        tuple_ty.display(context.db())
    ));
}

/// Emit a diagnostic declaring that a type does not support subscripting.
pub(super) fn report_not_subscriptable(
    context: &InferContext,
    node: &ast::ExprSubscript,
    not_subscriptable_ty: Type,
    method: &str,
) {
    let Some(builder) = context.report_lint(&NOT_SUBSCRIPTABLE, node) else {
        return;
    };
    if method == "__delitem__" {
        builder.into_diagnostic(format_args!(
            "Cannot delete subscript on object of type `{}` with no `{method}` method",
            not_subscriptable_ty.display(context.db())
        ));
    } else {
        builder.into_diagnostic(format_args!(
            "Cannot subscript object of type `{}` with no `{method}` method",
            not_subscriptable_ty.display(context.db())
        ));
    }
}

pub(super) fn report_slice_step_size_zero(context: &InferContext, node: AnyNodeRef) {
    let Some(builder) = context.report_lint(&ZERO_STEPSIZE_IN_SLICE, node) else {
        return;
    };
    builder.into_diagnostic("Slice step size cannot be zero");
}

// We avoid emitting invalid assignment diagnostic for literal assignments to a `TypedDict`, as
// they can only occur if we already failed to validate the dict (and emitted some diagnostic).
pub(crate) fn is_invalid_typed_dict_literal(
    db: &dyn Db,
    target_ty: Type,
    source: AnyNodeRef<'_>,
) -> bool {
    target_ty
        .filter_union(db, Type::is_typed_dict)
        .as_typed_dict()
        .is_some()
        && matches!(source, AnyNodeRef::ExprDict(_))
}

fn report_invalid_assignment_with_message<'db, 'ctx: 'db, T: Ranged>(
    context: &'ctx InferContext,
    node: T,
    message: std::fmt::Arguments,
) -> Option<LintDiagnosticGuard<'db, 'ctx>> {
    let builder = context.report_lint(&INVALID_ASSIGNMENT, node)?;
    Some(builder.into_diagnostic(message))
}

pub(super) fn note_numbers_module_not_supported<'db>(
    db: &'db dyn Db,
    diag: &mut Diagnostic,
    target_ty: Type<'db>,
    value_ty: Type<'db>,
) {
    const BUILTIN_NUMBERS: [KnownClass; 3] =
        [KnownClass::Int, KnownClass::Float, KnownClass::Complex];

    if let Type::NominalInstance(target_instance) = target_ty {
        let file = target_instance.class(db).class_literal(db).python_file(db);
        if let Some(module) = file_to_module(db, file)
            && module.is_known(db, KnownModule::Numbers)
        {
            let is_numeric = value_ty.is_subtype_of(
                db,
                UnionType::from_elements(db, BUILTIN_NUMBERS.iter().map(|cls| cls.to_instance(db))),
            );

            if is_numeric {
                diag.info(
                    "Types from the `numbers` module aren't supported for static type checking",
                );
                diag.help("Consider using a protocol instead, such as `typing.SupportsFloat`");
            }
        }
    }
}

fn covariant_supertype_hint<'db>(
    class: StaticClassLiteral<'db>,
    db: &'db dyn Db,
    mismatched_invariant_parameters: &[usize],
) -> Option<&'static str> {
    match (class.known(db), mismatched_invariant_parameters) {
        (Some(KnownClass::List | KnownClass::Deque), [0]) => {
            Some("Consider using the covariant supertype `collections.abc.Sequence`")
        }
        (Some(KnownClass::Set), [0]) => {
            Some("Consider using the covariant supertype `collections.abc.Set`")
        }
        (
            Some(
                KnownClass::Dict
                | KnownClass::DefaultDict
                | KnownClass::OrderedDict
                | KnownClass::ChainMap,
            ),
            [1],
        ) => Some(
            "Consider using the supertype `collections.abc.Mapping`, which is covariant in its value type",
        ),
        _ => None,
    }
}

/// Add a diagnostic hint for cases like an invalid `list[bool]` to `list[int]` assignment,
/// that fails due to invariance.
pub(super) fn add_invariant_generic_hints<'db>(
    db: &'db dyn Db,
    diag: &mut Diagnostic,
    expected_ty: Type<'db>,
    provided_ty: Type<'db>,
) {
    let Some((expected_class, expected_specialization)) = expected_ty.class_specialization(db)
    else {
        return;
    };
    let Some((provided_class, provided_specialization)) = provided_ty.class_specialization(db)
    else {
        return;
    };

    if expected_class != provided_class {
        return;
    }

    let generic_context = expected_specialization.generic_context(db);
    if generic_context != provided_specialization.generic_context(db) {
        return;
    }

    let mismatched_invariant_arguments = generic_context
        .variables(db)
        .zip(expected_specialization.types(db))
        .zip(provided_specialization.types(db))
        .enumerate()
        .filter_map(|(index, ((bound_typevar, expected_arg), provided_arg))| {
            (bound_typevar.variance(db) == TypeVarVariance::Invariant
                && !expected_arg.is_equivalent_to(db, *provided_arg))
            .then_some((index, expected_arg, provided_arg))
        });

    let mut mismatch_indices = Vec::new();
    for (index, expected_arg, provided_arg) in mismatched_invariant_arguments {
        if !provided_arg.is_assignable_to(db, *expected_arg) {
            return;
        }
        mismatch_indices.push(index);
    }

    if mismatch_indices.is_empty() {
        return;
    }

    let class_name = expected_class.name(db);
    let message = match (generic_context.len(db), mismatch_indices.as_slice()) {
        (1, _) => {
            format!("`{class_name}` is invariant in its type parameter")
        }
        (_, [0]) => format!("`{class_name}` is invariant in its first type parameter"),
        (_, [1]) => format!("`{class_name}` is invariant in its second type parameter"),
        (_, [2]) => format!("`{class_name}` is invariant in its third type parameter"),
        (2, [0, 1]) => {
            format!("`{class_name}` is invariant in its first and second type parameters")
        }
        _ => format!("`{class_name}` is invariant in (one of) its type parameters"),
    };
    diag.info(message);

    if let Some(note) = covariant_supertype_hint(expected_class, db, &mismatch_indices) {
        diag.info(note);
    }
    diag.info(
        "For more information, see https://docs.astral.sh/ty/reference/typing-faq/#invariant-generics",
    );
}

pub(super) fn report_invalid_assignment<'db>(
    context: &InferContext<'db, '_>,
    target_node: AnyNodeRef,
    definition: Definition<'db>,
    target_ty: Type,
    value_ty: Type<'db>,
) {
    let definition_kind = definition.kind(context.db());
    let value_node = match definition_kind {
        DefinitionKind::Assignment(def) => Some(def.value(context.module())),
        DefinitionKind::AnnotatedAssignment(def) => def.value(context.module()),
        DefinitionKind::NamedExpression(def) => Some(&*def.node(context.module()).value),
        _ => None,
    };

    if let Some(value_node) = value_node
        && is_invalid_typed_dict_literal(context.db(), target_ty, value_node.into())
    {
        return;
    }

    let settings =
        DisplaySettings::from_possibly_ambiguous_types(context.db(), [target_ty, value_ty]);

    let diagnostic_range = if let Some(value_node) = value_node {
        // Expand the range to include parentheses around the value, if any. This allows
        // invalid-assignment diagnostics to be suppressed on the opening or closing parenthesis:
        // ```py
        // x: str = ( # ty: ignore <- here
        //     1 + 2 + 3
        // )  # ty: ignore <- or here
        // ```

        parentheses_iterator(value_node.into(), None, context.module().tokens())
            .last()
            .unwrap_or(value_node.range())
    } else {
        target_node.range()
    };

    let Some(mut diag) = report_invalid_assignment_with_message(
        context,
        diagnostic_range,
        format_args!(
            "Object of type `{}` is not assignable to `{}`",
            value_ty.display_with(context.db(), settings.clone()),
            target_ty.display_with(context.db(), settings)
        ),
    ) else {
        return;
    };

    if matches!(target_node, AnyNodeRef::ExprName(_)) {
        match target_ty {
            Type::ClassLiteral(class) => {
                diag.info(format_args!(
                    "Implicit shadowing of class `{}`. Add an annotation to make it explicit if this is intentional",
                    class.name(context.db()),
                ));
            }
            Type::FunctionLiteral(function) => {
                diag.info(format_args!(
                    "Implicit shadowing of function `{}`. Add an annotation to make it explicit if this is intentional",
                    function.name(context.db()),
                ));
            }
            _ => {}
        }
    }

    if value_node.is_some() {
        match definition_kind {
            DefinitionKind::AnnotatedAssignment(assignment) => {
                // For annotated assignments, just point to the annotation in the source code.
                diag.annotate(
                    context
                        .secondary(assignment.annotation(context.module()))
                        .message("Declared type"),
                );
            }
            _ => {
                // Otherwise, annotate the target with its declared type.
                diag.annotate(context.secondary(target_node).message(format_args!(
                    "Declared type `{}`",
                    target_ty.display(context.db()),
                )));
            }
        }

        diag.set_primary_message(format_args!(
            "Incompatible value of type `{}`",
            value_ty.display(context.db()),
        ));

        let error_context = value_ty.assignability_error_context(context.db(), target_ty);
        error_context.attach_to(context.db(), &mut diag);

        // Overwrite the concise message to avoid showing the value type twice
        let message = diag.primary_message().to_string();
        diag.set_concise_message(message);
    }

    // special case message
    note_numbers_module_not_supported(context.db(), &mut diag, target_ty, value_ty);
    add_invariant_generic_hints(context.db(), &mut diag, target_ty, value_ty);
}

pub(super) fn report_invalid_attribute_assignment(
    context: &InferContext,
    range: TextRange,
    target_ty: Type,
    source_ty: Type,
    attribute_name: &'_ str,
) {
    // TODO: Ideally we would not emit diagnostics for `TypedDict` literal arguments
    // here (see `diagnostic::is_invalid_typed_dict_literal`). However, we may have
    // silenced diagnostics during attribute resolution, and rely on the assignability
    // diagnostic being emitted here.

    let Some(mut diag) = report_invalid_assignment_with_message(
        context,
        range,
        format_args!(
            "Object of type `{}` is not assignable to attribute `{attribute_name}` of type `{}`",
            source_ty.display(context.db()),
            target_ty.display(context.db()),
        ),
    ) else {
        return;
    };

    let error_context = source_ty.assignability_error_context(context.db(), target_ty);
    error_context.attach_to(context.db(), &mut diag);
}

pub(super) fn report_bad_dunder_set_call<'db>(
    context: &InferContext<'db, '_>,
    dunder_set_failure: &CallError<'db>,
    attribute: &str,
    object_type: Type<'db>,
    target: &ast::ExprAttribute,
) {
    let Some(builder) = context.report_lint(&INVALID_ASSIGNMENT, target) else {
        return;
    };
    let db = context.db();
    if let Some(property) = dunder_set_failure.as_attempt_to_set_property_with_no_setter() {
        let object_type = object_type.display(db);
        let mut diagnostic = builder.into_diagnostic(format_args!(
            "Cannot assign to read-only property `{attribute}` on object of type `{object_type}`",
        ));
        if let Some(file_range) = property
            .getter(db)
            .and_then(|getter| getter.definition(db))
            .and_then(|definition| definition.focus_range(db))
        {
            diagnostic.annotate(Annotation::secondary(Span::from(file_range)).message(
                format_args!("Property `{object_type}.{attribute}` defined here with no setter"),
            ));
            diagnostic.set_primary_message(format_args!(
                "Attempted assignment to `{object_type}.{attribute}` here"
            ));
        }
    } else {
        // TODO: Here, it would be nice to emit an additional diagnostic
        // that explains why the call failed
        builder.into_diagnostic(format_args!(
            "Invalid assignment to data descriptor attribute \
            `{attribute}` on type `{}` with custom `__set__` method",
            object_type.display(db)
        ));
    }
}

pub(super) fn report_bad_dunder_delete_call<'db>(
    context: &InferContext<'db, '_>,
    dunder_delete_failure: &CallError<'db>,
    attribute: &str,
    object_type: Type<'db>,
    target: &ast::ExprAttribute,
) {
    let Some(builder) = context.report_lint(&INVALID_ASSIGNMENT, target) else {
        return;
    };
    let db = context.db();
    if let Some(property) = dunder_delete_failure.as_attempt_to_delete_property_with_no_deleter() {
        let object_type = object_type.display(db);
        let mut diagnostic = builder.into_diagnostic(format_args!(
            "Cannot delete read-only property `{attribute}` on object of type `{object_type}`",
        ));
        if let Some(file_range) = property
            .getter(db)
            .and_then(|getter| getter.definition(db))
            .or_else(|| property.setter(db).and_then(|setter| setter.definition(db)))
            .and_then(|definition| definition.focus_range(db))
        {
            diagnostic.annotate(Annotation::secondary(Span::from(file_range)).message(
                format_args!("Property `{object_type}.{attribute}` defined here with no deleter"),
            ));
            diagnostic.set_primary_message(format_args!(
                "Attempted deletion of `{object_type}.{attribute}` here"
            ));
        }
    } else {
        builder.into_diagnostic(format_args!(
            "Invalid deletion of data descriptor attribute \
            `{attribute}` on type `{}` with custom `__delete__` method",
            object_type.display(db)
        ));
    }
}

pub(super) fn report_bad_dunder_delattr_call(
    context: &InferContext<'_, '_>,
    attribute: &str,
    object_type: Type,
    target: &ast::ExprAttribute,
    binding_error: bool,
) {
    let Some(builder) = context.report_lint(&INVALID_ASSIGNMENT, target) else {
        return;
    };
    let db = context.db();
    let mut diagnostic = builder.into_diagnostic(format_args!(
        "Cannot delete attribute `{attribute}` on type `{}` with custom `__delattr__` method",
        object_type.display(db),
    ));
    if binding_error {
        diagnostic.info(format_args!(
            "Type `{}` has a `__delattr__` method, but it cannot be called with the expected arguments",
            object_type.display(db)
        ));
        diagnostic.info(
            "Expected a signature at least as permissive as \
            `def __delattr__(self, name: str, /) -> None`",
        );
    }
}

pub(super) fn report_invalid_return_type(
    context: &InferContext,
    object_range: impl Ranged,
    return_type_range: impl Ranged,
    expected_ty: Type,
    actual_ty: Type,
) {
    let Some(builder) = context.report_lint(&INVALID_RETURN_TYPE, object_range) else {
        return;
    };

    let settings =
        DisplaySettings::from_possibly_ambiguous_types(context.db(), [expected_ty, actual_ty]);
    let return_type_span = context.span(return_type_range);

    let mut diag = builder.into_diagnostic("Return type does not match returned value");
    diag.set_primary_message(format_args!(
        "expected `{expected_ty}`, found `{actual_ty}`",
        expected_ty = expected_ty.display_with(context.db(), settings.clone()),
        actual_ty = actual_ty.display_with(context.db(), settings.clone()),
    ));
    diag.annotate(
        Annotation::secondary(return_type_span).message(format_args!(
            "Expected `{expected_ty}` because of return type",
            expected_ty = expected_ty.display_with(context.db(), settings),
        )),
    );

    let error_context = actual_ty.assignability_error_context(context.db(), expected_ty);
    error_context.attach_to(context.db(), &mut diag);
}

pub(super) fn report_invalid_generator_function_return_type(
    context: &InferContext,
    return_type_range: TextRange,
    inferred_return: KnownClass,
    expected_ty: Type,
) {
    let Some(builder) = context.report_lint(&INVALID_RETURN_TYPE, return_type_range) else {
        return;
    };

    let mut diag = builder.into_diagnostic("Return type does not match returned value");
    let inferred_ty = inferred_return.display(context.db());
    diag.set_primary_message(format_args!(
        "expected `{expected_ty}`, found `{inferred_ty}`",
        expected_ty = expected_ty.display(context.db()),
    ));

    let (description, link) = if inferred_return == KnownClass::AsyncGeneratorType {
        (
            "an async generator function",
            "https://docs.python.org/3/glossary.html#term-asynchronous-generator",
        )
    } else {
        (
            "a generator function",
            "https://docs.python.org/3/glossary.html#term-generator",
        )
    };

    diag.info(format_args!(
        "Function is inferred as returning `{inferred_ty}` because it is {description}"
    ));
    diag.info(format_args!("See {link} for more details"));
}

#[derive(Copy, Clone)]
pub(super) enum GeneratorMismatchKind {
    YieldType,
    SendType,
}

pub(super) fn report_invalid_generator_yield_type(
    context: &InferContext,
    object_range: impl Ranged,
    return_type_span: Option<Span>,
    expected_ty: Type,
    actual_ty: Type,
    kind: GeneratorMismatchKind,
) {
    let Some(builder) = context.report_lint(&INVALID_YIELD, object_range) else {
        return;
    };

    let settings =
        DisplaySettings::from_possibly_ambiguous_types(context.db(), [expected_ty, actual_ty]);
    let expected_display = expected_ty.display_with(context.db(), settings.clone());
    let actual_display = actual_ty.display_with(context.db(), settings);

    let (kind_name, title, concise) = match kind {
        GeneratorMismatchKind::YieldType => (
            "yield",
            "Yield expression type does not match annotation",
            format!(
                "Yield type `{actual_display}` does not match annotated yield type `{expected_display}`"
            ),
        ),
        GeneratorMismatchKind::SendType => (
            "send",
            "Send type does not match annotation",
            format!(
                "Send type `{actual_display}` does not match annotated send type `{expected_display}`"
            ),
        ),
    };

    let mut diag = builder.into_diagnostic(title);
    diag.set_concise_message(concise);
    let primary = match kind {
        GeneratorMismatchKind::YieldType => {
            format!("expression of type `{actual_display}`, expected `{expected_display}`")
        }
        GeneratorMismatchKind::SendType => {
            format!("generator with send type `{actual_display}`, expected `{expected_display}`")
        }
    };
    diag.set_primary_message(primary);

    if let Some(return_type_span) = return_type_span {
        diag.annotate(Annotation::secondary(return_type_span).message(format!(
            "Function annotated with {kind_name} type `{expected_display}` here"
        )));
    }

    let error_context = actual_ty.assignability_error_context(context.db(), expected_ty);
    error_context.attach_to(context.db(), &mut diag);
}

pub(super) fn report_implicit_return_type(
    context: &InferContext,
    range: impl Ranged,
    expected_ty: Type,
    has_empty_body: bool,
    enclosing_class_of_method: Option<ClassType>,
    no_return: bool,
) {
    let db = context.db();

    // Use EMPTY_BODY lint for functions with empty bodies, INVALID_RETURN_TYPE for others
    let lint_to_use = if has_empty_body {
        &EMPTY_BODY
    } else {
        &INVALID_RETURN_TYPE
    };

    let Some(builder) = context.report_lint(lint_to_use, range) else {
        return;
    };

    // If no return statement is defined in the function, then the function always returns `None`
    let mut diagnostic = if no_return {
        let mut diag = builder.into_diagnostic(format_args!(
            "Function always implicitly returns `None`, which is not assignable to return type `{}`",
            expected_ty.display(db),
        ));
        diag.info(
            "Consider changing the return annotation to `-> None` or adding a `return` statement",
        );
        diag
    } else {
        builder.into_diagnostic(format_args!(
            "Function can implicitly return `None`, which is not assignable to return type `{}`",
            expected_ty.display(db),
        ))
    };
    if !has_empty_body {
        return;
    }
    diagnostic.info("Functions with empty bodies and non-`None` return types are only permitted:");
    diagnostic.info(" - in stub files");
    diagnostic.info(" - in `if TYPE_CHECKING` blocks");
    diagnostic.info(" - as methods on protocol classes");
    diagnostic.info(" - or as `@abstractmethod`-decorated methods on abstract classes");
    let Some(class) = enclosing_class_of_method else {
        return;
    };
    if class.iter_mro(db).contains(&ClassBase::Protocol) {
        diagnostic.info(format_args!(
            "Class `{}` has `typing.Protocol` in its MRO, but it is not a protocol class",
            class.name(db)
        ));

        let mut sub_diagnostic = SubDiagnostic::new(
            SubDiagnosticSeverity::Info,
            "Only classes that directly inherit from `typing.Protocol` \
            or `typing_extensions.Protocol` are considered protocol classes",
        );
        sub_diagnostic.annotate(Annotation::primary(class.definition_span(db)).message(
            format_args!(
                "`Protocol` not present in `{class}`'s immediate bases",
                class = class.name(db)
            ),
        ));
        diagnostic.sub(sub_diagnostic);

        diagnostic.info("See https://typing.python.org/en/latest/spec/protocol.html#");
    }
}

pub(super) fn report_invalid_type_checking_constant(context: &InferContext, node: AnyNodeRef) {
    let Some(builder) = context.report_lint(&INVALID_TYPE_CHECKING_CONSTANT, node) else {
        return;
    };
    builder.into_diagnostic(
        "The name TYPE_CHECKING is reserved for use as a flag; only False can be assigned to it",
    );
}

pub(super) fn report_possibly_unresolved_reference(
    context: &InferContext,
    expr_name_node: &ast::ExprName,
) {
    let Some(builder) = context.report_lint(&POSSIBLY_UNRESOLVED_REFERENCE, expr_name_node) else {
        return;
    };

    let ast::ExprName { id, .. } = expr_name_node;
    builder.into_diagnostic(format_args!("Name `{id}` used when possibly not defined"));
}

pub(super) fn report_possibly_missing_attribute(
    context: &InferContext,
    target: &ast::ExprAttribute,
    attribute: &str,
    object_ty: Type,
) {
    let Some(builder) = context.report_lint(&POSSIBLY_MISSING_ATTRIBUTE, target) else {
        return;
    };
    let db = context.db();
    match object_ty {
        Type::ModuleLiteral(module) => builder.into_diagnostic(format_args!(
            "Member `{attribute}` may be missing on module `{}`",
            module.module(db).name(db),
        )),
        Type::ClassLiteral(class) => builder.into_diagnostic(format_args!(
            "Attribute `{attribute}` may be missing on class `{}`",
            class.name(db),
        )),
        Type::GenericAlias(alias) => builder.into_diagnostic(format_args!(
            "Attribute `{attribute}` may be missing on class `{}`",
            alias.display(db),
        )),
        _ => builder.into_diagnostic(format_args!(
            "Attribute `{attribute}` may be missing on object of type `{}`",
            object_ty.display(db),
        )),
    };
}

pub(super) fn report_invalid_exception_tuple_caught<'db, 'ast>(
    context: &InferContext<'db, 'ast>,
    node: &'ast ast::ExprTuple,
    node_type: Type<'db>,
    invalid_tuple_nodes: impl IntoIterator<Item = (&'ast ast::Expr, Type<'db>)>,
) {
    let Some(builder) = context.report_lint(&INVALID_EXCEPTION_CAUGHT, node) else {
        return;
    };

    let mut diagnostic = builder.into_diagnostic("Invalid tuple caught in an exception handler");
    diagnostic.set_concise_message(format_args!(
        "Cannot catch object of type `{}` in an exception handler",
        node_type.display(context.db())
    ));

    for (sub_node, ty) in invalid_tuple_nodes {
        let span = context.span(sub_node);
        diagnostic.annotate(Annotation::secondary(span.clone()).message(format_args!(
            "Invalid element of type `{}`",
            ty.display(context.db())
        )));
        if ty.is_notimplemented(context.db()) {
            diagnostic.annotate(
                Annotation::secondary(span).message("Did you mean `NotImplementedError`?"),
            );
        }
    }

    diagnostic.info(
        "Can only catch a subclass of `BaseException` or tuple of `BaseException` subclasses",
    );
}

pub(super) fn report_invalid_exception_caught(context: &InferContext, node: &ast::Expr, ty: Type) {
    let Some(builder) = context.report_lint(&INVALID_EXCEPTION_CAUGHT, node) else {
        return;
    };

    let mut diagnostic = if ty.is_notimplemented(context.db()) {
        let mut diag =
            builder.into_diagnostic("Cannot catch `NotImplemented` in an exception handler");
        diag.set_primary_message("Did you mean `NotImplementedError`?");
        diag
    } else {
        let mut diag = builder.into_diagnostic(format_args!(
            "Invalid {thing} caught in an exception handler",
            thing = if ty.tuple_instance_spec(context.db()).is_some() {
                "tuple"
            } else {
                "object"
            },
        ));
        diag.set_primary_message(format_args!(
            "Object has type `{}`",
            ty.display(context.db())
        ));
        diag
    };

    diagnostic.info(
        "Can only catch a subclass of `BaseException` or tuple of `BaseException` subclasses",
    );
}

pub(crate) fn report_invalid_exception_raised(
    context: &InferContext,
    raised_node: &ast::Expr,
    raise_type: Type,
) {
    let Some(builder) = context.report_lint(&INVALID_RAISE, raised_node) else {
        return;
    };
    if raise_type.is_notimplemented(context.db()) {
        let mut diagnostic = builder.into_diagnostic(format_args!("Cannot raise `NotImplemented`"));
        diagnostic.set_primary_message("Did you mean `NotImplementedError`?");
        diagnostic.info("Can only raise an instance or subclass of `BaseException`");
    } else {
        let mut diagnostic = builder.into_diagnostic(format_args!(
            "Cannot raise object of type `{}`",
            raise_type.display(context.db())
        ));
        diagnostic.set_primary_message("Not an instance or subclass of `BaseException`");
    }
}

pub(crate) fn report_invalid_exception_cause(context: &InferContext, node: &ast::Expr, ty: Type) {
    let Some(builder) = context.report_lint(&INVALID_RAISE, node) else {
        return;
    };
    let mut diagnostic = if ty.is_notimplemented(context.db()) {
        let mut diag = builder.into_diagnostic(format_args!(
            "Cannot use `NotImplemented` as an exception cause",
        ));
        diag.set_primary_message("Did you mean `NotImplementedError`?");
        diag
    } else {
        builder.into_diagnostic(format_args!(
            "Cannot use object of type `{}` as an exception cause",
            ty.display(context.db())
        ))
    };
    diagnostic.info(
        "An exception cause must be an instance of `BaseException`, \
        subclass of `BaseException`, or `None`",
    );
}

pub(crate) fn report_instance_layout_conflict(
    context: &InferContext,
    header_range: TextRange,
    base_nodes: Option<&[ast::Expr]>,
    disjoint_bases: &IncompatibleBases,
) {
    debug_assert!(disjoint_bases.len() > 1);

    let db = context.db();

    let Some(builder) = context.report_lint(&INSTANCE_LAYOUT_CONFLICT, header_range) else {
        return;
    };

    let mut diagnostic = builder
        .into_diagnostic("Class will raise `TypeError` at runtime due to incompatible bases");

    diagnostic.set_primary_message(format_args!(
        "Bases {} cannot be combined in multiple inheritance",
        disjoint_bases.describe_problematic_class_bases(db)
    ));

    let mut subdiagnostic = SubDiagnostic::new(
        SubDiagnosticSeverity::Info,
        "Two classes cannot coexist in a class's MRO if their instances \
        have incompatible memory layouts",
    );

    for (disjoint_base, disjoint_base_info) in disjoint_bases {
        let IncompatibleBaseInfo {
            node_index,
            originating_base,
        } = disjoint_base_info;

        // Get the span for this base from the AST (if available)
        let Some(base_node) = base_nodes.and_then(|nodes| nodes.get(*node_index)) else {
            continue;
        };

        let span = context.span(base_node);
        let mut annotation = Annotation::secondary(span.clone());
        if *originating_base == disjoint_base.class {
            match disjoint_base.kind {
                DisjointBaseKind::DefinesSlots => {
                    annotation = annotation.message(format_args!(
                        "`{base}` instances have a distinct memory layout because `{base}` defines non-empty `__slots__`",
                        base = originating_base.name(db)
                    ));
                }
                DisjointBaseKind::DisjointBaseDecorator => {
                    annotation = annotation.message(format_args!(
                        "`{base}` instances have a distinct memory layout because of the way `{base}` \
                        is implemented in a C extension",
                        base = originating_base.name(db)
                    ));
                }
            }
            subdiagnostic.annotate(annotation);
        } else {
            annotation = annotation.message(format_args!(
                "`{base}` instances have a distinct memory layout \
                because `{base}` inherits from `{disjoint_base}`",
                base = originating_base.name(db),
                disjoint_base = disjoint_base.class.name(db)
            ));
            subdiagnostic.annotate(annotation);

            let mut additional_annotation = Annotation::secondary(span);

            additional_annotation = match disjoint_base.kind {
                DisjointBaseKind::DefinesSlots => additional_annotation.message(format_args!(
                    "`{disjoint_base}` instances have a distinct memory layout because `{disjoint_base}` \
                        defines non-empty `__slots__`",
                    disjoint_base = disjoint_base.class.name(db),
                )),

                DisjointBaseKind::DisjointBaseDecorator => {
                    additional_annotation.message(format_args!(
                        "`{disjoint_base}` instances have a distinct memory layout \
                        because of the way `{disjoint_base}` is implemented in a C extension",
                        disjoint_base = disjoint_base.class.name(db),
                    ))
                }
            };

            subdiagnostic.annotate(additional_annotation);
        }
    }

    diagnostic.sub(subdiagnostic);
}

/// Emit a diagnostic for a metaclass conflict where both conflicting metaclasses
/// are inherited from base classes.
pub(super) fn report_conflicting_metaclass_from_bases(
    context: &InferContext,
    node: AnyNodeRef,
    class_name: &str,
    metaclass1: ClassType,
    base1: impl std::fmt::Display,
    metaclass2: ClassType,
    base2: impl std::fmt::Display,
) {
    let Some(builder) = context.report_lint(&CONFLICTING_METACLASS, node) else {
        return;
    };
    let db = context.db();
    builder.into_diagnostic(format_args!(
        "The metaclass of a derived class (`{class_name}`) \
            must be a subclass of the metaclasses of all its bases, \
            but `{metaclass1}` (metaclass of base class `{base1}`) \
            and `{metaclass2}` (metaclass of base class `{base2}`) \
            have no subclass relationship",
        metaclass1 = metaclass1.name(db),
        metaclass2 = metaclass2.name(db),
    ));
}

/// Information regarding the conflicting disjoint bases a class is inferred to have in its MRO.
///
/// For each disjoint base, we record information about which element in the class's bases list
/// caused the disjoint base to be included in the class's MRO.
///
/// The inner data is an `IndexMap` to ensure that diagnostics regarding conflicting disjoint bases
/// are reported in a stable order.
#[derive(Debug, Default)]
pub(super) struct IncompatibleBases<'db>(FxIndexMap<DisjointBase<'db>, IncompatibleBaseInfo<'db>>);

impl<'db> IncompatibleBases<'db> {
    pub(super) fn insert(
        &mut self,
        base: DisjointBase<'db>,
        node_index: usize,
        class: ClassLiteral<'db>,
    ) {
        let info = IncompatibleBaseInfo {
            node_index,
            originating_base: class,
        };
        self.0.insert(base, info);
    }

    /// List the problematic class bases in a human-readable format.
    fn describe_problematic_class_bases(&self, db: &dyn Db) -> String {
        let bad_base_names = self.0.values().map(|info| info.originating_base.name(db));

        format_enumeration(bad_base_names)
    }

    pub(super) fn len(&self) -> usize {
        self.0.len()
    }

    /// Two disjoint bases are allowed to coexist in an MRO if one is a subclass of the other.
    /// This method therefore removes any entry in `self` that is a subclass of one or more
    /// other entries also contained in `self`.
    pub(super) fn remove_redundant_entries(&mut self, db: &'db dyn Db) {
        self.0 = self
            .0
            .iter()
            .filter(|(disjoint_base, _)| {
                self.0
                    .keys()
                    .filter(|other_base| other_base != disjoint_base)
                    .all(|other_base| {
                        // CPython's layout check operates on runtime classes. Type arguments are
                        // irrelevant here: a generic disjoint base and any specialization of that
                        // base share the same layout.
                        !disjoint_base
                            .class
                            .default_specialization(db)
                            .is_subtype_of_class_literal(db, other_base.class)
                    })
            })
            .map(|(base, info)| (*base, *info))
            .collect();
    }
}

impl<'a, 'db> IntoIterator for &'a IncompatibleBases<'db> {
    type Item = (&'a DisjointBase<'db>, &'a IncompatibleBaseInfo<'db>);
    type IntoIter = indexmap::map::Iter<'a, DisjointBase<'db>, IncompatibleBaseInfo<'db>>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

/// Information about which class base the "disjoint base" stems from
#[derive(Debug, Copy, Clone)]
pub(super) struct IncompatibleBaseInfo<'db> {
    /// The index of the problematic base in the [`ast::StmtClassDef`]'s bases list.
    node_index: usize,

    /// The base class in the [`ast::StmtClassDef`]'s bases list that caused
    /// the disjoint base to be included in the class's MRO.
    ///
    /// This won't necessarily be the same class as the `DisjointBase`'s class,
    /// as the `DisjointBase` may have found its way into the class's MRO by dint of it being a
    /// superclass of one of the classes in the class definition's bases list.
    originating_base: ClassLiteral<'db>,
}

pub(crate) fn report_invalid_arguments_to_annotated(
    context: &InferContext,
    subscript: &ast::ExprSubscript,
) {
    let Some(builder) = context.report_lint(&INVALID_TYPE_FORM, subscript) else {
        return;
    };
    builder.into_diagnostic(
        "Special form `typing.Annotated` expected at least 2 arguments \
         (one type and at least one metadata element)",
    );
}

pub(crate) fn report_invalid_argument_number_to_special_form(
    context: &InferContext,
    subscript: &ast::ExprSubscript,
    special_form: impl Into<SpecialFormType>,
    received_arguments: usize,
    expected_arguments: u8,
) {
    let noun = if expected_arguments == 1 {
        "type argument"
    } else {
        "type arguments"
    };
    if let Some(builder) = context.report_lint(&INVALID_TYPE_FORM, subscript) {
        builder.into_diagnostic(format_args!(
            "Special form `{special_form}` expected exactly {expected_arguments} {noun}, \
            got {received_arguments}",
            special_form = special_form.into(),
        ));
    }
}

pub(crate) fn report_bad_argument_to_get_protocol_members(
    context: &InferContext,
    call: &ast::ExprCall,
    class: ClassLiteral,
) {
    let Some(builder) = context.report_lint(&INVALID_ARGUMENT_TYPE, call) else {
        return;
    };
    let db = context.db();
    let mut diagnostic = builder.into_diagnostic("Invalid argument to `get_protocol_members`");
    diagnostic.set_primary_message("This call will raise `TypeError` at runtime");
    diagnostic.info("Only protocol classes can be passed to `get_protocol_members`");

    let mut class_def_diagnostic = SubDiagnostic::new(
        SubDiagnosticSeverity::Info,
        format_args!(
            "`{}` is declared here, but it is not a protocol class:",
            class.name(db)
        ),
    );
    class_def_diagnostic.annotate(Annotation::primary(class.header_span(db)));
    diagnostic.sub(class_def_diagnostic);

    diagnostic.info(
        "A class is only a protocol class if it directly inherits \
            from `typing.Protocol` or `typing_extensions.Protocol`",
    );
    // TODO the typing spec isn't really designed as user-facing documentation,
    // but there isn't really any user-facing documentation that covers this specific issue well
    // (it's not described well in the CPython docs; and PEP-544 is a snapshot of a decision taken
    // years ago rather than up-to-date documentation). We should either write our own docs
    // describing this well or contribute to type-checker-agnostic docs somewhere and link to those.
    diagnostic.info("See https://typing.python.org/en/latest/spec/protocol.html#");
}

pub(crate) fn report_bad_argument_to_protocol_interface(
    context: &InferContext,
    call: &ast::ExprCall,
    param_type: Type,
) {
    let Some(builder) = context.report_lint(&INVALID_ARGUMENT_TYPE, call) else {
        return;
    };
    let db = context.db();
    let mut diagnostic = builder.into_diagnostic("Invalid argument to `reveal_protocol_interface`");
    diagnostic
        .set_primary_message("Only protocol classes can be passed to `reveal_protocol_interface`");

    if let Some(class) = param_type.to_class_type(context.db()) {
        let mut class_def_diagnostic = SubDiagnostic::new(
            SubDiagnosticSeverity::Info,
            format_args!(
                "`{}` is declared here, but it is not a protocol class:",
                class.name(db)
            ),
        );
        if let Some((class_literal, _)) = class.static_class_literal(db) {
            class_def_diagnostic.annotate(Annotation::primary(class_literal.header_span(db)));
        }
        diagnostic.sub(class_def_diagnostic);
    }

    diagnostic.info(
        "A class is only a protocol class if it directly inherits \
            from `typing.Protocol` or `typing_extensions.Protocol`",
    );
    // See TODO in `report_bad_argument_to_get_protocol_members` above
    diagnostic.info("See https://typing.python.org/en/latest/spec/protocol.html");
}

pub(crate) fn report_invalid_arguments_to_callable(
    context: &InferContext,
    subscript: &ast::ExprSubscript,
) {
    let Some(builder) = context.report_lint(&INVALID_TYPE_FORM, subscript) else {
        return;
    };
    builder.into_diagnostic(format_args!(
        "Special form `Callable` expected exactly two arguments (parameter types and return type)",
    ));
}

pub(crate) fn report_invalid_class_match_pattern<T: Ranged>(
    context: &InferContext,
    pattern_cls: T,
    cls_ty: Type,
) {
    let Some(builder) = context.report_lint(&INVALID_MATCH_PATTERN, pattern_cls) else {
        return;
    };
    let db = context.db();
    let class_display = cls_ty.display(db);
    let mut diagnostic = builder.into_diagnostic(format_args!(
        "`{class_display}` cannot be used in a class pattern because it is not a type"
    ));
    diagnostic.set_primary_message("This will raise `TypeError` at runtime");
}

pub(crate) fn report_too_many_positional_patterns_for_class_pattern<T: Ranged>(
    context: &InferContext,
    first_excess_pattern: T,
    positional_limit: usize,
    positional_count: usize,
    class_display: impl std::fmt::Display,
) {
    let Some(builder) = context.report_lint(&INVALID_MATCH_PATTERN, first_excess_pattern) else {
        return;
    };
    builder.into_diagnostic(format_args!(
        "Too many positional subpatterns for `{class_display}`: expected {positional_limit}, got {positional_count}"
    ));
}

pub(crate) fn report_invalid_match_args_type<T: Ranged>(
    context: &InferContext,
    pattern: T,
    match_args_ty: Type,
    cls_ty: Type,
) {
    let Some(builder) = context.report_lint(&INVALID_MATCH_PATTERN, pattern) else {
        return;
    };
    let db = context.db();
    let class_display = cls_ty.display(db);
    let match_args_display = match_args_ty.display(db);
    builder.into_diagnostic(format_args!(
        "`__match_args__` for `{class_display}` must be an exact tuple, not `{match_args_display}`"
    ));
}

pub(crate) fn add_type_expression_reference_link<'db, 'ctx>(
    mut diag: LintDiagnosticGuard<'db, 'ctx>,
) -> LintDiagnosticGuard<'db, 'ctx> {
    diag.info("See the following page for a reference on valid type expressions:");
    diag.info(
        "https://typing.python.org/en/latest/spec/annotations.html#type-and-annotation-expressions",
    );
    diag
}

pub(crate) fn report_runtime_check_against_non_runtime_checkable_protocol(
    context: &InferContext,
    call: &ast::ExprCall,
    protocol: ProtocolClass,
    function: KnownFunction,
) {
    let Some(builder) = context.report_lint(&ISINSTANCE_AGAINST_PROTOCOL, call) else {
        return;
    };
    let db = context.db();
    let class_name = protocol.name(db);
    let function_name = function.name();
    let mut diagnostic = builder.into_diagnostic(format_args!(
        "Class `{class_name}` cannot be used as the second argument to `{function_name}`",
    ));
    diagnostic.set_primary_message("This call will raise `TypeError` at runtime");
    add_non_runtime_checkable_protocol_context(db, &mut diagnostic, protocol);
    diagnostic.info(format_args!(
        "A protocol class can only be used in `{function_name}` checks if it is decorated \
            with `@typing.runtime_checkable` or `@typing_extensions.runtime_checkable`"
    ));
    diagnostic.info(format_args!("See {RUNTIME_CHECKABLE_DOCS_URL}"));
}

pub(crate) fn report_issubclass_check_against_protocol_with_non_method_members<'db>(
    context: &'db InferContext<'db, '_>,
    call: &ast::ExprCall,
    protocol: ProtocolClass<'db>,
    non_method_members: &[ProtocolMember<'db, 'db>],
) {
    let Some(builder) = context.report_lint(&ISINSTANCE_AGAINST_PROTOCOL, call) else {
        return;
    };
    let db = context.db();
    let class_name = protocol.name(db);
    let mut diagnostic = builder.into_diagnostic(format_args!(
        "Class `{class_name}` cannot be used as the second argument to `issubclass`",
    ));
    diagnostic.set_concise_message(format_args!(
        "`{class_name}` cannot be used as the second argument to `issubclass` \
        as it is a protocol with non-method members"
    ));
    diagnostic.set_primary_message("This call will raise `TypeError` at runtime");
    if let [single_member] = non_method_members {
        let mut sub = SubDiagnostic::new(
            SubDiagnosticSeverity::Info,
            "A protocol class cannot be used in `issubclass` checks \
            if it has non-method members",
        );
        if let Some(definition) = single_member.definition() {
            let module = parsed_module(db, definition.python_file(db)).load(db);
            let span = Span::from(definition.focus_range(db, &module));
            sub.annotate(Annotation::primary(span).message(format_args!(
                "Non-method member `{}` declared here",
                single_member.name()
            )));
        }
        diagnostic.sub(sub);
    } else {
        diagnostic.info(
            "A protocol class cannot be used in `issubclass` checks \
            if it has non-method members",
        );
        let mut sub = SubDiagnostic::new(
            SubDiagnosticSeverity::Info,
            format_args!(
                "`{class_name}` has non-method members {}",
                format_enumeration(non_method_members.iter().map(ProtocolMember::name))
            ),
        );
        if let Some((name, definition)) = non_method_members
            .iter()
            .find_map(|member| Some((member.name(), member.definition()?)))
        {
            let module = parsed_module(db, definition.python_file(db)).load(db);
            let span = Span::from(definition.focus_range(db, &module));
            sub.annotate(
                Annotation::primary(span)
                    .message(format_args!("Non-method member `{name}` declared here")),
            );
        }
        diagnostic.sub(sub);
    }
}

pub(crate) fn report_runtime_check_against_typed_dict(
    context: &InferContext,
    call: &ast::ExprCall,
    class: ClassLiteral,
    function: KnownFunction,
) {
    let Some(builder) = context.report_lint(&ISINSTANCE_AGAINST_TYPED_DICT, call) else {
        return;
    };
    let class_name = class.name(context.db());
    let mut diagnostic = builder.into_diagnostic(format_args!(
        "`TypedDict` class `{class_name}` cannot be used as the second argument to `{function_name}`",
        function_name = function.name()
    ));
    diagnostic.set_primary_message("This call will raise `TypeError` at runtime");
}

pub(crate) fn report_match_pattern_against_non_runtime_checkable_protocol<T: Ranged>(
    context: &InferContext,
    pattern_cls: T,
    protocol: ProtocolClass,
) {
    let Some(builder) = context.report_lint(&ISINSTANCE_AGAINST_PROTOCOL, pattern_cls) else {
        return;
    };
    let db = context.db();
    let class_name = protocol.name(db);
    let mut diagnostic = builder.into_diagnostic(format_args!(
        "Class `{class_name}` cannot be used in a class pattern",
    ));
    diagnostic.set_primary_message("This will raise `TypeError` at runtime");
    add_non_runtime_checkable_protocol_context(db, &mut diagnostic, protocol);
    diagnostic.info(
        "A protocol class can only be used in a match class pattern if it is decorated \
            with `@typing.runtime_checkable` or `@typing_extensions.runtime_checkable`",
    );
    diagnostic.info(format_args!("See {RUNTIME_CHECKABLE_DOCS_URL}"));
}

pub(crate) fn report_match_pattern_against_typed_dict<T: Ranged>(
    context: &InferContext,
    pattern_cls: T,
    class: ClassLiteral,
) {
    let Some(builder) = context.report_lint(&ISINSTANCE_AGAINST_TYPED_DICT, pattern_cls) else {
        return;
    };
    let db = context.db();
    let class_name = class.name(db);
    let mut diagnostic = builder.into_diagnostic(format_args!(
        "`TypedDict` class `{class_name}` cannot be used in a class pattern",
    ));
    diagnostic.set_primary_message("This will raise `TypeError` at runtime");
}

fn add_non_runtime_checkable_protocol_context<'db>(
    db: &'db dyn Db,
    diagnostic: &mut LintDiagnosticGuard<'db, '_>,
    protocol: ProtocolClass,
) {
    let class_name = protocol.name(db);
    let mut class_def_diagnostic = SubDiagnostic::new(
        SubDiagnosticSeverity::Info,
        format_args!(
            "`{class_name}` is declared as a protocol class, \
                but it is not declared as runtime-checkable"
        ),
    );
    class_def_diagnostic.annotate(
        Annotation::primary(protocol.definition_span(db))
            .message(format_args!("`{class_name}` declared here")),
    );
    diagnostic.sub(class_def_diagnostic);
}

pub(crate) fn report_attempted_protocol_instantiation(
    context: &InferContext,
    call: &ast::ExprCall,
    protocol: ProtocolClass,
) {
    let Some(builder) = context.report_lint(&CALL_NON_CALLABLE, call) else {
        return;
    };
    let db = context.db();
    let class_name = protocol.name(db);
    let mut diagnostic =
        builder.into_diagnostic(format_args!("Cannot instantiate class `{class_name}`"));
    diagnostic.set_primary_message("This call will raise `TypeError` at runtime");

    let mut class_def_diagnostic = SubDiagnostic::new(
        SubDiagnosticSeverity::Info,
        format_args!("Protocol classes cannot be instantiated"),
    );
    class_def_diagnostic.annotate(
        Annotation::primary(protocol.definition_span(db))
            .message(format_args!("`{class_name}` declared as a protocol here")),
    );
    diagnostic.sub(class_def_diagnostic);
}

pub(crate) fn report_call_to_abstract_method(
    context: &InferContext,
    call: &ast::ExprCall,
    function: FunctionType,
    method_kind: &str,
) {
    let Some(builder) = context.report_lint(&CALL_ABSTRACT_METHOD, call) else {
        return;
    };
    let db = context.db();
    let name = function.name(db);
    let mut diag = builder.into_diagnostic(format_args!("Cannot call `{name}` on class object"));
    diag.set_primary_message(format_args!(
        "`{name}` is an abstract {method_kind} with a trivial body"
    ));
    let span = abstract_method_span(
        db,
        function,
        AbstractMethodAnnotationPolicy::AlwaysIncludeBody,
    );
    diag.annotate(
        Annotation::secondary(span).message(format_args!("Method `{name}` defined here")),
    );
}

pub(super) fn abstract_method_span<'db>(
    db: &'db dyn Db,
    function: FunctionType<'db>,
    policy: AbstractMethodAnnotationPolicy,
) -> Span {
    let (_, implementation) = function.overloads_and_implementation(db);

    let Some(implementation) = implementation else {
        return function.spans(db).name;
    };

    let file = function.file(db);
    let module = parsed_module(db, function.python_file(db)).load(db);
    let node = implementation.node(db, file, &module);
    let source_text = source_text(db, file);

    if policy == AbstractMethodAnnotationPolicy::ExcludeVerboseBody
        && source_text.line_start(node.name.end()) != source_text.line_start(node.end())
    {
        return implementation.spans(db).decorators_and_header;
    }

    if let [single_stmt] = &*node.body
        && source_text.line_start(single_stmt.start()) == source_text.line_start(single_stmt.end())
    {
        Span::from(file).with_range(node.range())
    } else {
        implementation.spans(db).decorators_and_header
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(super) enum AbstractMethodAnnotationPolicy {
    AlwaysIncludeBody,
    ExcludeVerboseBody,
}

pub(crate) fn report_undeclared_protocol_member(
    context: &InferContext,
    definition: Definition,
    protocol_class: ProtocolClass,
    class_symbol_table: &PlaceTable,
) {
    /// We want to avoid suggesting an annotation for e.g. `x = None`,
    /// because the user almost certainly doesn't want to write `x: None = None`.
    /// We also want to avoid suggesting invalid syntax such as `x: <class 'int'> = int`.
    fn should_give_hint<'db>(db: &'db dyn Db, ty: Type<'db>) -> bool {
        let class = match ty {
            Type::ProtocolInstance(ProtocolInstanceType {
                inner: Protocol::FromClass(_),
                ..
            }) => return true,
            Type::SubclassOf(subclass_of) => match subclass_of.subclass_of() {
                SubclassOfInner::Class(class) => class,
                SubclassOfInner::Protocol(_) => return true,
                SubclassOfInner::Dynamic(DynamicType::Any) => return true,
                SubclassOfInner::Dynamic(_) | SubclassOfInner::TypeVar(_) => return false,
            },
            Type::NominalInstance(instance) => instance.class(db),
            Type::Union(union) => {
                return union
                    .elements(db)
                    .iter()
                    .all(|elem| should_give_hint(db, *elem));
            }
            _ => return false,
        };

        !matches!(
            class.known(db),
            Some(KnownClass::NoneType | KnownClass::EllipsisType)
        )
    }

    let db = context.db();

    let Some(builder) = context.report_lint(
        &AMBIGUOUS_PROTOCOL_MEMBER,
        definition.full_range(db, context.module()),
    ) else {
        return;
    };

    let ScopedPlaceId::Symbol(symbol_id) = definition.place(db) else {
        return;
    };

    let symbol_name = class_symbol_table.symbol(symbol_id).name();

    let mut diagnostic = builder
        .into_diagnostic("Cannot assign to undeclared variable in the body of a protocol class");

    if definition.kind(db).is_unannotated_assignment() {
        let binding_type = binding_type(db, definition);

        let suggestion = binding_type.promote(db);

        if should_give_hint(db, suggestion) {
            diagnostic.set_primary_message(format_args!(
                "Consider adding an annotation, e.g. `{symbol_name}: {} = ...`",
                suggestion.display(db)
            ));
        } else {
            diagnostic.set_primary_message(format_args!(
                "Consider adding an annotation for `{symbol_name}`"
            ));
        }
    } else {
        diagnostic.set_primary_message(format_args!(
            "`{symbol_name}` is not declared as a protocol member"
        ));
    }

    add_undeclared_protocol_member_context(
        &mut diagnostic,
        db,
        protocol_class,
        symbol_name,
        "Assigning to an undeclared variable in a protocol class leads to an ambiguous interface",
    );
}

pub(crate) fn report_undeclared_protocol_attribute(
    context: &InferContext,
    target: &ast::ExprAttribute,
    protocol_class: ProtocolClass,
) {
    let db = context.db();
    let Some(builder) = context.report_lint(&AMBIGUOUS_PROTOCOL_MEMBER, target) else {
        return;
    };

    let symbol_name = target.attr.as_str();
    let mut diagnostic =
        builder.into_diagnostic("Cannot assign to an undeclared attribute in a protocol method");
    diagnostic.set_primary_message(format_args!(
        "`{symbol_name}` is not declared as a protocol member"
    ));

    add_undeclared_protocol_member_context(
        &mut diagnostic,
        db,
        protocol_class,
        symbol_name,
        "Assigning to an undeclared attribute in a protocol method leads to an ambiguous interface",
    );
}

fn add_undeclared_protocol_member_context(
    diagnostic: &mut Diagnostic,
    db: &dyn Db,
    protocol_class: ProtocolClass,
    symbol_name: &str,
    ambiguity_message: &'static str,
) {
    let class_name = protocol_class.name(db);

    let mut class_def_diagnostic =
        SubDiagnostic::new(SubDiagnosticSeverity::Info, ambiguity_message);
    class_def_diagnostic.annotate(
        Annotation::primary(protocol_class.definition_span(db))
            .message(format_args!("`{class_name}` declared as a protocol here")),
    );
    diagnostic.sub(class_def_diagnostic);

    diagnostic.info(format_args!(
        "No declarations found for `{symbol_name}` \
        in the body of `{class_name}` or any of its superclasses"
    ));
}

pub(crate) fn report_duplicate_bases(
    context: &InferContext,
    class: StaticClassLiteral,
    duplicate_base_error: &DuplicateBaseError,
    bases_list: &[ExpandedClassBaseEntry],
) {
    let db = context.db();

    let Some(builder) = context.report_lint(&DUPLICATE_BASE, class.focus_range(db)) else {
        return;
    };

    let DuplicateBaseError {
        duplicate_base,
        first_index,
        later_indices,
    } = duplicate_base_error;

    let duplicate_name = duplicate_base.name(db);

    let mut diagnostic =
        builder.into_diagnostic(format_args!("Duplicate base class `{duplicate_name}`"));

    diagnostic.info(format_args!(
        "Definition of class `{}` will raise `TypeError` at runtime",
        class.name(db)
    ));

    let first_base = bases_list[*first_index].source_node();
    diagnostic.annotate(context.secondary(first_base).message(format_args!(
        "Class `{duplicate_name}` first included in bases list here"
    )));

    for index in later_indices {
        let repeated_base = bases_list[*index].source_node();
        diagnostic.annotate(
            Annotation::primary(context.span(repeated_base))
                .message(format_args!("Class `{duplicate_name}` later repeated here")),
        );
    }
}

pub(crate) fn report_invalid_or_unsupported_base(
    context: &InferContext,
    base_node: &ast::Expr,
    base_type: Type,
    class: StaticClassLiteral,
) {
    let db = context.db();
    let instance_of_type = KnownClass::Type.to_instance(db);

    if base_type.is_assignable_to(db, instance_of_type) {
        report_unsupported_base(context, base_node, base_type, class);
        return;
    }

    if let Type::KnownInstance(KnownInstanceType::NewType(newtype)) = base_type {
        let Some(builder) = context.report_lint(&INVALID_BASE, base_node) else {
            return;
        };
        let mut diagnostic = builder.into_diagnostic("Cannot subclass an instance of NewType");
        diagnostic.info(format_args!(
            "Perhaps you were looking for: `{} = NewType('{}', {})`",
            class.name(context.db()),
            class.name(context.db()),
            newtype.name(context.db()),
        ));
        diagnostic.info(format_args!(
            "Definition of class `{}` will raise `TypeError` at runtime",
            class.name(context.db())
        ));
        return;
    }

    let tuple_of_types = Type::homogeneous_tuple(db, instance_of_type);

    let explain_mro_entries = |diagnostic: &mut LintDiagnosticGuard| {
        diagnostic.info(
            "An instance type is only a valid class base \
            if it has a valid `__mro_entries__` method",
        );
    };

    match base_type.try_call_dunder(
        db,
        "__mro_entries__",
        CallArguments::positional([tuple_of_types]),
        TypeContext::default(),
    ) {
        Ok(ret) => {
            if ret.return_type(db).is_assignable_to(db, tuple_of_types) {
                report_unsupported_base(context, base_node, base_type, class);
            } else {
                let Some(mut diagnostic) =
                    report_invalid_base(context, base_node, base_type, class)
                else {
                    return;
                };
                explain_mro_entries(&mut diagnostic);
                diagnostic.info(format_args!(
                    "Type `{}` has an `__mro_entries__` method, but it does not return a tuple of types",
                    base_type.display(db)
                ));
            }
        }
        Err(mro_entries_call_error) => {
            let Some(mut diagnostic) = report_invalid_base(context, base_node, base_type, class)
            else {
                return;
            };

            match mro_entries_call_error {
                CallDunderError::MethodNotAvailable => {}
                CallDunderError::PossiblyUnbound { unbound_on, .. } => {
                    explain_mro_entries(&mut diagnostic);
                    diagnostic.info(format_args!(
                        "Type `{}` may have an `__mro_entries__` attribute, but it may be missing",
                        base_type.display(db)
                    ));
                    if let Some(unbound_on) = unbound_on {
                        for ty in unbound_on {
                            diagnostic.info(format_args!(
                                "`{}` does not implement `__mro_entries__`",
                                ty.display(db)
                            ));
                        }
                    }
                }
                CallDunderError::CallError(CallErrorKind::NotCallable, _, _) => {
                    explain_mro_entries(&mut diagnostic);
                    diagnostic.info(format_args!(
                        "Type `{}` has an `__mro_entries__` attribute, but it is not callable",
                        base_type.display(db)
                    ));
                }
                CallDunderError::CallError(CallErrorKind::BindingError, _, _) => {
                    explain_mro_entries(&mut diagnostic);
                    diagnostic.info(format_args!(
                        "Type `{}` has an `__mro_entries__` method, \
                        but it cannot be called with the expected arguments",
                        base_type.display(db)
                    ));
                    diagnostic.info(
                        "Expected a signature at least as permissive as \
                        `def __mro_entries__(self, bases: tuple[type, ...], /) -> tuple[type, ...]`"
                    );
                }
                CallDunderError::CallError(CallErrorKind::PossiblyNotCallable, _, _) => {
                    explain_mro_entries(&mut diagnostic);
                    diagnostic.info(format_args!(
                        "Type `{}` has an `__mro_entries__` method, \
                        but it may not be callable",
                        base_type.display(db)
                    ));
                }
            }
        }
    }
}

pub(crate) fn report_unsupported_base(
    context: &InferContext,
    base_node: &ast::Expr,
    base_type: Type,
    class: StaticClassLiteral,
) {
    let Some(builder) = context.report_lint(&UNSUPPORTED_BASE, base_node) else {
        return;
    };
    let db = context.db();
    let mut diagnostic = builder.into_diagnostic("Unsupported class base");
    diagnostic.set_primary_message(format_args!("Has type `{}`", base_type.display(db)));
    diagnostic.set_concise_message(format_args!(
        "Unsupported class base with type `{}`",
        base_type.display(db)
    ));
    diagnostic.info(format_args!(
        "ty cannot resolve a consistent method resolution order (MRO) for class `{}` due to this base",
        class.name(db)
    ));
    diagnostic.info("Only class objects or `Any` are supported as class bases");
}

fn report_invalid_base<'ctx, 'db>(
    context: &'ctx InferContext<'db, '_>,
    base_node: &ast::Expr,
    base_type: Type<'db>,
    class: StaticClassLiteral<'db>,
) -> Option<LintDiagnosticGuard<'ctx, 'db>> {
    let builder = context.report_lint(&INVALID_BASE, base_node)?;
    let mut diagnostic = builder.into_diagnostic(format_args!(
        "Invalid class base with type `{}`",
        base_type.display(context.db())
    ));
    diagnostic.info(format_args!(
        "Definition of class `{}` will raise `TypeError` at runtime",
        class.name(context.db())
    ));
    Some(diagnostic)
}

pub(crate) fn report_invalid_key_on_typed_dict<'db>(
    context: &InferContext<'db, '_>,
    typed_dict_node: AnyNodeRef,
    key_node: AnyNodeRef,
    typed_dict_ty: Type<'db>,
    full_object_ty: Option<Type<'db>>,
    key_ty: Type<'db>,
    items: &TypedDictSchema<'db>,
) {
    let db = context.db();
    if let Some(builder) = context.report_lint(&INVALID_KEY, key_node) {
        match key_ty.as_string_literal() {
            Some(key) => {
                let key = key.value(db);
                let typed_dict_name = typed_dict_ty.display(db);

                let mut diagnostic = builder.into_diagnostic(format_args!(
                    "Unknown key \"{key}\" for TypedDict `{typed_dict_name}`",
                ));

                diagnostic.annotate(if let Some(full_object_ty) = full_object_ty {
                    context.secondary(typed_dict_node).message(format_args!(
                        "TypedDict `{typed_dict_name}` in {kind} type `{full_object_ty}`",
                        kind = if full_object_ty.is_union() {
                            "union"
                        } else {
                            "intersection"
                        },
                        full_object_ty = full_object_ty.display(db)
                    ))
                } else {
                    context
                        .secondary(typed_dict_node)
                        .message(format_args!("TypedDict `{typed_dict_name}`"))
                });

                let existing_keys = items.keys().map(Name::as_str);

                if !matches!(full_object_ty, Some(Type::Union(_) | Type::Intersection(_)))
                    && let Some(suggestion) = did_you_mean(existing_keys, key)
                {
                    if let AnyNodeRef::ExprStringLiteral(literal) = key_node {
                        let quoted_suggestion = format!(
                            "{quote}{suggestion}{quote}",
                            quote = literal.value.first_literal_flags().quote_str()
                        );
                        diagnostic
                            .set_primary_message(format_args!("Did you mean {quoted_suggestion}?"));
                        diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
                            quoted_suggestion,
                            key_node.range(),
                        )));
                    } else {
                        diagnostic.set_primary_message(format_args!(
                            "Unknown key \"{key}\" - did you mean \"{suggestion}\"?",
                        ));
                    }
                    diagnostic.set_concise_message(format_args!(
                        "Unknown key \"{key}\" for TypedDict `{typed_dict_name}` - did you mean \"{suggestion}\"?",
                    ));
                } else {
                    diagnostic.set_primary_message(format_args!("Unknown key \"{key}\""));
                    if let Some(full_ty) = full_object_ty {
                        diagnostic.set_concise_message(format_args!(
                            "Unknown key \"{key}\" for TypedDict `{typed_dict_name}` (subscripted object has type `{full_ty}`)",
                            full_ty = full_ty.display(db),
                        ));
                    } else {
                        diagnostic.set_concise_message(format_args!(
                            "Unknown key \"{key}\" for TypedDict `{typed_dict_name}`",
                        ));
                    }
                }
            }
            _ => {
                let mut diagnostic = builder.into_diagnostic(format_args!(
                    "TypedDict `{}` can only be subscripted with a string literal key, \
                     got key of type `{}`",
                    typed_dict_ty.display(db),
                    key_ty.display(db),
                ));

                if let Some(full_object_ty) = full_object_ty {
                    diagnostic.info(format_args!(
                        "The full type of the subscripted object is `{}`",
                        full_object_ty.display(db)
                    ));
                }
            }
        }
    }
}

pub(super) fn report_namedtuple_field_without_default_after_field_with_default<'db>(
    context: &InferContext<'db, '_>,
    class: StaticClassLiteral<'db>,
    (field, field_def): (&str, Option<Definition<'db>>),
    (field_with_default, field_with_default_def): &(Name, Option<Definition<'db>>),
) {
    let db = context.db();
    let module = context.module();

    let diagnostic_range = field_def
        .map(|definition| definition.kind(db).full_range(module))
        .unwrap_or_else(|| class.header_range(db));

    let Some(builder) = context.report_lint(&INVALID_NAMED_TUPLE, diagnostic_range) else {
        return;
    };
    let mut diagnostic = builder.into_diagnostic(
        "NamedTuple field without default value cannot follow field(s) with default value(s)",
    );

    diagnostic.set_primary_message(format_args!(
        "Field `{field}` defined here without a default value",
    ));

    let Some(field_with_default_range) =
        field_with_default_def.map(|definition| definition.kind(db).full_range(module))
    else {
        return;
    };

    // If the end-of-scope definition in the class scope of the field-with-a-default-value
    // occurs after the range of the field-without-a-default-value,
    // avoid adding a subdiagnostic that points to the definition of the
    // field-with-a-default-value. It's confusing to talk about a field "before" the
    // field without the default value but then point to a definition that actually
    // occurs after the field without-a-default-value.
    if field_with_default_range.end() < diagnostic_range.start() {
        diagnostic.annotate(
            Annotation::secondary(context.span(field_with_default_range)).message(format_args!(
                "Earlier field `{field_with_default}` defined here with a default value",
            )),
        );
    } else {
        diagnostic.info(format_args!(
            "Earlier field `{field_with_default}` was defined with a default value",
        ));
    }
}

pub(super) fn report_named_tuple_field_with_leading_underscore<'db>(
    context: &InferContext<'db, '_>,
    class: StaticClassLiteral<'db>,
    field_name: &str,
    field_definition: Option<Definition<'db>>,
) {
    let db = context.db();
    let module = context.module();

    let diagnostic_range = field_definition
        .map(|definition| definition.kind(db).full_range(module))
        .unwrap_or_else(|| class.header_range(db));

    let Some(builder) = context.report_lint(&INVALID_NAMED_TUPLE, diagnostic_range) else {
        return;
    };
    let mut diagnostic =
        builder.into_diagnostic("NamedTuple field name cannot start with an underscore");

    if field_definition.is_some() {
        diagnostic.set_primary_message(
            "Class definition will raise `TypeError` at runtime due to this field",
        );
    } else {
        diagnostic.set_primary_message(format_args!(
            "Class definition will raise `TypeError` at runtime due to field `{field_name}`",
        ));
    }

    diagnostic.set_concise_message(format_args!(
        "NamedTuple field `{field_name}` cannot start with an underscore"
    ));
}

pub(crate) fn report_missing_typed_dict_key<'db>(
    context: &InferContext<'db, '_>,
    constructor_node: AnyNodeRef,
    typed_dict_ty: Type<'db>,
    missing_field: &str,
) {
    let db = context.db();
    if let Some(builder) = context.report_lint(&MISSING_TYPED_DICT_KEY, constructor_node) {
        let typed_dict_name = typed_dict_ty.display(db);
        builder.into_diagnostic(format_args!(
            "Missing required key '{missing_field}' in TypedDict `{typed_dict_name}` constructor",
        ));
    }
}

pub(crate) fn report_cannot_pop_required_field_on_typed_dict<'db>(
    context: &InferContext<'db, '_>,
    key_node: AnyNodeRef,
    typed_dict_ty: Type<'db>,
    field_name: &str,
) {
    let db = context.db();
    if let Some(builder) = context.report_lint(&INVALID_ARGUMENT_TYPE, key_node) {
        let typed_dict_name = typed_dict_ty.display(db);
        builder.into_diagnostic(format_args!(
            "Cannot pop required field '{field_name}' from TypedDict `{typed_dict_name}`",
        ));
    }
}

/// Enum representing the reason why a key cannot be deleted from a `TypedDict`.
#[derive(Copy, Clone)]
pub(crate) enum TypedDictDeleteErrorKind {
    /// The key exists but is required (not `NotRequired`)
    RequiredKey,
    /// The key refers to a read-only extra item.
    ReadOnlyExtraItem,
    /// The key does not exist in the `TypedDict`
    UnknownKey,
}

pub(crate) fn report_cannot_delete_typed_dict_key<'db>(
    context: &InferContext<'db, '_>,
    key_node: AnyNodeRef,
    typed_dict_ty: TypedDictType<'db>,
    field_name: &str,
    field: Option<&crate::types::typed_dict::TypedDictField<'db>>,
    error_kind: TypedDictDeleteErrorKind,
) {
    let db = context.db();
    let Some(builder) = context.report_lint(&INVALID_ARGUMENT_TYPE, key_node) else {
        return;
    };

    let typed_dict_name = Type::TypedDict(typed_dict_ty).display(db);

    let mut diagnostic = match error_kind {
        TypedDictDeleteErrorKind::RequiredKey => builder.into_diagnostic(format_args!(
            "Cannot delete required key \"{field_name}\" from TypedDict `{typed_dict_name}`"
        )),
        TypedDictDeleteErrorKind::ReadOnlyExtraItem => builder.into_diagnostic(format_args!(
            "Cannot delete read-only extra item \"{field_name}\" from TypedDict `{typed_dict_name}`"
        )),
        TypedDictDeleteErrorKind::UnknownKey => builder.into_diagnostic(format_args!(
            "Cannot delete unknown key \"{field_name}\" from TypedDict `{typed_dict_name}`"
        )),
    };

    // Add sub-diagnostic pointing to where the field is defined (if available)
    if let Some(field) = field
        && let Some(declaration) = field.first_declaration()
    {
        let file = declaration.file(db);
        let module = parsed_module(db, declaration.python_file(db)).load(db);

        let mut sub = SubDiagnostic::new(SubDiagnosticSeverity::Info, "Field defined here");
        for message in [
            format_args!("`{field_name}` declared as required here"),
            format_args!("Consider making it `NotRequired`"),
        ] {
            sub.annotate(
                Annotation::secondary(
                    Span::from(file).with_range(declaration.full_range(db, &module).range()),
                )
                .message(message),
            );
        }

        if let Some(class) = typed_dict_ty.defining_class() {
            sub.annotate(
                Annotation::secondary(
                    Span::from(file).with_range(class.class_literal(db).header_range(db)),
                )
                .message(format_args!("`{}` defined here", class.name(db))),
            );
        }

        diagnostic.sub(sub);
    }

    // Add hint about how to allow deletion
    if matches!(error_kind, TypedDictDeleteErrorKind::RequiredKey) {
        diagnostic.info(
            "Only keys marked as `NotRequired` (or in a TypedDict with `total=False`) can be deleted",
        );
    }
}

pub(crate) fn report_invalid_type_param_order<'db>(
    context: &InferContext<'db, '_>,
    class: StaticClassLiteral<'db>,
    node: &ast::StmtClassDef,
    typevar_with_default: TypeVarInstance<'db>,
    invalid_later_typevars: &[TypeVarInstance<'db>],
) {
    let db = context.db();

    let base_index = class
        .explicit_bases(db)
        .iter()
        .position(|base| {
            matches!(
                base,
                Type::KnownInstance(
                    KnownInstanceType::SubscriptedProtocol(_)
                        | KnownInstanceType::SubscriptedGeneric(_)
                )
            )
        })
        .expect(
            "It should not be possible for a class to have a legacy generic context \
            if it does not inherit from `Protocol[]` or `Generic[]`",
        );

    let base_node = &node.bases()[base_index];

    let primary_diagnostic_range = base_node
        .as_subscript_expr()
        .map(|subscript| &*subscript.slice)
        .unwrap_or(base_node)
        .range();

    let Some(builder) = context.report_lint(&INVALID_GENERIC_CLASS, primary_diagnostic_range)
    else {
        return;
    };

    let mut diagnostic = builder.into_diagnostic(
        "Type parameters without defaults cannot follow type parameters with defaults",
    );

    diagnostic.set_concise_message(format_args!(
        "Type parameter `{}` without a default cannot follow earlier parameter `{}` with a default",
        invalid_later_typevars[0].name(db),
        typevar_with_default.name(db),
    ));

    if let [single_typevar] = invalid_later_typevars {
        diagnostic.set_primary_message(format_args!(
            "Type variable `{}` does not have a default",
            single_typevar.name(db),
        ));
    } else {
        let later_typevars =
            format_enumeration(invalid_later_typevars.iter().map(|tv| tv.name(db)));
        diagnostic.set_primary_message(format_args!(
            "Type variables {later_typevars} do not have defaults",
        ));
    }

    diagnostic.annotate(
        Annotation::primary(Span::from(context.file()).with_range(primary_diagnostic_range))
            .message(format_args!(
                "Earlier TypeVar `{}` does",
                typevar_with_default.name(db)
            )),
    );

    for tvar in [typevar_with_default, invalid_later_typevars[0]] {
        let Some(definition) = tvar.definition(db) else {
            continue;
        };
        diagnostic.annotate(
            Annotation::secondary(Span::from(
                definition.full_range(db, &parsed_module(db, definition.python_file(db)).load(db)),
            ))
            .message(format_args!("`{}` defined here", tvar.name(db))),
        );
    }
}

pub(crate) fn report_invalid_typevar_default_reference<'db>(
    context: &InferContext<'db, '_>,
    class: StaticClassLiteral<'db>,
    typevar_with_bad_default: TypeVarInstance<'db>,
    referenced_typevar: TypeVarInstance<'db>,
    is_later_in_list: bool,
) {
    let db = context.db();

    let Some(builder) = context.report_lint(&INVALID_GENERIC_CLASS, class.header_range(db)) else {
        return;
    };

    let mut diagnostic = if is_later_in_list {
        builder.into_diagnostic(format_args!(
            "Default of `{}` cannot reference later type parameter `{}`",
            typevar_with_bad_default.name(db),
            referenced_typevar.name(db),
        ))
    } else {
        builder.into_diagnostic(format_args!(
            "Default of `{}` cannot reference out-of-scope type variable `{}`",
            typevar_with_bad_default.name(db),
            referenced_typevar.name(db),
        ))
    };

    let typevars_to_annotate = if is_later_in_list {
        &[typevar_with_bad_default, referenced_typevar][..]
    } else {
        &[typevar_with_bad_default][..]
    };

    for tvar in typevars_to_annotate {
        let Some(definition) = tvar.definition(db) else {
            continue;
        };
        diagnostic.annotate(
            Annotation::secondary(Span::from(
                definition.full_range(db, &parsed_module(db, definition.python_file(db)).load(db)),
            ))
            .message(format_args!("`{}` defined here", tvar.name(db))),
        );
    }
}

/// Report when separate bases contribute incompatible specializations of a generic ancestor.
///
/// For example, if `A` inherits `G[int]` and `B` inherits `G[str]`, neither
/// `class C(A, B): ...` nor `type("C", (A, B), {})` defines a valid class.
/// The conflicting specialization can also be inherited indirectly:
///
/// ```python
/// class Grandparent(Generic[T1, T2]): ...
/// class Parent(Grandparent[T1, T2]): ...
/// class BadChild(Parent[T1, T2], Grandparent[T2, T1]): ...  # Error
/// ```
///
/// Returns `true` if an inconsistency was found, even when it is inherited or the diagnostic is
/// disabled.
pub(crate) fn report_inconsistent_generic_bases<'db>(
    context: &InferContext<'db, '_>,
    header_range: TextRange,
    explicit_bases: &[Type<'db>],
    base_nodes: Option<&[ast::Expr]>,
) -> bool {
    let db = context.db();
    // Maps each generic ancestor's class literal to the first
    // specialization seen and the index of the explicit base it
    // came from.
    let mut ancestor_specs =
        FxHashMap::<StaticClassLiteral<'db>, (GenericAlias<'db>, usize)>::default();

    for (i, base) in explicit_bases.iter().enumerate() {
        let base_class = match base {
            Type::GenericAlias(alias) => ClassType::Generic(*alias),
            Type::ClassLiteral(class) if class.generic_context(db).is_none() => {
                ClassType::NonGeneric(*class)
            }
            _ => continue,
        };

        for supercls in base_class.iter_mro(db) {
            let ClassBase::Class(ClassType::Generic(supercls_alias)) = supercls else {
                continue;
            };
            let origin = supercls_alias.origin(db);

            if let Some(&(earlier_alias, earlier_idx)) = ancestor_specs.get(&origin) {
                if earlier_alias
                    .specialization(db)
                    .types(db)
                    .iter()
                    .zip(supercls_alias.specialization(db).types(db))
                    .any(|(t1, t2)| !t1.is_dynamic() && !t2.is_dynamic() && t1 != t2)
                {
                    if earlier_idx == i {
                        return true;
                    }
                    let Some(builder) = context.report_lint(&INVALID_GENERIC_CLASS, header_range)
                    else {
                        return true;
                    };
                    let mut diagnostic = builder.into_diagnostic(format_args!(
                        "Inconsistent type arguments for `{}` among class bases",
                        origin.name(db)
                    ));
                    let later_is_direct = matches!(
                        base,
                        Type::GenericAlias(alias) if alias.origin(db) == origin
                    );

                    if let (Some(earlier_base), Some(later_base)) = (
                        base_nodes.and_then(|nodes| nodes.get(earlier_idx)),
                        base_nodes.and_then(|nodes| nodes.get(i)),
                    ) {
                        diagnostic.annotate(context.secondary(earlier_base).message(format_args!(
                            "Earlier class base inherits from `{}`",
                            earlier_alias.display(db)
                        )));
                        let later_annotation = context.secondary(later_base);
                        diagnostic.annotate(if later_is_direct {
                            later_annotation.message(format_args!(
                                "Later class base is `{}`",
                                supercls_alias.display(db)
                            ))
                        } else {
                            later_annotation.message(format_args!(
                                "Later class base inherits from `{}`",
                                supercls_alias.display(db)
                            ))
                        });
                    } else {
                        diagnostic.info(format_args!(
                            "Earlier class base inherits from `{}`",
                            earlier_alias.display(db)
                        ));
                        if later_is_direct {
                            diagnostic.info(format_args!(
                                "Later class base is `{}`",
                                supercls_alias.display(db)
                            ));
                        } else {
                            diagnostic.info(format_args!(
                                "Later class base inherits from `{}`",
                                supercls_alias.display(db)
                            ));
                        }
                    }
                    diagnostic.set_concise_message(format_args!(
                        "Inconsistent type arguments: class cannot inherit from both `{}` and `{}`",
                        supercls_alias.display(db),
                        earlier_alias.display(db)
                    ));
                    return true;
                }
            } else if !supercls_alias
                .specialization(db)
                .types(db)
                .iter()
                .all(Type::is_dynamic)
            {
                ancestor_specs.insert(origin, (supercls_alias, i));
            }
        }
    }

    false
}

pub(crate) fn report_shadowed_type_variable<'db>(
    context: &InferContext<'db, '_>,
    typevar_name: &ast::name::Name,
    kind: &str,
    name: &ast::name::Name,
    range: TextRange,
    type_var_kind: TypeVarKind,
    other_typevar: BoundTypeVarInstance<'db>,
) {
    let db = context.db();
    let Some(builder) = context.report_lint(&SHADOWED_TYPE_VARIABLE, range) else {
        return;
    };
    let typevar_kind = match type_var_kind {
        TypeVarKind::LegacyTypeVar
        | TypeVarKind::Pep695TypeVar
        | TypeVarKind::TypingSelf
        | TypeVarKind::Pep613Alias => "type variable",
        TypeVarKind::LegacyParamSpec | TypeVarKind::Pep695ParamSpec => "ParamSpec",
        TypeVarKind::LegacyTypeVarTuple | TypeVarKind::Pep695TypeVarTuple => "TypeVarTuple",
    };
    let mut diagnostic = builder.into_diagnostic(format_args!(
        "Generic {kind} `{name}` uses {typevar_kind} `{typevar_name}` already bound by an enclosing scope",
    ));
    diagnostic.set_concise_message(format_args!(
        "Generic {kind} `{name}` uses {typevar_kind} `{typevar_name}` already bound by an enclosing scope",
    ));
    diagnostic.set_primary_message(format_args!(
        "`{typevar_name}` used in {kind} definition here"
    ));
    let Some(other_definition) = other_typevar.binding_context(db).definition() else {
        return;
    };
    let span = match binding_type(db, other_definition) {
        Type::ClassLiteral(class) => class.header_span(db),
        Type::FunctionLiteral(function) => function.spans(db).signature,
        _ => return,
    };
    let other_typevar_kind = if other_typevar.is_paramspec(db) {
        "ParamSpec"
    } else if other_typevar.is_typevartuple(db) {
        "TypeVarTuple"
    } else {
        "Type variable"
    };
    diagnostic.annotate(Annotation::secondary(span).message(format_args!(
        "{other_typevar_kind} `{typevar_name}` is bound in this enclosing scope"
    )));
}

// I tried refactoring this function to placate Clippy,
// but it did not improve readability! -- AW.
#[expect(clippy::too_many_arguments)]
pub(super) fn report_invalid_method_override<'db>(
    context: &InferContext<'db, '_>,
    member: &str,
    subclass: ClassType<'db>,
    subclass_definition: Definition<'db>,
    subclass_function: FunctionType<'db>,
    superclass: ClassType<'db>,
    superclass_type: Type<'db>,
    superclass_method_kind: MethodKind,
    error_context: impl FnOnce() -> ErrorContextTree<'db>,
) {
    let db = context.db();

    let signature_span =
        |function: FunctionType<'db>| function.literal(db).last_definition.spans(db).signature;

    let subclass_definition_kind = subclass_definition.kind(db);
    let subclass_definition_signature_span = signature_span(subclass_function);

    // If the function was originally defined elsewhere and simply assigned
    // in the body of the class here, we cannot use the range associated with the `FunctionType`
    let diagnostic_range = if subclass_definition_kind.is_function_def() {
        subclass_definition_signature_span
            .range()
            .unwrap_or_else(|| {
                subclass_function
                    .node(db, context.file(), context.module())
                    .range
            })
    } else {
        subclass_definition.full_range(db, context.module()).range()
    };

    let Some(builder) = context.report_lint(&INVALID_METHOD_OVERRIDE, diagnostic_range) else {
        return;
    };

    let class_name = subclass.name(db);
    let superclass_name = superclass.name(db);

    let overridden_method = if class_name == superclass_name {
        let qualified_name = superclass.qualified_name(db);
        format!("{qualified_name}.{member}")
    } else {
        format!("{superclass_name}.{member}")
    };

    let mut diagnostic =
        builder.into_diagnostic(format_args!("Invalid override of method `{member}`"));

    diagnostic.set_primary_message(format_args!(
        "Definition is incompatible with `{overridden_method}`"
    ));

    let class_member = |cls: ClassType<'db>| {
        cls.class_member(db, member, MemberLookupPolicy::default())
            .place
    };

    if let Place::Defined(DefinedPlace {
        ty: Type::FunctionLiteral(subclass_function),
        ..
    }) = class_member(subclass)
        && let Place::Defined(DefinedPlace {
            ty: Type::FunctionLiteral(superclass_function),
            ..
        }) = class_member(superclass)
        && let Some(superclass_function_kind) =
            MethodDecorator::try_from_fn_type(db, superclass_function)
        && let Some(subclass_function_kind) =
            MethodDecorator::try_from_fn_type(db, subclass_function)
        && superclass_function_kind != subclass_function_kind
    {
        diagnostic.info(format_args!(
            "`{class_name}.{member}` is {subclass_function_kind} \
            but `{overridden_method}` is {superclass_function_kind}",
            superclass_function_kind = superclass_function_kind.description(),
            subclass_function_kind = subclass_function_kind.description(),
        ));
    }

    error_context().attach_to(context.db(), &mut diagnostic);

    diagnostic.info("This violates the Liskov Substitution Principle");

    if !subclass_definition_kind.is_function_def() {
        diagnostic.annotate(
            Annotation::secondary(subclass_definition_signature_span)
                .message(format_args!("Signature of `{class_name}.{member}`")),
        );
    }

    let Some((superclass_literal, _)) = superclass.static_class_literal(db) else {
        return;
    };
    let superclass_scope = superclass_literal.body_scope(db);

    match superclass_method_kind {
        MethodKind::NotSynthesized => {
            if let Some(superclass_symbol) = place_table(db, superclass_scope).symbol_id(member)
                && let Some(binding) = use_def_map(db, superclass_scope)
                    .end_of_scope_bindings(ScopedPlaceId::Symbol(superclass_symbol))
                    .next()
                && let Some(definition) = binding.binding.definition()
            {
                let definition_span = Span::from(definition.full_range(
                    db,
                    &parsed_module(db, superclass_scope.python_file(db)).load(db),
                ));

                let superclass_function_span = match superclass_type {
                    Type::FunctionLiteral(function) => Some(signature_span(function)),
                    Type::BoundMethod(method) => Some(signature_span(method.function(db))),
                    _ => None,
                };

                let superclass_definition_kind = definition.kind(db);

                let secondary_span = if superclass_definition_kind.is_function_def()
                    && let Some(function_span) = superclass_function_span.clone()
                {
                    function_span
                } else {
                    definition_span
                };

                diagnostic.annotate(
                    Annotation::secondary(secondary_span.clone())
                        .message(format_args!("`{overridden_method}` defined here")),
                );

                if !superclass_definition_kind.is_function_def()
                    && let Some(function_span) = superclass_function_span
                    && function_span != secondary_span
                {
                    diagnostic.annotate(
                        Annotation::secondary(function_span)
                            .message(format_args!("Signature of `{overridden_method}`")),
                    );
                }
            }
        }
        MethodKind::Synthesized(class_kind) => {
            let make_sub =
                |message: fmt::Arguments| SubDiagnostic::new(SubDiagnosticSeverity::Info, message);

            let mut sub = match class_kind {
                CodeGeneratorKind::DataclassLike(_) => make_sub(format_args!(
                    "`{overridden_method}` is a generated method created because \
                        `{superclass_name}` is a dataclass"
                )),
                CodeGeneratorKind::Pydantic(_) => make_sub(format_args!(
                    "`{overridden_method}` is a generated method created because \
                        `{superclass_name}` is a Pydantic model"
                )),
                CodeGeneratorKind::NamedTuple => make_sub(format_args!(
                    "`{overridden_method}` is a generated method created because \
                        `{superclass_name}` inherits from `typing.NamedTuple`"
                )),
                CodeGeneratorKind::TypedDict => make_sub(format_args!(
                    "`{overridden_method}` is a generated method created because \
                        `{superclass_name}` is a `TypedDict`"
                )),
            };

            sub.annotate(
                Annotation::primary(superclass.definition_span(db))
                    .message(format_args!("Definition of `{superclass_name}`")),
            );
            diagnostic.sub(sub);
        }
    }

    if superclass.is_object(db) && matches!(member, "__eq__" | "__ne__") {
        // Inspired by mypy's subdiagnostic at <https://github.com/python/mypy/blob/1b6ebb17b7fe64488a7b3c3b4b0187bb14fe331b/mypy/messages.py#L1307-L1318>
        let eq_subdiagnostics = [
            format_args!(
                "It is recommended for `{member}` to work with arbitrary objects, for example:",
            ),
            format_args!(""),
            format_args!("    def {member}(self, other: object) -> bool:"),
            format_args!("        if not isinstance(other, {class_name}):"),
            format_args!("            return False"),
            format_args!("        return <logic to compare two `{class_name}` instances>"),
            format_args!(""),
        ];

        for subdiag in eq_subdiagnostics {
            diagnostic.help(subdiag);
        }
    }
}

/// Reports an incompatible pair of source-defined methods in a resolved MRO.
pub(super) fn report_incompatible_base_method<'db>(
    context: &InferContext<'db, '_>,
    class: StaticClassLiteral<'db>,
    member: &str,
    selected: (ClassType<'db>, Definition<'db>, MethodDecorator),
    contract: (ClassType<'db>, Definition<'db>, MethodDecorator),
    error_context: impl FnOnce() -> ErrorContextTree<'db>,
) {
    let db = context.db();
    let Some(builder) = context.report_lint(&INVALID_METHOD_OVERRIDE, class.header_range(db))
    else {
        return;
    };

    let (selected_owner, selected_definition, selected_decorator) = selected;
    let (contract_owner, contract_definition, contract_decorator) = contract;
    let (selected_name, contract_name) = if selected_owner.name(db) == contract_owner.name(db) {
        (
            selected_owner.qualified_name(db).to_string(),
            contract_owner.qualified_name(db).to_string(),
        )
    } else {
        (
            selected_owner.name(db).to_string(),
            contract_owner.name(db).to_string(),
        )
    };
    let mut diagnostic = builder.into_diagnostic(format_args!(
        "Base classes for class `{}` define method `{member}` incompatibly",
        class.name(db)
    ));
    diagnostic.set_primary_message(format_args!(
        "`{selected_name}.{member}` is incompatible with `{contract_name}.{member}`"
    ));
    if selected_decorator != contract_decorator {
        diagnostic.info(format_args!(
            "`{selected_name}.{member}` is {} but `{contract_name}.{member}` is {}",
            selected_decorator.description(),
            contract_decorator.description(),
        ));
    }
    error_context().attach_to(db, &mut diagnostic);
    diagnostic.info("This violates the Liskov Substitution Principle");

    for (definition, owner_name) in [
        (selected_definition, selected_name),
        (contract_definition, contract_name),
    ] {
        let module = parsed_module(db, definition.file(db)).load(db);
        diagnostic.annotate(
            Annotation::secondary(Span::from(definition.focus_range(db, &module)))
                .message(format_args!("`{owner_name}.{member}` defined here")),
        );
    }
}

pub(super) fn report_overridden_final_method<'db>(
    context: &InferContext<'db, '_>,
    member: &str,
    subclass_definition: Definition<'db>,
    // N.B. the type of the *definition*, not the type on an instance of the subclass
    subclass_type: Type<'db>,
    superclass: ClassType<'db>,
    subclass: ClassType<'db>,
    superclass_method_defs: &[FunctionType<'db>],
) {
    let db = context.db();

    // Some hijinks so that we emit a diagnostic on the property getter rather than the property setter
    let property_getter_definition = if subclass_definition.kind(db).is_function_def()
        && let Type::PropertyInstance(property) = subclass_type
        && let Some(Type::FunctionLiteral(getter)) = property.getter(db)
    {
        let getter_definition = getter.definition(db);
        if getter_definition.scope(db) == subclass_definition.scope(db) {
            Some(getter_definition)
        } else {
            None
        }
    } else {
        None
    };

    let subclass_definition = property_getter_definition.unwrap_or(subclass_definition);

    let Some(builder) = context.report_lint(
        &OVERRIDE_OF_FINAL_METHOD,
        subclass_definition.focus_range(db, context.module()),
    ) else {
        return;
    };

    let superclass_name = if superclass.name(db) == subclass.name(db) {
        superclass.qualified_name(db).to_string()
    } else {
        superclass.name(db).to_string()
    };

    let mut diagnostic =
        builder.into_diagnostic(format_args!("Cannot override `{superclass_name}.{member}`"));
    diagnostic.set_primary_message(format_args!(
        "Overrides a definition from superclass `{superclass_name}`"
    ));
    diagnostic.set_concise_message(format_args!(
        "Cannot override final member `{member}` from superclass `{superclass_name}`"
    ));

    let mut sub = SubDiagnostic::new(
        SubDiagnosticSeverity::Info,
        format_args!(
            "`{superclass_name}.{member}` is decorated with `@final`, forbidding overrides"
        ),
    );

    let first_final_superclass_definition = superclass_method_defs
        .iter()
        .find(|function| function.has_known_decorator(db, FunctionDecorators::FINAL))
        .expect(
            "At least one function definition in the superclass should be decorated with `@final`",
        );

    let superclass_function_literal = if first_final_superclass_definition.file(db).is_stub(db) {
        first_final_superclass_definition.first_overload_or_implementation(db)
    } else {
        first_final_superclass_definition
            .literal(db)
            .last_definition
    };

    sub.annotate(
        Annotation::secondary(Span::from(superclass_function_literal.focus_range(
            db,
            &parsed_module(db, first_final_superclass_definition.python_file(db)).load(db),
        )))
        .message(format_args!("`{superclass_name}.{member}` defined here")),
    );

    if let Some(decorator_span) =
        superclass_function_literal.find_known_decorator_span(db, KnownFunction::Final)
    {
        sub.annotate(Annotation::secondary(decorator_span));
    }

    diagnostic.sub(sub);

    // It's tempting to autofix properties as well,
    // but you'd want to delete the `@my_property.deleter` as well as the getter and the setter,
    // and we don't yet track those definitions precisely enough to offer a safe fix.
    //
    // We also only provide autofixes if the subclass member is a function definition (not an
    // assignment like `method = some_function`). If it's an assignment, the function type
    // might be from a different file, and the autofix should delete the assignment instead, which we don't handle today.
    if let Type::FunctionLiteral(function) = subclass_type
        && subclass_definition.kind(db).is_function_def()
    {
        let Some((subclass_literal, _)) = subclass.static_class_literal(db) else {
            return;
        };
        let class_node = subclass_literal
            .body_scope(db)
            .node(db)
            .expect_class()
            .node(context.module());

        let (overloads, implementation) = function.overloads_and_implementation(db);
        let overload_count = overloads.len() + usize::from(implementation.is_some());
        let is_only = overload_count >= class_node.body.len();

        let overload_deletion = |overload: &OverloadLiteral<'db>| {
            let range = overload.node(db, context.file(), context.module()).range();
            if is_only {
                Edit::range_replacement("pass".to_string(), range)
            } else {
                Edit::range_deletion(range)
            }
        };

        let should_fix = overloads
            .iter()
            .copied()
            .chain(implementation)
            .all(|overload| {
                class_node
                    .body
                    .iter()
                    .filter_map(ast::Stmt::as_function_def_stmt)
                    .contains(overload.node(db, context.file(), context.module()))
            });

        let isolate = IsolationLevel::Group(
            class_node
                .node_index()
                .load()
                .as_u32()
                .expect("`parsed_module` should have assigned a node index"),
        );

        match function.overloads_and_implementation(db) {
            ([first_overload, rest @ ..], None) => {
                diagnostic.help(format_args!("Remove all overloads for `{member}`"));
                diagnostic.set_optional_fix(should_fix.then(|| {
                    Fix::unsafe_edits(
                        overload_deletion(first_overload),
                        rest.iter().map(overload_deletion),
                    )
                    .isolate(isolate)
                }));
            }
            ([first_overload, rest @ ..], Some(implementation)) => {
                diagnostic.help(format_args!(
                    "Remove all overloads and the implementation for `{member}`"
                ));
                diagnostic.set_optional_fix(should_fix.then(|| {
                    Fix::unsafe_edits(
                        overload_deletion(first_overload),
                        rest.iter().chain([&implementation]).map(overload_deletion),
                    )
                    .isolate(isolate)
                }));
            }
            ([], Some(implementation)) => {
                diagnostic.help(format_args!("Remove the override of `{member}`"));
                diagnostic.set_optional_fix(should_fix.then(|| {
                    Fix::unsafe_edit(overload_deletion(&implementation)).isolate(isolate)
                }));
            }
            ([], None) => {
                // Should be impossible to get here: how would we even infer a function as a function
                // if it has 0 overloads and no implementation?
                unreachable!(
                    "A function should always have an implementation and/or >=1 overloads"
                );
            }
        }
    } else if let Type::PropertyInstance(property) = subclass_type
        && property.setter(db).is_some()
    {
        diagnostic.help(format_args!("Remove the getter and setter for `{member}`"));
    } else {
        diagnostic.help(format_args!("Remove the override of `{member}`"));
    }
}

pub(super) fn report_overridden_final_variable<'db>(
    context: &InferContext<'db, '_>,
    member: &str,
    subclass_definition: Definition<'db>,
    superclass: ClassType<'db>,
    subclass: ClassType<'db>,
    superclass_definition: Option<Definition<'db>>,
) {
    let db = context.db();

    let Some(builder) = context.report_lint(
        &OVERRIDE_OF_FINAL_VARIABLE,
        subclass_definition.focus_range(db, context.module()),
    ) else {
        return;
    };

    let superclass_name = if superclass.name(db) == subclass.name(db) {
        superclass.qualified_name(db).to_string()
    } else {
        superclass.name(db).to_string()
    };

    let mut diagnostic =
        builder.into_diagnostic(format_args!("Cannot override `{superclass_name}.{member}`"));
    diagnostic.set_primary_message(format_args!(
        "Overrides a final variable from superclass `{superclass_name}`"
    ));
    diagnostic.set_concise_message(format_args!(
        "Cannot override final variable `{member}` from superclass `{superclass_name}`"
    ));

    if let Some(superclass_def) = superclass_definition {
        let mut sub = SubDiagnostic::new(
            SubDiagnosticSeverity::Info,
            format_args!(
                "`{superclass_name}.{member}` is declared as `Final`, forbidding overrides"
            ),
        );
        sub.annotate(
            Annotation::secondary(Span::from(superclass_def.focus_range(
                db,
                &parsed_module(db, superclass_def.python_file(db)).load(db),
            )))
            .message(format_args!("`{superclass_name}.{member}` defined here")),
        );
        diagnostic.sub(sub);
    } else {
        diagnostic.info(format_args!(
            "`{superclass_name}.{member}` is declared as `Final`, forbidding overrides"
        ));
    }
}

pub(super) fn report_unsupported_comparison<'db>(
    context: &InferContext<'db, '_>,
    error: &UnsupportedComparisonError<'db>,
    range: TextRange,
    left: &ast::Expr,
    right: &ast::Expr,
    left_ty: Type<'db>,
    right_ty: Type<'db>,
) {
    let db = context.db();

    let Some(diagnostic_builder) = context.report_lint(&UNSUPPORTED_OPERATOR, range) else {
        return;
    };

    let display_settings = DisplaySettings::from_possibly_ambiguous_types(
        db,
        [error.left_ty, error.right_ty, left_ty, right_ty],
    );

    let mut diagnostic =
        diagnostic_builder.into_diagnostic(format_args!("Unsupported `{}` operation", error.op));

    if left_ty.is_equivalent_to(db, right_ty) {
        diagnostic.set_primary_message(format_args!(
            "Both operands have type `{}`",
            left_ty.display_with(db, display_settings.clone())
        ));
        diagnostic.annotate(context.secondary(left));
        diagnostic.annotate(context.secondary(right));
        diagnostic.set_concise_message(format_args!(
            "Operator `{}` is not supported between two objects of type `{}`",
            error.op,
            left_ty.display_with(db, display_settings.clone())
        ));
    } else {
        for (ty, expr) in [(left_ty, left), (right_ty, right)] {
            diagnostic.annotate(context.secondary(expr).message(format_args!(
                "Has type `{}`",
                ty.display_with(db, display_settings.clone())
            )));
        }
        diagnostic.set_concise_message(format_args!(
            "Operator `{}` is not supported between objects of type `{}` and `{}`",
            error.op,
            left_ty.display_with(db, display_settings.clone()),
            right_ty.display_with(db, display_settings.clone())
        ));
    }

    // For non-atomic types like unions and tuples, we now provide context
    // on the underlying elements that caused the error.
    // If we're emitting a diagnostic for something like `(1, "foo") < (2, 3)`:
    //
    // - `left_ty` is `tuple[Literal[1], Literal["foo"]]`
    // - `right_ty` is `tuple[Literal[2], Literal[3]]
    // - `error.left_ty` is `Literal["foo"]`
    // - `error.right_ty` is `Literal[3]`
    if (error.left_ty, error.right_ty) != (left_ty, right_ty) {
        if let Some(TupleSpec::Fixed(lhs_spec)) = left_ty.tuple_instance_spec(db).as_deref()
            && let Some(TupleSpec::Fixed(rhs_spec)) = right_ty.tuple_instance_spec(db).as_deref()
            && lhs_spec.len() == rhs_spec.len()
            && let Some(position) = lhs_spec
                .all_elements()
                .iter()
                .zip(rhs_spec.all_elements())
                .position(|tup| tup == (&error.left_ty, &error.right_ty))
        {
            if error.left_ty.is_equivalent_to(db, error.right_ty) {
                diagnostic.info(format_args!(
                    "Operation fails because operator `{}` is not supported between \
                    the tuple elements at index {} (both of type `{}`)",
                    error.op,
                    position + 1,
                    error.left_ty.display_with(db, display_settings),
                ));
            } else {
                diagnostic.info(format_args!(
                    "Operation fails because operator `{}` is not supported between \
                    the tuple elements at index {} (of type `{}` and `{}`)",
                    error.op,
                    position + 1,
                    error.left_ty.display_with(db, display_settings.clone()),
                    error.right_ty.display_with(db, display_settings),
                ));
            }
        } else {
            if error.left_ty.is_equivalent_to(db, error.right_ty) {
                diagnostic.info(format_args!(
                    "Operation fails because operator `{}` is not supported \
                    between two objects of type `{}`",
                    error.op,
                    error.left_ty.display_with(db, display_settings),
                ));
            } else {
                diagnostic.info(format_args!(
                    "Operation fails because operator `{}` is not supported \
                    between objects of type `{}` and `{}`",
                    error.op,
                    error.left_ty.display_with(db, display_settings.clone()),
                    error.right_ty.display_with(db, display_settings)
                ));
            }
        }
    }
}

pub(super) fn report_unsupported_augmented_assignment<'db>(
    context: &InferContext<'db, '_>,
    stmt: &ast::StmtAugAssign,
    left_ty: Type<'db>,
    right_ty: Type<'db>,
) {
    report_unsupported_binary_operation_impl(
        context,
        stmt.range(),
        &stmt.target,
        &stmt.value,
        left_ty,
        right_ty,
        OperatorDisplay {
            operator: stmt.op,
            is_augmented_assignment: true,
        },
    );
}

pub(super) fn report_unsupported_binary_operation<'db>(
    context: &InferContext<'db, '_>,
    binary_expression: &ast::ExprBinOp,
    left_ty: Type<'db>,
    right_ty: Type<'db>,
    operator: ast::Operator,
) {
    report_unsupported_binary_operation_impl(
        context,
        binary_expression.range(),
        &binary_expression.left,
        &binary_expression.right,
        left_ty,
        right_ty,
        OperatorDisplay {
            operator,
            is_augmented_assignment: false,
        },
    );
}

#[derive(Debug, Copy, Clone)]
struct OperatorDisplay {
    operator: ast::Operator,
    is_augmented_assignment: bool,
}

impl std::fmt::Display for OperatorDisplay {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_augmented_assignment {
            write!(f, "{}=", self.operator)
        } else {
            write!(f, "{}", self.operator)
        }
    }
}

fn report_unsupported_binary_operation_impl<'a>(
    context: &'a InferContext<'a, 'a>,
    range: TextRange,
    left: &ast::Expr,
    right: &ast::Expr,
    left_ty: Type<'a>,
    right_ty: Type<'a>,
    operator: OperatorDisplay,
) -> Option<LintDiagnosticGuard<'a, 'a>> {
    let db = context.db();
    let diagnostic_builder = context.report_lint(&UNSUPPORTED_OPERATOR, range)?;
    let display_settings = DisplaySettings::from_possibly_ambiguous_types(db, [left_ty, right_ty]);

    let mut diagnostic =
        diagnostic_builder.into_diagnostic(format_args!("Unsupported `{operator}` operation"));

    if left_ty.is_equivalent_to(db, right_ty) {
        diagnostic.set_primary_message(format_args!(
            "Both operands have type `{}`",
            left_ty.display_with(db, display_settings.clone())
        ));
        diagnostic.annotate(context.secondary(left));
        diagnostic.annotate(context.secondary(right));
        diagnostic.set_concise_message(format_args!(
            "Operator `{operator}` is not supported between two objects of type `{}`",
            left_ty.display_with(db, display_settings.clone())
        ));
    } else {
        for (ty, expr) in [(left_ty, left), (right_ty, right)] {
            diagnostic.annotate(context.secondary(expr).message(format_args!(
                "Has type `{}`",
                ty.display_with(db, display_settings.clone())
            )));
        }
        diagnostic.set_concise_message(format_args!(
            "Operator `{operator}` is not supported between objects of type `{}` and `{}`",
            left_ty.display_with(db, display_settings.clone()),
            right_ty.display_with(db, display_settings.clone())
        ));
    }

    Some(diagnostic)
}

pub(super) fn report_bad_frozen_dataclass_inheritance<'db>(
    context: &InferContext<'db, '_>,
    class: StaticClassLiteral<'db>,
    class_node: &ast::StmtClassDef,
    base_class: StaticClassLiteral<'db>,
    base_class_node: &ast::Expr,
    base_is_frozen: bool,
) {
    let db = context.db();

    let Some(builder) =
        context.report_lint(&INVALID_FROZEN_DATACLASS_SUBCLASS, class.header_range(db))
    else {
        return;
    };

    let mut diagnostic = if base_is_frozen {
        let mut diagnostic =
            builder.into_diagnostic("Non-frozen dataclass cannot inherit from frozen dataclass");
        diagnostic.set_concise_message(format_args!(
            "Non-frozen dataclass `{}` cannot inherit from frozen dataclass `{}`",
            class.name(db),
            base_class.name(db)
        ));
        diagnostic.set_primary_message(format_args!(
            "Subclass `{}` is not frozen but base class `{}` is",
            class.name(db),
            base_class.name(db)
        ));
        diagnostic
    } else {
        let mut diagnostic =
            builder.into_diagnostic("Frozen dataclass cannot inherit from non-frozen dataclass");
        diagnostic.set_concise_message(format_args!(
            "Frozen dataclass `{}` cannot inherit from non-frozen dataclass `{}`",
            class.name(db),
            base_class.name(db)
        ));
        diagnostic.set_primary_message(format_args!(
            "Subclass `{}` is frozen but base class `{}` is not",
            class.name(db),
            base_class.name(db)
        ));
        diagnostic
    };

    diagnostic.annotate(context.secondary(base_class_node));

    if let Some(position) = class.find_dataclass_decorator_position(db) {
        diagnostic.annotate(
            context
                .secondary(&class_node.decorator_list[position])
                .message(format_args!("`{}` dataclass parameters", class.name(db))),
        );
    }
    diagnostic.info("This causes the class creation to fail");

    if let Some(decorator_position) = base_class.find_dataclass_decorator_position(db) {
        let mut sub = SubDiagnostic::new(
            SubDiagnosticSeverity::Info,
            format_args!("Base class definition"),
        );
        sub.annotate(
            Annotation::primary(base_class.header_span(db))
                .message(format_args!("`{}` definition", base_class.name(db))),
        );

        let base_class_file = base_class.file(db);
        let module = parsed_module(db, base_class.python_file(db)).load(db);

        let decorator_range = base_class
            .body_scope(db)
            .node(db)
            .expect_class()
            .node(&module)
            .decorator_list[decorator_position]
            .range();

        sub.annotate(
            Annotation::secondary(Span::from(base_class_file).with_range(decorator_range)).message(
                format_args!("`{}` dataclass parameters", base_class.name(db)),
            ),
        );

        diagnostic.sub(sub);
    }
}

pub(super) fn report_invalid_total_ordering(
    context: &InferContext<'_, '_>,
    class: ClassLiteral<'_>,
    decorator: &ast::Decorator,
) {
    let db = context.db();

    let Some(builder) = context.report_lint(&INVALID_TOTAL_ORDERING, decorator) else {
        return;
    };

    let mut diagnostic = builder.into_diagnostic(
        "Class decorated with `@total_ordering` must define at least one ordering method",
    );
    diagnostic.set_primary_message(format_args!(
        "`{}` does not define `__lt__`, `__le__`, `__gt__`, or `__ge__`",
        class.name(db)
    ));
    diagnostic.annotate(context.secondary(class.header_range(db)));
    diagnostic.info("The decorator will raise `ValueError` at runtime");
}

/// Reports an invalid `total_ordering(cls)` function call where the class
/// does not define any ordering method.
pub(super) fn report_invalid_total_ordering_call(
    context: &InferContext<'_, '_>,
    class: ClassLiteral<'_>,
    call_expression: &ast::ExprCall,
) {
    let db = context.db();

    let Some(builder) = context.report_lint(&INVALID_TOTAL_ORDERING, call_expression) else {
        return;
    };

    let mut diagnostic = builder.into_diagnostic(
        "`@functools.total_ordering` requires at least one ordering method (`__lt__`, `__le__`, `__gt__`, or `__ge__`) to be defined",
    );
    diagnostic.set_primary_message(format_args!(
        "`{}` does not define `__lt__`, `__le__`, `__gt__`, or `__ge__`",
        class.name(db)
    ));
    diagnostic.info("The function will raise `ValueError` at runtime");
}

/// This function receives an unresolved `from foo import bar` import,
/// where `foo` can be resolved to a module but that module does not
/// have a `bar` member or submodule.
///
/// If the `foo` module originates from the standard library and `foo.bar`
/// *does* exist as a submodule in the standard library on *other* Python
/// versions, we add a hint to the diagnostic that the user may have
/// misconfigured their Python version.
///
/// The function returns `true` if a hint was added, `false` otherwise.
pub(super) fn hint_if_stdlib_submodule_exists_on_other_versions(
    db: &dyn Db,
    diagnostic: &mut Diagnostic,
    full_submodule_name: &ModuleName,
    parent_module: Module,
) -> bool {
    let Some(search_path) = parent_module.search_path(db) else {
        return false;
    };

    if !search_path.is_standard_library() {
        return false;
    }

    let program = Program::get(db);
    let typeshed_versions = program.search_paths(db).typeshed_versions();

    let Some(version_range) = typeshed_versions.exact(full_submodule_name) else {
        return false;
    };

    let python_version = program.python_version(db);
    if version_range.contains(python_version) {
        return false;
    }

    diagnostic.info(format_args!(
        "The stdlib module `{module_name}` only has a `{name}` \
            submodule on Python {version_range}",
        module_name = parent_module.name(db),
        name = full_submodule_name.last_component(),
        version_range = version_range.diagnostic_display(),
    ));

    add_inferred_python_version_hint_to_diagnostic(db, diagnostic, "resolving modules");

    true
}

/// This function receives an unresolved `foo.bar` attribute access,
/// where `foo` can be resolved to have a type but that type does not
/// have a `bar` attribute.
///
/// If the type of `foo` has a definition that originates in the
/// standard library and `foo.bar` *does* exist as an attribute on *other*
/// Python versions, we add a hint to the diagnostic that the user may have
/// misconfigured their Python version.
pub(super) fn hint_if_stdlib_attribute_exists_on_other_versions(
    db: &dyn Db,
    mut diagnostic: LintDiagnosticGuard,
    value_type: Type,
    attr: &str,
    action: &str,
) {
    // Currently we limit this analysis to attributes of stdlib modules,
    // as this covers the most important cases while not being too noisy
    // about basic typos or special types like `super(C, self)`
    let Type::ModuleLiteral(module_ty) = value_type else {
        return;
    };
    let module = module_ty.module(db);
    let Some(file) = module.python_file(db) else {
        return;
    };
    let Some(search_path) = module.search_path(db) else {
        return;
    };
    if !search_path.is_standard_library() {
        return;
    }

    // We populate place_table entries for stdlib items across all known versions and platforms,
    // so if this lookup succeeds then we know that this lookup *could* succeed with possible
    // configuration changes.
    let symbol_table = place_table(db, global_scope(db, file));
    let Some(symbol) = symbol_table.symbol_by_name(attr) else {
        return;
    };

    if !symbol.is_bound() {
        return;
    }

    diagnostic.info("The member may be available on other Python versions or platforms");

    // For now, we just mention the current version they're on, and hope that's enough of a nudge.
    // TODO: determine what version they need to be on
    // TODO: also mention the platform we're assuming
    // TODO: determine what platform they need to be on
    add_inferred_python_version_hint_to_diagnostic(db, &mut diagnostic, action);
}

pub(super) fn report_invalid_concatenate_last_arg<'db>(
    context: &InferContext<'db, '_>,
    last_arg: &ast::Expr,
    last_arg_type: Type<'db>,
) {
    if let Some(builder) = context.report_lint(&INVALID_TYPE_ARGUMENTS, last_arg) {
        let mut diag = builder.into_diagnostic(
            "The last argument to `typing.Concatenate` must be either `...` or a `ParamSpec` \
                type variable",
        );
        diag.set_primary_message(format_args!(
            "Got `{}`",
            last_arg_type.display(context.db())
        ));
    }
}

pub(super) fn report_subclass_of_class_with_non_callable_init_subclass<'db>(
    context: &InferContext<'db, '_>,
    call_error: CallError<'db>,
    class: StaticClassLiteral<'db>,
    class_node: &ast::StmtClassDef,
) {
    let db = context.db();
    let CallError(err_kind, bindings) = call_error;

    match err_kind {
        CallErrorKind::NotCallable | CallErrorKind::PossiblyNotCallable => {
            let Some(builder) =
                context.report_lint(&NON_CALLABLE_INIT_SUBCLASS, class.header_range(db))
            else {
                return;
            };
            let class_name = class.name(db);
            let mut diagnostic =
                builder.into_diagnostic(format_args!("Invalid definition of class `{class_name}`"));

            let class_and_def = class
                .iter_mro(db, None)
                .filter_map(|base| base.into_class()?.class_literal(db).as_static())
                .find_map(|class| {
                    let scope = class.body_scope(db);
                    let place_table = place_table(db, scope);
                    let symbol = place_table.symbol_id("__init_subclass__")?;
                    let use_def = use_def_map(db, scope);
                    let bindings = use_def.end_of_scope_bindings(ScopedPlaceId::Symbol(symbol));
                    let place_with_def = place_from_bindings(db, bindings);
                    if place_with_def.place.is_undefined() {
                        return None;
                    }
                    Some((class, place_with_def.first_definition?))
                });

            if let Some((superclass, definition)) = class_and_def {
                let superclass_name = superclass.name(db);
                diagnostic.set_primary_message(format_args!(
                    "Superclass `{superclass_name}` cannot be subclassed",
                ));
                let definition_module = parsed_module(db, definition.python_file(db));
                let mut annotation = Annotation::secondary(Span::from(
                    definition.focus_range(db, &definition_module.load(db)),
                ));
                if err_kind == CallErrorKind::NotCallable {
                    diagnostic.set_concise_message(format_args!(
                        "Class `{superclass_name}` cannot be subclassed due \
                        to a non-callable `__init_subclass__` definition"
                    ));
                    annotation = annotation.message(format_args!(
                        "`{superclass_name}.__init_subclass__` has type `{}`, \
                        which is not callable",
                        bindings.callable_type().display(db)
                    ));
                } else {
                    diagnostic.set_concise_message(format_args!(
                        "Class `{superclass_name}` cannot be subclassed due \
                        to an `__init_subclass__` definition that may not be callable"
                    ));
                    annotation = annotation.message(format_args!(
                        "`{superclass_name}.__init_subclass__` has type `{}`, \
                        which may not be callable",
                        bindings.callable_type().display(db)
                    ));
                }
                diagnostic.annotate(annotation);
            } else if err_kind == CallErrorKind::NotCallable {
                diagnostic.set_primary_message(
                    "`class` statement will fail because `__init_subclass__` \
                    on a superclass is not callable",
                );
                diagnostic.set_concise_message(format_args!(
                    "Creation of class `{class_name}` will fail due to a non-callable \
                    `__init_subclass__` definition on a superclass",
                ));
            } else {
                diagnostic.set_primary_message(
                    "`class` statement may fail because `__init_subclass__` \
                    on a superclass may not be callable",
                );
                diagnostic.set_concise_message(format_args!(
                    "Creation of class `{class_name}` may fail due to an \
                    `__init_subclass__` definition on a superclass that may \
                    not be callable",
                ));
            }
            diagnostic.info(
                "`__init_subclass__` on a superclass is implicitly called \
                during creation of a class object",
            );
            diagnostic.info(
                "See https://docs.python.org/3/reference/datamodel.html\
                #customizing-class-creation",
            );
        }
        CallErrorKind::BindingError => {
            bindings.report_diagnostics(context, class_node.into());
        }
    }
}
