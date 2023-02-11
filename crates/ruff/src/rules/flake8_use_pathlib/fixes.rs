use rustpython_parser::ast::{Expr, ExprKind};

use crate::ast::helpers;
use crate::ast::types::Range;
use crate::autofix::apply_fix;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::DiagnosticKind;
use crate::source_code::Locator;

pub fn pathlib_fix(
    checker: &mut Checker,
    diagnostic: &DiagnosticKind,
    func: &Expr,
    parent: Option<&Expr>,
) -> Option<Fix> {
    // Guard that Path is imported, `content` contains the name or aliaas
    if let Some(content) = helpers::get_member_import_name_alias(checker, "pathlib", "Path") {
        let mut fix = match diagnostic {
            DiagnosticKind::PathlibGetcwd(_) => Some(Fix::replacement(
                format!("{content}.cwd"),
                func.location,
                func.end_location.unwrap(),
            )),
            _ => None,
        };

        // Wrapped in a `Path()` call
        if let Some(fixme) = fix.clone() {
            if let Some(parent) = parent {
                if checker
                    .resolve_call_path(parent)
                    .map_or(false, |call_path| {
                        call_path.as_slice() == ["pathlib", "Path"]
                    })
                {
                    if let ExprKind::Call { args, keywords, .. } = &parent.node {
                        if args.len() == 1 && keywords.is_empty() {
                            // Reset the line index
                            let fixme = Fix::replacement(
                                fixme.content.to_string(),
                                helpers::to_relative(fixme.location, func.location),
                                helpers::to_relative(fixme.end_location, func.location),
                            );

                            // Apply the fix
                            let arg = args.first().unwrap();
                            let contents = checker.locator.slice_source_code_range(&Range::new(
                                arg.location,
                                arg.end_location.unwrap(),
                            ));

                            fix = Some(Fix::replacement(
                                apply_fix(&fixme, &Locator::new(contents)),
                                parent.location,
                                parent.end_location.unwrap(),
                            ));
                        }
                    }
                }
            }
        }
        fix
    } else {
        None
    }
}
