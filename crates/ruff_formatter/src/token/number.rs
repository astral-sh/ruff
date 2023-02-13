use crate::token::string::ToAsciiLowercaseCow;
use ruff_rowan::{Language, SyntaxToken};
use std::borrow::Cow;
use std::num::NonZeroUsize;

use crate::prelude::*;
use crate::{CstFormatContext, Format};

pub fn format_number_token<L>(token: &SyntaxToken<L>) -> CleanedNumberLiteralText<L>
where
    L: Language,
{
    CleanedNumberLiteralText { token }
}

pub struct CleanedNumberLiteralText<'token, L>
where
    L: Language,
{
    token: &'token SyntaxToken<L>,
}

impl<L, C> Format<C> for CleanedNumberLiteralText<'_, L>
where
    L: Language + 'static,
    C: CstFormatContext<Language = L>,
{
    fn fmt(&self, f: &mut Formatter<C>) -> FormatResult<()> {
        format_replaced(
            self.token,
            &syntax_token_cow_slice(
                format_trimmed_number(self.token.text_trimmed()),
                self.token,
                self.token.text_trimmed_range().start(),
            ),
        )
        .fmt(f)
    }
}

enum FormatNumberLiteralState {
    IntegerPart,
    DecimalPart(FormatNumberLiteralDecimalPart),
    Exponent(FormatNumberLiteralExponent),
}

