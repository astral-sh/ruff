use crate::checkers::ast::Checker;
use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::AnyNodeRef;
use ruff_python_semantic::analyze::typing;
use ruff_python_semantic::{Binding, BindingKind, NodeId, SemanticModel};
use ruff_text_size::{Ranged, TextRange};

/// ## What it does
/// Checks for usages of IO operation methods of context variables
/// outside of the original `with` statement.
///
/// ## Why is this bad?
/// Such operations will raise `ValueError: I/O operation on closed file` at runtime.
///
/// ## Example
///
/// ```python
/// with open(".txt") as f:
///     f.read()
///
/// with open(".md", "w"):
///     f.write("")
/// ```
///
/// Use instead:
///
/// ```python
/// with open(".txt") as f:
///     f.read()
///
/// with open(".md", "w") as f:
///     f.write("")
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct OperationOnClosedIO;

impl Violation for OperationOnClosedIO {
    #[derive_message_formats]
    fn message(&self) -> String {
        "IO operation performed on closed IO object".to_string()
    }
}

/// RUF050
pub(crate) fn operation_on_closed_io(
    checker: &Checker,
    binding: &Binding,
) -> Option<Vec<Diagnostic>> {
    if !matches!(&binding.kind, BindingKind::WithItemVar) {
        return None;
    }

    let semantic = checker.semantic();
    let with = binding.statement(semantic)?.as_with_stmt()?;

    if !typing::is_io_base(binding, semantic) {
        return None;
    }

    let mut diagnostics = vec![];

    for reference_id in binding.references() {
        let reference = semantic.reference(reference_id);

        if reference.end() <= with.end() {
            continue;
        }

        let Some(expression_id) = reference.expression_id() else {
            continue;
        };

        let Some(range) = method_reference_range(expression_id, semantic)
            .or_else(|| contains_check_range(expression_id, semantic))
            .or_else(|| for_loop_target_in_iter_range(expression_id, semantic))
        else {
            continue;
        };

        let diagnostic = Diagnostic::new(OperationOnClosedIO, range);

        diagnostics.push(diagnostic);
    }

    Some(diagnostics)
}

// `f.write(...)`
fn method_reference_range(expression_id: NodeId, semantic: &SemanticModel) -> Option<TextRange> {
    let mut ancestors = semantic.expressions(expression_id);

    let _io_object_ref = ancestors.next()?;
    let attribute = ancestors.next()?.as_attribute_expr()?;

    if !is_io_operation_method(&attribute.attr.id) {
        return None;
    }

    Some(attribute.range)
}

fn is_io_operation_method(name: &str) -> bool {
    matches!(
        name,
        "__iter__"
            | "__next__"
            | "detach"
            | "fileno"
            | "flush"
            | "isatty"
            | "read"
            | "readline"
            | "readlines"
            | "reconfigure"
            | "seek"
            | "seekable"
            | "tell"
            | "truncate"
            | "writable"
            | "write"
            | "writelines"
    )
}

// `_ in f`
fn contains_check_range(expression_id: NodeId, semantic: &SemanticModel) -> Option<TextRange> {
    let mut ancestors = semantic.expressions(expression_id);

    let io_object_ref = AnyNodeRef::from(ancestors.next()?);
    let compare = ancestors.next()?.as_compare_expr()?;

    compare
        .comparators
        .iter()
        .enumerate()
        .find_map(|(index, comparator)| {
            if !io_object_ref.ptr_eq(comparator.into()) {
                return None;
            }

            let op = compare.ops[index];

            if !op.is_in() && !op.is_not_in() {
                return None;
            }

            let start = if index == 0 {
                compare.left.start()
            } else {
                compare.comparators[index - 1].start()
            };
            let end = comparator.end();

            Some(TextRange::new(start, end))
        })
}

// `for _ in f: ...`
fn for_loop_target_in_iter_range(
    expression_id: NodeId,
    semantic: &SemanticModel,
) -> Option<TextRange> {
    let mut ancestor_statements = semantic.statements(expression_id);

    let io_object_ref = AnyNodeRef::from(semantic.expression(expression_id)?);

    let for_loop = ancestor_statements.next()?.as_for_stmt()?;
    let iter = for_loop.iter.as_ref();

    if !io_object_ref.ptr_eq(iter.into()) {
        return None;
    }

    let start = for_loop.target.start();
    let end = iter.end();

    Some(TextRange::new(start, end))
}
