use rustpython_parser::ast::{Expr, ExprKind, Located, Stmt, StmtKind};

use crate::checkers::ast::Checker;
use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::scope::{
    Binding, BindingKind, ClassDef, FunctionDef, Scope, ScopeKind, ScopeStack,
};
use ruff_python_ast::types::{Range, RefEquality};

const VALID_CLASS_IDS: [&str; 2] = ["cls", "self"];

#[violation]
pub struct FuncDefinedAbove {
    pub function_name: String,
    pub function_def_line: usize,
}

impl Violation for FuncDefinedAbove {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "newspaper style: function {} defined in line {} should be moved down.",
            self.function_name, self.function_def_line
        )
    }
}

/// NEW100
pub fn check_function_calls(checker: &mut Checker, func: &Expr) {
    let mut scope_stack = checker.ctx.scope_stack.clone();
    if let Some(diagnostic) = match_callee_expr(checker, &mut scope_stack, func) {
        checker.diagnostics.push(diagnostic);
    }
}

fn match_callee_expr(
    checker: &Checker,
    scope_stack: &mut ScopeStack,
    func: &Expr,
) -> Option<Diagnostic> {
    let current_scope_id = scope_stack.pop().expect("No current scope found");
    let kind = &checker.ctx.scopes[current_scope_id].kind;
    if let ScopeKind::Function(FunctionDef {
        name: caller_name, ..
    }) = *kind
    {
        let Located { node, .. } = func;
        let parent_scope_id = scope_stack.pop().expect("No parent scope found");
        let scope = &checker.ctx.scopes[parent_scope_id];
        match scope {
            Scope {
                // caller is defined in class
                kind:
                    ScopeKind::Class(
                        ClassDef {
                            name: class_name, ..
                        },
                        ..,
                    ),
                ..
            } => {
                match node {
                    // callee belongs to some class as caller
                    ExprKind::Attribute {
                        attr: callee_name,
                        value,
                        ..
                    } => {
                        if let Located {
                            node: ExprKind::Name { id, .. },
                            ..
                        } = &**value
                        {
                            if VALID_CLASS_IDS.contains(&&id[..]) {
                                return func_defined_above(
                                    checker,
                                    func,
                                    scope,
                                    callee_name,
                                    caller_name,
                                );
                            }
                        }
                    }
                    // callee belongs to module, scope switches to module and class becomes caller
                    ExprKind::Name {
                        id: callee_name, ..
                    } => {
                        let grandparent_scope_id =
                            scope_stack.pop().expect("No grandparent scope found");
                        let scope = &checker.ctx.scopes[grandparent_scope_id];
                        return func_defined_above(checker, func, scope, callee_name, class_name);
                    }
                    _ => {}
                }
            }
            Scope {
                // caller is defined in module
                kind: ScopeKind::Module,
                ..
            } => {
                // callee belongs to module
                if let ExprKind::Name {
                    id: callee_name, ..
                } = node
                {
                    return func_defined_above(checker, func, scope, callee_name, caller_name);
                }
            }
            Scope {
                // caller is defined in function
                kind: ScopeKind::Function(..),
                ..
            } => {
                match node {
                    // callee is defined in class
                    ExprKind::Attribute {
                        attr: callee_name,
                        value,
                        ..
                    } => {
                        if let Located {
                            node: ExprKind::Name { id, .. },
                            ..
                        } = &**value
                        {
                            if VALID_CLASS_IDS.contains(&&id[..]) {
                                return func_defined_above(
                                    checker,
                                    func,
                                    scope,
                                    callee_name,
                                    caller_name,
                                );
                            }
                        }
                    }
                    // callee is either defined in function or module
                    ExprKind::Name {
                        id: callee_name, ..
                    } => {
                        if let Some(diagnostic) =
                            func_defined_above(checker, func, scope, callee_name, caller_name)
                        {
                            // callee is defined in function
                            return Some(diagnostic);
                        }
                        // callee is in module, push caller parent function to stack and try again
                        scope_stack.push(parent_scope_id);
                        return match_callee_expr(checker, scope_stack, func);
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    None
}

// check if callee function that is called from caller is defined above caller
fn func_defined_above(
    checker: &Checker,
    func: &Expr,
    scope: &Scope,
    callee_name: &String,
    caller_name: &str,
) -> Option<Diagnostic> {
    if let (Some(callee_binding_idx), Some(caller_binding_idx)) =
        (scope.get(&callee_name[..]), scope.get(caller_name))
    {
        if let (
            Binding {
                source:
                    Some(RefEquality(Located {
                        node: StmtKind::FunctionDef { body, .. },
                        location: callee_location,
                        ..
                    })),
                kind,
                range,
                ..
            },
            Binding {
                source:
                    Some(RefEquality(Located {
                        location: caller_location,
                        ..
                    })),
                ..
            },
        ) = (
            &checker.ctx.bindings[*callee_binding_idx],
            &checker.ctx.bindings[*caller_binding_idx],
        ) {
            if caller_location > callee_location {
                if matches!(kind, BindingKind::FunctionDefinition)
                    && !mutual_recursion(body, caller_name)
                {
                    // report a violation if binding is a FunctionDefinition or no mutual recursion occurs
                    let kind = FuncDefinedAbove {
                        function_name: callee_name.to_string(),
                        function_def_line: range.location.row(),
                    };
                    return Some(Diagnostic::new(kind, Range::from(func)));
                }
            }
        }
    }
    None
}

// check if caller function is also called from callee function (mutual recursion)
fn mutual_recursion(body: &[Stmt], caller_name: &str) -> bool {
    for Located { node, .. } in body.iter() {
        match node {
            StmtKind::Return { value: Some(value) }
            | StmtKind::Assign { value, .. }
            | StmtKind::Expr { value, .. } => {
                if let ExprKind::Call { func, .. } = &value.node {
                    if let ExprKind::Attribute { attr, .. } = &func.node {
                        return attr == caller_name;
                    }
                }
            }
            StmtKind::For { body, .. }
            | StmtKind::AsyncFor { body, .. }
            | StmtKind::While { body, .. }
            | StmtKind::If { body, .. }
            | StmtKind::With { body, .. }
            | StmtKind::AsyncWith { body, .. }
            | StmtKind::Try { body, .. }
            | StmtKind::TryStar { body, .. } => return mutual_recursion(body, caller_name),
            _ => {}
        }
    }
    false
}
