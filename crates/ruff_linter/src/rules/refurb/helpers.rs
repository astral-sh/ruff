use std::borrow::Cow;

use ruff_python_ast::PythonVersion;
use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{self as ast, Expr, name::Name, token::parenthesized_range};
use ruff_python_codegen::Generator;
use ruff_python_semantic::ResolvedReference;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::rules::flake8_async::rules::blocking_open_call::is_open_call_from_pathlib;
use crate::{Applicability, Edit, Fix};

/// Format a code snippet to call `name.method()`.
pub(super) fn generate_method_call(name: Name, method: &str, generator: Generator) -> String {
    // Construct `name`.
    let var = ast::ExprName {
        id: name,
        ctx: ast::ExprContext::Load,
        range: TextRange::default(),
        node_index: ruff_python_ast::AtomicNodeIndex::NONE,
    };
    // Construct `name.method`.
    let attr = ast::ExprAttribute {
        value: Box::new(var.into()),
        attr: ast::Identifier::new(method.to_string(), TextRange::default()),
        ctx: ast::ExprContext::Load,
        range: TextRange::default(),
        node_index: ruff_python_ast::AtomicNodeIndex::NONE,
    };
    // Make it into a call `name.method()`
    let call = ast::ExprCall {
        func: Box::new(attr.into()),
        arguments: ast::Arguments {
            args: Box::from([]),
            keywords: Box::from([]),
            range: TextRange::default(),
            node_index: ruff_python_ast::AtomicNodeIndex::NONE,
        },
        range: TextRange::default(),
        node_index: ruff_python_ast::AtomicNodeIndex::NONE,
    };
    // And finally, turn it into a statement.
    let stmt = ast::StmtExpr {
        value: Box::new(call.into()),
        range: TextRange::default(),
        node_index: ruff_python_ast::AtomicNodeIndex::NONE,
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
        node_index: ruff_python_ast::AtomicNodeIndex::NONE,
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

    fn is_read(self) -> bool {
        matches!(self, OpenMode::ReadText | OpenMode::ReadBytes)
    }

    fn is_binary(self) -> bool {
        matches!(self, OpenMode::ReadBytes | OpenMode::WriteBytes)
    }
}

/// A grab bag struct that joins together every piece of information we need to track
/// about a file open operation.
#[derive(Debug)]
pub(super) struct FileOpen<'a> {
    /// With item where the open happens, we use it for the reporting range.
    pub(super) item: &'a ast::WithItem,
    /// The file open mode.
    pub(super) mode: OpenMode,
    /// The file open keywords.
    pub(super) keywords: Vec<&'a ast::Keyword>,
    /// We only check `open` operations whose file handles are used exactly once.
    pub(super) reference: TextRange,
    pub(super) argument: OpenArgument<'a>,
}

impl FileOpen<'_> {
    /// Determine whether an expression is a reference to the file handle, by comparing
    /// their ranges. If two expressions have the same range, they must be the same expression.
    pub(super) fn is_ref(&self, expr: &Expr) -> bool {
        expr.range() == self.reference
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) enum OpenArgument<'a> {
    /// The filename argument to `open`, e.g. "foo.txt" in:
    ///
    /// ```py
    /// f = open("foo.txt")
    /// ```
    Builtin { filename: &'a Expr },
    /// The `Path` receiver of a `pathlib.Path.open` call, e.g. the `p` in the
    /// context manager in:
    ///
    /// ```py
    /// p = Path("foo.txt")
    /// with p.open() as f: ...
    /// ```
    ///
    /// or `Path("foo.txt")` in
    ///
    /// ```py
    /// with Path("foo.txt").open() as f: ...
    /// ```
    Pathlib { path: &'a Expr },
}

impl OpenArgument<'_> {
    pub(super) fn display<'src>(&self, source: &'src str) -> &'src str {
        &source[self.range()]
    }
}

impl Ranged for OpenArgument<'_> {
    fn range(&self) -> TextRange {
        match self {
            OpenArgument::Builtin { filename } => filename.range(),
            OpenArgument::Pathlib { path } => path.range(),
        }
    }
}

