use ruff_diagnostics::Applicability;
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::visitor::{Visitor, walk_expr, walk_stmt};
use ruff_python_ast::{self as ast, Expr, Stmt};
use ruff_python_semantic::{SemanticModel, ScopeKind};
use ruff_text_size::{Ranged, TextSize};

use crate::checkers::ast::Checker;
use crate::{Edit, Fix, FixAvailability, Violation};

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
/// ## Fix safety
///
/// This rule's fix is marked as unsafe because removing the arguments from a call
/// may delete comments that are attached to the arguments.
///
/// ## References
/// - [Python documentation: `super`](https://docs.python.org/3/library/functions.html#super)
/// - [super/MRO, Python's most misunderstood feature.](https://www.youtube.com/watch?v=X1PQ7zzltz4)
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.155")]
pub(crate) struct SuperCallWithParameters;

impl Violation for SuperCallWithParameters {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "Use `super()` instead of `super(__class__, self)`".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Remove `super()` parameters".to_string())
    }
}

/// UP008
pub(crate) fn super_call_with_parameters(checker: &Checker, call: &ast::ExprCall) {
    // Only bother going through the super check at all if we're in a `super` call.
    // (We check this in `super_args` too, so this is just an optimization.)
    if !is_super_call_with_arguments(call, checker) {
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
    let Some(
        func_stmt @ Stmt::FunctionDef(ast::StmtFunctionDef {
            parameters: parent_parameters,
            ..
        }),
    ) = parents.find(|stmt| stmt.is_function_def_stmt())
    else {
        return;
    };

    if is_builtins_super(checker.semantic(), call)
        && !has_local_dunder_class_var_ref(checker.semantic(), func_stmt)
    {
        return;
    }

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

    let Expr::Name(ast::ExprName {
        id: second_arg_id, ..
    }) = second_arg
    else {
        return;
    };

    if second_arg_id != parent_arg.name().as_str() {
        return;
    }

    // Verify the first argument matches the enclosing class chain.
    // For `super(__class__, self)` or `super(ClassName, self)`, just check the immediate class.
    // For `super(Outer.Inner, self)`, verify each segment matches the enclosing class nesting.
    match first_arg {
        Expr::Name(ast::ExprName { id, .. }) => {
            if !((id == "__class__" || id == parent_name.as_str())
                && !checker.semantic().current_scope().has(id))
            {
                return;
            }
            // Check that the name resolves to the enclosing class definition.
            // For `super(ClassName, self)` where ClassName refers to an outer scope class
            // (like in `class Outer: class Inner(Inner):`), changing to `super()` would
            // change semantics, so we should not flag it.
            if *id != "__class__" {
                // Look up the symbol to see if it resolves to a class definition
                if let Some(binding_id) = checker.semantic().lookup_symbol(id) {
                    let binding = checker.semantic().binding(binding_id);
                    if let ruff_python_semantic::BindingKind::ClassDefinition(binding_scope_id) =
                        binding.kind
                    {
                        // Walk up from function scope through all class scopes to find
                        // a class with the same name as the first argument
                        let mut current_scope_id = checker.semantic().current_scope_ids().last();
                        while let Some(scope_id) = current_scope_id {
                            let current_scope = &checker.semantic().scopes[scope_id];
                            if let ScopeKind::Class(class_def) = current_scope.kind {
                                // If there's a class with the same name but different scope,
                                // it means the name refers to that outer class, not the one being defined
                                // So don't flag it (this is the #24001 fix)
                                if class_def.name.as_str() == *id && scope_id != binding_scope_id {
                                    return;
                                }
                            }
                            current_scope_id = checker.semantic().parent_scope_id(scope_id);
                        }
                    }
                }
            }
        }
        Expr::Attribute(_) => {
            let chain = collect_attribute_chain(first_arg);
            // The innermost name must match the immediately enclosing class.
            if chain.last() != Some(&parent_name.as_str()) {
                return;
            }
            // Each preceding name must match the next enclosing class.
            for name in chain.iter().rev().skip(1) {
                let Some(Stmt::ClassDef(ast::StmtClassDef {
                    name: enclosing_name,
                    ..
                })) = parents.find(|stmt| stmt.is_class_def_stmt())
                else {
                    return;
                };
                if *name != enclosing_name.as_str() {
                    return;
                }
            }
        }
        _ => return,
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

    let mut diagnostic = checker.report_diagnostic(SuperCallWithParameters, call.arguments.range());

    // Only provide a fix if there are no keyword arguments, since super() doesn't accept keyword arguments
    if call.arguments.keywords.is_empty() {
        let applicability = if checker.comment_ranges().intersects(call.arguments.range()) {
            Applicability::Unsafe
        } else {
            Applicability::Safe
        };

        diagnostic.set_fix(Fix::applicable_edit(
            Edit::deletion(
                call.arguments.start() + TextSize::new(1),
                call.arguments.end() - TextSize::new(1),
            ),
            applicability,
        ));
    }
}

/// Collects the chain of names from an attribute expression.
///
/// For example, `A.B.C` returns `["A", "B", "C"]`.
fn collect_attribute_chain(expr: &Expr) -> Vec<&str> {
    let mut chain = Vec::new();
    let mut current = expr;
    loop {
        match current {
            Expr::Attribute(ast::ExprAttribute { value, attr, .. }) => {
                chain.push(attr.id.as_str());
                current = value;
            }
            Expr::Name(ast::ExprName { id, .. }) => {
                chain.push(id.as_str());
                break;
            }
            _ => return Vec::new(),
        }
    }
    chain.reverse();
    chain
}

/// Returns `true` if a call is an argumented `super` invocation.
fn is_super_call_with_arguments(call: &ast::ExprCall, checker: &Checker) -> bool {
    checker.semantic().match_builtin_expr(&call.func, "super") && !call.arguments.is_empty()
}

/// Returns `true` if the function contains load references to `__class__` or `super` without
/// local binding.
///
/// This indicates that the function relies on the implicit `__class__` cell variable created by
/// Python when `super()` is called without arguments, making it unsafe to remove `super()` parameters.
fn has_local_dunder_class_var_ref(semantic: &SemanticModel, func_stmt: &Stmt) -> bool {
    if semantic.current_scope().has("__class__") {
        return false;
    }

    let mut finder = ClassCellReferenceFinder::new();
    finder.visit_stmt(func_stmt);

    finder.found()
}

/// Returns `true` if the call is to the built-in `builtins.super` function.
fn is_builtins_super(semantic: &SemanticModel, call: &ast::ExprCall) -> bool {
    semantic
        .resolve_qualified_name(&call.func)
        .is_some_and(|qualified_name| matches!(qualified_name.segments(), ["builtins", "super"]))
}

/// A [`Visitor`] that searches for implicit reference to `__class__` cell,
/// excluding nested class definitions.
#[derive(Debug)]
struct ClassCellReferenceFinder {
    has_class_cell: bool,
}

impl ClassCellReferenceFinder {
    pub(crate) fn new() -> Self {
        ClassCellReferenceFinder {
            has_class_cell: false,
        }
    }
    pub(crate) fn found(&self) -> bool {
        self.has_class_cell
    }
}

impl<'a> Visitor<'a> for ClassCellReferenceFinder {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::ClassDef(_) => {}
            _ => {
                if !self.has_class_cell {
                    walk_stmt(self, stmt);
                }
            }
        }
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        if expr.as_name_expr().is_some_and(|name| {
            matches!(name.id.as_str(), "super" | "__class__") && name.ctx.is_load()
        }) {
            self.has_class_cell = true;
            return;
        }
        walk_expr(self, expr);
    }
}
