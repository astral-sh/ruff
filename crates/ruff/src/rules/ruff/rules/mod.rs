mod ambiguous_unicode_character;
mod asyncio_dangling_task;
mod pairwise_over_zipped;
mod unpack_instead_of_concatenating_to_collection_literal;
mod unused_noqa;

pub use ambiguous_unicode_character::{
    ambiguous_unicode_character, AmbiguousUnicodeCharacterComment,
    AmbiguousUnicodeCharacterDocstring, AmbiguousUnicodeCharacterString,
};
pub use asyncio_dangling_task::{asyncio_dangling_task, AsyncioDanglingTask};
pub use pairwise_over_zipped::{pairwise_over_zipped, PairwiseOverZipped};
pub use unpack_instead_of_concatenating_to_collection_literal::{
    unpack_instead_of_concatenating_to_collection_literal, CollectionLiteralConcatenation,
};
pub use unused_noqa::{UnusedCodes, UnusedNOQA};

#[derive(Clone, Copy)]
pub enum Context {
    String,
    Docstring,
    Comment,
}
