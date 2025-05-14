use ruff_diagnostics::{Applicability, Edit, Fix};
use ruff_python_ast::name::Name;
use ruff_python_ast::{self as ast, Expr};
use ruff_python_codegen::Generator;
use ruff_python_semantic::{BindingId, ResolvedReference, SemanticModel};
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use ruff_python_ast::PythonVersion;

/// Format a code snippet to call `name.method()`.
pub(super) fn generate_method_call(name: Name, method: &str, generator: Generator) -> String {
    // Construct `name`.
    let var = ast::ExprName {
        id: name,
        ctx: ast::ExprContext::Load,
        range: TextRange::default(),
    };
    // Construct `name.method`.
    let attr = ast::ExprAttribute {
        value: Box::new(var.into()),
        attr: ast::Identifier::new(method.to_string(), TextRange::default()),
        ctx: ast::ExprContext::Load,
        range: TextRange::default(),
    };
    // Make it into a call `name.method()`
    let call = ast::ExprCall {
        func: Box::new(attr.into()),
        arguments: ast::Arguments {
            args: Box::from([]),
            keywords: Box::from([]),
            range: TextRange::default(),
        },
        range: TextRange::default(),
    };
    // And finally, turn it into a statement.
    let stmt = ast::StmtExpr {
        value: Box::new(call.into()),
        range: TextRange::default(),
    };
    generator.stmt(&stmt.into())
}

/// Returns a fix that replace `range` with
/// a generated `a is None`/`a is not None` check.
pub(super) fn replace_with_identity_check(
    left: &Expr,
    range: TextRange,
    negate: bool,
    checker: &Checker,
) -> Fix {
    let (semantic, generator) = (checker.semantic(), checker.generator());

    let op = if negate {
        ast::CmpOp::IsNot
    } else {
        ast::CmpOp::Is
    };

    let new_expr = Expr::Compare(ast::ExprCompare {
        left: left.clone().into(),
        ops: [op].into(),
        comparators: [ast::ExprNoneLiteral::default().into()].into(),
        range: TextRange::default(),
    });

    let new_content = generator.expr(&new_expr);
    let new_content = if semantic.current_expression_parent().is_some() {
        format!("({new_content})")
    } else {
        new_content
    };

    let applicability = if checker.comment_ranges().intersects(range) {
        Applicability::Unsafe
    } else {
        Applicability::Safe
    };

    let edit = Edit::range_replacement(new_content, range);

    Fix::applicable_edit(edit, applicability)
}

// Helpers for read-whole-file and write-whole-file
#[derive(Debug, Copy, Clone)]
pub(super) enum OpenMode {
    /// "r"
    ReadText,
    /// "rb"
    ReadBytes,
    /// "w"
    WriteText,
    /// "wb"
    WriteBytes,
}

impl OpenMode {
    pub(super) fn pathlib_method(self) -> Name {
        match self {
            OpenMode::ReadText => Name::new_static("read_text"),
            OpenMode::ReadBytes => Name::new_static("read_bytes"),
            OpenMode::WriteText => Name::new_static("write_text"),
            OpenMode::WriteBytes => Name::new_static("write_bytes"),
        }
    }
}

/// A grab bag struct that joins together every piece of information we need to track
/// about a file open operation.
#[derive(Debug)]
pub(super) struct FileOpen<'a> {
    /// With item where the open happens, we use it for the reporting range.
    pub(super) item: &'a ast::WithItem,
    /// Filename expression used as the first argument in `open`, we use it in the diagnostic message.
    pub(super) filename: &'a Expr,
    /// The file open mode.
    pub(super) mode: OpenMode,
    /// The file open keywords.
    pub(super) keywords: Vec<&'a ast::Keyword>,
    /// We only check `open` operations whose file handles are used exactly once.
    pub(super) reference: &'a ResolvedReference,
}

impl FileOpen<'_> {
    /// Determine whether an expression is a reference to the file handle, by comparing
    /// their ranges. If two expressions have the same range, they must be the same expression.
    pub(super) fn is_ref(&self, expr: &Expr) -> bool {
        expr.range() == self.reference.range()
    }
}

