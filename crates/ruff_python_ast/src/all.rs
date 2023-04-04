use bitflags::bitflags;
use rustpython_parser::ast::{Constant, Expr, ExprKind, Stmt, StmtKind};

bitflags! {
    #[derive(Default)]
    pub struct AllNamesFlags: u32 {
        const INVALID_FORMAT = 0b0000_0001;
        const INVALID_OBJECT = 0b0000_0010;
    }
}

/// Extract the names bound to a given __all__ assignment.
///
/// Accepts a closure that determines whether a given name (e.g., `"list"`) is a Python builtin.
pub fn extract_all_names<F>(stmt: &Stmt, is_builtin: F) -> (Vec<&str>, AllNamesFlags)
where
    F: Fn(&str) -> bool,
{
    fn add_to_names<'a>(elts: &'a [Expr], names: &mut Vec<&'a str>, flags: &mut AllNamesFlags) {
        for elt in elts {
            if let ExprKind::Constant {
                value: Constant::Str(value),
                ..
            } = &elt.node
            {
                names.push(value);
            } else {
                *flags |= AllNamesFlags::INVALID_OBJECT;
            }
        }
    }

    fn extract_elts<F>(expr: &Expr, is_builtin: F) -> (Option<&Vec<Expr>>, AllNamesFlags)
    where
        F: Fn(&str) -> bool,
    {
        match &expr.node {
            ExprKind::List { elts, .. } => {
                return (Some(elts), AllNamesFlags::empty());
            }
            ExprKind::Tuple { elts, .. } => {
                return (Some(elts), AllNamesFlags::empty());
            }
            ExprKind::ListComp { .. } => {
                // Allow comprehensions, even though we can't statically analyze them.
                return (None, AllNamesFlags::empty());
            }
            ExprKind::Call {
                func,
                args,
                keywords,
                ..
            } => {
                // Allow `tuple()` and `list()` calls.
                if keywords.is_empty() && args.len() <= 1 {
                    if let ExprKind::Name { id, .. } = &func.node {
                        if id == "tuple" || id == "list" {
                            if is_builtin(id) {
                                if args.is_empty() {
                                    return (None, AllNamesFlags::empty());
                                }
                                match &args[0].node {
                                    ExprKind::List { elts, .. }
                                    | ExprKind::Set { elts, .. }
                                    | ExprKind::Tuple { elts, .. } => {
                                        return (Some(elts), AllNamesFlags::empty());
                                    }
                                    ExprKind::ListComp { .. }
                                    | ExprKind::SetComp { .. }
                                    | ExprKind::GeneratorExp { .. } => {
                                        // Allow comprehensions, even though we can't statically analyze
                                        // them.
                                        return (None, AllNamesFlags::empty());
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }
        (None, AllNamesFlags::INVALID_FORMAT)
    }

    let mut names: Vec<&str> = vec![];
    let mut flags = AllNamesFlags::empty();

    if let Some(value) = match &stmt.node {
        StmtKind::Assign { value, .. } => Some(value),
        StmtKind::AnnAssign { value, .. } => value.as_ref(),
        StmtKind::AugAssign { value, .. } => Some(value),
        _ => None,
    } {
        if let ExprKind::BinOp { left, right, .. } = &value.node {
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
                if let ExprKind::BinOp { left, right, .. } = &current_left.node {
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
