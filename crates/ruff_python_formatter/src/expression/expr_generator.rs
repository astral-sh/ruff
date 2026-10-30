use ruff_formatter::{FormatRuleWithOptions, format_args, write};
use ruff_python_ast::ExprGenerator;
use ruff_python_ast::flat;
use ruff_python_ast::flat::AnyNodeRef;

use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses, parenthesized};
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

impl FormatRuleWithOptions<flat::View<'_, ExprGenerator>, PyFormatContext<'_>>
    for FormatExprGenerator
{
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
    fn fmt_fields(
        &self,
        item: flat::View<'_, ExprGenerator>,
        f: &mut PyFormatter,
    ) -> FormatResult<()> {
        let elt = item.elt();
        let generators = item.generators();
        let is_parenthesized = item.parenthesized();

        let joined = format_with(|f| {
            f.join_with(soft_line_break_or_space())
                .entries(generators.iter().formatted())
                .finish()
        });

        let comments = f.context().comments().clone();
        let dangling = comments.dangling(item);

        if self.parentheses == GeneratorExpParentheses::Preserve
            && dangling.is_empty()
            && !is_parenthesized
        {
            write!(
                f,
                [group(&elt.format()), soft_line_break_or_space(), &joined]
            )
        } else {
            write!(
                f,
                [parenthesized(
                    "(",
                    &group(&format_args!(
                        group(&elt.format()),
                        soft_line_break_or_space(),
                        joined
                    )),
                    ")"
                )
                .with_dangling_comments(dangling)]
            )
        }
    }
}

impl NeedsParentheses for flat::View<'_, ExprGenerator> {
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
