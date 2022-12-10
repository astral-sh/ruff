use rustc_hash::FxHashMap;
use rustpython_ast::{Cmpop, Located};
use rustpython_parser::ast::{Constant, Expr, ExprKind, Stmt, StmtKind};
use rustpython_parser::lexer;
use rustpython_parser::lexer::Tok;

use crate::ast::types::{Binding, BindingKind, Scope};
use crate::ast::visitor;
use crate::ast::visitor::Visitor;

/// Extract the names bound to a given __all__ assignment.
pub fn extract_all_names(stmt: &Stmt, scope: &Scope, bindings: &[Binding]) -> Vec<String> {
    fn add_to_names(names: &mut Vec<String>, elts: &[Expr]) {
        for elt in elts {
            if let ExprKind::Constant {
                value: Constant::Str(value),
                ..
            } = &elt.node
            {
                names.push(value.to_string());
            }
        }
    }

    let mut names: Vec<String> = vec![];

    // Grab the existing bound __all__ values.
    if let StmtKind::AugAssign { .. } = &stmt.node {
        if let Some(index) = scope.values.get("__all__") {
            if let BindingKind::Export(existing) = &bindings[*index].kind {
                names.extend_from_slice(existing);
            }
        }
    }

    if let Some(value) = match &stmt.node {
        StmtKind::Assign { value, .. } => Some(value),
        StmtKind::AnnAssign { value, .. } => value.as_ref(),
        StmtKind::AugAssign { value, .. } => Some(value),
        _ => None,
    } {
        match &value.node {
            ExprKind::List { elts, .. } | ExprKind::Tuple { elts, .. } => {
                add_to_names(&mut names, elts);
            }
            ExprKind::BinOp { left, right, .. } => {
                let mut current_left = left;
                let mut current_right = right;
                while let Some(elts) = match &current_right.node {
                    ExprKind::List { elts, .. } => Some(elts),
                    ExprKind::Tuple { elts, .. } => Some(elts),
                    _ => None,
                } {
                    add_to_names(&mut names, elts);
                    match &current_left.node {
                        ExprKind::BinOp { left, right, .. } => {
                            current_left = left;
                            current_right = right;
                        }
                        ExprKind::List { elts, .. } | ExprKind::Tuple { elts, .. } => {
                            add_to_names(&mut names, elts);
                            break;
                        }
                        _ => break,
                    }
                }
            }
            _ => {}
        }
    }

    names
}

#[derive(Default)]
struct GlobalVisitor<'a> {
    globals: FxHashMap<&'a str, &'a Stmt>,
}

impl<'a> Visitor<'a> for GlobalVisitor<'a> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match &stmt.node {
            StmtKind::Global { names } => {
                for name in names {
                    self.globals.insert(name, stmt);
                }
            }
            StmtKind::FunctionDef { .. }
            | StmtKind::AsyncFunctionDef { .. }
            | StmtKind::ClassDef { .. } => {
                // Don't recurse.
            }
            _ => visitor::walk_stmt(self, stmt),
        }
    }
}

/// Extract a map from global name to its last-defining `Stmt`.
pub fn extract_globals(body: &[Stmt]) -> FxHashMap<&str, &Stmt> {
    let mut visitor = GlobalVisitor::default();
    for stmt in body {
        visitor.visit_stmt(stmt);
    }
    visitor.globals
}

/// Check if a node is parent of a conditional branch.
pub fn on_conditional_branch<'a>(parents: &mut impl Iterator<Item = &'a Stmt>) -> bool {
    parents.any(|parent| {
        if matches!(parent.node, StmtKind::If { .. } | StmtKind::While { .. }) {
            return true;
        }
        if let StmtKind::Expr { value } = &parent.node {
            if matches!(value.node, ExprKind::IfExp { .. }) {
                return true;
            }
        }
        false
    })
}

/// Check if a node is in a nested block.
pub fn in_nested_block<'a>(parents: &mut impl Iterator<Item = &'a Stmt>) -> bool {
    parents.any(|parent| {
        matches!(
            parent.node,
            StmtKind::Try { .. } | StmtKind::If { .. } | StmtKind::With { .. }
        )
    })
}

