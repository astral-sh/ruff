use ruff_diagnostics::{Applicability, Edit, Fix};
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{
    self as ast, Expr, Stmt,
    visitor::{self, Visitor},
};
use ruff_python_codegen::Generator;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::fix::snippet::SourceCodeSnippet;
use crate::importer::ImportRequest;
use crate::rules::refurb::helpers::{FileOpen, find_file_opens};
use crate::{FixAvailability, Violation};

/// ## What it does
/// Checks for uses of `open` and `read` that can be replaced by `pathlib`
/// methods, like `Path.read_text` and `Path.read_bytes`.
///
/// ## Why is this bad?
/// When reading the entire contents of a file into a variable, it's simpler
/// and more concise to use `pathlib` methods like `Path.read_text` and
/// `Path.read_bytes` instead of `open` and `read` calls via `with` statements.
///
/// ## Example
/// ```python
/// with open(filename) as f:
///     contents = f.read()
/// ```
///
/// Use instead:
/// ```python
/// from pathlib import Path
///
/// contents = Path(filename).read_text()
/// ```
/// ## Fix Safety
/// This rule's fix is marked as unsafe if the replacement would remove comments attached to the original expression.
///
/// ## References
/// - [Python documentation: `Path.read_bytes`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.read_bytes)
/// - [Python documentation: `Path.read_text`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.read_text)
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "v0.1.2")]
pub(crate) struct ReadWholeFile {
    filename: SourceCodeSnippet,
    suggestion: SourceCodeSnippet,
}

impl Violation for ReadWholeFile {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let filename = self.filename.truncated_display();
        let suggestion = self.suggestion.truncated_display();
        format!("`open` and `read` should be replaced by `Path({filename}).{suggestion}`")
    }

    fn fix_title(&self) -> Option<String> {
        Some(format!(
            "Replace with `Path({}).{}`",
            self.filename.truncated_display(),
            self.suggestion.truncated_display(),
        ))
    }
}

/// FURB101
pub(crate) fn read_whole_file(checker: &Checker, with: &ast::StmtWith) {
    // `async` check here is more of a precaution.
    if with.is_async {
        return;
    }

    // First we go through all the items in the statement and find all `open` operations.
    let candidates = find_file_opens(with, checker.semantic(), true, checker.target_version());
    if candidates.is_empty() {
        return;
    }

    // Then we need to match each `open` operation with exactly one `read` call.
    let mut matcher = ReadMatcher::new(checker, candidates, with);
    visitor::walk_body(&mut matcher, &with.body);
}

/// AST visitor that matches `open` operations with the corresponding `read` calls.
struct ReadMatcher<'a, 'b> {
    checker: &'a Checker<'b>,
    candidates: Vec<FileOpen<'a>>,
    with_stmt: &'a ast::StmtWith,
}

impl<'a, 'b> ReadMatcher<'a, 'b> {
    fn new(
        checker: &'a Checker<'b>,
        candidates: Vec<FileOpen<'a>>,
        with_stmt: &'a ast::StmtWith,
    ) -> Self {
        Self {
            checker,
            candidates,
            with_stmt,
        }
    }
}

impl<'a> Visitor<'a> for ReadMatcher<'a, '_> {
    fn visit_expr(&mut self, expr: &'a Expr) {
        if let Some(read_from) = match_read_call(expr) {
            if let Some(open) = self
                .candidates
                .iter()
                .position(|open| open.is_ref(read_from))
            {
                let open = self.candidates.remove(open);
                let suggestion = make_suggestion(&open, self.checker.generator());
                let mut diagnostic = self.checker.report_diagnostic(
                    ReadWholeFile {
                        filename: SourceCodeSnippet::from_str(
                            &self.checker.generator().expr(open.filename),
                        ),
                        suggestion: SourceCodeSnippet::from_str(&suggestion),
                    },
                    open.item.range(),
                );

                if let Some(fix) =
                    generate_fix(self.checker, &open, expr, self.with_stmt, &suggestion)
                {
                    diagnostic.set_fix(fix);
                }
            }
            return;
        }
        visitor::walk_expr(self, expr);
    }
}

/// Match `x.read()` expression and return expression `x` on success.
fn match_read_call(expr: &Expr) -> Option<&Expr> {
    let call = expr.as_call_expr()?;
    let attr = call.func.as_attribute_expr()?;
    let method_name = &attr.attr;

    if method_name != "read"
        || !attr.value.is_name_expr()
        || !call.arguments.args.is_empty()
        || !call.arguments.keywords.is_empty()
    {
        return None;
    }

    Some(&*attr.value)
}

fn make_suggestion(open: &FileOpen<'_>, generator: Generator) -> String {
    let name = ast::ExprName {
        id: open.mode.pathlib_method(),
        ctx: ast::ExprContext::Load,
        range: TextRange::default(),
        node_index: ruff_python_ast::AtomicNodeIndex::NONE,
    };
    let call = ast::ExprCall {
        func: Box::new(name.into()),
        arguments: ast::Arguments {
            args: Box::from([]),
            keywords: open.keywords.iter().copied().cloned().collect(),
            range: TextRange::default(),
            node_index: ruff_python_ast::AtomicNodeIndex::NONE,
        },
        range: TextRange::default(),
        node_index: ruff_python_ast::AtomicNodeIndex::NONE,
    };
    generator.expr(&call.into())
}

fn generate_fix(
    checker: &Checker,
    open: &FileOpen,
    expr: &Expr,
    with_stmt: &ast::StmtWith,
    suggestion: &str,
) -> Option<Fix> {
    if with_stmt.items.len() != 1 {
        return None;
    }

    let locator = checker.locator();

    let filename_code = locator.slice(open.filename.range());

    let (import_edit, binding) = checker
        .importer()
        .get_or_import_symbol(
            &ImportRequest::import("pathlib", "Path"),
            with_stmt.start(),
            checker.semantic(),
        )
        .ok()?;

    // Only replace context managers with a single assignment or annotated assignment in the body.
    // The assignment's RHS must also be the same as the `read` call in `expr`, otherwise this fix
    // would remove the rest of the expression.
    let replacement = match with_stmt.body.as_slice() {
        [Stmt::Assign(ast::StmtAssign { targets, value, .. })] if value.range() == expr.range() => {
            match targets.as_slice() {
                [Expr::Name(name)] => {
                    format!(
                        "{name} = {binding}({filename_code}).{suggestion}",
                        name = name.id
                    )
                }
                _ => return None,
            }
        }
        [
            Stmt::AnnAssign(ast::StmtAnnAssign {
                target,
                annotation,
                value: Some(value),
                ..
            }),
        ] if value.range() == expr.range() => match target.as_ref() {
            Expr::Name(name) => {
                format!(
                    "{var}: {ann} = {binding}({filename_code}).{suggestion}",
                    var = name.id,
                    ann = locator.slice(annotation.range())
                )
            }
            _ => return None,
        },
        _ => return None,
    };

    let applicability = if checker.comment_ranges().intersects(with_stmt.range()) {
        Applicability::Unsafe
    } else {
        Applicability::Safe
    };

    Some(Fix::applicable_edits(
        Edit::range_replacement(replacement, with_stmt.range()),
        [import_edit],
        applicability,
    ))
}
