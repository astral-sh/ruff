use ruff_formatter::{FormatOwnedWithRule, FormatRefWithRule};
use ruff_python_ast::Stmt;

use crate::prelude::*;

pub(super) mod clause;
pub(crate) mod stmt_ann_assign;
pub(crate) mod stmt_assert;
pub(crate) mod stmt_assign;
pub(crate) mod stmt_aug_assign;
pub(crate) mod stmt_break;
pub(crate) mod stmt_class_def;
pub(crate) mod stmt_continue;
pub(crate) mod stmt_delete;
pub(crate) mod stmt_expr;
pub(crate) mod stmt_for;
pub(crate) mod stmt_function_def;
pub(crate) mod stmt_global;
pub(crate) mod stmt_if;
pub(crate) mod stmt_import;
pub(crate) mod stmt_import_from;
pub(crate) mod stmt_ipy_escape_command;
pub(crate) mod stmt_match;
pub(crate) mod stmt_nonlocal;
pub(crate) mod stmt_pass;
pub(crate) mod stmt_raise;
pub(crate) mod stmt_return;
pub(crate) mod stmt_try;
pub(crate) mod stmt_type_alias;
pub(crate) mod stmt_while;
pub(crate) mod stmt_with;
pub(crate) mod suite;

#[derive(Default)]
pub struct FormatStmt;

impl FormatRule<Stmt, PyFormatContext<'_>> for FormatStmt {
    fn fmt(&self, item: &Stmt, f: &mut PyFormatter) -> FormatResult<()> {
        match item {
            Stmt::FunctionDef(x) => x.format().fmt(f),
            Stmt::ClassDef(x) => x.format().fmt(f),
            Stmt::Return(x) => x.format().fmt(f),
            Stmt::Delete(x) => x.format().fmt(f),
            Stmt::Assign(x) => x.format().fmt(f),
            Stmt::AugAssign(x) => x.format().fmt(f),
            Stmt::AnnAssign(x) => x.format().fmt(f),
            Stmt::For(x) => x.format().fmt(f),
            Stmt::While(x) => x.format().fmt(f),
            Stmt::If(x) => x.format().fmt(f),
            Stmt::With(x) => x.format().fmt(f),
            Stmt::Match(x) => x.format().fmt(f),
            Stmt::Raise(x) => x.format().fmt(f),
            Stmt::Try(x) => x.format().fmt(f),
            Stmt::Assert(x) => x.format().fmt(f),
            Stmt::Import(x) => x.format().fmt(f),
            Stmt::ImportFrom(x) => x.format().fmt(f),
            Stmt::Global(x) => x.format().fmt(f),
            Stmt::Nonlocal(x) => x.format().fmt(f),
            Stmt::Expr(x) => x.format().fmt(f),
            Stmt::Pass(x) => x.format().fmt(f),
            Stmt::Break(x) => x.format().fmt(f),
            Stmt::Continue(x) => x.format().fmt(f),
            Stmt::TypeAlias(x) => x.format().fmt(f),
            Stmt::IpyEscapeCommand(x) => x.format().fmt(f),
        }
    }
}

impl<'ast> AsFormat<PyFormatContext<'ast>> for Stmt {
    type Format<'a> = FormatRefWithRule<'a, Stmt, FormatStmt, PyFormatContext<'ast>>;

    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(self, FormatStmt)
    }
}

impl<'ast> IntoFormat<PyFormatContext<'ast>> for Stmt {
    type Format = FormatOwnedWithRule<Stmt, FormatStmt, PyFormatContext<'ast>>;

    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, FormatStmt)
    }
}
