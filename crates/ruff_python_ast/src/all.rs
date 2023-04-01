use bitflags::bitflags;
use rustpython_parser::ast::{Constant, Expr, ExprKind, Stmt, StmtKind};

use crate::context::Context;
use crate::scope::{BindingKind, Export, Scope};

bitflags! {
    #[derive(Default)]
    pub struct AllNamesFlags: u32 {
        const INVALID_FORMAT = 0b0000_0001;
        const INVALID_OBJECT = 0b0000_0010;
    }
}

/// Extract the names bound to a given __all__ assignment.
pub fn extract_all_names(
    ctx: &Context,
    stmt: &Stmt,
    scope: &Scope,
) -> (Vec<String>, AllNamesFlags) {
    fn add_to_names(names: &mut Vec<String>, elts: &[Expr], flags: &mut AllNamesFlags) {
        for elt in elts {
            if let ExprKind::Constant {
                value: Constant::Str(value),
                ..
            } = &elt.node
            {
                names.push(value.to_string());
            } else {
                *flags |= AllNamesFlags::INVALID_OBJECT;
            }
        }
    }

    fn extract_elts<'a>(
        ctx: &'a Context,
        expr: &'a Expr,
    ) -> (Option<&'a Vec<Expr>>, AllNamesFlags) {
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
                    if ctx.resolve_call_path(func).map_or(false, |call_path| {
                        call_path.as_slice() == ["", "tuple"]
                            || call_path.as_slice() == ["", "list"]
                    }) {
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
            _ => {}
        }
        (None, AllNamesFlags::INVALID_FORMAT)
    }

    let mut names: Vec<String> = vec![];
    let mut flags = AllNamesFlags::empty();

    // Grab the existing bound __all__ values.
    if let StmtKind::AugAssign { .. } = &stmt.node {
        if let Some(index) = scope.get("__all__") {
            if let BindingKind::Export(Export { names: existing }) = &ctx.bindings[*index].kind {
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
        if let ExprKind::BinOp { left, right, .. } = &value.node {
            let mut current_left = left;
            let mut current_right = right;
            loop {
                // Process the right side, which should be a "real" value.
                let (elts, new_flags) = extract_elts(ctx, current_right);
                flags |= new_flags;
                if let Some(elts) = elts {
                    add_to_names(&mut names, elts, &mut flags);
                }

                // Process the left side, which can be a "real" value or the "rest" of the
                // binary operation.
                if let ExprKind::BinOp { left, right, .. } = &current_left.node {
                    current_left = left;
                    current_right = right;
                } else {
                    let (elts, new_flags) = extract_elts(ctx, current_left);
                    flags |= new_flags;
                    if let Some(elts) = elts {
                        add_to_names(&mut names, elts, &mut flags);
                    }
                    break;
                }
            }
        } else {
            let (elts, new_flags) = extract_elts(ctx, value);
            flags |= new_flags;
            if let Some(elts) = elts {
                add_to_names(&mut names, elts, &mut flags);
            }
        }
    }

    (names, flags)
}
