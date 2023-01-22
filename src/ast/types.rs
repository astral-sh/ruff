use std::ops::Deref;
use std::sync::atomic::{AtomicUsize, Ordering};

use rustc_hash::FxHashMap;
use rustpython_ast::{Arguments, Expr, Keyword, Stmt};
use rustpython_parser::ast::{Located, Location};

fn id() -> usize {
    static COUNTER: AtomicUsize = AtomicUsize::new(1);
    COUNTER.fetch_add(1, Ordering::Relaxed)
}

#[derive(Clone)]
pub enum Node<'a> {
    Stmt(&'a Stmt),
    Expr(&'a Expr),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Range {
    pub location: Location,
    pub end_location: Location,
}

impl Range {
    pub fn new(location: Location, end_location: Location) -> Self {
        Self {
            location,
            end_location,
        }
    }

    pub fn from_located<T>(located: &Located<T>) -> Self {
        Range::new(located.location, located.end_location.unwrap())
    }
}

#[derive(Debug)]
pub struct FunctionDef<'a> {
    // Properties derived from StmtKind::FunctionDef.
    pub name: &'a str,
    pub args: &'a Arguments,
    pub body: &'a [Stmt],
    pub decorator_list: &'a [Expr],
    // pub returns: Option<&'a Expr>,
    // pub type_comment: Option<&'a str>,
    // Scope-specific properties.
    // TODO(charlie): Create AsyncFunctionDef to mirror the AST.
    pub async_: bool,
    pub globals: FxHashMap<&'a str, &'a Stmt>,
}

#[derive(Debug)]
pub struct ClassDef<'a> {
    // Properties derived from StmtKind::ClassDef.
    pub name: &'a str,
    pub bases: &'a [Expr],
    pub keywords: &'a [Keyword],
    // pub body: &'a [Stmt],
    pub decorator_list: &'a [Expr],
    // Scope-specific properties.
    pub globals: FxHashMap<&'a str, &'a Stmt>,
}

#[derive(Debug)]
pub struct Lambda<'a> {
    pub args: &'a Arguments,
    pub body: &'a Expr,
}

#[derive(Debug)]
pub enum ScopeKind<'a> {
    Class(ClassDef<'a>),
    Function(FunctionDef<'a>),
    Generator,
    Module,
    Lambda(Lambda<'a>),
}

#[derive(Debug)]
pub struct Scope<'a> {
    pub id: usize,
    pub kind: ScopeKind<'a>,
    pub import_starred: bool,
    pub uses_locals: bool,
    /// A map from bound name to binding index.
    pub values: FxHashMap<&'a str, usize>,
    /// A list of (name, index) pairs for bindings that were overridden in the
    /// scope.
    pub overridden: Vec<(&'a str, usize)>,
}

impl<'a> Scope<'a> {
    pub fn new(kind: ScopeKind<'a>) -> Self {
        Scope {
            id: id(),
            kind,
            import_starred: false,
            uses_locals: false,
            values: FxHashMap::default(),
            overridden: Vec::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub enum BindingKind<'a> {
    Annotation,
    Argument,
    Assignment,
    Binding,
    LoopVar,
    Global,
    Nonlocal,
    Builtin,
    ClassDefinition,
    FunctionDefinition,
    Export(Vec<String>),
    FutureImportation,
    StarImportation(Option<usize>, Option<String>),
    Importation(&'a str, &'a str),
    FromImportation(&'a str, String),
    SubmoduleImportation(&'a str, &'a str),
}

#[derive(Clone, Debug)]
pub struct Binding<'a> {
    pub kind: BindingKind<'a>,
    pub range: Range,
    /// The statement in which the [`Binding`] was defined.
    pub source: Option<RefEquality<'a, Stmt>>,
    /// Tuple of (scope index, range) indicating the scope and range at which
    /// the binding was last used.
    pub used: Option<(usize, Range)>,
}

// Pyflakes defines the following binding hierarchy (via inheritance):
//   Binding
//    ExportBinding
//    Annotation
//    Argument
//    Assignment
//      NamedExprAssignment
//    Definition
//      FunctionDefinition
//      ClassDefinition
//      Builtin
//      Importation
//        SubmoduleImportation
//        ImportationFrom
//        StarImportation
//        FutureImportation

impl<'a> Binding<'a> {
    pub fn is_definition(&self) -> bool {
        matches!(
            self.kind,
            BindingKind::ClassDefinition
                | BindingKind::FunctionDefinition
                | BindingKind::Builtin
                | BindingKind::FutureImportation
                | BindingKind::StarImportation(..)
                | BindingKind::Importation(..)
                | BindingKind::FromImportation(..)
                | BindingKind::SubmoduleImportation(..)
        )
    }

    pub fn redefines(&self, existing: &'a Binding) -> bool {
        match &self.kind {
            BindingKind::Importation(.., full_name) => {
                if let BindingKind::SubmoduleImportation(.., existing) = &existing.kind {
                    return full_name == existing;
                }
            }
            BindingKind::FromImportation(.., full_name) => {
                if let BindingKind::SubmoduleImportation(.., existing) = &existing.kind {
                    return full_name == existing;
                }
            }
            BindingKind::SubmoduleImportation(.., full_name) => match &existing.kind {
                BindingKind::Importation(.., existing)
                | BindingKind::SubmoduleImportation(.., existing) => {
                    return full_name == existing;
                }
                BindingKind::FromImportation(.., existing) => {
                    return full_name == existing;
                }
                _ => {}
            },
            BindingKind::Annotation => {
                return false;
            }
            BindingKind::FutureImportation => {
                return false;
            }
            BindingKind::StarImportation(..) => {
                return false;
            }
            _ => {}
        }
        existing.is_definition()
    }
}

#[derive(Debug, Copy, Clone)]
pub struct RefEquality<'a, T>(pub &'a T);

impl<'a, T> std::hash::Hash for RefEquality<'a, T> {
    fn hash<H>(&self, state: &mut H)
    where
        H: std::hash::Hasher,
    {
        (self.0 as *const T).hash(state);
    }
}

impl<'a, 'b, T> PartialEq<RefEquality<'b, T>> for RefEquality<'a, T> {
    fn eq(&self, other: &RefEquality<'b, T>) -> bool {
        std::ptr::eq(self.0, other.0)
    }
}

impl<'a, T> Eq for RefEquality<'a, T> {}

impl<'a, T> Deref for RefEquality<'a, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.0
    }
}

impl<'a> From<&RefEquality<'a, Stmt>> for &'a Stmt {
    fn from(r: &RefEquality<'a, Stmt>) -> Self {
        r.0
    }
}

impl<'a> From<&RefEquality<'a, Expr>> for &'a Expr {
    fn from(r: &RefEquality<'a, Expr>) -> Self {
        r.0
    }
}

pub type CallPath<'a> = smallvec::SmallVec<[&'a str; 8]>;
