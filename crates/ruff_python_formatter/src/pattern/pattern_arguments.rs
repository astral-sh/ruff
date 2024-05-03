use ruff_formatter::write;
use ruff_python_ast::AstNode;
use ruff_python_ast::{Pattern, PatternArguments};
use ruff_python_trivia::{SimpleTokenKind, SimpleTokenizer};
use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::expression::parentheses::{empty_parenthesized, parenthesized, Parentheses};
use crate::prelude::*;

#[derive(Default)]
pub struct FormatPatternArguments;

impl FormatNodeRule<PatternArguments> for FormatPatternArguments {
    fn fmt_fields(&self, item: &PatternArguments, f: &mut PyFormatter) -> FormatResult<()> {
        // If there are no arguments, all comments are dangling:
        // ```python
        // case Point2D(  # dangling
        //     # dangling
        // )
        // ```
        if item.patterns.is_empty() && item.keywords.is_empty() {
            let comments = f.context().comments().clone();
            let dangling = comments.dangling(item);
            return write!(f, [empty_parenthesized("(", dangling, ")")]);
        }

        let all_arguments = format_with(|f: &mut PyFormatter| {
            let source = f.context().source();
            let mut joiner = f.join_comma_separated(item.end());
            match item.patterns.as_slice() {
                [pattern] if item.keywords.is_empty() => {
                    let parentheses =
                        if is_single_argument_parenthesized(pattern, item.end(), source) {
                            Parentheses::Always
                        } else {
                            // Note: no need to handle opening-parenthesis comments, since
                            // an opening-parenthesis comment implies that the argument is
                            // parenthesized.
                            Parentheses::Never
                        };
                    joiner.entry(pattern, &pattern.format().with_options(parentheses));
                }
                patterns => {
                    joiner
                        .entries(patterns.iter().map(|pattern| {
                            (
                                pattern,
                                pattern.format().with_options(Parentheses::Preserve),
                            )
                        }))
                        .nodes(item.keywords.iter());
                }
            }

            joiner.finish()
        });

        // If the arguments are non-empty, then a dangling comment indicates a comment on the
        // same line as the opening parenthesis, e.g.:
        // ```python
        // case Point2D(  # dangling
        //     ...
        // )
        // ```
        let comments = f.context().comments().clone();
        let dangling_comments = comments.dangling(item.as_any_node_ref());

        write!(
            f,
            [parenthesized("(", &group(&all_arguments), ")")
                .with_dangling_comments(dangling_comments)]
        )
    }
}

/// Returns `true` if the pattern (which is the only argument to a [`PatternMatchClass`]) is
/// parenthesized. Used to avoid falsely assuming that `x` is parenthesized in cases like:
/// ```python
/// case Point2D(x): ...
/// ```
fn is_single_argument_parenthesized(pattern: &Pattern, call_end: TextSize, source: &str) -> bool {
    let mut has_seen_r_paren = false;
    for token in SimpleTokenizer::new(source, TextRange::new(pattern.end(), call_end)).skip_trivia()
    {
        match token.kind() {
            SimpleTokenKind::RParen => {
                if has_seen_r_paren {
                    return true;
                }
                has_seen_r_paren = true;
            }
            // Skip over any trailing comma
            SimpleTokenKind::Comma => continue,
            _ => {
                // Passed the arguments
                break;
            }
        }
    }
    false
}
