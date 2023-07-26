use rustpython_ast::text_size::TextSize;
use rustpython_ast::{CmpOp, Expr, Mod, ModModule, Ranged, Suite};
use rustpython_parser as parser;
use rustpython_parser::lexer::LexResult;
use rustpython_parser::text_size::TextRange;
use rustpython_parser::{lexer, Mode, ParseError, Tok};

pub mod token_kind;
pub mod typing;

/// Collect tokens up to and including the first error.
pub fn tokenize(contents: &str, mode: Mode) -> Vec<LexResult> {
    let mut tokens: Vec<LexResult> = vec![];
    for tok in lexer::lex(contents, mode) {
        let is_err = tok.is_err();
        tokens.push(tok);
        if is_err {
            break;
        }
    }
    tokens
}

/// Parse a full Python program from its tokens.
pub fn parse_program_tokens(
    lxr: Vec<LexResult>,
    source_path: &str,
    is_jupyter_notebook: bool,
) -> anyhow::Result<Suite, ParseError> {
    let mode = if is_jupyter_notebook {
        Mode::Jupyter
    } else {
        Mode::Module
    };
    parser::parse_tokens(lxr, mode, source_path).map(|top| match top {
        Mod::Module(ModModule { body, .. }) => body,
        _ => unreachable!(),
    })
}

/// Return the `Range` of the first `Tok::Colon` token in a `Range`.
pub fn first_colon_range(
    range: TextRange,
    source: &str,
    is_jupyter_notebook: bool,
) -> Option<TextRange> {
    let contents = &source[range];
    let mode = if is_jupyter_notebook {
        Mode::Jupyter
    } else {
        Mode::Module
    };
    let range = lexer::lex_starts_at(contents, mode, range.start())
        .flatten()
        .find(|(tok, _)| tok.is_colon())
        .map(|(_, range)| range);
    range
}

