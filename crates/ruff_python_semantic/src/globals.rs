//! When building a semantic model, we often need to know which names in a given scope are declared
//! as `global`. This module provides data structures for storing and querying the set of `global`
//! names in a given scope.

use std::ops::Index;

use ruff_python_ast::{self as ast, Stmt};
use ruff_text_size::{Ranged, TextRange};
use rustc_hash::FxHashMap;

use ruff_index::{newtype_index, IndexVec};
use ruff_python_ast::statement_visitor::{walk_stmt, StatementVisitor};

/// Id uniquely identifying the set of global names for a given scope.
#[newtype_index]
pub struct GlobalsId;

#[derive(Debug, Default)]
pub(crate) struct GlobalsArena<'a>(IndexVec<GlobalsId, Globals<'a>>);

impl<'a> GlobalsArena<'a> {
    /// Inserts a new set of global names into the global names arena and returns its unique id.
    pub(crate) fn push(&mut self, globals: Globals<'a>) -> GlobalsId {
        self.0.push(globals)
    }
}

impl<'a> Index<GlobalsId> for GlobalsArena<'a> {
    type Output = Globals<'a>;

    #[inline]
    fn index(&self, index: GlobalsId) -> &Self::Output {
        &self.0[index]
    }
}

/// The set of global names for a given scope, represented as a map from the name of the global to
/// the range of the declaration in the source code.
#[derive(Debug)]
pub struct Globals<'a>(FxHashMap<&'a str, TextRange>);

impl<'a> Globals<'a> {
    /// Extracts the set of global names from a given scope, or return `None` if the scope does not
    /// contain any `global` declarations.
    pub fn from_body(body: &'a [Stmt]) -> Option<Self> {
        let mut builder = GlobalsVisitor::new();
        builder.visit_body(body);
        builder.finish()
    }

    pub(crate) fn get(&self, name: &str) -> Option<TextRange> {
        self.0.get(name).copied()
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = (&&'a str, &TextRange)> + '_ {
        self.0.iter()
    }
}

/// Extracts the set of global names from a given scope.
#[derive(Debug)]
struct GlobalsVisitor<'a>(FxHashMap<&'a str, TextRange>);

impl<'a> GlobalsVisitor<'a> {
    fn new() -> Self {
        Self(FxHashMap::default())
    }

    fn finish(self) -> Option<Globals<'a>> {
        (!self.0.is_empty()).then_some(Globals(self.0))
    }
}

impl<'a> StatementVisitor<'a> for GlobalsVisitor<'a> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::Global(ast::StmtGlobal { names, range: _ }) => {
                for name in names {
                    self.0.insert(name.as_str(), name.range());
                }
            }
            Stmt::FunctionDef(_) | Stmt::ClassDef(_) => {
                // Don't recurse.
            }
            _ => walk_stmt(self, stmt),
        }
    }
}
