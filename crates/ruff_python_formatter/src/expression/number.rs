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
