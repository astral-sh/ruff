use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{self as ast, Expr};
use ruff_python_codegen::Generator;
use ruff_python_semantic::{BindingId, ResolvedReference, SemanticModel};
use ruff_text_size::{Ranged, TextRange};

use crate::autofix::snippet::SourceCodeSnippet;
use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for uses of `open` and `read` that can be replaced by `pathlib`.
///
/// ## Why is this bad?
/// When you just want to read the contents of a whole file, using a `with` block
/// is a bit of an overkill. A simpler alternative is to use pathlib's `read_text`
/// and `read_bytes` functions:
///
/// ## Example
/// ```python
/// with open(filename) as f:
///     contents = f.read()
/// ```
///
/// Use instead:
/// ```python
/// contents = Path(filename).read_text()
/// ```
///
/// ## References
/// - [Python documentation: `Path.read_bytes()`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.read_bytes)
/// - [Python documentation: `Path.read_text()`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.read_text)
#[violation]
pub struct ReadWholeFile {
    filename: SourceCodeSnippet,
    suggestion: SourceCodeSnippet,
}

impl Violation for ReadWholeFile {
    #[derive_message_formats]
    fn message(&self) -> String {
        let filename = self.filename.truncated_display();
        let suggestion = self.suggestion.truncated_display();

        format!("`open` and `read` should be replaced by `Path({filename}).{suggestion}`")
    }
}

/// FURB101
pub(crate) fn read_whole_file(checker: &mut Checker, with: &ast::StmtWith) {
    // async check here is more of a precaution.
    if with.is_async || !checker.semantic().is_builtin("open") {
        return;
    }

    // First we go through all the items in the statement and find all `open` operations.
    let opens = find_file_opens(checker.semantic(), with);
    if opens.is_empty() {
        return;
    }

    // Then we need to match each `open` operation with exactly one `read` call.
    let mut matcher = ReadMatcher { candidates: opens };
    visitor::walk_body(&mut matcher, &with.body);

    // All the matched operations should be reported.
    let diagnostics: Vec<Diagnostic> = matcher
        .candidates
        .iter()
        .filter(|open| open.matched)
        .map(|open| {
            Diagnostic::new(
                ReadWholeFile {
                    filename: SourceCodeSnippet::from_str(&checker.generator().expr(open.filename)),
                    suggestion: make_suggestion(open, checker.generator()),
                },
                open.item.range(),
            )
        })
        .collect();
    checker.diagnostics.extend(diagnostics);
}

#[derive(Debug)]
enum ReadMode {
    /// "r"  -> `read_text`
    Text,
    /// "rb" -> `read_bytes`
    Bytes,
}

/// A grab bag struct that joins together every piece of information we need to track
/// about a file open operation.
struct FileOpen<'a> {
    /// With item where the open happens, we use it for the reporting range.
    item: &'a ast::WithItem,
    /// Filename expression used as the first argument in `open`, we use it in the diagnostic message.
    filename: &'a Expr,
    /// The type of read to choose `read_text` or `read_bytes`.
    mode: ReadMode,
    /// Keywords that can be used in the new read call.
    keywords: Vec<&'a ast::Keyword>,
    /// We only check `open` operations which file handles are used exactly once.
    /// The easiest way to figure out if something IS that reference we knew all along
    /// is to compare text ranges.
    ref_range: TextRange,
    /// A flag signifying that there is a matching `read` in the body.
    matched: bool,
}

impl<'a> FileOpen<'a> {
    fn is_ref(&self, expr: &Expr) -> bool {
        expr.range() == self.ref_range
    }
}

/// Find and return all `open` operations in the given `with` statement.
fn find_file_opens<'a>(semantic: &SemanticModel, with: &'a ast::StmtWith) -> Vec<FileOpen<'a>> {
    with.items
        .iter()
        .filter_map(|item| find_file_open(semantic, item, with.range()))
        .collect()
}