struct FormatNumberLiteralDecimalPart {
    dot_index: usize,
    last_non_zero_index: Option<NonZeroUsize>,
}
struct FormatNumberLiteralExponent {
    e_index: usize,
    is_negative: bool,
    first_digit_index: Option<NonZeroUsize>,
    first_non_zero_index: Option<NonZeroUsize>,
}
// Regex-free version of https://github.com/prettier/prettier/blob/ca246afacee8e6d5db508dae01730c9523bbff1d/src/common/util.js#L341-L356
fn format_trimmed_number(text: &str) -> Cow<str> {
    use FormatNumberLiteralState::*;

    let text = text.to_ascii_lowercase_cow();
    let mut copied_or_ignored_chars = 0usize;
    let mut iter = text.chars().enumerate();
    let mut curr = iter.next();
    let mut state = IntegerPart;

    // Will be filled only if and when the first place that needs reformatting is detected.
    let mut cleaned_text = String::new();

    // Look at only the start of the text, ignore any sign, and make sure numbers always start with a digit. Add 0 if missing.
    if let Some((_, '+' | '-')) = curr {
        curr = iter.next();
    }
    if let Some((curr_index, '.')) = curr {
        cleaned_text.push_str(&text[copied_or_ignored_chars..curr_index]);
        copied_or_ignored_chars = curr_index;
        cleaned_text.push('0');
    }

    // Loop over the rest of the text, applying the remaining rules.
    loop {
        // We use a None pseudo-char at the end of the string to simplify the match cases that follow
        let curr_or_none_terminator_char = match curr {
            Some((curr_index, curr_char)) => (curr_index, Some(curr_char)),
            None => (text.len(), None),
        };
        // Look for termination of the decimal part or exponent and see if we need to print it differently.
        match (&state, curr_or_none_terminator_char) {
            (
                DecimalPart(FormatNumberLiteralDecimalPart {
                    dot_index,
                    last_non_zero_index: None,
                }),
                (curr_index, Some('e') | None),
            ) => {
                // The decimal part equals zero, ignore it completely.
                // Caveat: Prettier still prints a single `.0` unless there was *only* a trailing dot.
                if curr_index > dot_index + 1 {
                    cleaned_text.push_str(&text[copied_or_ignored_chars..=*dot_index]);
                    cleaned_text.push('0');
                } else {
                    cleaned_text.push_str(&text[copied_or_ignored_chars..*dot_index]);
                }
                copied_or_ignored_chars = curr_index;
            }
            (
                DecimalPart(FormatNumberLiteralDecimalPart {
                    last_non_zero_index: Some(last_non_zero_index),
                    ..
                }),
                (curr_index, Some('e') | None),
            ) if last_non_zero_index.get() < curr_index - 1 => {
                // The decimal part ends with at least one zero, ignore them but copy the part from the dot until the last non-zero.
                cleaned_text.push_str(&text[copied_or_ignored_chars..=last_non_zero_index.get()]);
                copied_or_ignored_chars = curr_index;
            }
            (
                Exponent(FormatNumberLiteralExponent {
                    e_index,
                    first_non_zero_index: None,
                    ..
                }),
                (curr_index, None),
            ) => {
                // The exponent equals zero, ignore it completely.
                cleaned_text.push_str(&text[copied_or_ignored_chars..*e_index]);
                copied_or_ignored_chars = curr_index;
            }
            (
                Exponent(FormatNumberLiteralExponent {
                    e_index,
                    is_negative,
                    first_digit_index: Some(first_digit_index),
                    first_non_zero_index: Some(first_non_zero_index),
                }),
                (curr_index, None),
            ) if (first_digit_index.get() > e_index + 1 && !is_negative)
                || (first_non_zero_index.get() > first_digit_index.get()) =>
            {
                // The exponent begins with a plus or at least one zero, ignore them but copy the part from the first non-zero until the end.
                cleaned_text.push_str(&text[copied_or_ignored_chars..=*e_index]);
                if *is_negative {
                    cleaned_text.push('-');
                }
                cleaned_text.push_str(&text[first_non_zero_index.get()..curr_index]);
                copied_or_ignored_chars = curr_index;
            }
            _ => {}
        }

        // Update state after the current char
        match (&state, curr) {
            // Cases entering or remaining in decimal part
            (_, Some((curr_index, '.'))) => {
                state = DecimalPart(FormatNumberLiteralDecimalPart {
                    dot_index: curr_index,
                    last_non_zero_index: None,
                });
            }
            (DecimalPart(decimal_part), Some((curr_index, '1'..='9'))) => {
                state = DecimalPart(FormatNumberLiteralDecimalPart {
                    last_non_zero_index: Some(unsafe {
                        // We've already entered InDecimalPart, so curr_index must be >0
                        NonZeroUsize::new_unchecked(curr_index)
                    }),
                    ..*decimal_part
                });
            }
            // Cases entering or remaining in exponent
            (_, Some((curr_index, 'e'))) => {
                state = Exponent(FormatNumberLiteralExponent {
                    e_index: curr_index,
                    is_negative: false,
                    first_digit_index: None,
                    first_non_zero_index: None,
                });
            }
            (Exponent(exponent), Some((_, '-'))) => {
                state = Exponent(FormatNumberLiteralExponent {
                    is_negative: true,
                    ..*exponent
                });
            }
            (
                Exponent(
                    exponent @ FormatNumberLiteralExponent {
                        first_digit_index: None,
                        ..
                    },
                ),
                Some((curr_index, curr_char @ '0'..='9')),
            ) => {
                state = Exponent(FormatNumberLiteralExponent {
                    first_digit_index: Some(unsafe {
                        // We've already entered InExponent, so curr_index must be >0
                        NonZeroUsize::new_unchecked(curr_index)
                    }),
                    first_non_zero_index: if curr_char != '0' {
                        Some(unsafe {
                            // We've already entered InExponent, so curr_index must be >0
                            NonZeroUsize::new_unchecked(curr_index)
                        })
                    } else {
                        None
                    },
                    ..*exponent
                });
            }
            (
                Exponent(
                    exponent @ FormatNumberLiteralExponent {
                        first_non_zero_index: None,
                        ..
                    },
                ),
                Some((curr_index, '1'..='9')),
            ) => {
                state = Exponent(FormatNumberLiteralExponent {
                    first_non_zero_index: Some(unsafe { NonZeroUsize::new_unchecked(curr_index) }),
                    ..*exponent
                });
            }
            _ => {}
        }

        // Repeat or exit
        match curr {
            None | Some((_, 'x') /* hex bailout */) => break,
            Some(_) => curr = iter.next(),
        }
    }

    if cleaned_text.is_empty() {
        text
    } else {
        // Append any unconsidered text
        cleaned_text.push_str(&text[copied_or_ignored_chars..]);
        Cow::Owned(cleaned_text)
    }
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use super::format_trimmed_number;

    #[test]
    fn removes_unnecessary_plus_and_zeros_from_scientific_notation() {
        assert_eq!("1e2", format_trimmed_number("1e02"));
        assert_eq!("1e2", format_trimmed_number("1e+2"));
    }

    #[test]
    fn removes_unnecessary_scientific_notation() {
        assert_eq!("1", format_trimmed_number("1e0"));
        assert_eq!("1", format_trimmed_number("1e-0"));
    }
    #[test]
    fn does_not_get_bamboozled_by_hex() {
        assert_eq!("0xe0", format_trimmed_number("0xe0"));
        assert_eq!("0x10e0", format_trimmed_number("0x10e0"));
    }

    #[test]
    fn makes_sure_numbers_always_start_with_a_digit() {
        assert_eq!("0.2", format_trimmed_number(".2"));
    }

    #[test]
    fn removes_extraneous_trailing_decimal_zeroes() {
        assert_eq!("0.1", format_trimmed_number("0.10"));
    }
    #[test]
    fn keeps_one_trailing_decimal_zero() {
        assert_eq!("0.0", format_trimmed_number("0.00"));
    }

    #[test]
    fn removes_trailing_dot() {
        assert_eq!("1", format_trimmed_number("1."));
    }

    #[test]
    fn cleans_all_at_once() {
        assert_eq!("0.0", format_trimmed_number(".00e-0"));
    }

    #[test]
    fn keeps_the_input_string_if_no_change_needed() {
        assert!(matches!(
            format_trimmed_number("0.1e2"),
            Cow::Borrowed("0.1e2")
        ));
    }
}
