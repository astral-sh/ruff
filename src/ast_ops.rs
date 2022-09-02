use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};

use rustpython_parser::ast::{Constant, Expr, ExprKind, Location, Stmt, StmtKind};

fn id() -> usize {
    static COUNTER: AtomicUsize = AtomicUsize::new(1);
    COUNTER.fetch_add(1, Ordering::Relaxed)
}

pub enum ScopeKind {
    Class,
    Function,
    Generator,
    Module,
}

pub struct Scope {
    pub id: usize,
    pub kind: ScopeKind,
    pub values: BTreeMap<String, Binding>,
}

impl Scope {
    pub fn new(kind: ScopeKind) -> Self {
        Scope {
            id: id(),
            kind,
            values: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub enum BindingKind {
    Argument,
    Assignment,
    Builtin,
    ClassDefinition,
    Definition,
    Export(Vec<String>),
    FutureImportation,
    Importation(String),
    StarImportation,
    SubmoduleImportation(String),
}

#[derive(Clone, Debug)]
pub struct Binding {
    pub kind: BindingKind,
    pub location: Location,
    pub used: Option<usize>,
}

/// Extract the names bound to a given __all__ assignment.
pub fn extract_all_names(stmt: &Stmt, scope: &Scope) -> Vec<String> {
    let mut names: Vec<String> = vec![];

    fn add_to_names(names: &mut Vec<String>, elts: &[Expr]) {
        for elt in elts {
            if let ExprKind::Constant {
                value: Constant::Str(value),
                ..
            } = &elt.node
            {
                names.push(value.to_string())
            }
        }
    }

    // Grab the existing bound __all__ values.
    if let StmtKind::AugAssign { .. } = &stmt.node {
        if let Some(binding) = scope.values.get("__all__") {
            if let BindingKind::Export(existing) = &binding.kind {
                names.extend(existing.clone());
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
                add_to_names(&mut names, elts)
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
