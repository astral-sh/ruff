use ruff_formatter::FormatOwnedWithRule;
use ruff_python_ast::flat;
use ruff_python_ast::{Stmt, flat::AnyNodeRef};
use ruff_python_trivia::{SimpleToken, SimpleTokenKind, SimpleTokenizer};
use ruff_text_size::{Ranged, TextRange};

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

impl FormatRule<flat::View<'_, Stmt>, PyFormatContext<'_>> for FormatStmt {
    fn fmt(&self, item: &flat::View<'_, Stmt>, f: &mut PyFormatter) -> FormatResult<()> {
        let item = *item;
        match item.classify() {
            flat::StmtKind::FunctionDef(x) => x.format().fmt(f),
            flat::StmtKind::ClassDef(x) => x.format().fmt(f),
            flat::StmtKind::Return(x) => x.format().fmt(f),
            flat::StmtKind::Delete(x) => x.format().fmt(f),
            flat::StmtKind::Assign(x) => x.format().fmt(f),
            flat::StmtKind::AugAssign(x) => x.format().fmt(f),
            flat::StmtKind::AnnAssign(x) => x.format().fmt(f),
            flat::StmtKind::For(x) => x.format().fmt(f),
            flat::StmtKind::While(x) => x.format().fmt(f),
            flat::StmtKind::If(x) => x.format().fmt(f),
            flat::StmtKind::With(x) => x.format().fmt(f),
            flat::StmtKind::Match(x) => x.format().fmt(f),
            flat::StmtKind::Raise(x) => x.format().fmt(f),
            flat::StmtKind::Try(x) => x.format().fmt(f),
            flat::StmtKind::Assert(x) => x.format().fmt(f),
            flat::StmtKind::Import(x) => x.format().fmt(f),
            flat::StmtKind::ImportFrom(x) => x.format().fmt(f),
            flat::StmtKind::Global(x) => x.format().fmt(f),
            flat::StmtKind::Nonlocal(x) => x.format().fmt(f),
            flat::StmtKind::Expr(x) => x.format().fmt(f),
            flat::StmtKind::Pass(x) => x.format().fmt(f),
            flat::StmtKind::Break(x) => x.format().fmt(f),
            flat::StmtKind::Continue(x) => x.format().fmt(f),
            flat::StmtKind::TypeAlias(x) => x.format().fmt(f),
            flat::StmtKind::IpyEscapeCommand(x) => x.format().fmt(f),
        }
    }
}

impl<'ast> IntoFormat<PyFormatContext<'ast>> for flat::View<'_, Stmt> {
    type Format = FormatOwnedWithRule<Self, FormatStmt, PyFormatContext<'ast>>;

    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, FormatStmt)
    }
}

/// Returns the range of the semicolon terminating the statement or `None` if the statement
/// isn't terminated by a semicolon.
pub(super) fn trailing_semicolon(node: AnyNodeRef, source: &str) -> Option<TextRange> {
    debug_assert!(node.is_statement());

    let tokenizer = SimpleTokenizer::starts_at(node.end(), source);

    let next_token = tokenizer
        .take_while(|token| !token.kind().is_comment())
        .find(|token| !token.kind().is_trivia());

    if let Some(SimpleToken {
        kind: SimpleTokenKind::Semi,
        range,
    }) = next_token
    {
        Some(range)
    } else {
        None
    }
}
