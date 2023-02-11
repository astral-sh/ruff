use rustpython_parser::ast::{Expr, ExprContext, ExprKind};

use crate::ast::helpers::{self, create_expr, unparse_expr};
use crate::ast::types::Range;
use crate::autofix::apply_fix;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::DiagnosticKind;
use crate::source_code::Locator;

pub fn pathlib_fix(
    checker: &mut Checker,
    diagnostic: &DiagnosticKind,
    expr: &Expr,
    func: &Expr,
    parent: Option<&Expr>,
) -> Option<Fix> {
    // Guard that Path is imported, `content` contains the name or aliaas
    if let Some(content) = helpers::get_member_import_name_alias(checker, "pathlib", "Path") {
        if let ExprKind::Call { args, keywords, .. } = &expr.node {
            // TODO: validate args/keywords, possibly map
            // TODO: add non-call replacements
            let replacement = match diagnostic {
                DiagnosticKind::PathlibAbspath(_) => "resolve",
                DiagnosticKind::PathlibChmod(_) => "chmod",
                DiagnosticKind::PathlibMkdir(_) => "mkdir",
                // Makedirs
                DiagnosticKind::PathlibRename(_) => "rename",
                DiagnosticKind::PathlibReplace(_) => "replace",
                DiagnosticKind::PathlibRmdir(_) => "rmdir",
                DiagnosticKind::PathlibRemove(_) => "unlink",
                DiagnosticKind::PathlibUnlink(_) => "unlink",
                DiagnosticKind::PathlibGetcwd(_) => "cwd",
                DiagnosticKind::PathlibExists(_) => "exists",
                DiagnosticKind::PathlibExpanduser(_) => "expanduser",
                DiagnosticKind::PathlibIsDir(_) => "is_dir",
                DiagnosticKind::PathlibIsFile(_) => "is_file",
                DiagnosticKind::PathlibIsLink(_) => "is_symlink",
                DiagnosticKind::PathlibReadlink(_) => "readlink",
                // Stat
                DiagnosticKind::PathlibIsAbs(_) => "is_absolute",
                // Join
                // Basename
                // Dirname
                DiagnosticKind::PathlibSamefile(_) => "samefile",
                // Splitext
                // Open
                _ => return None,
            };

            if let Some((head, tail)) = args.clone().split_first() {
                let fix_str = unparse_expr(
                    &create_expr(ExprKind::Call {
                        func: Box::new(create_expr(ExprKind::Attribute {
                            value: Box::new(create_expr(ExprKind::Call {
                                func: Box::new(create_expr(ExprKind::Name {
                                    id: content,
                                    ctx: ExprContext::Load,
                                })),
                                args: vec![head.clone()],
                                keywords: vec![],
                            })),
                            attr: replacement.to_string(),
                            ctx: ExprContext::Load,
                        })),
                        args: tail.to_vec(),
                        keywords: keywords.clone(),
                    }),
                    checker.stylist,
                );

                let mut fix = Some(Fix::replacement(
                    fix_str,
                    expr.location,
                    expr.end_location.unwrap(),
                ));

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
                                    let contents = checker.locator.slice_source_code_range(
                                        &Range::new(arg.location, arg.end_location.unwrap()),
                                    );

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
        } else {
            None
        }
    } else {
        None
    }
}
