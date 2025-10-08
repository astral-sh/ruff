use ruff_python_ast::StmtExpr;
use ruff_python_semantic::Binding;
use ruff_python_semantic::all::DunderAllName;
use ruff_python_semantic::analyze::visibility::Visibility;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::rules::pydocstyle::rules::{
    UndocumentedPublicClassAttribute, UndocumentedPublicModuleVariable,
};

/// D1001, D1002
pub(crate) fn undocumented_public_binding(
    checker: &Checker,
    binding: &Binding,
    exports: Option<&[DunderAllName]>,
) {
    if !binding.kind.is_assignment() && !binding.kind.is_annotation() {
        return;
    }
    let name = binding.name(checker.source());

    if let Visibility::Private = binding.visibility(checker.source()) {
        return;
    }
    let Some(stmt) = binding.statement(checker.semantic()) else {
        return;
    };
    let scope = &checker.semantic().scopes[binding.scope];
    let (in_class, export_to_check, body) = if let Some(cls) = scope.kind.as_class() {
        let cls_name = cls.name.as_str();
        // we should probably get class visibility from ContextualizedDefinitions?
        if cls_name.starts_with('_') {
            return;
        }

        (true, cls_name, &cls.body[..])
    } else if scope.kind.is_module() {
        (false, name, checker.module.python_ast)
    } else {
        return;
    };

    if exports.is_some_and(|exports| {
        !exports
            .iter()
            .any(|export| export.name() == export_to_check)
    }) {
        return;
    }
    let next_statement = body.iter().skip_while(|e| *e != stmt).nth(1);
    let has_docstring = next_statement.is_some_and(|next_stmt| {
        if let ruff_python_ast::Stmt::Expr(StmtExpr { value, .. }) = next_stmt {
            if matches!(value.as_ref(), ruff_python_ast::Expr::StringLiteral(_)) {
                return true;
            }
        }
        false
    });
    // eprintln!("- {name} type: {:?}", &binding.kind);
    // eprintln!("- {name} docstring: {has_docstring}");
    if has_docstring {
        return;
    }
    if in_class {
        // D1001: Undocumented public class attribute
        checker.report_diagnostic_if_enabled(UndocumentedPublicClassAttribute, binding.range());
    } else {
        // D1002: Undocumented public module variable
        checker.report_diagnostic_if_enabled(UndocumentedPublicModuleVariable, binding.range());
    }
}
