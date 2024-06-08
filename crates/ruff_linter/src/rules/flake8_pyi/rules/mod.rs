pub(crate) use any_eq_ne_annotation::*;
pub(crate) use bad_generator_return_type::*;
pub(crate) use bad_version_info_comparison::*;
pub(crate) use bytestring_usage::*;
pub(crate) use collections_named_tuple::*;
pub(crate) use complex_assignment_in_stub::*;
pub(crate) use complex_if_statement_in_stub::*;
pub(crate) use custom_type_var_return_type::*;
pub(crate) use docstring_in_stubs::*;
pub(crate) use duplicate_literal_member::*;
pub(crate) use duplicate_union_member::*;
pub(crate) use ellipsis_in_non_empty_class_body::*;
pub(crate) use exit_annotations::*;
pub(crate) use future_annotations_in_stub::*;
pub(crate) use generic_not_last_base_class::*;
pub(crate) use iter_method_return_iterable::*;
pub(crate) use no_return_argument_annotation::*;
pub(crate) use non_empty_stub_body::*;
pub(crate) use non_self_return_type::*;
pub(crate) use numeric_literal_too_long::*;
pub(crate) use pass_in_class_body::*;
pub(crate) use pass_statement_stub_body::*;
pub(crate) use pre_pep570_positional_argument::*;
pub(crate) use prefix_type_params::*;
pub(crate) use quoted_annotation_in_stub::*;
pub(crate) use redundant_final_literal::*;
pub(crate) use redundant_literal_union::*;
pub(crate) use redundant_numeric_union::*;
pub(crate) use simple_defaults::*;
use std::fmt;
pub(crate) use str_or_repr_defined_in_stub::*;
pub(crate) use string_or_bytes_too_long::*;
pub(crate) use stub_body_multiple_statements::*;
pub(crate) use type_alias_naming::*;
pub(crate) use type_comment_in_stub::*;
pub(crate) use unaliased_collections_abc_set_import::*;
pub(crate) use unnecessary_literal_union::*;
pub(crate) use unnecessary_type_union::*;
pub(crate) use unrecognized_platform::*;
pub(crate) use unrecognized_version_info::*;
pub(crate) use unsupported_method_call_on_all::*;
pub(crate) use unused_private_type_definition::*;

mod any_eq_ne_annotation;
mod bad_generator_return_type;
mod bad_version_info_comparison;
mod bytestring_usage;
mod collections_named_tuple;
mod complex_assignment_in_stub;
mod complex_if_statement_in_stub;
mod custom_type_var_return_type;
mod docstring_in_stubs;
mod duplicate_literal_member;
mod duplicate_union_member;
mod ellipsis_in_non_empty_class_body;
mod exit_annotations;
mod future_annotations_in_stub;
mod generic_not_last_base_class;
mod iter_method_return_iterable;
mod no_return_argument_annotation;
mod non_empty_stub_body;
mod non_self_return_type;
mod numeric_literal_too_long;
mod pass_in_class_body;
mod pass_statement_stub_body;
mod pre_pep570_positional_argument;
mod prefix_type_params;
mod quoted_annotation_in_stub;
mod redundant_final_literal;
mod redundant_literal_union;
mod redundant_numeric_union;
mod simple_defaults;
mod str_or_repr_defined_in_stub;
mod string_or_bytes_too_long;
mod stub_body_multiple_statements;
mod type_alias_naming;
mod type_comment_in_stub;
mod unaliased_collections_abc_set_import;
mod unnecessary_literal_union;
mod unnecessary_type_union;
mod unrecognized_platform;
mod unrecognized_version_info;
mod unsupported_method_call_on_all;
mod unused_private_type_definition;

// TODO(charlie): Replace this with a common utility for selecting the appropriate source
// module for a given `typing` member.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TypingModule {
    Typing,
    TypingExtensions,
}

impl TypingModule {
    fn as_str(self) -> &'static str {
        match self {
            TypingModule::Typing => "typing",
            TypingModule::TypingExtensions => "typing_extensions",
        }
    }
}

impl fmt::Display for TypingModule {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.write_str(self.as_str())
    }
}
