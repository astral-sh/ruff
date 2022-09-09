use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};

use rustpython_parser::ast::Location;

fn id() -> usize {
    static COUNTER: AtomicUsize = AtomicUsize::new(1);
    COUNTER.fetch_add(1, Ordering::Relaxed)
}

#[derive(Clone, Debug)]
pub enum ScopeKind {
    Class,
    Function,
    Generator,
    Module,
}

#[derive(Clone, Debug)]
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
