pub(crate) use any_eq_ne_annotation::*;
pub(crate) use bad_version_info_comparison::*;
pub(crate) use collections_named_tuple::*;
pub(crate) use docstring_in_stubs::*;
pub(crate) use duplicate_union_member::*;
pub(crate) use ellipsis_in_non_empty_class_body::*;
pub(crate) use future_annotations_in_stub::*;
pub(crate) use iter_method_return_iterable::*;
pub(crate) use no_return_argument_annotation::*;
pub(crate) use non_empty_stub_body::*;
pub(crate) use non_self_return_type::*;
pub(crate) use numeric_literal_too_long::*;
pub(crate) use pass_in_class_body::*;
pub(crate) use pass_statement_stub_body::*;
pub(crate) use prefix_type_params::*;
pub(crate) use quoted_annotation_in_stub::*;
pub(crate) use simple_defaults::*;
pub(crate) use str_or_repr_defined_in_stub::*;
pub(crate) use string_or_bytes_too_long::*;
pub(crate) use stub_body_multiple_statements::*;
pub(crate) use type_alias_naming::*;
pub(crate) use type_comment_in_stub::*;
pub(crate) use unaliased_collections_abc_set_import::*;
pub(crate) use unrecognized_platform::*;
pub(crate) use version_info::*;

mod any_eq_ne_annotation;
mod bad_version_info_comparison;
mod collections_named_tuple;
mod docstring_in_stubs;
mod duplicate_union_member;
mod ellipsis_in_non_empty_class_body;
mod future_annotations_in_stub;
mod iter_method_return_iterable;
mod no_return_argument_annotation;
mod non_empty_stub_body;
mod non_self_return_type;
mod numeric_literal_too_long;
mod pass_in_class_body;
mod pass_statement_stub_body;
mod prefix_type_params;
mod quoted_annotation_in_stub;
mod simple_defaults;
mod str_or_repr_defined_in_stub;
mod string_or_bytes_too_long;
mod stub_body_multiple_statements;
mod type_alias_naming;
mod type_comment_in_stub;
mod unaliased_collections_abc_set_import;
mod unrecognized_platform;
mod version_info;
