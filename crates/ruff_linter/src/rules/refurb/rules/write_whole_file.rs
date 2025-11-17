use ruff_diagnostics::{Applicability, Edit, Fix};
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{
    self as ast, Expr, Stmt,
    visitor::{self, Visitor},
};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::fix::snippet::SourceCodeSnippet;
use crate::importer::ImportRequest;
use crate::rules::refurb::helpers::{FileOpen, find_file_opens};
use crate::{FixAvailability, Locator, Violation};

/// ## What it does
/// Checks for uses of `open` and `write` that can be replaced by `pathlib`
/// methods, like `Path.write_text` and `Path.write_bytes`.
///
/// ## Why is this bad?
/// When writing a single string to a file, it's simpler and more concise
/// to use `pathlib` methods like `Path.write_text` and `Path.write_bytes`
/// instead of `open` and `write` calls via `with` statements.
///
/// ## Example
/// ```python
/// with open(filename, "w") as f:
///     f.write(contents)
/// ```
///
/// Use instead:
/// ```python
/// from pathlib import Path
///
/// Path(filename).write_text(contents)
/// ```
///
/// ## Fix Safety
/// This rule's fix is marked as unsafe if the replacement would remove comments attached to the original expression.
///
/// ## References
/// - [Python documentation: `Path.write_bytes`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.write_bytes)
/// - [Python documentation: `Path.write_text`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.write_text)
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "v0.3.6")]
pub(crate) struct WriteWholeFile {
    filename: SourceCodeSnippet,
    suggestion: SourceCodeSnippet,
}

impl Violation for WriteWholeFile {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let filename = self.filename.truncated_display();
        let suggestion = self.suggestion.truncated_display();
        format!("`open` and `write` should be replaced by `Path({filename}).{suggestion}`")
    }
    fn fix_title(&self) -> Option<String> {
        Some(format!(
            "Replace with `Path({}).{}`",
            self.filename.truncated_display(),
            self.suggestion.truncated_display(),
        ))
    }
}

/// FURB103
pub(crate) fn write_whole_file(checker: &Checker, with: &ast::StmtWith) {
    // `async` check here is more of a precaution.
    if with.is_async {
        return;
    }

    // First we go through all the items in the statement and find all `open` operations.
    let candidates = find_file_opens(with, checker.semantic(), false, checker.target_version());
    if candidates.is_empty() {
        return;
    }

    // Then we need to match each `open` operation with exactly one `write` call.
    let mut matcher = WriteMatcher::new(checker, candidates, with);
    visitor::walk_body(&mut matcher, &with.body);
}

/// AST visitor that matches `open` operations with the corresponding `write` calls.
struct WriteMatcher<'a, 'b> {
    checker: &'a Checker<'b>,
    candidates: Vec<FileOpen<'a>>,
    loop_counter: u32,
    with_stmt: &'a ast::StmtWith,
}

impl<'a, 'b> WriteMatcher<'a, 'b> {
    fn new(
        checker: &'a Checker<'b>,
        candidates: Vec<FileOpen<'a>>,
        with_stmt: &'a ast::StmtWith,
    ) -> Self {
        Self {
            checker,
            candidates,
            loop_counter: 0,
            with_stmt,
        }
    }
}

impl<'a> Visitor<'a> for WriteMatcher<'a, '_> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        if matches!(stmt, Stmt::While(_) | Stmt::For(_)) {
            self.loop_counter += 1;
            visitor::walk_stmt(self, stmt);
            self.loop_counter -= 1;
        } else {
            visitor::walk_stmt(self, stmt);
        }
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        if let Some((write_to, content)) = match_write_call(expr) {
            if let Some(open) = self
                .candidates
                .iter()
                .position(|open| open.is_ref(write_to))
            {
                let open = self.candidates.remove(open);

                if self.loop_counter == 0 {
                    let suggestion = make_suggestion(&open, content, self.checker.locator());

                    let mut diagnostic = self.checker.report_diagnostic(
                        WriteWholeFile {
                            filename: SourceCodeSnippet::from_str(
                                &self.checker.generator().expr(open.filename),
                            ),
                            suggestion: SourceCodeSnippet::from_str(&suggestion),
                        },
                        open.item.range(),
                    );

                    if let Some(fix) =
                        generate_fix(self.checker, &open, self.with_stmt, &suggestion)
                    {
                        diagnostic.set_fix(fix);
                    }
                }
            }
            return;
        }
        visitor::walk_expr(self, expr);
    }
}

/// Match `x.write(foo)` expression and return expression `x` and `foo` on success.
fn match_write_call(expr: &Expr) -> Option<(&Expr, &Expr)> {
    let call = expr.as_call_expr()?;
    let attr = call.func.as_attribute_expr()?;
    let method_name = &attr.attr;

    if method_name != "write"
        || !attr.value.is_name_expr()
        || call.arguments.args.len() != 1
        || !call.arguments.keywords.is_empty()
    {
        return None;
    }

    // `write` only takes in a single positional argument.
    Some((&*attr.value, call.arguments.args.first()?))
}

fn make_suggestion(open: &FileOpen<'_>, arg: &Expr, locator: &Locator) -> String {
    let method_name = open.mode.pathlib_method();
    let arg_code = locator.slice(arg.range());

    if open.keywords.is_empty() {
        format!("{method_name}({arg_code})")
    } else {
        format!(
            "{method_name}({arg_code}, {})",
            itertools::join(
                open.keywords.iter().map(|kw| locator.slice(kw.range())),
                ", "
            )
        )
    }
}

fn generate_fix(
    checker: &Checker,
    open: &FileOpen,
    with_stmt: &ast::StmtWith,
    suggestion: &str,
) -> Option<Fix> {
    if !(with_stmt.items.len() == 1 && matches!(with_stmt.body.as_slice(), [Stmt::Expr(_)])) {
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

    let replacement = format!("{binding}({filename_code}).{suggestion}");

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
