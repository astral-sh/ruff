use rustpython_parser::ast::{
    Constant, Excepthandler, ExcepthandlerKind, ExprKind, Located, Location, Stmt, StmtKind,
};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::call_path::compose_call_path;
use ruff_python_ast::helpers;
use ruff_python_ast::types::Range;

use crate::autofix::actions::get_or_import_symbol;
use crate::checkers::ast::Checker;
use crate::registry::AsRule;

#[violation]
pub struct SuppressibleException {
    pub exception: String,
}

impl AlwaysAutofixableViolation for SuppressibleException {
    #[derive_message_formats]
    fn message(&self) -> String {
        let SuppressibleException { exception } = self;
        format!("Use `contextlib.suppress({exception})` instead of `try`-`except`-`pass`")
    }

    fn autofix_title(&self) -> String {
        let SuppressibleException { exception } = self;
        format!("Replace with `contextlib.suppress({exception})`")
    }
}

/// SIM105
pub fn suppressible_exception(
    checker: &mut Checker,
    stmt: &Stmt,
    try_body: &[Stmt],
    handlers: &[Excepthandler],
    orelse: &[Stmt],
    finalbody: &[Stmt],
) {
    if !matches!(
        try_body,
        [Located {
            node: StmtKind::Delete { .. }
                | StmtKind::Assign { .. }
                | StmtKind::AugAssign { .. }
                | StmtKind::AnnAssign { .. }
                | StmtKind::Assert { .. }
                | StmtKind::Import { .. }
                | StmtKind::ImportFrom { .. }
                | StmtKind::Expr { .. }
                | StmtKind::Pass,
            ..
        }]
    ) || handlers.len() != 1
        || !orelse.is_empty()
        || !finalbody.is_empty()
    {
        return;
    }
    let handler = &handlers[0];
    let ExcepthandlerKind::ExceptHandler { body, .. } = &handler.node;
    if body.len() == 1 {
        let node = &body[0].node;
        if matches!(node, StmtKind::Pass)
            || (matches!(
            node,
            StmtKind::Expr {
                value,
                    ..
                }
            if matches!(**value, Located { node: ExprKind::Constant { value: Constant::Ellipsis, .. }, ..})
            ))
        {
            let handler_names: Vec<String> = helpers::extract_handled_exceptions(handlers)
                .into_iter()
                .filter_map(compose_call_path)
                .collect();
            let exception = if handler_names.is_empty() {
                "Exception".to_string()
            } else {
                handler_names.join(", ")
            };
            let mut diagnostic = Diagnostic::new(
                SuppressibleException {
                    exception: exception.clone(),
                },
                Range::from(stmt),
            );

            if checker.patch(diagnostic.kind.rule()) {
                diagnostic.try_set_fix(|| {
                    let (import_edit, binding) = get_or_import_symbol(
                        "contextlib",
                        "suppress",
                        &checker.ctx,
                        &checker.importer,
                        checker.locator,
                    )?;
                    let try_ending = stmt.location.with_col_offset(3); // size of "try"
                    let replace_try = Edit::replacement(
                        format!("with {binding}({exception})"),
                        stmt.location,
                        try_ending,
                    );
                    let handler_line_begin = Location::new(handler.location.row(), 0);
                    let remove_handler =
                        Edit::deletion(handler_line_begin, handler.end_location.unwrap());
                    Ok(Fix::from_iter([import_edit, replace_try, remove_handler]))
                });
            }

            checker.diagnostics.push(diagnostic);
        }
    }
}
