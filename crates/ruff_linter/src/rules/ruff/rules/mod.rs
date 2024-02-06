pub(crate) use ambiguous_unicode_character::*;
pub(crate) use assignment_in_assert::*;
pub(crate) use asyncio_dangling_task::*;
pub(crate) use collection_literal_concatenation::*;
pub(crate) use default_factory_kwarg::*;
pub(crate) use explicit_f_string_type_conversion::*;
pub(crate) use function_call_in_dataclass_default::*;
pub(crate) use implicit_optional::*;
pub(crate) use invalid_index_type::*;
pub(crate) use invalid_pyproject_toml::*;
pub(crate) use missing_fstring_syntax::*;
pub(crate) use mutable_class_default::*;
pub(crate) use mutable_dataclass_default::*;
pub(crate) use mutable_fromkeys_value::*;
pub(crate) use never_union::*;
pub(crate) use pairwise_over_zipped::*;
pub(crate) use parenthesize_logical_operators::*;
pub(crate) use quadratic_list_summation::*;
pub(crate) use sort_dunder_all::*;
pub(crate) use sort_dunder_slots::*;
pub(crate) use static_key_dict_comprehension::*;
#[cfg(feature = "test-rules")]
pub(crate) use test_rules::*;
pub(crate) use unnecessary_dict_comprehension_for_iterable::*;
pub(crate) use unnecessary_iterable_allocation_for_first_element::*;
pub(crate) use unnecessary_key_check::*;
pub(crate) use unused_noqa::*;

mod ambiguous_unicode_character;
mod assignment_in_assert;
mod asyncio_dangling_task;
mod collection_literal_concatenation;
mod confusables;
mod default_factory_kwarg;
mod explicit_f_string_type_conversion;
mod function_call_in_dataclass_default;
mod helpers;
mod implicit_optional;
mod invalid_index_type;
mod invalid_pyproject_toml;
mod missing_fstring_syntax;
mod mutable_class_default;
mod mutable_dataclass_default;
mod mutable_fromkeys_value;
mod never_union;
mod pairwise_over_zipped;
mod parenthesize_logical_operators;
mod quadratic_list_summation;
mod sequence_sorting;
mod sort_dunder_all;
mod sort_dunder_slots;
mod static_key_dict_comprehension;
#[cfg(feature = "test-rules")]
pub(crate) mod test_rules;
mod unnecessary_dict_comprehension_for_iterable;
mod unnecessary_iterable_allocation_for_first_element;
mod unnecessary_key_check;
mod unused_noqa;

#[derive(Clone, Copy)]
pub(crate) enum Context {
    String,
    Docstring,
    Comment,
}
