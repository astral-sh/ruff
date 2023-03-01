use rustpython_parser::ast::{Excepthandler, ExcepthandlerKind, Located, Stmt, StmtKind};

use ruff_macros::{define_violation, derive_message_formats};

use crate::ast::helpers;
use crate::ast::helpers::compose_call_path;
use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violation::Violation;

define_violation!(
    pub struct UseContextlibSuppress {
        pub exception: String,
    }
);
impl Violation for UseContextlibSuppress {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UseContextlibSuppress { exception } = self;
        format!("Use `contextlib.suppress({exception})` instead of try-except-pass")
    }
}
/// SIM105
pub fn use_contextlib_suppress(
    checker: &mut Checker,
    stmt: &Stmt,
    body: &[Stmt],
    handlers: &[Excepthandler],
    orelse: &[Stmt],
    finalbody: &[Stmt],
) {
    if !matches!(
        body,
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
        if matches!(body[0].node, StmtKind::Pass) {
            let handler_names: Vec<String> = helpers::extract_handled_exceptions(handlers)
                .into_iter()
                .filter_map(compose_call_path)
                .collect();
            let exception = if handler_names.is_empty() {
                "Exception".to_string()
            } else {
                handler_names.join(", ")
            };
            checker.diagnostics.push(Diagnostic::new(
                UseContextlibSuppress { exception },
                Range::from_located(stmt),
            ));
        }
    }
}