/// Returns `true` if `parent` contains `child`.
fn contains(parent: &Expr, child: &Expr) -> bool {
    match &parent.node {
        ExprKind::BoolOp { values, .. } => values.iter().any(|parent| contains(parent, child)),
        ExprKind::NamedExpr { target, value } => contains(target, child) || contains(value, child),
        ExprKind::BinOp { left, right, .. } => contains(left, child) || contains(right, child),
        ExprKind::UnaryOp { operand, .. } => contains(operand, child),
        ExprKind::Lambda { body, .. } => contains(body, child),
        ExprKind::IfExp { test, body, orelse } => {
            contains(test, child) || contains(body, child) || contains(orelse, child)
        }
        ExprKind::Dict { keys, values } => keys
            .iter()
            .chain(values.iter())
            .any(|parent| contains(parent, child)),
        ExprKind::Set { elts } => elts.iter().any(|parent| contains(parent, child)),
        ExprKind::ListComp { elt, .. } => contains(elt, child),
        ExprKind::SetComp { elt, .. } => contains(elt, child),
        ExprKind::DictComp { key, value, .. } => contains(key, child) || contains(value, child),
        ExprKind::GeneratorExp { elt, .. } => contains(elt, child),
        ExprKind::Await { value } => contains(value, child),
        ExprKind::Yield { value } => value.as_ref().map_or(false, |value| contains(value, child)),
        ExprKind::YieldFrom { value } => contains(value, child),
        ExprKind::Compare {
            left, comparators, ..
        } => contains(left, child) || comparators.iter().any(|parent| contains(parent, child)),
        ExprKind::Call {
            func,
            args,
            keywords,
        } => {
            contains(func, child)
                || args.iter().any(|parent| contains(parent, child))
                || keywords
                    .iter()
                    .any(|keyword| contains(&keyword.node.value, child))
        }
        ExprKind::FormattedValue {
            value, format_spec, ..
        } => {
            contains(value, child)
                || format_spec
                    .as_ref()
                    .map_or(false, |value| contains(value, child))
        }
        ExprKind::JoinedStr { values } => values.iter().any(|parent| contains(parent, child)),
        ExprKind::Constant { .. } => false,
        ExprKind::Attribute { value, .. } => contains(value, child),
        ExprKind::Subscript { value, slice, .. } => {
            contains(value, child) || contains(slice, child)
        }
        ExprKind::Starred { value, .. } => contains(value, child),
        ExprKind::Name { .. } => parent == child,
        ExprKind::List { elts, .. } => elts.iter().any(|parent| contains(parent, child)),
        ExprKind::Tuple { elts, .. } => elts.iter().any(|parent| contains(parent, child)),
        ExprKind::Slice { lower, upper, step } => {
            lower.as_ref().map_or(false, |value| contains(value, child))
                || upper.as_ref().map_or(false, |value| contains(value, child))
                || step.as_ref().map_or(false, |value| contains(value, child))
        }
    }
}

/// Check if a node represents an unpacking assignment.
pub fn is_unpacking_assignment(parent: &Stmt, child: &Expr) -> bool {
    match &parent.node {
        StmtKind::With { items, .. } => items.iter().any(|item| {
            if let Some(optional_vars) = &item.optional_vars {
                if matches!(optional_vars.node, ExprKind::Tuple { .. }) {
                    if contains(optional_vars, child) {
                        return true;
                    }
                }
            }
            false
        }),
        StmtKind::Assign { targets, value, .. } => {
            // In `(a, b) = (1, 2)`, `(1, 2)` is the target, and it is a tuple.
            let value_is_tuple = matches!(
                &value.node,
                ExprKind::Set { .. } | ExprKind::List { .. } | ExprKind::Tuple { .. }
            );
            // In `(a, b) = coords = (1, 2)`, `(a, b)` and `coords` are the targets, and
            // `(a, b`) is a tuple. (We use "tuple" as a placeholder for any
            // unpackable expression.)
            let targets_are_tuples = targets.iter().all(|item| {
                matches!(
                    item.node,
                    ExprKind::Set { .. } | ExprKind::List { .. } | ExprKind::Tuple { .. }
                )
            });
            // If we're looking at `a` in `(a, b) = coords = (1, 2)`, then we should
            // identify that the current expression is in a tuple.
            let child_in_tuple = targets_are_tuples
                || targets.iter().any(|item| {
                    matches!(
                        item.node,
                        ExprKind::Set { .. } | ExprKind::List { .. } | ExprKind::Tuple { .. }
                    ) && contains(item, child)
                });

            // If our child is a tuple, and value is not, it's always an unpacking
            // expression. Ex) `x, y = tup`
            if child_in_tuple && !value_is_tuple {
                return true;
            }

            // If our child isn't a tuple, but value is, it's never an unpacking expression.
            // Ex) `coords = (1, 2)`
            if !child_in_tuple && value_is_tuple {
                return false;
            }

            // If our target and the value are both tuples, then it's an unpacking
            // expression assuming there's at least one non-tuple child.
            // Ex) Given `(x, y) = coords = 1, 2`, `(x, y)` is considered an unpacking
            // expression. Ex) Given `(x, y) = (a, b) = 1, 2`, `(x, y)` isn't
            // considered an unpacking expression.
            if child_in_tuple && value_is_tuple {
                return !targets_are_tuples;
            }

            false
        }
        _ => false,
    }
}

