use ruff_python_ast::{Pattern, PatternMatchAs, PatternMatchMapping, PatternMatchStar};

use crate::checkers::ast::Checker;
use crate::codes::Rule;
use crate::rules::wemake_python_styleguide;

/// Run lint rules over a [`Pattern`] syntax node.
pub(crate) fn pattern(pattern: &Pattern, checker: &mut Checker) {
    if let Pattern::MatchAs(PatternMatchAs {
        name: Some(name), ..
    })
    | Pattern::MatchStar(PatternMatchStar {
        name: Some(name),
        range: _,
    })
    | Pattern::MatchMapping(PatternMatchMapping {
        rest: Some(name), ..
    }) = pattern
    {
        if checker.enabled(Rule::TooShortName) {
            wemake_python_styleguide::rules::too_short_name(checker, name);
        }
    }
}
