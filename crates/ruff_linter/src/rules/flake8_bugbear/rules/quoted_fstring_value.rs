use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{
    ExprFString, FStringElement, FStringExpressionElement,
    FStringPart::{FString, Literal},
};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for f-string values inside of quotes which can be rewritten
/// with `!r` or `repr`
///
/// ## Why is this bad?
/// The f-string value may itself be a string which contains quote characters,
/// leading to an ambiguous output.
///
/// ## Example
/// ```python
/// foo = "hello"
/// bar = "g'day"
/// print(f"'{foo}' '{bar}'")
/// ```
/// would output
/// ```text
/// 'hello' 'g'day'
/// ```
///
/// Use instead:
/// ```python
/// foo = "hello"
/// bar = "g'day"
/// print(f"{foo!r} {bar!r}")
/// ```
/// which would output
/// ```text
/// 'hello' "g'day"
/// ```
#[violation]
pub struct QuotedFStringValue {
    mark: char,
}

impl Violation for QuotedFStringValue {
    #[derive_message_formats]
    fn message(&self) -> String {
        let mark = self.mark;
        format!("If value contains character '\\{mark}', output may be confusing. Consider removing surrounding quotes and adding !r")
    }
}

/// B907
pub(crate) fn quoted_fstring_value(checker: &mut Checker, expr_fstring: &ExprFString) {
    // maintain a finite state machine
    enum State<'a> {
        // haven't seen any quotes recently
        None,
        // just saw a literal that ended in a quote
        SeenMark(char),
        // the end of the previous literal ended in a quote, and there was
        // a single expression between then and now
        SeenMarkAndVar(char, &'a FStringExpressionElement),
    }
    let mut state = State::None;

    // iterate through all components and update state machine
    for part in &expr_fstring.value {
        // separated this out into its own function because we have two
        // different string literal types that have the same behavior
        fn process_str_lit(s: &str, is_raw: bool, state: &mut State, checker: &mut Checker) {
            if let State::SeenMarkAndVar(mark, var) = *state {
                // if we are in this state, then we want to look for
                // a matching quote right after the variable

                let first_char = {
                    let mut chars = s.chars();
                    let Some(c) = chars.next() else {
                        return;
                    };
                    if c != '\\' || is_raw {
                        c
                    } else {
                        match chars.next() {
                            None => return,
                            Some(c) => c,
                        }
                    }
                };

                if first_char == mark {
                    // found one, so report it
                    checker
                        .diagnostics
                        .push(Diagnostic::new(QuotedFStringValue { mark }, var.range));
                }
            }

            // regardless of what state we are in, we should check
            // the last character to determine the next state

            let Some(last_char) = s.chars().last() else {
                return;
            };

            *state = match last_char {
                '\'' | '"' => State::SeenMark(last_char),
                _ => State::None,
            };
        }
        match part {
            Literal(str_lit) => {
                use ruff_python_ast::str_prefix::StringLiteralPrefix;
                process_str_lit(
                    str_lit.value.as_ref(),
                    matches!(str_lit.flags.prefix(), StringLiteralPrefix::Raw { .. }),
                    &mut state,
                    checker,
                );
            }
            FString(fstr) => {
                for elem in &fstr.elements {
                    match elem {
                        FStringElement::Literal(str_lit) => {
                            use ruff_python_ast::str_prefix::FStringPrefix;
                            process_str_lit(
                                str_lit.value.as_ref(),
                                matches!(fstr.flags.prefix(), FStringPrefix::Raw { .. }),
                                &mut state,
                                checker,
                            );
                        }
                        FStringElement::Expression(fstr_expr) => {
                            use ruff_python_ast::ConversionFlag;

                            // we only want to check this if we saw
                            // a quote before
                            let State::SeenMark(mark) = state else {
                                state = State::None;
                                continue;
                            };

                            // skip if it already has a !r
                            if fstr_expr.conversion == ConversionFlag::Repr {
                                state = State::None;
                                continue;
                            }

                            let Some(format_spec) = &fstr_expr.format_spec else {
                                // there is no format spec, so our job is easy
                                state = State::SeenMarkAndVar(mark, fstr_expr);
                                continue;
                            };

                            let mut elems = format_spec.elements.iter();

                            let Some(first_elem) = elems.next() else {
                                // there is no format spec, so our job is easy
                                state = State::SeenMarkAndVar(mark, fstr_expr);
                                continue;
                            };

                            let Some(format_spec) = first_elem.as_literal() else {
                                // the format spec contains substitutions,
                                // this is too complex to check, so just skip
                                state = State::None;
                                continue;
                            };
                            let format_spec = format_spec.value.as_ref();

                            if elems.next().is_some() {
                                // the format spec contains substitutions,
                                // this is too complex to check, so just skip
                                state = State::None;
                                continue;
                            }

                            // check that the format spec is valid for strings;
                            // format specs intended for other types of values
                            // give us a type hint that this lint doesn't
                            // make sense
                            let mut chars = format_spec.chars();
                            let c1 = chars.next();
                            let c2 = chars.next();
                            let c3 = chars.next();
                            if chars.next().is_some() {
                                state = State::None;
                                continue;
                            }
                            match (c1, c2, c3) {
                                (Some('s'), None, None)
                                | (Some('<' | '>' | '^'), None | Some('s'), None)
                                | (Some(_), Some('<' | '>' | '^'), None | Some('s')) => {
                                    state = State::SeenMarkAndVar(mark, fstr_expr);
                                }
                                _ => {
                                    state = State::None;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