/// Find `open` operation in the given `with` item.
fn find_file_open<'a>(
    semantic: &SemanticModel,
    item: &'a ast::WithItem,
    body_range: TextRange,
) -> Option<FileOpen<'a>> {
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

    // Match positional arguments, get filename and read mode.
    let (filename, pos_mode) = match_open_args(args)?;
    // Match keyword arguments, get keyword arguments to forward and possibly read mode.
    let (keywords, kw_mode) = match_open_keywords(keywords)?;

    // `pos_mode` could've been assigned default value corresponding to "r", while
    // keyword mode should override that.
    let mode = kw_mode.unwrap_or(pos_mode);

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
        .filter(|reference| body_range.contains_range(reference.range()))
        .collect();

    // And even with all these restrictions, if the file handle gets used not exactly once,
    // it doesn't fit the bill.
    let [reference] = references.as_slice() else {
        return None;
    };

    // Range seems to be the easiest way to understand that this is the right
    // reference when we'll be looking at it.
    let ref_range = reference.range();

    Some(FileOpen {
        item,
        filename,
        mode,
        keywords,
        ref_range,
        // matcher will set this later
        matched: false,
    })
}

/// Match positional arguments. Return expression for the file name and read mode.
fn match_open_args(args: &[Expr]) -> Option<(&Expr, ReadMode)> {
    match args {
        [filename] => Some((filename, ReadMode::Text)),
        [filename, mode_literal] => match_open_mode(mode_literal).map(|mode| (filename, mode)),
        // The third positional argument is buffering and `read_text` doesn't support it.
        _ => None,
    }
}

/// Match keyword arguments. Return keywrod arguments to forward and read mode.
fn match_open_keywords(
    keywords: &[ast::Keyword],
) -> Option<(Vec<&ast::Keyword>, Option<ReadMode>)> {
    let mut result: Vec<&ast::Keyword> = vec![];
    let mut mode: Option<ReadMode> = None;

    for keyword in keywords {
        match keyword.arg.as_ref()?.as_str() {
            "encoding" | "errors" => result.push(keyword),

            // This might look bizarre, - why do we re-wrap this optional?
            //
            // The answer is quite simple, in the result of the current function
            // mode being `None` is a possible and correct option meaning that there
            // was NO "mode" keyword argument.
            //
            // The result of `match_open_mode` on the other hand is None
            // in the cases when the mode is not compatible with `read_text`/`read_bytes`.
            //
            // So, here we return None from this whole function if the mode
            // is incompatible.
            "mode" => mode = Some(match_open_mode(&keyword.value)?),

            // All other keywords cannot be directly forwarded.
            _ => return None,
        };
    }
    Some((result, mode))
}

/// Match open mode to see if it is supported.
fn match_open_mode(mode: &Expr) -> Option<ReadMode> {
    let ast::StringConstant {
        value,
        implicit_concatenated: false,
        ..
    } = mode.as_constant_expr()?.value.as_str()?
    else {
        return None;
    };
    match value.as_str() {
        "r" => Some(ReadMode::Text),
        "rb" => Some(ReadMode::Bytes),
        _ => None,
    }
}

/// AST visitor that matches `open` operations with the corresponding `read` calls.
struct ReadMatcher<'a> {
    candidates: Vec<FileOpen<'a>>,
}

impl<'a> Visitor<'a> for ReadMatcher<'a> {
    fn visit_expr(&mut self, expr: &'a Expr) {
        if let Some(read_from) = match_read_call(expr) {
            if let Some(open) = self
                .candidates
                .iter_mut()
                .find(|open| open.is_ref(read_from))
            {
                open.matched = true;
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

    Some(attr.value.as_ref())
}

/// Construct the replacement suggestion call.
fn make_suggestion(open: &FileOpen<'_>, generator: Generator) -> SourceCodeSnippet {
    let method_name = match open.mode {
        ReadMode::Text => "read_text",
        ReadMode::Bytes => "read_bytes",
    };
    let name = ast::ExprName {
        id: method_name.to_string(),
        ctx: ast::ExprContext::Load,
        range: TextRange::default(),
    };
    let call = ast::ExprCall {
        func: Box::new(name.into()),
        arguments: ast::Arguments {
            args: vec![],
            keywords: open.keywords.iter().copied().cloned().collect(),
            range: TextRange::default(),
        },
        range: TextRange::default(),
    };
    SourceCodeSnippet::from_str(&generator.expr(&call.into()))
}
