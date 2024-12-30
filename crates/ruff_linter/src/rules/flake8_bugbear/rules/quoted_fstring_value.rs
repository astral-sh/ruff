use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{
    ExprFString, FStringElement,
    FStringPart::{FString, Literal},
};
use ruff_text_size::TextRange;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for f-string values inside of quotes which may generate
/// confusing output.
///
/// ## Why is this bad?
/// The f-string value may itself be a string which contains quote characters,
/// leading to a confusing output.
///
/// ## Fix
/// The appropriate fix will be different depending on a case-by-case basis.
///
/// ### False positives and intended behavior
/// It may be that this lint does not make sense for your use-case, and
/// the lint triggers on behavior which is intentional. In this case, you may
/// signal to the rule that this is intentional behavior by either
/// - adding a non-empty format string for non-string types, or
/// - adding `!s` for string types or for types for which a format string does
/// not make sense.
///
/// ## Example
/// For the following code:
/// ```python
/// print(f"'{foo}'")
/// ```
/// the output would be confusing on input `foo = "g'day"`:
/// ```text
/// 'g'day'
/// ```
/// maybe use instead:
/// ```python
/// print(f"'{foo.replace("'", "\\'")!s}'")
/// ```
/// which would output:
/// ```text
/// 'g\'day'
/// ```
///
/// ### False positives
/// For the following code:
/// ```python
/// print(f"'{bar}'")
/// ```
/// you may know that `bar` is always an `int`, so signal this to the lint with:
/// ```python
/// print(f"'{bar:d}'")
/// ```
#[violation]
#[derive(Clone, Copy)]
pub struct QuotedFStringValue {
    mark: char,
    suggestion: Suggestion,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Suggestion {
    FormatSpec,
    TypeHint,
    SFlag,
}

impl Violation for QuotedFStringValue {
    #[derive_message_formats]
    fn message(&self) -> String {
        let mark = self.mark;
        let suggestion_str = match self.suggestion {
            Suggestion::FormatSpec => "add a format spec with a type hint",
            Suggestion::TypeHint => "add a type hint at the end of the format spec",
            Suggestion::SFlag => "add the `!s` conversion flag",
        };

        format!("If value is a string containing character '\\{mark}', output may be confusing. Consider if this is an issue for this use-case. If this is intended behavior, {suggestion_str}")
    }
}

/// B907
pub(crate) fn quoted_fstring_value(checker: &mut Checker, expr_fstring: &ExprFString) {
    // maintain a finite state machine
    enum State {
        // haven't seen any quotes recently
        None,
        // just saw a literal that ended in a quote
        SeenMark(char),
        // the end of the previous literal ended in a quote, and there was
        // a single expression between then and now
        SeenMarkAndVar {
            violation: QuotedFStringValue,
            range: TextRange,
        },
    }
    let mut state = State::None;

    // iterate through all components and update state machine
    for part in &expr_fstring.value {
        // separated this out into its own function because we have two
        // different string literal types that have the same behavior
        fn process_str_lit(s: &str, is_raw: bool, state: &mut State, checker: &mut Checker) {
            if let State::SeenMarkAndVar { violation, range } = *state {
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

                if first_char == violation.mark {
                    // found one, so report it
                    checker.diagnostics.push(Diagnostic::new(violation, range));
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

                            // skip if it already has a `!s` or `!r`
                            match fstr_expr.conversion {
                                ConversionFlag::Str | ConversionFlag::Repr => {
                                    state = State::None;
                                    continue;
                                }
                                _ => {}
                            }

                            // skip if it has debug formatting (has `=`)
                            if fstr_expr.debug_text.is_some() {
                                state = State::None;
                                continue;
                            }

                            let Some(format_spec) = &fstr_expr.format_spec else {
                                // there is no format spec, so our job is easy
                                state = State::SeenMarkAndVar {
                                    violation: QuotedFStringValue {
                                        mark,
                                        suggestion: Suggestion::FormatSpec,
                                    },
                                    range: fstr_expr.range,
                                };
                                continue;
                            };

                            let mut elems = format_spec.elements.iter();

                            let Some(first_elem) = elems.next() else {
                                // there is no format spec, so our job is easy
                                state = State::SeenMarkAndVar {
                                    violation: QuotedFStringValue {
                                        mark,
                                        suggestion: Suggestion::FormatSpec,
                                    },
                                    range: fstr_expr.range,
                                };
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
                            {
                                #[derive(Clone, Copy)]
                                enum FormatState {
                                    Start,
                                    Fill(char),
                                    Align,
                                    Width,
                                    Precision,
                                    Type,
                                    Skip,
                                }
                                let mut format_state = FormatState::Start;
                                for c in format_spec.chars() {
                                    format_state = match (format_state, c) {
                                        (FormatState::Start, c) => FormatState::Fill(c),
                                        (FormatState::Fill(_), '>' | '<' | '^') => {
                                            FormatState::Align
                                        }
                                        (
                                            FormatState::Fill('>' | '<' | '^' | '0'..='9')
                                            | FormatState::Align
                                            | FormatState::Width,
                                            '0'..='9',
                                        ) => FormatState::Width,
                                        (
                                            FormatState::Fill('>' | '<' | '^' | '0'..='9')
                                            | FormatState::Align
                                            | FormatState::Width,
                                            '.',
                                        ) => FormatState::Precision,
                                        (
                                            FormatState::Fill('.') | FormatState::Precision,
                                            '0'..='9',
                                        ) => FormatState::Precision,
                                        (
                                            FormatState::Fill('>' | '<' | '^' | '0'..='9')
                                            | FormatState::Align
                                            | FormatState::Width
                                            | FormatState::Precision,
                                            's',
                                        ) => FormatState::Type,
                                        (FormatState::Skip, _) => break,
                                        (_, _) => FormatState::Skip,
                                    };
                                }

                                state = match format_state {
                                    FormatState::Skip => State::None,
                                    FormatState::Start => State::SeenMarkAndVar {
                                        violation: QuotedFStringValue {
                                            mark,
                                            suggestion: Suggestion::FormatSpec,
                                        },
                                        range: fstr_expr.range,
                                    },
                                    FormatState::Type | FormatState::Fill('s') => {
                                        State::SeenMarkAndVar {
                                            violation: QuotedFStringValue {
                                                mark,
                                                suggestion: Suggestion::SFlag,
                                            },
                                            range: fstr_expr.range,
                                        }
                                    }
                                    FormatState::Fill('>' | '<' | '^' | '0'..='9' | '.') => {
                                        State::SeenMarkAndVar {
                                            violation: QuotedFStringValue {
                                                mark,
                                                suggestion: Suggestion::TypeHint,
                                            },
                                            range: fstr_expr.range,
                                        }
                                    }
                                    FormatState::Fill(_) => State::None,
                                    _ => State::SeenMarkAndVar {
                                        violation: QuotedFStringValue {
                                            mark,
                                            suggestion: Suggestion::TypeHint,
                                        },
                                        range: fstr_expr.range,
                                    },
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
