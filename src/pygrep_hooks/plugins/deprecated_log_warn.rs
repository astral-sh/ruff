use rustpython_ast::Expr;

use crate::ast::helpers::{collect_call_paths, dealias_call_path, match_call_path};
use crate::ast::types::Range;
use crate::registry::Diagnostic;
use crate::violations;
use crate::xxxxxxxxs::ast::xxxxxxxx;

/// PGH002 - deprecated use of logging.warn
pub fn deprecated_log_warn(xxxxxxxx: &mut xxxxxxxx, func: &Expr) {
    let call_path = dealias_call_path(collect_call_paths(func), &xxxxxxxx.import_aliases);
    if call_path == ["log", "warn"]
        || match_call_path(&call_path, "logging", "warn", &xxxxxxxx.from_imports)
    {
        xxxxxxxx.diagnostics.push(Diagnostic::new(
            violations::DeprecatedLogWarn,
            Range::from_located(func),
        ));
    }
}
