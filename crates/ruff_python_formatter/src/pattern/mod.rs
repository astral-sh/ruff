use ruff_formatter::{FormatOwnedWithRule, FormatRefWithRule};
use ruff_python_ast::Pattern;

use crate::prelude::*;

pub(crate) mod pattern_match_as;
pub(crate) mod pattern_match_class;
pub(crate) mod pattern_match_mapping;
pub(crate) mod pattern_match_or;
pub(crate) mod pattern_match_sequence;
pub(crate) mod pattern_match_singleton;
pub(crate) mod pattern_match_star;
pub(crate) mod pattern_match_value;

#[derive(Default)]
pub struct FormatPattern;

impl FormatRule<Pattern, PyFormatContext<'_>> for FormatPattern {
    fn fmt(&self, item: &Pattern, f: &mut PyFormatter) -> FormatResult<()> {
        match item {
            Pattern::MatchValue(p) => p.format().fmt(f),
            Pattern::MatchSingleton(p) => p.format().fmt(f),
            Pattern::MatchSequence(p) => p.format().fmt(f),
            Pattern::MatchMapping(p) => p.format().fmt(f),
            Pattern::MatchClass(p) => p.format().fmt(f),
            Pattern::MatchStar(p) => p.format().fmt(f),
            Pattern::MatchAs(p) => p.format().fmt(f),
            Pattern::MatchOr(p) => p.format().fmt(f),
        }
    }
}

impl<'ast> AsFormat<PyFormatContext<'ast>> for Pattern {
    type Format<'a> = FormatRefWithRule<'a, Pattern, FormatPattern, PyFormatContext<'ast>>;

    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(self, FormatPattern)
    }
}

impl<'ast> IntoFormat<PyFormatContext<'ast>> for Pattern {
    type Format = FormatOwnedWithRule<Pattern, FormatPattern, PyFormatContext<'ast>>;

    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, FormatPattern)
    }
}
