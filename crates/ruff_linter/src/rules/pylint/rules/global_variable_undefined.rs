use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Expr, Stmt, StmtGlobal};
use ruff_python_semantic::{BindingKind, ScopeKind};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks that all `global` variables are indeed defined on module level
///
/// ## Why is this bad?
/// If the module level declaration is missing, then either if was
/// forgotten or the `global` can be omitted.
///
/// ## Example
/// ```python
/// def foo():
///     global var  # [global-variable-undefined]
///     var = 10
///     print(var)
/// ```
///
/// Use instead:
/// ```python
/// var = 1
///
///
/// def foo():
///     global var
///     var = 10
///     print(var)
/// ```
#[violation]
pub struct GlobalVariableUndefined {
    name: String,
}

impl Violation for GlobalVariableUndefined {
    #[derive_message_formats]
    fn message(&self) -> String {
        let GlobalVariableUndefined { name } = self;
        format!("Global variable `{name}` is undefined at the module")
    }
}

/// PLW0601
pub(crate) fn global_variable_undefined(checker: &mut Checker, stmt: &Stmt) {
    if checker.semantic().current_scope().kind.is_module() {
        return;
    }

    let Stmt::Global(StmtGlobal { names, range }) = stmt else {
        return;
    };
    let Some(module_scope) = checker
        .semantic()
        .current_scopes()
        .find(|scope| scope.kind.is_module())
    else {
        return;
    };
    let imported_names = get_imports(checker);
    let mut undefined_names = vec![];

    for name in names {
        // Skip if imported names
        if imported_names.contains(&name.as_str()) {
            continue;
        }
        // Skip if module level class or function definition
        let Some(binding_id) = module_scope.get(name) else {
            continue;
        };
        let binding = checker.semantic().binding(binding_id);
        if matches!(
            binding.kind,
            BindingKind::ClassDefinition(_) | BindingKind::FunctionDefinition(_)
        ) {
            continue;
        }
        // Skip if module level definition
        let Some(node_id) = binding.source else {
            continue;
        };
        let node = checker.semantic().node(node_id);
        if let Some(Expr::Name(ast::ExprName { .. })) = node.as_expression() {
            continue;
        };

        undefined_names.push(name);
    }

    for name in undefined_names {
        checker.diagnostics.push(Diagnostic::new(
            GlobalVariableUndefined {
                name: name.to_string(),
            },
            *range,
        ));
    }
}

fn get_imports<'a>(checker: &'a Checker) -> Vec<&'a str> {
    // Get all names imported in the current scope
    let Some(fs) = checker
        .semantic()
        .current_scopes()
        .find(|scope| scope.kind.is_function())
    else {
        return vec![];
    };
    let ScopeKind::Function(ast::StmtFunctionDef { body, .. }) = fs.kind else {
        return vec![];
    };
    let mut import_names = vec![];
    for stmt in body {
        match stmt {
            Stmt::Import(ast::StmtImport { names, .. })
            | Stmt::ImportFrom(ast::StmtImportFrom { names, .. }) => {
                for name in names {
                    import_names.push(name.name.as_str());
                }
            }
            _ => (),
        }
    }
    import_names
}
