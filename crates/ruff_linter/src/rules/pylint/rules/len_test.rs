use ruff_diagnostics::Applicability;
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{self as ast, Expr, ExprCall, Stmt};
use ruff_python_semantic::SemanticModel;
use ruff_python_semantic::analyze::type_inference::{PythonType, ResolvedPythonType};
use ruff_python_semantic::analyze::typing::find_binding_value;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::fix::edits;
use crate::fix::snippet::SourceCodeSnippet;
use crate::{AlwaysFixableViolation, Edit, Fix};

/// ## What it does
/// Checks for `len` calls on sequences in a boolean test context.
///
/// ## Why is this bad?
/// Empty sequences are considered false in a boolean context.
/// You can either remove the call to `len`
/// or compare the length against a scalar.
///
/// ## Example
/// ```python
/// fruits = ["orange", "apple"]
/// vegetables = []
///
/// if len(fruits):
///     print(fruits)
///
/// if not len(vegetables):
///     print(vegetables)
/// ```
///
/// Use instead:
/// ```python
/// fruits = ["orange", "apple"]
/// vegetables = []
///
/// if fruits:
///     print(fruits)
///
/// if not vegetables:
///     print(vegetables)
/// ```
///
/// ## Fix safety
/// This rule's fix is marked as unsafe when the `len` call includes a comment,
/// as the comment would be removed.
///
/// For example, the fix would be marked as unsafe in the following case:
/// ```python
/// fruits = []
/// if len(
///     fruits  # comment
/// ):
///     ...
/// ```
///
/// ## References
/// [PEP 8: Programming Recommendations](https://peps.python.org/pep-0008/#programming-recommendations)
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "0.10.0")]
pub(crate) struct LenTest {
    expression: SourceCodeSnippet,
}

impl AlwaysFixableViolation for LenTest {
    #[derive_message_formats]
    fn message(&self) -> String {
        if let Some(expression) = self.expression.full_display() {
            format!("`len({expression})` used as condition without comparison")
        } else {
            "`len(SEQUENCE)` used as condition without comparison".to_string()
        }
    }

    fn fix_title(&self) -> String {
        "Remove `len`".to_string()
    }
}

/// PLC1802
pub(crate) fn len_test(checker: &Checker, call: &ExprCall) {
    let ExprCall {
        func, arguments, ..
    } = call;
    let semantic = checker.semantic();

    if !semantic.in_boolean_test() {
        return;
    }

    if !semantic.match_builtin_expr(func, "len") {
        return;
    }

    // Single argument and no keyword arguments
    let [argument] = &*arguments.args else { return };
    if !arguments.keywords.is_empty() {
        return;
    }

    // Simple inferred sequence type (e.g., list, set, dict, tuple, string, bytes, varargs, kwargs).
    if !is_sequence(argument, semantic) && !is_indirect_sequence(argument, semantic, checker) {
        return;
    }

    let replacement = checker.locator().slice(argument.range()).to_string();

    checker
        .report_diagnostic(
            LenTest {
                expression: SourceCodeSnippet::new(replacement.clone()),
            },
            call.range(),
        )
        .set_fix(Fix::applicable_edit(
            Edit::range_replacement(
                edits::pad(replacement, call.range(), checker.locator()),
                call.range(),
            ),
            if checker.comment_ranges().intersects(call.range()) {
                Applicability::Unsafe
            } else {
                Applicability::Safe
            },
        ));
}

