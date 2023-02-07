mod ambiguous_unicode_character;
mod keyword_argument_before_star_argument;
mod unpack_instead_of_concatenating_to_collection_literal;
mod unused_noqa;

pub use ambiguous_unicode_character::{
    ambiguous_unicode_character, AmbiguousUnicodeCharacterComment,
    AmbiguousUnicodeCharacterDocstring, AmbiguousUnicodeCharacterString,
};
pub use keyword_argument_before_star_argument::{
    keyword_argument_before_star_argument, KeywordArgumentBeforeStarArgument,
};
pub use unpack_instead_of_concatenating_to_collection_literal::{
    unpack_instead_of_concatenating_to_collection_literal,
    UnpackInsteadOfConcatenatingToCollectionLiteral,
};
pub use unused_noqa::{UnusedCodes, UnusedNOQA};

#[derive(Clone, Copy)]
pub enum Context {
    String,
    Docstring,
    Comment,
}