/// Find and return all `open` operations in the given `with` statement.
pub(super) fn find_file_opens<'a>(
    with: &'a ast::StmtWith,
    semantic: &'a SemanticModel<'a>,
    read_mode: bool,
    python_version: PythonVersion,
) -> Vec<FileOpen<'a>> {
    with.items
        .iter()
        .filter_map(|item| find_file_open(item, with, semantic, read_mode, python_version))
        .collect()
}

/// Find `open` operation in the given `with` item.
fn find_file_open<'a>(
    item: &'a ast::WithItem,
    with: &'a ast::StmtWith,
    semantic: &'a SemanticModel<'a>,
    read_mode: bool,
    python_version: PythonVersion,
) -> Option<FileOpen<'a>> {
    // We want to match `open(...) as var`.
    let ast::ExprCall {
        func,
        arguments: ast::Arguments { args, keywords, .. },
        ..
    } = item.context_expr.as_call_expr()?;

    let var = item.optional_vars.as_deref()?.as_name_expr()?;

    // Ignore calls with `*args` and `**kwargs`. In the exact case of `open(*filename, mode="w")`,
    // it could be a match; but in all other cases, the call _could_ contain unsupported keyword
    // arguments, like `buffering`.
    if args.iter().any(Expr::is_starred_expr)
        || keywords.iter().any(|keyword| keyword.arg.is_none())
    {
        return None;
    }

    if !semantic.match_builtin_expr(func, "open") {
        return None;
    }

    // Match positional arguments, get filename and mode.
    let (filename, pos_mode) = match_open_args(args)?;

    // Match keyword arguments, get keyword arguments to forward and possibly mode.
    let (keywords, kw_mode) = match_open_keywords(keywords, read_mode, python_version)?;

    let mode = kw_mode.unwrap_or(pos_mode);

    match mode {
        OpenMode::ReadText | OpenMode::ReadBytes => {
            if !read_mode {
                return None;
            }
        }
        OpenMode::WriteText | OpenMode::WriteBytes => {
            if read_mode {
                return None;
            }
        }
    }

    // Path.read_bytes and Path.write_bytes do not support any kwargs.
    if matches!(mode, OpenMode::ReadBytes | OpenMode::WriteBytes) && !keywords.is_empty() {
        return None;
    }

    // Now we need to find what is this variable bound to...
    let scope = semantic.current_scope();
    let bindings: Vec<BindingId> = scope.get_all(var.id.as_str()).collect();

    let binding = bindings
        .iter()
        .map(|id| semantic.binding(*id))
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

    Some(FileOpen {
        item,
        filename,
        mode,
        keywords,
        reference,
    })
}

/// Match positional arguments. Return expression for the file name and open mode.
fn match_open_args(args: &[Expr]) -> Option<(&Expr, OpenMode)> {
    match args {
        [filename] => Some((filename, OpenMode::ReadText)),
        [filename, mode_literal] => match_open_mode(mode_literal).map(|mode| (filename, mode)),
        // The third positional argument is `buffering` and the pathlib methods don't support it.
        _ => None,
    }
}

/// Match keyword arguments. Return keyword arguments to forward and mode.
fn match_open_keywords(
    keywords: &[ast::Keyword],
    read_mode: bool,
    target_version: PythonVersion,
) -> Option<(Vec<&ast::Keyword>, Option<OpenMode>)> {
    let mut result: Vec<&ast::Keyword> = vec![];
    let mut mode: Option<OpenMode> = None;

    for keyword in keywords {
        match keyword.arg.as_ref()?.as_str() {
            "encoding" | "errors" => result.push(keyword),
            "newline" => {
                if read_mode {
                    // newline is only valid for write_text
                    return None;
                } else if target_version < PythonVersion::PY310 {
                    // `pathlib` doesn't support `newline` until Python 3.10.
                    return None;
                }

                result.push(keyword);
            }

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
            "mode" => mode = Some(match_open_mode(&keyword.value)?),

            // All other keywords cannot be directly forwarded.
            _ => return None,
        }
    }
    Some((result, mode))
}

/// Match open mode to see if it is supported.
fn match_open_mode(mode: &Expr) -> Option<OpenMode> {
    let mode = mode.as_string_literal_expr()?.as_single_part_string()?;

    match &*mode.value {
        "r" => Some(OpenMode::ReadText),
        "rb" => Some(OpenMode::ReadBytes),
        "w" => Some(OpenMode::WriteText),
        "wb" => Some(OpenMode::WriteBytes),
        _ => None,
    }
}
