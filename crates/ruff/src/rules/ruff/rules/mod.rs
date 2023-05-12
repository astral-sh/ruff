mod ambiguous_unicode_character;
mod asyncio_dangling_task;
mod collection_literal_concatenation;
mod confusables;
mod mutable_defaults_in_dataclass_fields;
mod pairwise_over_zipped;
mod unused_noqa;

pub(crate) use ambiguous_unicode_character::{
    ambiguous_unicode_character, AmbiguousUnicodeCharacterComment,
    AmbiguousUnicodeCharacterDocstring, AmbiguousUnicodeCharacterString,
};
pub(crate) use asyncio_dangling_task::{asyncio_dangling_task, AsyncioDanglingTask};
pub(crate) use collection_literal_concatenation::{
    collection_literal_concatenation, CollectionLiteralConcatenation,
};
pub(crate) use mutable_defaults_in_dataclass_fields::{
    function_call_in_dataclass_defaults, is_dataclass, mutable_dataclass_default,
    FunctionCallInDataclassDefaultArgument, MutableDataclassDefault,
};
pub(crate) use pairwise_over_zipped::{pairwise_over_zipped, PairwiseOverZipped};
pub(crate) use unused_noqa::{UnusedCodes, UnusedNOQA};

#[derive(Clone, Copy)]
pub(crate) enum Context {
    String,
    Docstring,
    Comment,
}
