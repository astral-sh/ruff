use crate::checkers::ast::Checker;
use crate::fix::snippet::SourceCodeSnippet;
use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Expr, ExprCall, Parameter};
use ruff_python_semantic::analyze::type_inference::{PythonType, ResolvedPythonType};
use ruff_python_semantic::analyze::typing::find_binding_value;
use ruff_python_semantic::{BindingId, SemanticModel};
use ruff_text_size::Ranged;

/// ## What it does
/// Checks for usage of call of 'len' on sequences
/// in boolean test context.
///
/// ## Why is this bad?
/// Empty sequences are considered false in a boolean context.
/// You can either remove the call to 'len' (``if not x``)
/// or compare the length against a scalar (``if len(x) > 0``).
///
/// ## Example
/// ```python
/// fruits = ["orange", "apple"]
///
/// if len(fruits):
///     print(fruits)
/// ```
///
/// Use instead:
/// ```python
/// fruits = ["orange", "apple"]
///
/// if fruits:
///     print(fruits)
/// ```
#[violation]
pub struct LenAsCondition {
    expression: SourceCodeSnippet,
}

impl AlwaysFixableViolation for LenAsCondition {
    #[derive_message_formats]
    fn message(&self) -> String {
        if let Some(expression) = self.expression.full_display() {
            format!("`len({expression})` without comparison used as condition")
        } else {
            "`len(SEQUENCE)` without comparison used as condition".to_string()
        }
    }

    fn fix_title(&self) -> String {
        "Remove `len`".to_string()
    }
}

/// PLC1802
pub(crate) fn len_as_condition(checker: &mut Checker, call: &ExprCall) {
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

    // Simple inferred sequence type (e.g., list, set, dict, tuple, string, generator) or a vararg.
    if !is_sequence(argument, semantic) && !is_indirect_sequence(argument, semantic) {
        return;
    }

    let replacement = checker.locator().slice(argument.range()).to_string();

    let mut diagnostic = Diagnostic::new(
        LenAsCondition {
            expression: SourceCodeSnippet::new(replacement.clone()),
        },
        call.range(),
    );

    diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
        // Generator without parentheses would create syntax error
        if argument.is_generator_expr() {
            format!("({replacement})")
        } else {
            replacement
        },
        call.range(),
    )));

    checker.diagnostics.push(diagnostic);
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
        // check for `vararg`
        return binding.kind.is_argument()
            && binding
                .statement(semantic)
                .and_then(|statement| statement.as_function_def_stmt())
                .and_then(|function| function.parameters.vararg.as_deref())
                .map_or(false, |Parameter { name: var_arg, .. }| {
                    var_arg.id() == name
                });
    };

    // If `binding_value` is found, check if it is a sequence
    is_sequence(binding_value, semantic)
}

fn is_sequence(expr: &Expr, semantic: &SemanticModel) -> bool {
    // Check if the expression type is a direct sequence match (dict, list, set, tuple, generator, string)
    if matches!(
        ResolvedPythonType::from(expr),
        ResolvedPythonType::Atom(
            PythonType::Dict
                | PythonType::List
                | PythonType::Set
                | PythonType::Tuple
                | PythonType::Generator
                | PythonType::String
        )
    ) {
        return true;
    }

    // Check if the expression is a function call to a built-in sequence constructor
    let Some(ExprCall { func, .. }) = expr.as_call_expr() else {
        return false;
    };

    // Match against specific built-in constructors that return sequences
    return semantic
        .resolve_builtin_symbol(func)
        .is_some_and(|func| matches!(func, "list" | "set" | "dict" | "tuple"));
}