pub type LocatedCmpop<U = ()> = Located<Cmpop, U>;

/// Extract all `Cmpop` operators from a source code snippet, with appropriate
/// ranges.
///
/// `RustPython` doesn't include line and column information on `Cmpop` nodes.
/// `CPython` doesn't either. This method iterates over the token stream and
/// re-identifies `Cmpop` nodes, annotating them with valid ranges.
pub fn locate_cmpops(contents: &str) -> Vec<LocatedCmpop> {
    let mut tok_iter = lexer::make_tokenizer(contents)
        .flatten()
        .into_iter()
        .peekable();
    let mut ops: Vec<LocatedCmpop> = vec![];
    let mut count: usize = 0;
    loop {
        let Some((start, tok, end)) = tok_iter.next() else {
            break;
        };
        if matches!(tok, Tok::Lpar) {
            count += 1;
            continue;
        } else if matches!(tok, Tok::Rpar) {
            count -= 1;
            continue;
        }
        if count == 0 {
            match tok {
                Tok::Not => {
                    if let Some((_, _, end)) =
                        tok_iter.next_if(|(_, tok, _)| matches!(tok, Tok::In))
                    {
                        ops.push(LocatedCmpop::new(start, end, Cmpop::NotIn));
                    }
                }
                Tok::In => {
                    ops.push(LocatedCmpop::new(start, end, Cmpop::In));
                }
                Tok::Is => {
                    if let Some((_, _, end)) =
                        tok_iter.next_if(|(_, tok, _)| matches!(tok, Tok::Not))
                    {
                        ops.push(LocatedCmpop::new(start, end, Cmpop::IsNot));
                    } else {
                        ops.push(LocatedCmpop::new(start, end, Cmpop::Is));
                    }
                }
                Tok::NotEqual => {
                    ops.push(LocatedCmpop::new(start, end, Cmpop::NotEq));
                }
                Tok::EqEqual => {
                    ops.push(LocatedCmpop::new(start, end, Cmpop::Eq));
                }
                Tok::GreaterEqual => {
                    ops.push(LocatedCmpop::new(start, end, Cmpop::GtE));
                }
                Tok::Greater => {
                    ops.push(LocatedCmpop::new(start, end, Cmpop::Gt));
                }
                Tok::LessEqual => {
                    ops.push(LocatedCmpop::new(start, end, Cmpop::LtE));
                }
                Tok::Less => {
                    ops.push(LocatedCmpop::new(start, end, Cmpop::Lt));
                }
                _ => {}
            }
        }
    }
    ops
}

#[cfg(test)]
mod tests {
    use rustpython_ast::{Cmpop, Location};

    use crate::ast::operations::{locate_cmpops, LocatedCmpop};

    #[test]
    fn locates_cmpops() {
        assert_eq!(
            locate_cmpops("x == 1"),
            vec![LocatedCmpop::new(
                Location::new(1, 2),
                Location::new(1, 4),
                Cmpop::Eq
            )]
        );

        assert_eq!(
            locate_cmpops("x != 1"),
            vec![LocatedCmpop::new(
                Location::new(1, 2),
                Location::new(1, 4),
                Cmpop::NotEq
            )]
        );

        assert_eq!(
            locate_cmpops("x is 1"),
            vec![LocatedCmpop::new(
                Location::new(1, 2),
                Location::new(1, 4),
                Cmpop::Is
            )]
        );

        assert_eq!(
            locate_cmpops("x is not 1"),
            vec![LocatedCmpop::new(
                Location::new(1, 2),
                Location::new(1, 8),
                Cmpop::IsNot
            )]
        );

        assert_eq!(
            locate_cmpops("x in 1"),
            vec![LocatedCmpop::new(
                Location::new(1, 2),
                Location::new(1, 4),
                Cmpop::In
            )]
        );

        assert_eq!(
            locate_cmpops("x not in 1"),
            vec![LocatedCmpop::new(
                Location::new(1, 2),
                Location::new(1, 8),
                Cmpop::NotIn
            )]
        );

        assert_eq!(
            locate_cmpops("x != (1 is not 2)"),
            vec![LocatedCmpop::new(
                Location::new(1, 2),
                Location::new(1, 4),
                Cmpop::NotEq
            )]
        );
    }
}
