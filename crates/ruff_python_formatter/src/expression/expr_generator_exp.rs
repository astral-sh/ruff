use ruff_formatter::{format_args, write, FormatRuleWithOptions};
use ruff_python_ast::node::AnyNodeRef;
use ruff_python_ast::ExprGeneratorExp;
use ruff_python_trivia::{SimpleTokenKind, SimpleTokenizer};
use ruff_text_size::{Ranged, TextRange};

use crate::comments::SourceComment;
use crate::expression::parentheses::{parenthesized, NeedsParentheses, OptionalParentheses};
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

impl FormatRuleWithOptions<ExprGeneratorExp, PyFormatContext<'_>> for FormatExprGeneratorExp {
    type Options = GeneratorExpParentheses;

    fn with_options(mut self, options: Self::Options) -> Self {
        self.parentheses = options;
        self
    }
}

#[derive(Default)]
pub struct FormatExprGeneratorExp {
    parentheses: GeneratorExpParentheses,
}

impl FormatNodeRule<ExprGeneratorExp> for FormatExprGeneratorExp {
    fn fmt_fields(&self, item: &ExprGeneratorExp, f: &mut PyFormatter) -> FormatResult<()> {
        let ExprGeneratorExp {
            range: _,
            elt,
            generators,
        } = item;

        let joined = format_with(|f| {
            f.join_with(soft_line_break_or_space())
                .entries(generators.iter().formatted())
                .finish()
        });

        let comments = f.context().comments().clone();
        let dangling = comments.dangling(item);

        if self.parentheses == GeneratorExpParentheses::Preserve
            && dangling.is_empty()
            && !is_generator_parenthesized(item, f.context().source())
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

    fn fmt_dangling_comments(
        &self,
        _dangling_comments: &[SourceComment],
        _f: &mut PyFormatter,
    ) -> FormatResult<()> {
        // Handled as part of `fmt_fields`
        Ok(())
    }
}

impl NeedsParentheses for ExprGeneratorExp {
    fn needs_parentheses(
        &self,
        _parent: AnyNodeRef,
        _context: &PyFormatContext,
    ) -> OptionalParentheses {
        OptionalParentheses::Never
    }
}

fn is_generator_parenthesized(generator: &ExprGeneratorExp, source: &str) -> bool {
    // / Count the number of open parentheses between the start of the tuple and the first element.
    let open_parentheses_count = SimpleTokenizer::new(
        source,
        TextRange::new(generator.start(), generator.elt.start()),
    )
    .skip_trivia()
    .filter(|token| token.kind() == SimpleTokenKind::LParen)
    .count();
    if open_parentheses_count == 0 {
        return false;
    }

    // Count the number of parentheses between the end of the first element and its trailing comma.
    let close_parentheses_count = SimpleTokenizer::new(
        source,
        TextRange::new(
            generator.elt.end(),
            generator
                .generators
                .first()
                .map_or(generator.end(), Ranged::start),
        ),
    )
    .skip_trivia()
    .filter(|token| token.kind() == SimpleTokenKind::RParen)
    .count();

    // If the number of open parentheses is greater than the number of close parentheses, the generator
    // is parenthesized.
    open_parentheses_count > close_parentheses_count
}