/// Extract all [`CmpOp`] operators from an expression snippet, with appropriate
/// ranges.
///
/// `RustPython` doesn't include line and column information on [`CmpOp`] nodes.
/// `CPython` doesn't either. This method iterates over the token stream and
/// re-identifies [`CmpOp`] nodes, annotating them with valid ranges.
pub fn locate_cmp_ops(expr: &Expr, source: &str) -> Vec<LocatedCmpOp> {
    // If `Expr` is a multi-line expression, we need to parenthesize it to
    // ensure that it's lexed correctly.
    let contents = &source[expr.range()];
    let parenthesized_contents = format!("({contents})");
    let mut tok_iter = lexer::lex(&parenthesized_contents, Mode::Expression)
        .flatten()
        .skip(1)
        .map(|(tok, range)| (tok, range - TextSize::from(1)))
        .filter(|(tok, _)| !matches!(tok, Tok::NonLogicalNewline | Tok::Comment(_)))
        .peekable();

    let mut ops: Vec<LocatedCmpOp> = vec![];
    let mut count = 0u32;
    loop {
        let Some((tok, range)) = tok_iter.next() else {
            break;
        };
        if matches!(tok, Tok::Lpar) {
            count = count.saturating_add(1);
            continue;
        } else if matches!(tok, Tok::Rpar) {
            count = count.saturating_sub(1);
            continue;
        }
        if count == 0 {
            match tok {
                Tok::Not => {
                    if let Some((_, next_range)) =
                        tok_iter.next_if(|(tok, _)| matches!(tok, Tok::In))
                    {
                        ops.push(LocatedCmpOp::new(
                            TextRange::new(range.start(), next_range.end()),
                            CmpOp::NotIn,
                        ));
                    }
                }
                Tok::In => {
                    ops.push(LocatedCmpOp::new(range, CmpOp::In));
                }
                Tok::Is => {
                    let op = if let Some((_, next_range)) =
                        tok_iter.next_if(|(tok, _)| matches!(tok, Tok::Not))
                    {
                        LocatedCmpOp::new(
                            TextRange::new(range.start(), next_range.end()),
                            CmpOp::IsNot,
                        )
                    } else {
                        LocatedCmpOp::new(range, CmpOp::Is)
                    };
                    ops.push(op);
                }
                Tok::NotEqual => {
                    ops.push(LocatedCmpOp::new(range, CmpOp::NotEq));
                }
                Tok::EqEqual => {
                    ops.push(LocatedCmpOp::new(range, CmpOp::Eq));
                }
                Tok::GreaterEqual => {
                    ops.push(LocatedCmpOp::new(range, CmpOp::GtE));
                }
                Tok::Greater => {
                    ops.push(LocatedCmpOp::new(range, CmpOp::Gt));
                }
                Tok::LessEqual => {
                    ops.push(LocatedCmpOp::new(range, CmpOp::LtE));
                }
                Tok::Less => {
                    ops.push(LocatedCmpOp::new(range, CmpOp::Lt));
                }
                _ => {}
            }
        }
    }
    ops
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocatedCmpOp {
    pub range: TextRange,
    pub op: CmpOp,
}

impl LocatedCmpOp {
    fn new<T: Into<TextRange>>(range: T, op: CmpOp) -> Self {
        Self {
            range: range.into(),
            op,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{first_colon_range, locate_cmp_ops, LocatedCmpOp};
    use anyhow::Result;
    use ruff_text_size::TextSize;
    use rustpython_ast::text_size::{TextLen, TextRange};
    use rustpython_ast::CmpOp;
    use rustpython_ast::Expr;
    use rustpython_parser::Parse;

    #[test]
    fn extract_first_colon_range() {
        let contents = "with a: pass";
        let range = first_colon_range(
            TextRange::new(TextSize::from(0), contents.text_len()),
            contents,
            false,
        )
        .unwrap();
        assert_eq!(&contents[range], ":");
        assert_eq!(range, TextRange::new(TextSize::from(6), TextSize::from(7)));
    }

    #[test]
    fn extract_cmp_op_location() -> Result<()> {
        let contents = "x == 1";
        let expr = Expr::parse(contents, "<filename>")?;
        assert_eq!(
            locate_cmp_ops(&expr, contents),
            vec![LocatedCmpOp::new(
                TextSize::from(2)..TextSize::from(4),
                CmpOp::Eq
            )]
        );

        let contents = "x != 1";
        let expr = Expr::parse(contents, "<filename>")?;
        assert_eq!(
            locate_cmp_ops(&expr, contents),
            vec![LocatedCmpOp::new(
                TextSize::from(2)..TextSize::from(4),
                CmpOp::NotEq
            )]
        );

        let contents = "x is 1";
        let expr = Expr::parse(contents, "<filename>")?;
        assert_eq!(
            locate_cmp_ops(&expr, contents),
            vec![LocatedCmpOp::new(
                TextSize::from(2)..TextSize::from(4),
                CmpOp::Is
            )]
        );

        let contents = "x is not 1";
        let expr = Expr::parse(contents, "<filename>")?;
        assert_eq!(
            locate_cmp_ops(&expr, contents),
            vec![LocatedCmpOp::new(
                TextSize::from(2)..TextSize::from(8),
                CmpOp::IsNot
            )]
        );

        let contents = "x in 1";
        let expr = Expr::parse(contents, "<filename>")?;
        assert_eq!(
            locate_cmp_ops(&expr, contents),
            vec![LocatedCmpOp::new(
                TextSize::from(2)..TextSize::from(4),
                CmpOp::In
            )]
        );

        let contents = "x not in 1";
        let expr = Expr::parse(contents, "<filename>")?;
        assert_eq!(
            locate_cmp_ops(&expr, contents),
            vec![LocatedCmpOp::new(
                TextSize::from(2)..TextSize::from(8),
                CmpOp::NotIn
            )]
        );

        let contents = "x != (1 is not 2)";
        let expr = Expr::parse(contents, "<filename>")?;
        assert_eq!(
            locate_cmp_ops(&expr, contents),
            vec![LocatedCmpOp::new(
                TextSize::from(2)..TextSize::from(4),
                CmpOp::NotEq
            )]
        );

        Ok(())
    }
}
