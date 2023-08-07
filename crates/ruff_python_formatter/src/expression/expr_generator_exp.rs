use ruff_formatter::{format_args, write, Buffer, FormatResult, FormatRuleWithOptions};
use ruff_python_ast::node::AnyNodeRef;
use ruff_python_ast::ExprGeneratorExp;

use crate::comments::leading_comments;
use crate::context::PyFormatContext;
use crate::expression::parentheses::{parenthesized, NeedsParentheses, OptionalParentheses};
use crate::prelude::*;
use crate::AsFormat;
use crate::{FormatNodeRule, PyFormatter};

#[derive(Eq, PartialEq, Debug, Default)]
pub enum GeneratorExpParentheses {
    #[default]
    Default,

    // skip parens if the generator exp is the only argument to a function, e.g.
    // ```python
    //  all(x for y in z)`
    //  ```
    StripIfOnlyFunctionArg,
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
        let dangling = comments.dangling_comments(item);

        if self.parentheses == GeneratorExpParentheses::StripIfOnlyFunctionArg {
            write!(
                f,
                [
                    leading_comments(dangling),
                    group(&elt.format()),
                    soft_line_break_or_space(),
                    &joined
                ]
            )
        } else {
            write!(
                f,
                [parenthesized(
                    "(",
                    &group(&format_args!(
                        group(&elt.format()),
                        soft_line_break_or_space(),
                        &joined
                    )),
                    ")"
                )
                .with_dangling_comments(dangling)]
            )
        }
    }

    fn fmt_dangling_comments(
        &self,
        _node: &ExprGeneratorExp,
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
