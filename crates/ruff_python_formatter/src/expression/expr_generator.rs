use crate::expression::parentheses::{parenthesized, NeedsParentheses, OptionalParentheses};
use crate::prelude::*;
use ruff_formatter::{write, FormatRuleWithOptions};
use ruff_python_ast::AnyNodeRef;
use ruff_python_ast::ExprGenerator;

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
            elt,
            generators,
            parenthesized: is_parenthesized,
        } = item;

        let comments = f.context().comments().clone();
        let dangling = comments.dangling(item);

        let inner_content = format_with(|f| {
            write!(f, [
                group(&elt.format()),
                soft_line_break_or_space(),
            ])?;

            f.join_with(soft_line_break_or_space())
                .entries(generators.iter().formatted())
                .finish()
        });

        if self.parentheses == GeneratorExpParentheses::Preserve
            && dangling.is_empty()
            && !is_parenthesized
        {
            write!(f, [group_with_flat_width_limit(
                &inner_content,
                f.options().generator_expression_width_limit().into(),
                true,
            )])
        } else {
            write!(
                f,
                [parenthesized(
                    "(",
                    &group_with_flat_width_limit(
                        &inner_content,
                        f.options().generator_expression_width_limit().into(),
                        true,
                    ),
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
