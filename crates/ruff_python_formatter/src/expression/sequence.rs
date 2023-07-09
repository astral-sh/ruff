use ruff_formatter::{prelude::Formatter, Format, FormatResult};
use rustpython_parser::ast::Expr;

use crate::{builders::PyFormatterExtensions, context::PyFormatContext};

#[derive(Debug)]
pub(crate) struct ExprSequence<'a> {
    elts: &'a [Expr],
}

impl<'a> ExprSequence<'a> {
    pub(crate) const fn new(elts: &'a [Expr]) -> Self {
        Self { elts }
    }
}

impl Format<PyFormatContext<'_>> for ExprSequence<'_> {
    fn fmt(&self, f: &mut Formatter<PyFormatContext<'_>>) -> FormatResult<()> {
        f.join_comma_separated().nodes(self.elts.iter()).finish()
    }
}
