use crate::lexer::{LexResult, Tok};
pub use crate::mode::Mode;

/// Collect all tokens from a token stream in a vector.
fn collect_tokens(tokenizer: impl IntoIterator<Item = LexResult>) -> Vec<LexResult> {
    let mut tokens: Vec<LexResult> = vec![];
    for tok in tokenizer {
        let is_err = tok.is_err();
        tokens.push(tok);
        if is_err {
            break;
        }
    }
    tokens
}

/// Modify a token stream to accommodate soft keywords (namely, `match` and `case`).
///
/// [PEP 634](https://www.python.org/dev/peps/pep-0634/) introduced the `match` and `case` keywords
/// as soft keywords, meaning that they can be used as identifiers (e.g., variable names) in certain
/// contexts.
///
/// This function modifies a token stream to accommodate this change. In particular, it replaces
/// `match` and `case` tokens with `identifier` tokens if they are used as identifiers.
///
/// Handling soft keywords in this intermediary pass allows us to simplify both the lexer and
/// parser, as neither of them need to be aware of soft keywords.
pub fn soft_keywords(
    tokenizer: impl IntoIterator<Item = LexResult>,
    mode: Mode,
) -> Vec<LexResult> {
    let mut tokenizer: Vec<LexResult> = collect_tokens(tokenizer);
    let mut start_of_line = matches!(mode, Mode::Module | Mode::Interactive);
    for i in 0..tokenizer.len() {
        // If the token is a `match` or `case` token, check if it's used as an identifier.
        // We assume every `match` or `case` is an identifier unless both of the following
        // conditions are met:
        // 1. The token is at the start of a logical line.
        // 2. The logical line contains a top-level colon (that is, a colon that is not nested
        //    inside a parenthesized expression, list, or dictionary).
        // 3. The top-level colon is not the immediate sibling of a `match` or `case` token.
        //    (This is to avoid treating `match` and `case` as identifiers when annotated with
        //    type hints.)
        if tokenizer[i]
            .as_ref()
            .map_or(false, |(_, tok, _)| matches!(tok, Tok::Match | Tok::Case))
        {
            let is_identifier = {
                if !start_of_line {
                    // If the `match` or `case` token is not at the start of a line, it's definitely
                    // an identifier.
                    true
                } else {
                    //
                    let mut seen_colon = false;
                    let mut first = true;
                    let mut par_count = 0;
                    let mut sqb_count = 0;
                    let mut brace_count = 0;
                    for (_, tok, _) in tokenizer.iter().skip(i + 1).flatten() {
                        match tok {
                            Tok::Newline => break,
                            Tok::Colon if par_count == 0 && sqb_count == 0 && brace_count == 0 => {
                                if !first {
                                    seen_colon = true;
                                }
                                break;
                            }
                            Tok::Lpar => {
                                par_count += 1;
                            }
                            Tok::Rpar => {
                                par_count -= 1;
                            }
                            Tok::Lsqb => {
                                sqb_count += 1;
                            }
                            Tok::Rsqb => {
                                sqb_count -= 1;
                            }
                            Tok::Lbrace => {
                                brace_count += 1;
                            }
                            Tok::Rbrace => {
                                brace_count -= 1;
                            }
                            _ => {}
                        }
                        first = false;
                    }
                    !seen_colon
                }
            };
            if is_identifier {
                if let Ok((_, tok, _)) = &mut tokenizer[i] {
                    if let Tok::Match = tok {
                        *tok = Tok::Name {
                            name: "match".to_string(),
                        };
                    } else if let Tok::Case = tok {
                        *tok = Tok::Name {
                            name: "case".to_string(),
                        };
                    }
                }
            }
        }
        start_of_line = tokenizer[i].as_ref().map_or(false, |(_, tok, _)| {
            matches!(
                tok,
                Tok::StartModule | Tok::StartInteractive | Tok::Newline | Tok::Indent | Tok::Dedent
            )
        });
    }

    tokenizer
}
