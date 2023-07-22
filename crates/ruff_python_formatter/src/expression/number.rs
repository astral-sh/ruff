use ruff_formatter::write;
use rustpython_parser::ast::{ExprConstant, Ranged};

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
    fn fmt(&self, f: &mut Formatter<PyFormatContext<'_>>) -> FormatResult<()> {
        let range = self.constant.range();
        let content = f.context().locator().slice(range);

        if content.starts_with("0x") || content.starts_with("0X") {
            let hex = content.get(2..).unwrap().to_ascii_uppercase();
            write!(f, [text("0x"), dynamic_text(&hex, None)])
        } else {
            let lowercase_content = content.to_ascii_lowercase();
            dynamic_text(&lowercase_content, None).fmt(f)
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
    fn fmt(&self, f: &mut Formatter<PyFormatContext<'_>>) -> FormatResult<()> {
        let range = self.constant.range();
        let content = f.context().locator().slice(range);
        FormatFloatNumber::new(content).fmt(f)
    }
}

struct FormatFloatNumber<'a> {
    number: &'a str,
}

impl<'a> FormatFloatNumber<'a> {
    fn new(number: &'a str) -> Self {
        Self { number }
    }
}

impl Format<PyFormatContext<'_>> for FormatFloatNumber<'_> {
    fn fmt(&self, f: &mut Formatter<PyFormatContext<'_>>) -> FormatResult<()> {
        // split exponent
        let (fraction, exponent) = match self.number.split_once(['e', 'E']) {
            Some((frac, exp)) => (frac, Some(exp)),
            None => (self.number, None),
        };

        write!(
            f,
            [
                fraction.starts_with('.').then_some(text("0")),
                dynamic_text(fraction, None),
                fraction.ends_with('.').then_some(text("0")),
            ]
        )?;

        if let Some(exp) = exponent {
            write!(
                f,
                [text("e"), dynamic_text(exp.trim_start_matches('+'), None)]
            )?;
        }

        Ok(())
    }
}
