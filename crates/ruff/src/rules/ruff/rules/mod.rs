pub(crate) use ambiguous_unicode_character::{
    ambiguous_unicode_character, AmbiguousUnicodeCharacterComment,
    AmbiguousUnicodeCharacterDocstring, AmbiguousUnicodeCharacterString,
};
pub(crate) use asyncio_dangling_task::{asyncio_dangling_task, AsyncioDanglingTask};
pub(crate) use collection_literal_concatenation::{
    collection_literal_concatenation, CollectionLiteralConcatenation,
};
pub(crate) use explicit_f_string_type_conversion::{
    explicit_f_string_type_conversion, ExplicitFStringTypeConversion,
};
pub(crate) use invalid_pyproject_toml::InvalidPyprojectToml;
pub(crate) use mutable_defaults_in_class_fields::{
    function_call_in_dataclass_defaults, is_dataclass, mutable_class_default,
    FunctionCallInDataclassDefaultArgument, MutableClassDefault, MutableDataclassDefault,
};
pub(crate) use pairwise_over_zipped::{pairwise_over_zipped, PairwiseOverZipped};
pub(crate) use unused_noqa::{UnusedCodes, UnusedNOQA};

mod ambiguous_unicode_character;
mod asyncio_dangling_task;
mod collection_literal_concatenation;
mod confusables;
mod explicit_f_string_type_conversion;
mod invalid_pyproject_toml;
mod mutable_defaults_in_class_fields;
mod pairwise_over_zipped;
mod unused_noqa;

#[derive(Clone, Copy)]
pub(crate) enum Context {
    String,
    Docstring,
    Comment,
}
