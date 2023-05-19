use itertools::izip;
use log::error;
use once_cell::unsync::Lazy;
use ruff_text_size::TextRange;
use rustpython_parser::ast::{Cmpop, Expr};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub(crate) enum IsCmpop {
    Is,
    IsNot,
}

impl From<&Cmpop> for IsCmpop {
    fn from(cmpop: &Cmpop) -> Self {
        match cmpop {
            Cmpop::Is => IsCmpop::Is,
            Cmpop::IsNot => IsCmpop::IsNot,
            _ => panic!("Expected Cmpop::Is | Cmpop::IsNot"),
        }
    }
}

#[violation]
pub struct IsLiteral {
    cmpop: IsCmpop,
}

impl AlwaysAutofixableViolation for IsLiteral {
    #[derive_message_formats]
    fn message(&self) -> String {
        let IsLiteral { cmpop } = self;
        match cmpop {
            IsCmpop::Is => format!("Use `==` to compare constant literals"),
            IsCmpop::IsNot => format!("Use `!=` to compare constant literals"),
        }
    }

    fn autofix_title(&self) -> String {
        let IsLiteral { cmpop } = self;
        match cmpop {
            IsCmpop::Is => "Replace `is` with `==`".to_string(),
            IsCmpop::IsNot => "Replace `is not` with `!=`".to_string(),
        }
    }
}

/// F632
pub(crate) fn invalid_literal_comparison(
    checker: &mut Checker,
    left: &Expr,
    ops: &[Cmpop],
    comparators: &[Expr],
    location: TextRange,
) {
    let located = Lazy::new(|| helpers::locate_cmpops(checker.locator.slice(location)));
    let mut left = left;
    for (index, (op, right)) in izip!(ops, comparators).enumerate() {
        if matches!(op, Cmpop::Is | Cmpop::IsNot)
            && (helpers::is_constant_non_singleton(left)
                || helpers::is_constant_non_singleton(right))
        {
            let mut diagnostic = Diagnostic::new(IsLiteral { cmpop: op.into() }, location);
            if checker.patch(diagnostic.kind.rule()) {
                if let Some(located_op) = &located.get(index) {
                    assert_eq!(located_op.op, *op);
                    if let Some(content) = match located_op.op {
                        Cmpop::Is => Some("==".to_string()),
                        Cmpop::IsNot => Some("!=".to_string()),
                        node => {
                            error!("Failed to fix invalid comparison: {node:?}");
                            None
                        }
                    } {
                        #[allow(deprecated)]
                        diagnostic.set_fix(Fix::unspecified(Edit::range_replacement(
                            content,
                            located_op.range + location.start(),
                        )));
                    }
                } else {
                    error!("Failed to fix invalid comparison due to missing op");
                }
            }
            checker.diagnostics.push(diagnostic);
        }
        left = right;
    }
}
