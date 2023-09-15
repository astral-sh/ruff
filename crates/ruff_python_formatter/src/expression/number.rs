use std::borrow::Cow;

use ruff_python_ast::ExprConstant;
use ruff_text_size::{Ranged, TextSize};

use crate::prelude::*;

pub(super) struct FormatInt<'a> {
    constant: &'a ExprConstant,
}

impl<'a> FormatInt<'a> {
    pub(super) fn new(constant: &'a ExprConstant) -> Self {
        debug_assert!(constant.value.is_int());
        Self { constant }
    }
}

impl Format<PyFormatContext<'_>> for FormatInt<'_> {
    fn fmt(&self, f: &mut PyFormatter) -> FormatResult<()> {
        let range = self.constant.range();
        let content = f.context().locator().slice(range);

        let normalized = normalize_integer(content);

        match normalized {
            Cow::Borrowed(_) => source_text_slice(range).fmt(f),
            Cow::Owned(normalized) => text(&normalized, Some(range.start())).fmt(f),
        }
    }
}

pub(super) struct FormatFloat<'a> {
    constant: &'a ExprConstant,
}

impl<'a> FormatFloat<'a> {
    pub(super) fn new(constant: &'a ExprConstant) -> Self {
        debug_assert!(constant.value.is_float());
        Self { constant }
    }
}

impl Format<PyFormatContext<'_>> for FormatFloat<'_> {
    fn fmt(&self, f: &mut PyFormatter) -> FormatResult<()> {
        let range = self.constant.range();
        let content = f.context().locator().slice(range);

        let normalized = normalize_floating_number(content);

        match normalized {
            Cow::Borrowed(_) => source_text_slice(range).fmt(f),
            Cow::Owned(normalized) => text(&normalized, Some(range.start())).fmt(f),
        }
    }
}

pub(super) struct FormatComplex<'a> {
    constant: &'a ExprConstant,
}

impl<'a> FormatComplex<'a> {
    pub(super) fn new(constant: &'a ExprConstant) -> Self {
        debug_assert!(constant.value.is_complex());
        Self { constant }
    }
}

impl Format<PyFormatContext<'_>> for FormatComplex<'_> {
    fn fmt(&self, f: &mut PyFormatter) -> FormatResult<()> {
        let range = self.constant.range();
        let content = f.context().locator().slice(range);

        let normalized = normalize_floating_number(content.trim_end_matches(['j', 'J']));

        match normalized {
            Cow::Borrowed(_) => {
                source_text_slice(range.sub_end(TextSize::from(1))).fmt(f)?;
            }
            Cow::Owned(normalized) => {
                text(&normalized, Some(range.start())).fmt(f)?;
            }
        }

        token("j").fmt(f)
    }
}

/// Returns the normalized integer string.
fn normalize_integer(input: &str) -> Cow<str> {
    // The normalized string if `input` is not yet normalized.
    // `output` must remain empty if `input` is already normalized.
    let mut output = String::new();
    // Tracks the last index of `input` that has been written to `output`.
    // If `last_index` is `0` at the end, then the input is already normalized and can be returned as is.
    let mut last_index = 0;

    let mut is_hex = false;

    let mut chars = input.char_indices();

    if let Some((_, '0')) = chars.next() {
        if let Some((index, c)) = chars.next() {
            is_hex = matches!(c, 'x' | 'X');
            if matches!(c, 'B' | 'O' | 'X') {
                // Lowercase the prefix.
                output.push('0');
                output.push(c.to_ascii_lowercase());
                last_index = index + c.len_utf8();
            }
        }
    }

    // Skip the rest if `input` is not a hexinteger because there are only digits.
    if is_hex {
        for (index, c) in chars {
            if matches!(c, 'a'..='f') {
                // Uppercase hexdigits.
                output.push_str(&input[last_index..index]);
                output.push(c.to_ascii_uppercase());
                last_index = index + c.len_utf8();
            }
        }
    }

    if last_index == 0 {
        Cow::Borrowed(input)
    } else {
        output.push_str(&input[last_index..]);
        Cow::Owned(output)
    }
}

/// Returns the normalized floating number string.
fn normalize_floating_number(input: &str) -> Cow<str> {
    // The normalized string if `input` is not yet normalized.
    // `output` must remain empty if `input` is already normalized.
    let mut output = String::new();
    // Tracks the last index of `input` that has been written to `output`.
    // If `last_index` is `0` at the end, then the input is already normalized and can be returned as is.
    let mut last_index = 0;

    let mut chars = input.char_indices();

    let fraction_ends_with_dot = if let Some((index, '.')) = chars.next() {
        // Add a leading `0` if `input` starts with `.`.
        output.push('0');
        output.push('.');
        last_index = index + '.'.len_utf8();
        true
    } else {
        false
    };

    loop {
        match chars.next() {
            Some((index, c @ ('e' | 'E'))) => {
                if fraction_ends_with_dot {
                    // Add `0` if fraction part ends with `.`.
                    output.push_str(&input[last_index..index]);
                    output.push('0');
                    last_index = index;
                }

                if c == 'E' {
                    // Lowercase exponent part.
                    output.push_str(&input[last_index..index]);
                    output.push('e');
                    last_index = index + 'E'.len_utf8();
                }

                if let Some((index, '+')) = chars.next() {
                    // Remove `+` in exponent part.
                    output.push_str(&input[last_index..index]);
                    last_index = index + '+'.len_utf8();
                }

                break;
            }
            Some(_) => continue,
            None => {
                if input.ends_with('.') {
                    // Add `0` if fraction part ends with `.`.
                    output.push_str(&input[last_index..]);
                    output.push('0');
                    last_index = input.len();
                }

                break;
            }
        }
    }

    if last_index == 0 {
        Cow::Borrowed(input)
    } else {
        output.push_str(&input[last_index..]);
        Cow::Owned(output)
    }
}
