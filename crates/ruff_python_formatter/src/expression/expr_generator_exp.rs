use crate::context::PyFormatContext;
use crate::expression::parentheses::parenthesized;
use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses};
use crate::prelude::*;
use crate::AsFormat;
use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::{format_args, write, Buffer, FormatResult, FormatRuleWithOptions};
use ruff_python_ast::node::AnyNodeRef;
use ruff_python_ast::ExprGeneratorExp;

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

        if self.parentheses == GeneratorExpParentheses::StripIfOnlyFunctionArg {
            write!(
                f,
                [
                    group(&elt.format()),
                    soft_line_break_or_space(),
                    group(&joined),
                ]
            )
        } else {
            write!(
                f,
                [parenthesized(
                    "(",
                    &format_args!(
                        group(&elt.format()),
                        soft_line_break_or_space(),
                        group(&joined)
                    ),
                    ")"
                )]
            )
        }
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
