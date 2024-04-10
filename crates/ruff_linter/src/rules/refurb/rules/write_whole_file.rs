use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{self as ast, Expr, Stmt};
use ruff_python_codegen::Generator;
use ruff_python_semantic::{BindingId, ResolvedReference, SemanticModel};
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::fix::snippet::SourceCodeSnippet;
use crate::rules::refurb::helpers;

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
/// ## References
/// - [Python documentation: `Path.write_bytes`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.write_bytes)
/// - [Python documentation: `Path.write_text`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.write_text)
#[violation]
pub struct WriteWholeFile {
    filename: SourceCodeSnippet,
    suggestion: SourceCodeSnippet,
}

impl Violation for WriteWholeFile {
    #[derive_message_formats]
    fn message(&self) -> String {
        let filename = self.filename.truncated_display();
        let suggestion = self.suggestion.truncated_display();
        format!("`open` and `write` should be replaced by `Path({filename}).{suggestion}`")
    }
}

/// FURB103
pub(crate) fn write_whole_file(checker: &mut Checker, with: &ast::StmtWith) {
    // `async` check here is more of a precaution.
    if with.is_async || !checker.semantic().is_builtin("open") {
        return;
    }

    // First we go through all the items in the statement and find all `open` operations.
    let candidates = find_file_opens(with, checker.semantic());
    if candidates.is_empty() {
        return;
    }

    // Then we need to match each `open` operation with exactly one `write` call.
    let mut matcher = WriteMatcher::new(candidates);
    visitor::walk_body(&mut matcher, &with.body);
    let contents = matcher.contents();

    // All the matched operations should be reported.
    let diagnostics: Vec<Diagnostic> = matcher
        .into_matches()
        .iter()
        .zip(contents)
        .map(|(open, content)| {
            Diagnostic::new(
                WriteWholeFile {
                    filename: SourceCodeSnippet::from_str(&checker.generator().expr(open.filename)),
                    suggestion: make_suggestion(open, content, checker.generator()),
                },
                open.item.range(),
            )
        })
        .collect();
    checker.diagnostics.extend(diagnostics);
}

/// Find and return all `open` operations in the given `with` statement.
fn find_file_opens<'a>(
    with: &'a ast::StmtWith,
    semantic: &'a SemanticModel<'a>,
) -> Vec<helpers::FileOpen<'a>> {
    with.items
        .iter()
        .filter_map(|item| find_file_open(item, with, semantic))
        .collect()
}

/// Find `open` operation in the given `with` item.
fn find_file_open<'a>(
    item: &'a ast::WithItem,
    with: &'a ast::StmtWith,
    semantic: &'a SemanticModel<'a>,
) -> Option<helpers::FileOpen<'a>> {
    // We want to match `open(...) as var`.
    let ast::ExprCall {
        func,
        arguments: ast::Arguments { args, keywords, .. },
        ..
    } = item.context_expr.as_call_expr()?;

    if func.as_name_expr()?.id != "open" {
        return None;
    }

    let var = item.optional_vars.as_deref()?.as_name_expr()?;

    // Ignore calls with `*args` and `**kwargs`. In the exact case of `open(*filename, mode="w")`,
    // it could be a match; but in all other cases, the call _could_ contain unsupported keyword
    // arguments, like `buffering`.
    if args.iter().any(Expr::is_starred_expr)
        || keywords.iter().any(|keyword| keyword.arg.is_none())
    {
        return None;
    }

    // Match positional arguments, get filename and mode.
    let (filename, pos_mode) = helpers::match_open_args(args)?;

    // Match keyword arguments, get keyword arguments to forward and possibly mode.
    let (keywords, kw_mode) = match_open_keywords(keywords)?;

    let mode = kw_mode.unwrap_or(pos_mode);

    if !matches!(
        mode,
        helpers::OpenMode::WriteText | helpers::OpenMode::WriteBytes,
    ) {
        return None;
    }

    // Path.write_bytes does not support any kwargs.
    if matches!(mode, helpers::OpenMode::WriteBytes) && !keywords.is_empty() {
        return None;
    }

    // Now we need to find what is this variable bound to...
    let scope = semantic.current_scope();
    let bindings: Vec<BindingId> = scope.get_all(var.id.as_str()).collect();

    let binding = bindings
        .iter()
        .map(|x| semantic.binding(*x))
        // We might have many bindings with the same name, but we only care
        // for the one we are looking at right now.
        .find(|binding| binding.range() == var.range())?;

    // Since many references can share the same binding, we can limit our attention span
    // exclusively to the body of the current `with` statement.
    let references: Vec<&ResolvedReference> = binding
        .references
        .iter()
        .map(|id| semantic.reference(*id))
        .filter(|reference| with.range().contains_range(reference.range()))
        .collect();

    // And even with all these restrictions, if the file handle gets used not exactly once,
    // it doesn't fit the bill.
    let [reference] = references.as_slice() else {
        return None;
    };

    Some(helpers::FileOpen {
        item,
        filename,
        mode,
        keywords,
        reference,
    })
}