/// Find and return all `open` operations in the given `with` statement.
pub(super) fn find_file_opens<'a>(
    with: &'a ast::StmtWith,
    checker: &Checker<'_>,
    read_mode: bool,
) -> Vec<FileOpen<'a>> {
    let semantic = checker.semantic();
    let python_version = checker.target_version();
    let with_parent = semantic.current_statement_parent();
    let module_body = checker.parsed.suite();
    let following_statements = following_statements_after_with(with, with_parent, module_body);

    with.items
        .iter()
        .filter_map(|item| {
            find_file_open(
                item,
                with,
                checker,
                read_mode,
                python_version,
                following_statements,
            )
            .or_else(|| {
                find_path_open(
                    item,
                    with,
                    checker,
                    read_mode,
                    python_version,
                    following_statements,
                )
            })
        })
        .collect()
}

fn resolve_file_open<'a>(
    item: &'a ast::WithItem,
    with: &'a ast::StmtWith,
    checker: &Checker,
    following_statements: Option<&[ast::Stmt]>,
    mode: OpenMode,
    keywords: Vec<&'a ast::Keyword>,
    argument: OpenArgument<'a>,
) -> Option<FileOpen<'a>> {
    let semantic = checker.semantic();
    let var = item.optional_vars.as_deref()?.as_name_expr()?;
    let scope = semantic.current_scope();

    let binding_id = scope.get_all(var.id.as_str()).find(|id| {
        let binding = semantic.binding(*id);
        binding.range() == var.range()
    })?;

    let binding = semantic.binding(binding_id);
    let same_name_bindings: Vec<_> = scope.get_all(var.id.as_str()).collect();

    let references: Vec<&ResolvedReference> = binding
        .references
        .iter()
        .map(|id| semantic.reference(*id))
        .collect();

    let next_binding_after_with = same_name_bindings
        .iter()
        .copied()
        .map(|id| semantic.binding(id).start())
        .filter(|start| *start > with.end())
        .min();

    let has_use_after_with = same_name_bindings.iter().copied().any(|id| {
        semantic.binding(id).references.iter().any(|reference_id| {
            let reference = semantic.reference(*reference_id);
            reference.start() > with.end()
                && next_binding_after_with
                    .is_none_or(|next_binding_start| reference.start() < next_binding_start)
        })
    });

    if has_use_after_with {
        return None;
    }

    if use_after_with_before_unconditional_rebind(var.id.as_str(), following_statements) {
        return None;
    }

    let with_references: Vec<&ResolvedReference> = references
        .into_iter()
        .filter(|reference| with.range().contains_range(reference.range()))
        .collect();

    let [reference] = with_references.as_slice() else {
        return None;
    };

    Some(FileOpen {
        item,
        mode,
        keywords,
        reference: reference.range(),
        argument,
    })
}

/// Find `open` operation in the given `with` item.
fn find_file_open<'a>(
    item: &'a ast::WithItem,
    with: &'a ast::StmtWith,
    checker: &Checker,
    read_mode: bool,
    python_version: PythonVersion,
    following_statements: Option<&[ast::Stmt]>,
) -> Option<FileOpen<'a>> {
    let semantic = checker.semantic();

    // We want to match `open(...) as var`.
    let ast::ExprCall {
        func,
        arguments: ast::Arguments { args, keywords, .. },
        ..
    } = item.context_expr.as_call_expr()?;

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

    if !is_supported_mode(mode, &keywords, read_mode) {
        return None;
    }

    resolve_file_open(
        item,
        with,
        checker,
        following_statements,
        mode,
        keywords,
        OpenArgument::Builtin { filename },
    )
}

fn find_path_open<'a>(
    item: &'a ast::WithItem,
    with: &'a ast::StmtWith,
    checker: &Checker,
    read_mode: bool,
    python_version: PythonVersion,
    following_statements: Option<&[ast::Stmt]>,
) -> Option<FileOpen<'a>> {
    let semantic = checker.semantic();

    let ast::ExprCall {
        func,
        arguments: ast::Arguments { args, keywords, .. },
        ..
    } = item.context_expr.as_call_expr()?;
    if args.iter().any(Expr::is_starred_expr)
        || keywords.iter().any(|keyword| keyword.arg.is_none())
    {
        return None;
    }

    if !is_open_call_from_pathlib(func, semantic) {
        return None;
    }

    let attr = func.as_attribute_expr()?;
    let mode = if args.is_empty() {
        OpenMode::ReadText
    } else {
        match_open_mode(args.first()?)?
    };

    let (keywords, kw_mode) = match_open_keywords(keywords, read_mode, python_version)?;
    let mode = kw_mode.unwrap_or(mode);

    if !is_supported_mode(mode, &keywords, read_mode) {
        return None;
    }

    resolve_file_open(
        item,
        with,
        checker,
        following_statements,
        mode,
        keywords,
        OpenArgument::Pathlib {
            path: attr.value.as_ref(),
        },
    )
}

