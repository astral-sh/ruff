use ruff_db::diagnostic::{CompileDiagnostic, Diagnostic, Severity};
use ruff_db::files::File;
use ruff_python_ast::{self as ast, AnyNodeRef};
use ruff_text_size::{Ranged, TextRange};
use std::borrow::Cow;

use crate::types::{ClassLiteralType, Type};
use crate::Db;

/// Returns `true` if any diagnostic is enabled for this file.
pub(crate) fn is_any_diagnostic_enabled(db: &dyn Db, file: File) -> bool {
    db.is_file_open(file)
}

pub(crate) fn report_type_diagnostic(
    db: &dyn Db,
    file: File,
    node: AnyNodeRef,
    rule: &str,
    message: std::fmt::Arguments,
) {
    if !is_any_diagnostic_enabled(db, file) {
        return;
    }

    // TODO: Don't emit the diagnostic if:
    // * The enclosing node contains any syntax errors
    // * The rule is disabled for this file. We probably want to introduce a new query that
    //   returns a rule selector for a given file that respects the package's settings,
    //   any global pragma comments in the file, and any per-file-ignores.

    CompileDiagnostic::report(
        db.upcast(),
        TypeCheckDiagnostic {
            file,
            rule: rule.to_string(),
            message: message.to_string(),
            range: node.range(),
        },
    );
}

/// Emit a diagnostic declaring that the object represented by `node` is not iterable
pub(super) fn report_not_iterable(
    db: &dyn Db,
    file: File,
    node: AnyNodeRef,
    not_iterable_ty: Type,
) {
    report_type_diagnostic(
        db,
        file,
        node,
        "not-iterable",
        format_args!(
            "Object of type `{}` is not iterable",
            not_iterable_ty.display(db)
        ),
    );
}

/// Emit a diagnostic declaring that an index is out of bounds for a tuple.
pub(super) fn report_index_out_of_bounds(
    db: &dyn Db,
    file: File,
    kind: &'static str,
    node: AnyNodeRef,
    tuple_ty: Type,
    length: usize,
    index: i64,
) {
    report_type_diagnostic(
        db,
        file,
        node,
        "index-out-of-bounds",
        format_args!(
            "Index {index} is out of bounds for {kind} `{}` with length {length}",
            tuple_ty.display(db)
        ),
    );
}

/// Emit a diagnostic declaring that a type does not support subscripting.
pub(super) fn report_non_subscriptable(
    db: &dyn Db,
    file: File,
    node: AnyNodeRef,
    non_subscriptable_ty: Type,
    method: &str,
) {
    report_type_diagnostic(
        db,
        file,
        node,
        "non-subscriptable",
        format_args!(
            "Cannot subscript object of type `{}` with no `{method}` method",
            non_subscriptable_ty.display(db)
        ),
    );
}

pub(super) fn report_unresolved_module<'a>(
    db: &dyn Db,
    file: File,
    import_node: impl Into<AnyNodeRef<'a>>,
    level: u32,
    module: Option<&str>,
) {
    report_type_diagnostic(
        db,
        file,
        import_node.into(),
        "unresolved-import",
        format_args!(
            "Cannot resolve import `{}{}`",
            ".".repeat(level as usize),
            module.unwrap_or_default()
        ),
    );
}

pub(super) fn report_slice_step_size_zero(db: &dyn Db, file: File, node: AnyNodeRef) {
    report_type_diagnostic(
        db,
        file,
        node,
        "zero-stepsize-in-slice",
        format_args!("Slice step size can not be zero"),
    );
}

pub(super) fn report_invalid_assignment(
    db: &dyn Db,
    file: File,
    node: AnyNodeRef,
    declared_ty: Type,
    assigned_ty: Type,
) {
    match declared_ty {
        Type::ClassLiteral(ClassLiteralType { class }) => {
            report_type_diagnostic(db, file, node, "invalid-assignment", format_args!(
                    "Implicit shadowing of class `{}`; annotate to make it explicit if this is intentional",
                    class.name(db)));
        }
        Type::FunctionLiteral(function) => {
            report_type_diagnostic(db, file, node, "invalid-assignment", format_args!(
                    "Implicit shadowing of function `{}`; annotate to make it explicit if this is intentional",
                    function.name(db)));
        }
        _ => {
            report_type_diagnostic(
                db,
                file,
                node,
                "invalid-assignment",
                format_args!(
                    "Object of type `{}` is not assignable to `{}`",
                    assigned_ty.display(db),
                    declared_ty.display(db),
                ),
            );
        }
    }
}

pub(super) fn report_possibly_unresolved_reference(
    db: &dyn Db,
    file: File,
    expr_name_node: &ast::ExprName,
) {
    let ast::ExprName { id, .. } = expr_name_node;

    report_type_diagnostic(
        db,
        file,
        expr_name_node.into(),
        "possibly-unresolved-reference",
        format_args!("Name `{id}` used when possibly not defined"),
    );
}

pub(super) fn report_unresolved_reference(db: &dyn Db, file: File, expr_name_node: &ast::ExprName) {
    let ast::ExprName { id, .. } = expr_name_node;

    report_type_diagnostic(
        db,
        file,
        expr_name_node.into(),
        "unresolved-reference",
        format_args!("Name `{id}` used when not defined"),
    );
}

#[derive(Debug, Eq, PartialEq)]
pub struct TypeCheckDiagnostic {
    // TODO: Don't use string keys for rules
    pub(super) rule: String,
    pub(super) message: String,
    pub(super) range: TextRange,
    pub(super) file: File,
}

impl TypeCheckDiagnostic {
    pub fn rule(&self) -> &str {
        &self.rule
    }

    pub fn range(&self) -> TextRange {
        self.range
    }
}

impl Diagnostic for TypeCheckDiagnostic {
    fn message(&self) -> std::borrow::Cow<str> {
        Cow::Borrowed(&self.message)
    }

    fn file(&self) -> File {
        self.file
    }

    fn range(&self) -> Option<TextRange> {
        Some(self.range)
    }

    fn severity(&self) -> Severity {
        Severity::Error
    }

    fn rule(&self) -> &str {
        &self.rule
    }
}