/// Match keyword arguments. Return keyword arguments to forward and mode.
fn match_open_keywords(
    keywords: &[ast::Keyword],
) -> Option<(Vec<&ast::Keyword>, Option<helpers::OpenMode>)> {
    let mut result: Vec<&ast::Keyword> = vec![];
    let mut mode: Option<helpers::OpenMode> = None;

    for keyword in keywords {
        match keyword.arg.as_ref()?.as_str() {
            "encoding" | "errors" | "newline" => result.push(keyword),

            // This might look bizarre, - why do we re-wrap this optional?
            //
            // The answer is quite simple, in the result of the current function
            // mode being `None` is a possible and correct option meaning that there
            // was NO "mode" keyword argument.
            //
            // The result of `match_open_mode` on the other hand is None
            // in the cases when the mode is not compatible with `write_text`/`write_bytes`.
            //
            // So, here we return None from this whole function if the mode
            // is incompatible.
            "mode" => mode = Some(helpers::match_open_mode(&keyword.value)?),

            // All other keywords cannot be directly forwarded.
            _ => return None,
        };
    }
    Some((result, mode))
}

/// AST visitor that matches `open` operations with the corresponding `write` calls.
#[derive(Debug)]
struct WriteMatcher<'a> {
    candidates: Vec<helpers::FileOpen<'a>>,
    matches: Vec<helpers::FileOpen<'a>>,
    contents: Vec<Vec<Expr>>,
    loop_counter: u32,
}

impl<'a> WriteMatcher<'a> {
    fn new(candidates: Vec<helpers::FileOpen<'a>>) -> Self {
        Self {
            candidates,
            matches: vec![],
            contents: vec![],
            loop_counter: 0,
        }
    }

    fn into_matches(self) -> Vec<helpers::FileOpen<'a>> {
        self.matches
    }

    fn contents(&self) -> Vec<Vec<Expr>> {
        self.contents.clone()
    }
}

impl<'a> Visitor<'a> for WriteMatcher<'a> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        if matches!(stmt, ast::Stmt::While(_) | ast::Stmt::For(_)) {
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
                if self.loop_counter == 0 {
                    self.matches.push(self.candidates.remove(open));
                    self.contents.push(content);
                } else {
                    self.candidates.remove(open);
                }
            }
            return;
        }
        visitor::walk_expr(self, expr);
    }
}

/// Match `x.write(foo)` expression and return expression `x` and `foo` on success.
fn match_write_call(expr: &Expr) -> Option<(&Expr, Vec<Expr>)> {
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

    Some((attr.value.as_ref(), call.arguments.args.as_ref().to_vec()))
}

/// Construct the replacement suggestion call.
fn make_suggestion(
    open: &helpers::FileOpen<'_>,
    write_arguments: Vec<Expr>,
    generator: Generator,
) -> SourceCodeSnippet {
    let name = ast::ExprName {
        id: open.mode.pathlib_method(),
        ctx: ast::ExprContext::Load,
        range: TextRange::default(),
    };
    let call = ast::ExprCall {
        func: Box::new(name.into()),
        arguments: ast::Arguments {
            args: Box::from(write_arguments),
            keywords: open.keywords.iter().copied().cloned().collect(),
            range: TextRange::default(),
        },
        range: TextRange::default(),
    };
    SourceCodeSnippet::from_str(&generator.expr(&call.into()))
}
