use ast::Ranged;
use ruff_diagnostics::{AutofixKind, Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Expr};
use ruff_python_codegen::Generator;
use ruff_python_semantic::{Binding, SemanticModel};
use ruff_text_size::TextRange;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;
use crate::rules::refurb::helpers::{is_dict, is_list};

/// ## What it does
/// Checks for delete statements with full slice on lists and dictionaries.
///
/// ## Why is this bad?
/// It is faster and more succinct to remove all items via the `clear()` method.
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
/// - [Python documentation: dict.clear()](https://docs.python.org/3/library/stdtypes.html?highlight=list#dict.clear)
#[violation]
pub struct DeleteFullSlice;

impl Violation for DeleteFullSlice {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Prefer `clear` over deleting the full slice")
    }

    fn autofix_title(&self) -> Option<String> {
        Some("Replace with `clear()`".to_string())
    }
}

// FURB131
pub(crate) fn delete_full_slice(checker: &mut Checker, delete: &ast::StmtDelete) {
    // ATM, we can only auto-fix when delete has a single target.
    let only_target = delete.targets.len() == 1;
    for target in &delete.targets {
        let Some(name) = match_full_slice(checker.semantic(), target) else {
            continue;
        };
        let mut diagnostic = Diagnostic::new(DeleteFullSlice {}, delete.range);

        if checker.patch(diagnostic.kind.rule()) && only_target {
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

fn match_full_slice<'a>(semantic: &SemanticModel, expr: &'a Expr) -> Option<&'a str> {
    // Check that it is del expr[...]
    let Expr::Subscript(subscript) = expr else {
        return None;
    };

    // Check that it is del expr[:]
    let Expr::Slice(ast::ExprSlice {
        lower: None,
        upper: None,
        step: None,
        ..
    }) = subscript.slice.as_ref()
    else {
        return None;
    };

    // Check that it is del var[:]
    let Expr::Name(ast::ExprName { id: name, .. }) = subscript.value.as_ref() else {
        return None;
    };

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
    if binding.source.is_none()
        || !(is_dict(semantic, binding, name) || is_list(semantic, binding, name))
    {
        return None;
    }

    // Name is needed for the fix suggestion.
    Some(name)
}
