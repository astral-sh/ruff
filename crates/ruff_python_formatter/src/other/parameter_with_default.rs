use ruff_formatter::write;
use ruff_python_ast::ParameterWithDefault;
use ruff_python_trivia::{SimpleTokenKind, SimpleTokenizer};
use ruff_text_size::{Ranged, TextRange};

use crate::prelude::*;

#[derive(Default)]
pub struct FormatParameterWithDefault;

impl FormatNodeRule<ParameterWithDefault> for FormatParameterWithDefault {
    fn fmt_fields(&self, item: &ParameterWithDefault, f: &mut PyFormatter) -> FormatResult<()> {
        let ParameterWithDefault {
            range: _,
            parameter,
            default,
        } = item;

        write!(f, [parameter.format()])?;

        if let Some(default) = default {
            let space = parameter.annotation.is_some().then_some(space());
            // ```python
            // def f(
            //     a = # parameter trailing comment; needs line break
            //     1,
            //     b =
            //     # default leading comment; needs line break
            //     2,
            //     c = ( # the default leading can only be end-of-line with parentheses; no line break
            //         3
            //     ),
            //     d = (
            //         # own line leading comment with parentheses; no line break
            //         4
            //     )
            // )
            // ```
            let needs_line_break_trailing = f.context().comments().has_trailing(parameter);
            let default_first_comment = f.context().comments().leading(default.as_ref()).first();
            let needs_line_break_leading = default_first_comment.is_some_and(|default_leading_comment| {
                let mut tokenizer = SimpleTokenizer::new(
                    f.context().source(),
                    TextRange::new(parameter.end(), default_leading_comment.start()),
                )
                .skip_trivia()
                .skip_while(|token| token.kind == SimpleTokenKind::RParen);
                let equals = tokenizer.next();
                debug_assert!(equals.is_some_and(|token| token.kind == SimpleTokenKind::Equals));
                let lparens = tokenizer.next();
                debug_assert!(lparens
                    .as_ref()
                    .map_or(true, |token| token.kind == SimpleTokenKind::LParen));
                lparens.is_none()
            });
            let needs_line_break = needs_line_break_trailing || needs_line_break_leading;

            write!(
                f,
                [
                    space,
                    token("="),
                    (!needs_line_break).then_some(space),
                    needs_line_break.then_some(hard_line_break()),
                    default.format()
                ]
            )?;
        }

        Ok(())
    }
}
