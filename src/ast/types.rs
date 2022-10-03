use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};

use rustpython_parser::ast::{Located, Location};

fn id() -> usize {
    static COUNTER: AtomicUsize = AtomicUsize::new(1);
    COUNTER.fetch_add(1, Ordering::Relaxed)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Range {
    pub location: Location,
    pub end_location: Location,
}

impl Range {
    pub fn from_located<T>(located: &Located<T>) -> Self {
        Range {
            location: located.location,
            end_location: located.end_location,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct FunctionScope {
    pub uses_locals: bool,
}

#[derive(Clone, Debug)]
pub enum ScopeKind {
    Class,
    Function(FunctionScope),
    Generator,
    Module,
}

#[derive(Clone, Debug)]
pub struct Scope {
    pub id: usize,
    pub kind: ScopeKind,
    pub import_starred: bool,
    pub values: BTreeMap<String, Binding>,
}

impl Scope {
    pub fn new(kind: ScopeKind) -> Self {
        Scope {
            id: id(),
            kind,
            import_starred: false,
            values: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct BindingContext {
    pub defined_by: usize,
    pub defined_in: Option<usize>,
}

#[derive(Clone, Debug)]
pub enum BindingKind {
    Annotation,
    Argument,
    Assignment,
    Binding,
    LoopVar,
    Builtin,
    ClassDefinition,
    Definition,
    Export(Vec<String>),
    FutureImportation,
    StarImportation,
    Importation(String, BindingContext),
    FromImportation(String, BindingContext),
    SubmoduleImportation(String, BindingContext),
}

#[derive(Clone, Debug)]
pub struct Binding {
    pub kind: BindingKind,
    pub range: Range,
    /// Tuple of (scope index, range) indicating the scope and range at which the binding was
    /// last used.
    pub used: Option<(usize, Range)>,
}

pub trait CheckLocator {
    fn locate_check(&self, default: Range) -> Range;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ImportKind {
    Import,
    ImportFrom,
}