pub(super) fn following_statements_after_with<'a>(
    with: &ast::StmtWith,
    with_parent: Option<&'a ast::Stmt>,
    module_body: &'a [ast::Stmt],
) -> Option<&'a [ast::Stmt]> {
    let with_range = with.range();
    match with_parent {
        Some(parent) => match parent {
            ast::Stmt::FunctionDef(ast::StmtFunctionDef { body, .. })
            | ast::Stmt::ClassDef(ast::StmtClassDef { body, .. })
            | ast::Stmt::With(ast::StmtWith { body, .. })
            | ast::Stmt::For(ast::StmtFor { body, .. })
            | ast::Stmt::While(ast::StmtWhile { body, .. }) => {
                find_following_in_body(body, with_range)
            }
            ast::Stmt::If(ast::StmtIf {
                body,
                elif_else_clauses,
                ..
            }) => find_following_in_body(body, with_range).or_else(|| {
                elif_else_clauses
                    .iter()
                    .find_map(|clause| find_following_in_body(&clause.body, with_range))
            }),
            ast::Stmt::Try(ast::StmtTry {
                body,
                handlers,
                orelse,
                finalbody,
                ..
            }) => find_following_in_body(body, with_range)
                .or_else(|| {
                    handlers.iter().find_map(|handler| match handler {
                        ast::ExceptHandler::ExceptHandler(ast::ExceptHandlerExceptHandler {
                            body,
                            ..
                        }) => find_following_in_body(body, with_range),
                    })
                })
                .or_else(|| find_following_in_body(orelse, with_range))
                .or_else(|| find_following_in_body(finalbody, with_range)),
            ast::Stmt::Match(ast::StmtMatch { cases, .. }) => cases
                .iter()
                .find_map(|case| find_following_in_body(&case.body, with_range)),
            _ => None,
        },
        None => find_following_in_body(module_body, with_range),
    }
}

fn find_following_in_body(body: &[ast::Stmt], with_range: TextRange) -> Option<&[ast::Stmt]> {
    body.iter()
        .position(|stmt| stmt.range() == with_range)
        .map(|index| {
            let next = index + 1;
            &body[next..]
        })
}

fn use_after_with_before_unconditional_rebind(
    name: &str,
    following_statements: Option<&[ast::Stmt]>,
) -> bool {
    let Some(following_statements) = following_statements else {
        return false;
    };

    for stmt in following_statements {
        if statement_rebinds_name_before_uses(stmt, name) {
            return false;
        }
        if statement_uses_name(stmt, name) {
            return true;
        }
        if statement_unconditionally_rebinds_name(stmt, name) {
            return false;
        }
    }

    false
}

fn statement_rebinds_name_before_uses(stmt: &ast::Stmt, name: &str) -> bool {
    match stmt {
        ast::Stmt::With(ast::StmtWith { items, .. }) => items.iter().any(|item| {
            item.optional_vars
                .as_deref()
                .is_some_and(|target| target_contains_name(target, name))
        }),
        _ => false,
    }
}

fn statement_unconditionally_rebinds_name(stmt: &ast::Stmt, name: &str) -> bool {
    match stmt {
        ast::Stmt::Assign(ast::StmtAssign { targets, .. }) => targets
            .iter()
            .any(|target| target_contains_name(target, name)),
        ast::Stmt::AnnAssign(ast::StmtAnnAssign { target, .. })
        | ast::Stmt::AugAssign(ast::StmtAugAssign { target, .. }) => {
            target_contains_name(target, name)
        }
        ast::Stmt::With(ast::StmtWith { items, .. }) => items.iter().any(|item| {
            item.optional_vars
                .as_deref()
                .is_some_and(|target| target_contains_name(target, name))
        }),
        ast::Stmt::FunctionDef(ast::StmtFunctionDef {
            name: stmt_name, ..
        })
        | ast::Stmt::ClassDef(ast::StmtClassDef {
            name: stmt_name, ..
        }) => stmt_name.as_str() == name,
        ast::Stmt::Import(ast::StmtImport { names, .. }) => names.iter().any(|alias| {
            alias.asname.as_ref().map_or_else(
                || alias.name.id.as_str() == name,
                |asname| asname.as_str() == name,
            )
        }),
        ast::Stmt::ImportFrom(ast::StmtImportFrom { names, .. }) => names.iter().any(|alias| {
            alias.asname.as_ref().map_or_else(
                || alias.name.id.as_str() == name,
                |asname| asname.as_str() == name,
            )
        }),
        _ => false,
    }
}

