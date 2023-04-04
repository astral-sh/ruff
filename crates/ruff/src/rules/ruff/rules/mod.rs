mod ambiguous_unicode_character;
mod asyncio_dangling_task;
mod collection_literal_concatenation;
mod mutable_defaults_in_dataclass_fields;
mod pairwise_over_zipped;
mod unused_noqa;

pub use ambiguous_unicode_character::{
    ambiguous_unicode_character, AmbiguousUnicodeCharacterComment,
    AmbiguousUnicodeCharacterDocstring, AmbiguousUnicodeCharacterString,
};
pub use asyncio_dangling_task::{asyncio_dangling_task, AsyncioDanglingTask};
pub use collection_literal_concatenation::{
    collection_literal_concatenation, CollectionLiteralConcatenation,
};
pub use mutable_defaults_in_dataclass_fields::{
    function_call_in_dataclass_defaults, is_dataclass, mutable_dataclass_default,
    FunctionCallInDataclassDefaultArgument, MutableDataclassDefault,
};
pub use pairwise_over_zipped::{pairwise_over_zipped, PairwiseOverZipped};
pub use unused_noqa::{UnusedCodes, UnusedNOQA};

#[derive(Clone, Copy)]
pub enum Context {
    String,
    Docstring,
    Comment,
}
