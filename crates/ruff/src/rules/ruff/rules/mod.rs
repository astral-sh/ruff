pub(crate) use ambiguous_unicode_character::*;
pub(crate) use asyncio_dangling_task::*;
pub(crate) use collection_literal_concatenation::*;
pub(crate) use explicit_f_string_type_conversion::*;
pub(crate) use function_call_in_dataclass_default::*;
pub(crate) use implicit_optional::*;
pub(crate) use invalid_index_type::*;
pub(crate) use invalid_pyproject_toml::*;
pub(crate) use mutable_class_default::*;
pub(crate) use mutable_dataclass_default::*;
pub(crate) use pairwise_over_zipped::*;
pub(crate) use static_key_dict_comprehension::*;
pub(crate) use unnecessary_iterable_allocation_for_first_element::*;
#[cfg(feature = "unreachable-code")]
pub(crate) use unreachable::*;
pub(crate) use unused_noqa::*;

mod ambiguous_unicode_character;
mod asyncio_dangling_task;
mod collection_literal_concatenation;
mod confusables;
mod explicit_f_string_type_conversion;
mod function_call_in_dataclass_default;
mod helpers;
mod implicit_optional;
mod invalid_index_type;
mod invalid_pyproject_toml;
mod mutable_class_default;
mod mutable_dataclass_default;
mod pairwise_over_zipped;
mod static_key_dict_comprehension;
mod unnecessary_iterable_allocation_for_first_element;
#[cfg(feature = "unreachable-code")]
pub(crate) mod unreachable;
mod unused_noqa;

#[derive(Clone, Copy)]
pub(crate) enum Context {
    String,
    Docstring,
    Comment,
}
pub(crate) use quadratic_list_summation::*;

mod quadratic_list_summation;
