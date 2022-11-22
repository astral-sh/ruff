use anyhow::Result;
use log::error;
use rustpython_ast::{Excepthandler, ExcepthandlerKind, ExprKind, Located};
use rustpython_parser::lexer;
use rustpython_parser::lexer::Tok;

use crate::ast::helpers;
use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::check_ast::Checker;
use crate::checks::{Check, CheckKind};
use crate::code_gen::SourceGenerator;
use crate::SourceCodeLocator;

/// Given a statement like `except (ValueError,)`, find the range of the
/// parenthesized expression.
fn match_tuple_range<T>(located: &Located<T>, locator: &SourceCodeLocator) -> Result<Range> {
    // Extract contents from the source code.
    let range = Range::from_located(located);
    let contents = locator.slice_source_code_range(&range);

    // Find the left (opening) and right (closing) parentheses.
    let mut location = None;
    let mut end_location = None;
    let mut count: usize = 0;
    for (start, tok, end) in lexer::make_tokenizer(&contents).flatten() {
        if matches!(tok, Tok::Lpar) {
            if count == 0 {
                location = Some(helpers::to_absolute(start, range.location));
            }
            count += 1;
        }

        if matches!(tok, Tok::Rpar) {
            count -= 1;
            if count == 0 {
                end_location = Some(helpers::to_absolute(end, range.location));
                break;
            }
        }
    }
    if let (Some(location), Some(end_location)) = (location, end_location) {
        Ok(Range {
            location,
            end_location,
        })
    } else {
        Err(anyhow::anyhow!(
            "Unable to find left and right parentheses."
        ))
    }
}

/// B013
pub fn redundant_tuple_in_exception_handler(checker: &mut Checker, handlers: &[Excepthandler]) {
    for handler in handlers {
        let ExcepthandlerKind::ExceptHandler { type_, .. } = &handler.node;
        if let Some(type_) = type_ {
            if let ExprKind::Tuple { elts, .. } = &type_.node {
                if let [elt] = &elts[..] {
                    let mut check = Check::new(
                        CheckKind::RedundantTupleInExceptionHandler(elt.to_string()),
                        Range::from_located(type_),
                    );
                    if checker.patch(check.kind.code()) {
                        let mut generator = SourceGenerator::new();
                        generator.unparse_expr(elt, 0);
                        if let Ok(content) = generator.generate() {
                            match match_tuple_range(handler, checker.locator) {
                                Ok(range) => {
                                    check.amend(Fix::replacement(
                                        content,
                                        range.location,
                                        range.end_location,
                                    ));
                                }
                                Err(e) => error!("Failed to locate parentheses: {}", e),
                            }
                        }
                    }
                    checker.add_check(check);
                }
            }
        }
    }
}
