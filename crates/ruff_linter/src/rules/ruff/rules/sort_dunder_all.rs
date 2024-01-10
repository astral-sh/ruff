use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast as ast;
use ruff_python_parser::{lexer, Mode};
use ruff_source_file::Locator;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;

use strum_macros::EnumIs;

#[violation]
pub struct UnsortedDunderAll;

impl Violation for UnsortedDunderAll {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`__all__` is not alphabetically sorted")
    }

    fn fix_title(&self) -> Option<String> {
        Some("Sort `__all__` alphabetically".to_string())
    }
}

#[derive(EnumIs)]
enum DunderAllKind {
    List,
    Tuple { is_parenthesized: bool },
}

struct DunderAllValue<'a> {
    kind: DunderAllKind,
    items: Vec<&'a ast::ExprStringLiteral>,
    range: &'a TextRange,
    ctx: &'a ast::ExprContext,
}

impl<'a> DunderAllValue<'a> {
    fn from_expr(value: &'a ast::Expr, locator: &Locator) -> Option<DunderAllValue<'a>> {
        let (kind, elts, range, ctx) = match value {
            ast::Expr::List(ast::ExprList { elts, range, ctx }) => {
                (DunderAllKind::List, elts, range, ctx)
            }
            ast::Expr::Tuple(ast::ExprTuple { elts, range, ctx }) => {
                let is_parenthesized =
                    lexer::lex_starts_at(locator.slice(range), Mode::Expression, range.start())
                        .next()?
                        .ok()?
                        .0
                        .is_lpar();
                (DunderAllKind::Tuple { is_parenthesized }, elts, range, ctx)
            }
            _ => return None,
        };
        let mut items = vec![];
        for elt in elts {
            let string_literal = elt.as_string_literal_expr()?;
            if string_literal.value.is_implicit_concatenated() {
                return None;
            }
            items.push(string_literal)
        }
        Some(DunderAllValue {
            kind,
            items,
            range,
            ctx,
        })
    }

    fn sorted_items(&self) -> Vec<&ast::ExprStringLiteral> {
        let mut sorted_items = self.items.clone();
        sorted_items.sort_by_key(|item| item.value.to_str());
        sorted_items
    }
}

impl Ranged for DunderAllValue<'_> {
    fn range(&self) -> TextRange {
        *self.range
    }
}

pub(crate) fn sort_dunder_all(checker: &mut Checker, stmt: &ast::Stmt) {
    // We're only interested in `__all__` in the global scope
    if !checker.semantic().current_scope().kind.is_module() {
        return;
    }

    // We're only interested in `__all__ = ...` and `__all__ += ...`
    let (target, original_value) = match stmt {
        ast::Stmt::Assign(ast::StmtAssign { value, targets, .. }) => match targets.as_slice() {
            [ast::Expr::Name(ast::ExprName { id, .. })] => (id, value.as_ref()),
            _ => return,
        },
        ast::Stmt::AugAssign(ast::StmtAugAssign {
            value,
            target,
            op: ast::Operator::Add,
            ..
        }) => match target.as_ref() {
            ast::Expr::Name(ast::ExprName { id, .. }) => (id, value.as_ref()),
            _ => return,
        },
        _ => return,
    };

    if target != "__all__" {
        return;
    }

    let Some(dunder_all_val) = DunderAllValue::from_expr(original_value, checker.locator()) else {
        return;
    };

    if dunder_all_val.items.len() < 2 {
        return;
    }

    let sorted_items = dunder_all_val.sorted_items();
    if sorted_items == dunder_all_val.items {
        return;
    }

    let dunder_all_range = dunder_all_val.range();
    let mut diagnostic = Diagnostic::new(UnsortedDunderAll, dunder_all_range);

    if !checker.locator().contains_line_break(dunder_all_range) {
        let new_elts = sorted_items
            .iter()
            .map(|elt| ast::Expr::StringLiteral(elt.to_owned().clone()))
            .collect();
        let new_node = match dunder_all_val.kind {
            DunderAllKind::List => ast::Expr::List(ast::ExprList {
                range: dunder_all_range,
                elts: new_elts,
                ctx: *dunder_all_val.ctx,
            }),
            DunderAllKind::Tuple { .. } => ast::Expr::Tuple(ast::ExprTuple {
                range: dunder_all_range,
                elts: new_elts,
                ctx: *dunder_all_val.ctx,
            }),
        };
        let mut content = checker.generator().expr(&new_node);
        if let DunderAllKind::Tuple {
            is_parenthesized: true,
        } = dunder_all_val.kind
        {
            content = format!("({})", content);
        }
        diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
            content,
            dunder_all_range,
        )));
    }

    checker.diagnostics.push(diagnostic);
}