fn target_contains_name(target: &Expr, name: &str) -> bool {
    match target {
        Expr::Name(ast::ExprName { id, .. }) => id.as_str() == name,
        Expr::Tuple(ast::ExprTuple { elts, .. }) | Expr::List(ast::ExprList { elts, .. }) => {
            elts.iter().any(|elt| target_contains_name(elt, name))
        }
        Expr::Starred(ast::ExprStarred { value, .. }) => target_contains_name(value, name),
        _ => false,
    }
}

fn statement_uses_name(stmt: &ast::Stmt, name: &str) -> bool {
    let mut visitor = NameUseVisitor { name, found: false };
    visitor.visit_stmt(stmt);
    visitor.found
}

struct NameUseVisitor<'a> {
    name: &'a str,
    found: bool,
}

impl<'a> Visitor<'a> for NameUseVisitor<'_> {
    fn visit_stmt(&mut self, stmt: &'a ast::Stmt) {
        if self.found {
            return;
        }
        if let ast::Stmt::With(ast::StmtWith { items, .. }) = stmt {
            if items.iter().any(|item| {
                item.optional_vars
                    .as_deref()
                    .is_some_and(|target| target_contains_name(target, self.name))
            }) {
                // `with ... as name` rebinds `name` before the body executes.
                // The body should not count as a use of the previous binding.
                for item in items {
                    self.visit_expr(&item.context_expr);
                    if self.found {
                        return;
                    }
                }
                return;
            }
        }
        if matches!(stmt, ast::Stmt::FunctionDef(_) | ast::Stmt::ClassDef(_)) {
            return;
        }
        visitor::walk_stmt(self, stmt);
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        if self.found {
            return;
        }
        if matches!(
            expr,
            Expr::Lambda(_)
                | Expr::ListComp(_)
                | Expr::SetComp(_)
                | Expr::DictComp(_)
                | Expr::Generator(_)
        ) {
            return;
        }
        if let Expr::Name(ast::ExprName { id, ctx, .. }) = expr {
            if id.as_str() == self.name && matches!(ctx, ast::ExprContext::Load) {
                self.found = true;
                return;
            }
        }
        visitor::walk_expr(self, expr);
    }
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

fn is_supported_mode(mode: OpenMode, keywords: &[&ast::Keyword], read_mode: bool) -> bool {
    mode.is_read() == read_mode && (!mode.is_binary() || keywords.is_empty())
}

/// A helper function that extracts the `iter` from a [`ast::StmtFor`] node and
/// adds parentheses if needed.
///
/// These cases are okay and will not be modified:
///
/// - `for x in z: ...`       ->  `"z"`
/// - `for x in (y, z): ...`  ->  `"(y, z)"`
/// - `for x in [y, z]: ...`  ->  `"[y, z]"`
///
/// While these cases require parentheses:
///
/// - `for x in y, z: ...`                   ->  `"(y, z)"`
/// - `for x in lambda: 0: ...`              ->  `"(lambda: 0)"`
/// - `for x in (1,) if True else (2,): ...` ->  `"((1,) if True else (2,))"`
pub(super) fn parenthesize_loop_iter_if_necessary<'a>(
    for_stmt: &'a ast::StmtFor,
    checker: &'a Checker,
    location: IterLocation,
) -> Cow<'a, str> {
    let locator = checker.locator();
    let iter = for_stmt.iter.as_ref();

    let original_parenthesized_range =
        parenthesized_range(iter.into(), for_stmt.into(), checker.tokens());

    if let Some(range) = original_parenthesized_range {
        return Cow::Borrowed(locator.slice(range));
    }

    let iter_in_source = locator.slice(iter);

    match iter {
        Expr::Tuple(tuple) if !tuple.parenthesized => Cow::Owned(format!("({iter_in_source})")),
        Expr::Lambda(_) | Expr::If(_) if matches!(location, IterLocation::Comprehension) => {
            Cow::Owned(format!("({iter_in_source})"))
        }
        _ => Cow::Borrowed(iter_in_source),
    }
}

#[derive(Copy, Clone)]
pub(super) enum IterLocation {
    Call,
    Comprehension,
}
