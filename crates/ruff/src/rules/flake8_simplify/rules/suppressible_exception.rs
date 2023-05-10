use ruff_text_size::{TextLen, TextRange};
use rustpython_parser::ast::{
    self, Attributed, Constant, Excepthandler, ExcepthandlerKind, ExprKind, Stmt, StmtKind,
};

use ruff_diagnostics::{AutofixKind, Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::call_path::compose_call_path;
use ruff_python_ast::helpers;
use ruff_python_ast::helpers::has_comments;

use crate::autofix::actions::get_or_import_symbol;
use crate::checkers::ast::Checker;
use crate::registry::AsRule;

#[violation]
pub struct SuppressibleException {
    exception: String,
    fixable: bool,
}

impl Violation for SuppressibleException {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let SuppressibleException { exception, .. } = self;
        format!("Use `contextlib.suppress({exception})` instead of `try`-`except`-`pass`")
    }

    fn autofix_title(&self) -> Option<String> {
        let SuppressibleException { exception, .. } = self;
        Some(format!("Replace with `contextlib.suppress({exception})`"))
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
        [Attributed {
            node: StmtKind::Delete(_)
                | StmtKind::Assign(_)
                | StmtKind::AugAssign(_)
                | StmtKind::AnnAssign(_)
                | StmtKind::Assert(_)
                | StmtKind::Import(_)
                | StmtKind::ImportFrom(_)
                | StmtKind::Expr(_)
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
    let ExcepthandlerKind::ExceptHandler(ast::ExcepthandlerExceptHandler { body, .. }) =
        &handler.node;
    if body.len() == 1 {
        let node = &body[0].node;
        if matches!(node, StmtKind::Pass)
            || (matches!(
            node,
            StmtKind::Expr(ast::StmtExpr {
                value,
                    ..
                })
            if matches!(**value, Attributed { node: ExprKind::Constant(ast::ExprConstant { value: Constant::Ellipsis, .. }), ..})
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
            let fixable = !has_comments(stmt, checker.locator);
            let mut diagnostic = Diagnostic::new(
                SuppressibleException {
                    exception: exception.clone(),
                    fixable,
                },
                stmt.range(),
            );

            if fixable && checker.patch(diagnostic.kind.rule()) {
                diagnostic.try_set_fix(|| {
                    let (import_edit, binding) = get_or_import_symbol(
                        "contextlib",
                        "suppress",
                        stmt.start(),
                        &checker.ctx,
                        &checker.importer,
                        checker.locator,
                    )?;
                    let replace_try = Edit::range_replacement(
                        format!("with {binding}({exception})"),
                        TextRange::at(stmt.start(), "try".text_len()),
                    );
                    let handler_line_begin = checker.locator.line_start(handler.start());
                    let remove_handler = Edit::deletion(handler_line_begin, handler.end());
                    #[allow(deprecated)]
                    Ok(Fix::unspecified_edits(
                        import_edit,
                        [replace_try, remove_handler],
                    ))
                });
            }

            checker.diagnostics.push(diagnostic);
        }
    }
}
