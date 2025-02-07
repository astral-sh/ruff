use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{self as ast, Expr, Stmt};
use ruff_text_size::{Ranged, TextSize};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `super` calls that pass redundant arguments.
///
/// ## Why is this bad?
/// In Python 3, `super` can be invoked without any arguments when: (1) the
/// first argument is `__class__`, and (2) the second argument is equivalent to
/// the first argument of the enclosing method.
///
/// When possible, omit the arguments to `super` to make the code more concise
/// and maintainable.
///
/// ## Example
/// ```python
/// class A:
///     def foo(self):
///         pass
///
///
/// class B(A):
///     def bar(self):
///         super(B, self).foo()
/// ```
///
/// Use instead:
/// ```python
/// class A:
///     def foo(self):
///         pass
///
///
/// class B(A):
///     def bar(self):
///         super().foo()
/// ```
///
/// ## References
/// - [Python documentation: `super`](https://docs.python.org/3/library/functions.html#super)
/// - [super/MRO, Python's most misunderstood feature.](https://www.youtube.com/watch?v=X1PQ7zzltz4)
#[derive(ViolationMetadata)]
pub(crate) struct SuperCallWithParameters;

impl AlwaysFixableViolation for SuperCallWithParameters {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Use `super()` instead of `super(__class__, self)`".to_string()
    }

    fn fix_title(&self) -> String {
        "Remove `__super__` parameters".to_string()
    }
}

/// UP008
pub(crate) fn super_call_with_parameters(checker: &Checker, call: &ast::ExprCall) {
    // Only bother going through the super check at all if we're in a `super` call.
    // (We check this in `super_args` too, so this is just an optimization.)
    if !is_super_call_with_arguments(call) {
        return;
    }
    let scope = checker.semantic().current_scope();

    // Check: are we in a Function scope?
    if !scope.kind.is_function() {
        return;
    }

    let mut parents = checker.semantic().current_statements();

    // For a `super` invocation to be unnecessary, the first argument needs to match
    // the enclosing class, and the second argument needs to match the first
    // argument to the enclosing function.
    let [first_arg, second_arg] = &*call.arguments.args else {
        return;
    };

    // Find the enclosing function definition (if any).
    let Some(Stmt::FunctionDef(ast::StmtFunctionDef {
        parameters: parent_parameters,
        ..
    })) = parents.find(|stmt| stmt.is_function_def_stmt())
    else {
        return;
    };

    // Extract the name of the first argument to the enclosing function.
    let Some(parent_arg) = parent_parameters.args.first() else {
        return;
    };

    // Find the enclosing class definition (if any).
    let Some(Stmt::ClassDef(ast::StmtClassDef {
        name: parent_name,
        decorator_list,
        ..
    })) = parents.find(|stmt| stmt.is_class_def_stmt())
    else {
        return;
    };

    let (
        Expr::Name(ast::ExprName {
            id: first_arg_id, ..
        }),
        Expr::Name(ast::ExprName {
            id: second_arg_id, ..
        }),
    ) = (first_arg, second_arg)
    else {
        return;
    };

    if !(first_arg_id == parent_name.as_str() && second_arg_id == parent_arg.name().as_str()) {
        return;
    }

    drop(parents);

    // If the class is an `@dataclass` with `slots=True`, calling `super()` without arguments raises
    // a `TypeError`.
    //
    // See: https://docs.python.org/3/library/dataclasses.html#dataclasses.dataclass
    if decorator_list.iter().any(|decorator| {
        let Expr::Call(ast::ExprCall {
            func, arguments, ..
        }) = &decorator.expression
        else {
            return false;
        };

        if checker
            .semantic()
            .resolve_qualified_name(func)
            .is_some_and(|name| name.segments() == ["dataclasses", "dataclass"])
        {
            arguments.find_keyword("slots").is_some_and(|keyword| {
                matches!(
                    keyword.value,
                    Expr::BooleanLiteral(ast::ExprBooleanLiteral { value: true, .. })
                )
            })
        } else {
            false
        }
    }) {
        return;
    }

    let mut diagnostic = Diagnostic::new(SuperCallWithParameters, call.arguments.range());
    diagnostic.set_fix(Fix::unsafe_edit(Edit::deletion(
        call.arguments.start() + TextSize::new(1),
        call.arguments.end() - TextSize::new(1),
    )));
    checker.report_diagnostic(diagnostic);
}

/// Returns `true` if a call is an argumented `super` invocation.
fn is_super_call_with_arguments(call: &ast::ExprCall) -> bool {
    if let Expr::Name(ast::ExprName { id, .. }) = call.func.as_ref() {
        id == "super" && !call.arguments.is_empty()
    } else {
        false
    }
}