fn is_indirect_sequence(expr: &Expr, semantic: &SemanticModel, checker: &Checker) -> bool {
    match expr {
        Expr::Name(ast::ExprName { id: name, .. }) => {
            let scope = semantic.current_scope();
            let Some(binding_id) = scope.get(name) else {
                return false;
            };

            let binding = semantic.binding(binding_id);

            // Attempt to find the binding's value
            let Some(binding_value) = find_binding_value(binding, semantic) else {
                // If the binding is not an argument, return false
                if !binding.kind.is_argument() {
                    return false;
                }

                // Attempt to retrieve the function definition statement
                let Some(function) = binding
                    .statement(semantic)
                    .and_then(|statement| statement.as_function_def_stmt())
                else {
                    return false;
                };

                // If not find in non-default params, it must be varargs or kwargs
                return function.parameters.find(name).is_none();
            };

            // If `binding_value` is found, check if it is a sequence
            is_sequence(binding_value, semantic)
        }
        Expr::Attribute(ast::ExprAttribute { value, attr, .. }) => {
            // For attribute access like `self.fruits`, we check if the attribute
            // was assigned a sequence value in the current statement hierarchy
            if let Expr::Name(ast::ExprName { id: base_name, .. }) = value.as_ref() {
                // Look through the current statement hierarchy for assignments to this attribute
                for stmt in semantic.current_statements() {
                    if let Some(assign_value) =
                        find_attribute_assignment(stmt, base_name, attr, semantic)
                    {
                        return is_sequence(assign_value, semantic);
                    }
                }

                // Also check the function body if we're in a function
                if let Some(function_def) = checker
                    .semantic()
                    .current_statements()
                    .find_map(|stmt| stmt.as_function_def_stmt())
                {
                    for stmt in &function_def.body {
                        if let Some(assign_value) =
                            find_attribute_assignment(stmt, base_name, attr, semantic)
                        {
                            return is_sequence(assign_value, semantic);
                        }
                    }
                }
            }
            false
        }
        _ => false,
    }
}

/// Find an assignment to an attribute in a statement.
/// Returns the assigned value if found.
fn find_attribute_assignment<'a>(
    stmt: &'a Stmt,
    base_name: &str,
    attr_name: &str,
    _semantic: &SemanticModel,
) -> Option<&'a Expr> {
    match stmt {
        Stmt::Assign(ast::StmtAssign { targets, value, .. }) => {
            for target in targets {
                if let Expr::Attribute(ast::ExprAttribute {
                    value: target_value,
                    attr: target_attr,
                    ..
                }) = target
                {
                    if let Expr::Name(ast::ExprName {
                        id: target_base, ..
                    }) = target_value.as_ref()
                    {
                        if target_base == base_name && target_attr == attr_name {
                            return Some(value);
                        }
                    }
                }
            }
            None
        }
        Stmt::AnnAssign(ast::StmtAnnAssign {
            target,
            value: Some(value),
            ..
        }) => {
            if let Expr::Attribute(ast::ExprAttribute {
                value: target_value,
                attr: target_attr,
                ..
            }) = target.as_ref()
            {
                if let Expr::Name(ast::ExprName {
                    id: target_base, ..
                }) = target_value.as_ref()
                {
                    if target_base == base_name && target_attr == attr_name {
                        return Some(value);
                    }
                }
            }
            None
        }
        _ => None,
    }
}

fn is_sequence(expr: &Expr, semantic: &SemanticModel) -> bool {
    // Check if the expression type is a direct sequence match (dict, list, set, tuple, string or bytes)
    if matches!(
        ResolvedPythonType::from(expr),
        ResolvedPythonType::Atom(
            PythonType::Dict
                | PythonType::List
                | PythonType::Set
                | PythonType::Tuple
                | PythonType::String
                | PythonType::Bytes
        )
    ) {
        return true;
    }

    // Check if the expression is a function call to a built-in sequence constructor
    let Some(ExprCall { func, .. }) = expr.as_call_expr() else {
        return false;
    };

    // Match against specific built-in constructors that return sequences
    semantic.resolve_builtin_symbol(func).is_some_and(|func| {
        matches!(
            func,
            "chr"
                | "format"
                | "input"
                | "repr"
                | "str"
                | "list"
                | "dir"
                | "locals"
                | "globals"
                | "vars"
                | "dict"
                | "set"
                | "frozenset"
                | "tuple"
                | "range"
                | "bin"
                | "bytes"
                | "bytearray"
                | "hex"
                | "memoryview"
                | "oct"
                | "ascii"
                | "sorted"
        )
    })
}
