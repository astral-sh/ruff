use crate::checkers::ast::Checker;
use crate::fix::snippet::SourceCodeSnippet;
use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{self as ast, Expr, ExprCall};
use ruff_python_semantic::analyze::type_inference::{PythonType, ResolvedPythonType};
use ruff_python_semantic::analyze::typing::find_binding_value;
use ruff_python_semantic::{BindingId, SemanticModel};
use ruff_text_size::Ranged;

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
/// ## References
/// [PEP 8: Programming Recommendations](https://peps.python.org/pep-0008/#programming-recommendations)
#[derive(ViolationMetadata)]
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
    if !is_sequence(argument, semantic) && !is_indirect_sequence(argument, semantic) {
        return;
    }

    let replacement = checker.locator().slice(argument.range()).to_string();

    checker.report_diagnostic(
        Diagnostic::new(
            LenTest {
                expression: SourceCodeSnippet::new(replacement.clone()),
            },
            call.range(),
        )
        .with_fix(Fix::safe_edit(Edit::range_replacement(
            replacement,
            call.range(),
        ))),
    );
}

fn is_indirect_sequence(expr: &Expr, semantic: &SemanticModel) -> bool {
    let Expr::Name(ast::ExprName { id: name, .. }) = expr else {
        return false;
    };

    let scope = semantic.current_scope();
    let bindings: Vec<BindingId> = scope.get_all(name).collect();
    let [binding_id] = bindings.as_slice() else {
        return false;
    };

    let binding = semantic.binding(*binding_id);

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
