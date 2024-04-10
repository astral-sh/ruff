use ruff_python_ast::{self as ast, Expr};
use ruff_python_codegen::Generator;
use ruff_python_semantic::ResolvedReference;
use ruff_text_size::{Ranged, TextRange};

/// Format a code snippet to call `name.method()`.
pub(super) fn generate_method_call(name: &str, method: &str, generator: Generator) -> String {
    // Construct `name`.
    let var = ast::ExprName {
        id: name.to_string(),
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

/// Format a code snippet comparing `name` to `None` (e.g., `name is None`).
pub(super) fn generate_none_identity_comparison(
    name: &str,
    negate: bool,
    generator: Generator,
) -> String {
    // Construct `name`.
    let var = ast::ExprName {
        id: name.to_string(),
        ctx: ast::ExprContext::Load,
        range: TextRange::default(),
    };
    // Construct `name is None` or `name is not None`.
    let op = if negate {
        ast::CmpOp::IsNot
    } else {
        ast::CmpOp::Is
    };
    let compare = ast::ExprCompare {
        left: Box::new(var.into()),
        ops: Box::from([op]),
        comparators: Box::from([ast::Expr::NoneLiteral(ast::ExprNoneLiteral::default())]),
        range: TextRange::default(),
    };
    generator.expr(&compare.into())
}

#[derive(Debug)]
pub(crate) enum OpenMode {
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
    pub(crate) fn pathlib_method(&self) -> String {
        match self {
            OpenMode::ReadText => "read_text".to_string(),
            OpenMode::ReadBytes => "read_bytes".to_string(),
            OpenMode::WriteText => "write_text".to_string(),
            OpenMode::WriteBytes => "write_bytes".to_string(),
        }
    }
}

/// A grab bag struct that joins together every piece of information we need to track
/// about a file open operation.
#[derive(Debug)]
pub(crate) struct FileOpen<'a> {
    /// With item where the open happens, we use it for the reporting range.
    pub(crate) item: &'a ast::WithItem,
    /// Filename expression used as the first argument in `open`, we use it in the diagnostic message.
    pub(crate) filename: &'a Expr,
    /// The file open mode.
    pub(crate) mode: OpenMode,
    /// The file open keywords.
    pub(crate) keywords: Vec<&'a ast::Keyword>,
    /// We only check `open` operations whose file handles are used exactly once.
    pub(crate) reference: &'a ResolvedReference,
}

impl<'a> FileOpen<'a> {
    /// Determine whether an expression is a reference to the file handle, by comparing
    /// their ranges. If two expressions have the same range, they must be the same expression.
    pub(crate) fn is_ref(&self, expr: &Expr) -> bool {
        expr.range() == self.reference.range()
    }
}

/// Match positional arguments. Return expression for the file name and open mode.
pub(crate) fn match_open_args(args: &[Expr]) -> Option<(&Expr, OpenMode)> {
    match args {
        [filename] => Some((filename, OpenMode::ReadText)),
        [filename, mode_literal] => match_open_mode(mode_literal).map(|mode| (filename, mode)),
        // The third positional argument is `buffering` and the pathlib methods don't support it.
        _ => None,
    }
}

/// Match open mode to see if it is supported.
pub(crate) fn match_open_mode(mode: &Expr) -> Option<OpenMode> {
    let ast::ExprStringLiteral { value, .. } = mode.as_string_literal_expr()?;
    if value.is_implicit_concatenated() {
        return None;
    }
    match value.to_str() {
        "r" => Some(OpenMode::ReadText),
        "rb" => Some(OpenMode::ReadBytes),
        "w" => Some(OpenMode::WriteText),
        "wb" => Some(OpenMode::WriteBytes),
        _ => None,
    }
}
