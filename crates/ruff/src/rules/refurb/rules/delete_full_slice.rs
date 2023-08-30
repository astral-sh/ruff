use ruff_diagnostics::{AutofixKind, Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Expr};
use ruff_python_codegen::Generator;
use ruff_python_semantic::analyze::typing::{is_dict, is_list};
use ruff_python_semantic::{Binding, SemanticModel};
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

/// ## What it does
/// Checks for `del` statements that delete the entire slice of a list or
/// dictionary.
///
/// ## Why is this bad?
/// It's is faster and more succinct to remove all items via the `clear()`
/// method.
///
/// ## Example
/// ```python
/// names = {"key": "value"}
/// nums = [1, 2, 3]
///
/// del names[:]
/// del nums[:]
/// ```
///
/// Use instead:
/// ```python
/// names = {"key": "value"}
/// nums = [1, 2, 3]
///
/// names.clear()
/// nums.clear()
/// ```
///
/// ## References
/// - [Python documentation: Mutable Sequence Types](https://docs.python.org/3/library/stdtypes.html?highlight=list#mutable-sequence-types)
/// - [Python documentation: `dict.clear()`](https://docs.python.org/3/library/stdtypes.html?highlight=list#dict.clear)
#[violation]
pub struct DeleteFullSlice;

impl Violation for DeleteFullSlice {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Prefer `clear` over deleting a full slice")
    }

    fn autofix_title(&self) -> Option<String> {
        Some("Replace with `clear()`".to_string())
    }
}

/// FURB131
pub(crate) fn delete_full_slice(checker: &mut Checker, delete: &ast::StmtDelete) {
    for target in &delete.targets {
        let Some(name) = match_full_slice(target, checker.semantic()) else {
            continue;
        };

        let mut diagnostic = Diagnostic::new(DeleteFullSlice, delete.range);

        // Fix is only supported for single-target deletions.
        if checker.patch(diagnostic.kind.rule()) && delete.targets.len() == 1 {
            let replacement = make_suggestion(name, checker.generator());
            diagnostic.set_fix(Fix::suggested(Edit::replacement(
                replacement,
                delete.start(),
                delete.end(),
            )));
        }

        checker.diagnostics.push(diagnostic);
    }
}

/// Match `del expr[:]` where `expr` is a list or a dict.
fn match_full_slice<'a>(expr: &'a Expr, semantic: &SemanticModel) -> Option<&'a str> {
    // Check that it is `del expr[...]`.
    let subscript = expr.as_subscript_expr()?;

    // Check that it is` `del expr[:]`.
    if !matches!(
        subscript.slice.as_ref(),
        Expr::Slice(ast::ExprSlice {
            lower: None,
            upper: None,
            step: None,
            range: _,
        })
    ) {
        return None;
    }

    // Check that it is del var[:]
    let ast::ExprName { id: name, .. } = subscript.value.as_name_expr()?;

    // Let's find definition for var
    let scope = semantic.current_scope();
    let bindings: Vec<&Binding> = scope
        .get_all(name)
        .map(|binding_id| semantic.binding(binding_id))
        .collect();

    // NOTE: Maybe it is too strict of a limitation, but it seems reasonable.
    let [binding] = bindings.as_slice() else {
        return None;
    };

    // It should only apply to variables that are known to be lists or dicts.
    if !(is_dict(binding, semantic) || is_list(binding, semantic)) {
        return None;
    }

    // Name is needed for the fix suggestion.
    Some(name)
}

/// Make fix suggestion for the given name, ie `name.clear()`.
fn make_suggestion(name: &str, generator: Generator) -> String {
    // Here we construct `var.clear()`
    //
    // And start with construction of `var`
    let var = ast::ExprName {
        id: name.to_string(),
        ctx: ast::ExprContext::Load,
        range: TextRange::default(),
    };
    // Make `var.clear`.
    let attr = ast::ExprAttribute {
        value: Box::new(var.into()),
        attr: ast::Identifier::new("clear".to_string(), TextRange::default()),
        ctx: ast::ExprContext::Load,
        range: TextRange::default(),
    };
    // Make it into a call `var.clear()`
    let call = ast::ExprCall {
        func: Box::new(attr.into()),
        arguments: ast::Arguments {
            args: vec![],
            keywords: vec![],
            range: TextRange::default(),
        },
        range: TextRange::default(),
    };
    // And finally, turn it into a statement.
    let stmt = ast::StmtExpr {
        value: Box::new(call.into()),
        range: TextRange::default(),
    };
    generator.stmt(&stmt.into())
}
