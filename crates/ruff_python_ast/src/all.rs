use crate::helpers::map_subscript;
use crate::{self as ast, Expr, Stmt};
use bitflags::bitflags;

bitflags! {
    #[derive(Default, Debug, Copy, Clone, PartialEq, Eq)]
    pub struct DunderAllFlags: u8 {
        /// The right-hand-side assignment to __all__ uses an invalid expression (i.e., an
        /// expression that doesn't evaluate to a container type, like `__all__ = 1`).
        const INVALID_FORMAT = 1 << 0;
        /// The right-hand-side assignment to __all__ contains an invalid member (i.e., a
        /// non-string, like `__all__ = [1]`).
        const INVALID_OBJECT = 1 << 1;
    }
}

/// Extract the names bound to a given __all__ assignment.
///
/// Accepts a closure that determines whether a given name (e.g., `"list"`) is a Python builtin.
pub fn extract_all_names<F>(stmt: &Stmt, is_builtin: F) -> (Vec<&str>, DunderAllFlags)
where
    F: Fn(&str) -> bool,
{
    fn add_to_names<'a>(elts: &'a [Expr], names: &mut Vec<&'a str>, flags: &mut DunderAllFlags) {
        for elt in elts {
            if let Expr::StringLiteral(ast::ExprStringLiteral { value, .. }) = elt {
                names.push(value);
            } else {
                *flags |= DunderAllFlags::INVALID_OBJECT;
            }
        }
    }

    fn extract_elts<F>(expr: &Expr, is_builtin: F) -> (Option<&[Expr]>, DunderAllFlags)
    where
        F: Fn(&str) -> bool,
    {
        match expr {
            Expr::List(ast::ExprList { elts, .. }) => {
                return (Some(elts), DunderAllFlags::empty());
            }
            Expr::Tuple(ast::ExprTuple { elts, .. }) => {
                return (Some(elts), DunderAllFlags::empty());
            }
            Expr::ListComp(_) => {
                // Allow comprehensions, even though we can't statically analyze them.
                return (None, DunderAllFlags::empty());
            }
            Expr::Name(ast::ExprName { id, .. }) => {
                // Ex) `__all__ = __all__ + multiprocessing.__all__`
                if id == "__all__" {
                    return (None, DunderAllFlags::empty());
                }
            }
            Expr::Attribute(ast::ExprAttribute { attr, .. }) => {
                // Ex) `__all__ = __all__ + multiprocessing.__all__`
                if attr == "__all__" {
                    return (None, DunderAllFlags::empty());
                }
            }
            Expr::Call(ast::ExprCall {
                func,
                arguments: ast::Arguments { args, keywords, .. },
                ..
            }) => {
                // Allow `tuple()`, `list()`, and their generic forms, like `list[int]()`.
                if keywords.is_empty() && args.len() <= 1 {
                    if let Expr::Name(ast::ExprName { id, .. }) = map_subscript(func) {
                        let id = id.as_str();
                        if matches!(id, "tuple" | "list") && is_builtin(id) {
                            let [arg] = args.as_slice() else {
                                return (None, DunderAllFlags::empty());
                            };
                            match arg {
                                Expr::List(ast::ExprList { elts, .. })
                                | Expr::Set(ast::ExprSet { elts, .. })
                                | Expr::Tuple(ast::ExprTuple { elts, .. }) => {
                                    return (Some(elts), DunderAllFlags::empty());
                                }
                                _ => {
                                    // We can't analyze other expressions, but they must be
                                    // valid, since the `list` or `tuple` call will ultimately
                                    // evaluate to a list or tuple.
                                    return (None, DunderAllFlags::empty());
                                }
                            }
                        }
                    }
                }
            }
            Expr::NamedExpr(ast::ExprNamedExpr { value, .. }) => {
                // Allow, e.g., `__all__ += (value := ["A", "B"])`.
                return extract_elts(value, is_builtin);
            }
            _ => {}
        }
        (None, DunderAllFlags::INVALID_FORMAT)
    }

    let mut names: Vec<&str> = vec![];
    let mut flags = DunderAllFlags::empty();

    if let Some(value) = match stmt {
        Stmt::Assign(ast::StmtAssign { value, .. }) => Some(value),
        Stmt::AnnAssign(ast::StmtAnnAssign { value, .. }) => value.as_ref(),
        Stmt::AugAssign(ast::StmtAugAssign { value, .. }) => Some(value),
        _ => None,
    } {
        if let Expr::BinOp(ast::ExprBinOp { left, right, .. }) = value.as_ref() {
            let mut current_left = left;
            let mut current_right = right;
            loop {
                // Process the right side, which should be a "real" value.
                let (elts, new_flags) = extract_elts(current_right, |expr| is_builtin(expr));
                flags |= new_flags;
                if let Some(elts) = elts {
                    add_to_names(elts, &mut names, &mut flags);
                }

                // Process the left side, which can be a "real" value or the "rest" of the
                // binary operation.
                if let Expr::BinOp(ast::ExprBinOp { left, right, .. }) = current_left.as_ref() {
                    current_left = left;
                    current_right = right;
                } else {
                    let (elts, new_flags) = extract_elts(current_left, |expr| is_builtin(expr));
                    flags |= new_flags;
                    if let Some(elts) = elts {
                        add_to_names(elts, &mut names, &mut flags);
                    }
                    break;
                }
            }
        } else {
            let (elts, new_flags) = extract_elts(value, |expr| is_builtin(expr));
            flags |= new_flags;
            if let Some(elts) = elts {
                add_to_names(elts, &mut names, &mut flags);
            }
        }
    }

    (names, flags)
}
