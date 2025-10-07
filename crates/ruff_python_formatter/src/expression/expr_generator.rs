use ruff_formatter::{FormatRuleWithOptions, write};
use ruff_python_ast::AnyNodeRef;
use ruff_python_ast::ExprGenerator;

use crate::expression::comprehension_helpers::is_comprehension_multiline;
use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses, parenthesized};
use crate::options::ComprehensionLineBreak;
use crate::prelude::*;

#[derive(Eq, PartialEq, Debug, Default)]
pub enum GeneratorExpParentheses {
    #[default]
    Default,

    /// Skips the parentheses if they aren't present in the source code. Used when formatting call expressions
    /// because the parentheses are optional if the generator is the **only** argument:
    ///
    /// ```python
    /// all(x for y in z)`
    /// ```
    Preserve,
}

impl FormatRuleWithOptions<ExprGenerator, PyFormatContext<'_>> for FormatExprGenerator {
    type Options = GeneratorExpParentheses;

    fn with_options(mut self, options: Self::Options) -> Self {
        self.parentheses = options;
        self
    }
}

#[derive(Default)]
pub struct FormatExprGenerator {
    parentheses: GeneratorExpParentheses,
}

impl FormatNodeRule<ExprGenerator> for FormatExprGenerator {
    fn fmt_fields(&self, item: &ExprGenerator, f: &mut PyFormatter) -> FormatResult<()> {
        let ExprGenerator {
            range: _,
            node_index: _,
            elt,
            generators,
            parenthesized: is_parenthesized,
        } = item;

        let joined = format_with(|f| {
            f.join_with(soft_line_break_or_space())
                .entries(generators.iter().formatted())
                .finish()
        });

        let comments = f.context().comments().clone();
        let dangling = comments.dangling(item);

        // Check if we should preserve multi-line formatting
        let should_preserve_multiline =
            f.options().comprehension_line_break() == ComprehensionLineBreak::Preserve
            && is_comprehension_multiline(item, f.context());

        if self.parentheses == GeneratorExpParentheses::Preserve
            && dangling.is_empty()
            && !is_parenthesized
        {
            if should_preserve_multiline {
                // Force expansion to preserve multi-line format
                let formatted = format_with(|f| {
                    write!(f, [group(&elt.format()), soft_line_break_or_space(), joined])
                });
                write!(f, [group(&formatted).should_expand(true)])
            } else {
                write!(
                    f,
                    [group(&elt.format()), soft_line_break_or_space(), &joined]
                )
            }
        } else {
            let formatted_content = format_with(|f| {
                write!(f, [
                    group(&elt.format()),
                    soft_line_break_or_space(),
                    joined
                ])
            });

            write!(
                f,
                [parenthesized(
                    "(",
                    &if should_preserve_multiline {
                        // Force expansion to preserve multi-line format
                        group(&formatted_content).should_expand(true)
                    } else {
                        // Default behavior - try to fit on one line
                        group(&formatted_content)
                    },
                    ")"
                )
                .with_dangling_comments(dangling)]
            )
        }
    }
}

impl NeedsParentheses for ExprGenerator {
    fn needs_parentheses(
        &self,
        parent: AnyNodeRef,
        _context: &PyFormatContext,
    ) -> OptionalParentheses {
        if parent.is_expr_await() {
            OptionalParentheses::Always
        } else {
            OptionalParentheses::Never
        }
    }
}
