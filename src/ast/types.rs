use std::sync::atomic::{AtomicUsize, Ordering};

use rustc_hash::FxHashMap;
use rustpython_ast::{Expr, Keyword};
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
            end_location: located.end_location.unwrap(),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct FunctionScope {
    pub uses_locals: bool,
}

#[derive(Clone, Debug, Default)]
pub struct ClassScope<'a> {
    pub name: &'a str,
    pub bases: &'a [Expr],
    pub keywords: &'a [Keyword],
    pub decorator_list: &'a [Expr],
}

#[derive(Clone, Debug)]
pub enum ScopeKind<'a> {
    Class(ClassScope<'a>),
    Function(FunctionScope),
    Generator,
    Module,
    Arg,
    Lambda,
}

#[derive(Clone, Debug)]
pub struct Scope<'a> {
    pub id: usize,
    pub kind: ScopeKind<'a>,
    pub import_starred: bool,
    pub values: FxHashMap<&'a str, Binding>,
}

impl<'a> Scope<'a> {
    pub fn new(kind: ScopeKind<'a>) -> Self {
        Scope {
            id: id(),
            kind,
            import_starred: false,
            values: FxHashMap::default(),
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
    // TODO(charlie): This seems to be a catch-all.
    Binding,
    LoopVar,
    Global,
    Nonlocal,
    Builtin,
    ClassDefinition,
    Definition,
    Export(Vec<String>),
    FutureImportation,
    StarImportation(Option<usize>, Option<String>),
    Importation(String, String, BindingContext),
    FromImportation(String, String, BindingContext),
    SubmoduleImportation(String, String, BindingContext),
}

#[derive(Clone, Debug)]
pub struct Binding {
    pub kind: BindingKind,
    pub range: Range,
    /// Tuple of (scope index, range) indicating the scope and range at which
    /// the binding was last used.
    pub used: Option<(usize, Range)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ImportKind {
    Import,
    ImportFrom,
}
