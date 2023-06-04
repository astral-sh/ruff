pub(crate) use any_eq_ne_annotation::{any_eq_ne_annotation, AnyEqNeAnnotation};
pub(crate) use bad_version_info_comparison::{
    bad_version_info_comparison, BadVersionInfoComparison,
};
pub(crate) use collections_named_tuple::{collections_named_tuple, CollectionsNamedTuple};
pub(crate) use docstring_in_stubs::{docstring_in_stubs, DocstringInStub};
pub(crate) use duplicate_union_member::{duplicate_union_member, DuplicateUnionMember};
pub(crate) use ellipsis_in_non_empty_class_body::{
    ellipsis_in_non_empty_class_body, EllipsisInNonEmptyClassBody,
};
pub(crate) use iter_method_return_iterable::{
    iter_method_return_iterable, IterMethodReturnIterable,
};
pub(crate) use non_empty_stub_body::{non_empty_stub_body, NonEmptyStubBody};
pub(crate) use non_self_return_type::{non_self_return_type, NonSelfReturnType};
pub(crate) use numeric_literal_too_long::{numeric_literal_too_long, NumericLiteralTooLong};
pub(crate) use pass_in_class_body::{pass_in_class_body, PassInClassBody};
pub(crate) use pass_statement_stub_body::{pass_statement_stub_body, PassStatementStubBody};
pub(crate) use prefix_type_params::{prefix_type_params, UnprefixedTypeParam};
pub(crate) use quoted_annotation_in_stub::{quoted_annotation_in_stub, QuotedAnnotationInStub};
pub(crate) use simple_defaults::{
    annotated_assignment_default_in_stub, argument_simple_defaults, assignment_default_in_stub,
    typed_argument_simple_defaults, unannotated_assignment_in_stub,
    unassigned_special_variable_in_stub, ArgumentDefaultInStub, AssignmentDefaultInStub,
    TypedArgumentDefaultInStub, UnannotatedAssignmentInStub, UnassignedSpecialVariableInStub,
};
pub(crate) use string_or_bytes_too_long::{string_or_bytes_too_long, StringOrBytesTooLong};
pub(crate) use stub_body_multiple_statements::{
    stub_body_multiple_statements, StubBodyMultipleStatements,
};
pub(crate) use type_alias_naming::{
    snake_case_type_alias, t_suffixed_type_alias, SnakeCaseTypeAlias, TSuffixedTypeAlias,
};
pub(crate) use type_comment_in_stub::{type_comment_in_stub, TypeCommentInStub};
pub(crate) use unaliased_collections_abc_set_import::{
    unaliased_collections_abc_set_import, UnaliasedCollectionsAbcSetImport,
};
pub(crate) use unrecognized_platform::{
    unrecognized_platform, UnrecognizedPlatformCheck, UnrecognizedPlatformName,
};

mod any_eq_ne_annotation;
mod bad_version_info_comparison;
mod collections_named_tuple;
mod docstring_in_stubs;
mod duplicate_union_member;
mod ellipsis_in_non_empty_class_body;
mod iter_method_return_iterable;
mod non_empty_stub_body;
mod non_self_return_type;
mod numeric_literal_too_long;
mod pass_in_class_body;
mod pass_statement_stub_body;
mod prefix_type_params;
mod quoted_annotation_in_stub;
mod simple_defaults;
mod string_or_bytes_too_long;
mod stub_body_multiple_statements;
mod type_alias_naming;
mod type_comment_in_stub;
mod unaliased_collections_abc_set_import;
mod unrecognized_platform;
