//! Implements logic used by the document symbol provider, workspace symbol
//! provider, and auto-import feature of the completion provider.

use std::borrow::Cow;
use std::ops::Range;

use regex::Regex;

use ruff_db::files::File;
use ruff_db::parsed::parsed_module;
use ruff_index::{IndexVec, newtype_index};
use ruff_python_ast as ast;
use ruff_python_ast::name::Name;
use ruff_python_ast::visitor::source_order::{self, SourceOrderVisitor};
use ruff_text_size::{Ranged, TextRange};
use ty_project::Db;

use crate::completion::CompletionKind;

/// A compiled query pattern used for searching symbols.
///
/// This can be used with the `FlatSymbols::search` API.
#[derive(Clone, Debug)]
pub struct QueryPattern {
    re: Option<Regex>,
    original: String,
    original_is_exact: bool,
}

impl QueryPattern {
    /// Create a new query pattern from a literal search string given.
    pub fn fuzzy(literal_query_string: &str) -> QueryPattern {
        let mut pattern = "(?i)".to_string();
        for ch in literal_query_string.chars() {
            pattern.push_str(&regex::escape(ch.encode_utf8(&mut [0; 4])));
            pattern.push_str(".*");
        }
        // In theory regex compilation could fail if the pattern string
        // was long enough to exceed the default regex compilation size
        // limit. But this length would be approaching ~10MB or so. If
        // is does somehow fail, we'll just fall back to simple substring
        // search using `original`.
        QueryPattern {
            re: Regex::new(&pattern).ok(),
            original: literal_query_string.to_string(),
            original_is_exact: false,
        }
    }

    /// Create a new query
    pub fn exactly(symbol: &str) -> QueryPattern {
        QueryPattern {
            re: None,
            original: symbol.to_string(),
            original_is_exact: true,
        }
    }

    /// Create a new query pattern that matches all symbols.
    pub fn matches_all_symbols() -> QueryPattern {
        QueryPattern {
            re: None,
            original: String::new(),
            original_is_exact: false,
        }
    }

    fn is_match_symbol(&self, symbol: &SymbolInfo<'_>) -> bool {
        self.is_match_symbol_name(&symbol.name)
    }

    pub fn is_match_symbol_name(&self, symbol_name: &str) -> bool {
        if let Some(ref re) = self.re {
            re.is_match(symbol_name)
        } else if self.original_is_exact {
            symbol_name == self.original
        } else {
            // This is a degenerate case. The only way
            // we should get here is if the query string
            // was thousands (or more) characters long.
            // ... or, if "typed" text could not be found.
            symbol_name.contains(&self.original)
        }
    }

    /// Returns true when it is known that this pattern will return `true` for
    /// all inputs given to `QueryPattern::is_match_symbol_name`.
    ///
    /// This will never return `true` incorrectly, but it may return `false`
    /// incorrectly. That is, it's possible that this query will match all
    /// inputs but this still returns `false`.
    pub fn will_match_everything(&self) -> bool {
        self.re.is_none() && self.original.is_empty()
    }
}

impl From<&str> for QueryPattern {
    fn from(literal_query_string: &str) -> QueryPattern {
        QueryPattern::fuzzy(literal_query_string)
    }
}

impl Eq for QueryPattern {}

impl PartialEq for QueryPattern {
    fn eq(&self, rhs: &QueryPattern) -> bool {
        self.original == rhs.original
    }
}

/// A flat list of indexed symbols for a single file.
#[derive(Clone, Debug, Default, PartialEq, Eq, get_size2::GetSize)]
pub struct FlatSymbols {
    symbols: IndexVec<SymbolId, SymbolTree>,
}

impl FlatSymbols {
    /// Get the symbol info for the symbol identified by the given ID.
    ///
    /// Returns `None` when the given ID does not reference a symbol in this
    /// collection.
    pub fn get(&self, id: SymbolId) -> Option<SymbolInfo<'_>> {
        self.symbols.get(id).map(Into::into)
    }

    /// Returns true if and only if this collection is empty.
    pub fn is_empty(&self) -> bool {
        self.symbols.is_empty()
    }

    /// Returns the total number of symbols in this collection.
    pub fn len(&self) -> usize {
        self.symbols.len()
    }

    /// Returns an iterator over every symbol along with its ID.
    pub fn iter(&self) -> impl Iterator<Item = (SymbolId, SymbolInfo<'_>)> {
        self.symbols
            .iter_enumerated()
            .map(|(id, symbol)| (id, symbol.into()))
    }

    /// Returns a sequence of symbols that matches the given query.
    pub fn search(&self, query: &QueryPattern) -> impl Iterator<Item = (SymbolId, SymbolInfo<'_>)> {
        self.iter()
            .filter(|(_, symbol)| query.is_match_symbol(symbol))
    }

    /// Turns this flat sequence of symbols into a hierarchy of symbols.
    pub fn to_hierarchical(&self) -> HierarchicalSymbols {
        let mut children_ids: IndexVec<SymbolId, Vec<SymbolId>> = IndexVec::new();
        for (id, symbol) in self.symbols.iter_enumerated() {
            children_ids.push(vec![]);
            let Some(parent_id) = symbol.parent else {
                continue;
            };
            // OK because the symbol visitor guarantees that
            // all parents are ordered before their children.
            assert!(parent_id.index() < id.index());
            children_ids[parent_id].push(id);
        }

        // Now flatten our map of symbol ID to its children
        // IDs into a single vec that doesn't nest allocations.
        let mut symbols = IndexVec::new();
        let mut children: Vec<SymbolId> = vec![];
        let mut last_end: usize = 0;
        for (tree, child_symbol_ids) in self.symbols.iter().zip(children_ids) {
            let start = last_end;
            let end = start + child_symbol_ids.len();
            symbols.push(SymbolTreeWithChildren {
                tree: tree.clone(),
                children: start..end,
            });
            children.extend_from_slice(&child_symbol_ids);
            last_end = end;
        }

        HierarchicalSymbols { symbols, children }
    }
}

/// A collection of hierarchical indexed symbols for a single file.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HierarchicalSymbols {
    symbols: IndexVec<SymbolId, SymbolTreeWithChildren>,
    children: Vec<SymbolId>,
}

impl HierarchicalSymbols {
    /// Get the symbol info for the symbol identified by the given ID.
    ///
    /// Returns `None` when the given ID does not reference a symbol in this
    /// collection.
    pub fn get(&self, id: SymbolId) -> Option<SymbolInfo<'_>> {
        self.symbols.get(id).map(Into::into)
    }

    /// Returns true if and only if this collection is empty.
    pub fn is_empty(&self) -> bool {
        self.symbols.is_empty()
    }

    /// Returns the total number of symbols in this collection.
    pub fn len(&self) -> usize {
        self.symbols.len()
    }

    /// Returns an iterator over every top-level symbol along with its ID.
    pub fn iter(&self) -> impl Iterator<Item = (SymbolId, SymbolInfo<'_>)> {
        self.symbols
            .iter_enumerated()
            .filter(|(_, symbol)| symbol.tree.parent.is_none())
            .map(|(id, symbol)| (id, symbol.into()))
    }

    /// Returns an iterator over the child symbols for the symbol
    /// identified by the given ID.
    ///
    /// Returns `None` when there aren't any children or when the given
    /// ID does not reference a symbol in this collection.
    pub fn children(&self, id: SymbolId) -> impl Iterator<Item = (SymbolId, SymbolInfo<'_>)> {
        self.symbols
            .get(id)
            .into_iter()
            .flat_map(|symbol| self.children[symbol.children.clone()].iter())
            .copied()
            .map(|id| (id, SymbolInfo::from(&self.symbols[id])))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SymbolTreeWithChildren {
    tree: SymbolTree,
    /// The index range into `HierarchicalSymbols::children`
    /// corresponding to the children symbol IDs for this
    /// symbol.
    children: Range<usize>,
}

/// Uniquely identifies a symbol.
#[newtype_index]
#[derive(get_size2::GetSize)]
pub struct SymbolId;

/// Symbol information for IDE features like document outline.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SymbolInfo<'a> {
    /// The name of the symbol
    pub name: Cow<'a, str>,
    /// The kind of symbol (function, class, variable, etc.)
    pub kind: SymbolKind,
    /// The range of the symbol name
    pub name_range: TextRange,
    /// The full range of the symbol (including body)
    pub full_range: TextRange,
}

impl SymbolInfo<'_> {
    pub fn to_owned(&self) -> SymbolInfo<'static> {
        SymbolInfo {
            name: Cow::Owned(self.name.to_string()),
            kind: self.kind,
            name_range: self.name_range,
            full_range: self.full_range,
        }
    }
}

impl<'a> From<&'a SymbolTree> for SymbolInfo<'a> {
    fn from(symbol: &'a SymbolTree) -> SymbolInfo<'a> {
        SymbolInfo {
            name: Cow::Borrowed(&symbol.name),
            kind: symbol.kind,
            name_range: symbol.name_range,
            full_range: symbol.full_range,
        }
    }
}

impl<'a> From<&'a SymbolTreeWithChildren> for SymbolInfo<'a> {
    fn from(symbol: &'a SymbolTreeWithChildren) -> SymbolInfo<'a> {
        SymbolInfo::from(&symbol.tree)
    }
}

/// The kind of symbol
#[derive(Debug, Clone, Copy, PartialEq, Eq, get_size2::GetSize)]
pub enum SymbolKind {
    Module,
    Class,
    Method,
    Function,
    Variable,
    Constant,
    Property,
    Field,
    Constructor,
    Parameter,
    TypeParameter,
    Import,
}

impl SymbolKind {
    /// Returns the string representation of the symbol kind.
    pub fn to_string(self) -> &'static str {
        match self {
            SymbolKind::Module => "Module",
            SymbolKind::Class => "Class",
            SymbolKind::Method => "Method",
            SymbolKind::Function => "Function",
            SymbolKind::Variable => "Variable",
            SymbolKind::Constant => "Constant",
            SymbolKind::Property => "Property",
            SymbolKind::Field => "Field",
            SymbolKind::Constructor => "Constructor",
            SymbolKind::Parameter => "Parameter",
            SymbolKind::TypeParameter => "TypeParameter",
            SymbolKind::Import => "Import",
        }
    }

    /// Maps this to a "completion" kind if a sensible mapping exists.
    pub fn to_completion_kind(self) -> Option<CompletionKind> {
        Some(match self {
            SymbolKind::Module => CompletionKind::Module,
            SymbolKind::Class => CompletionKind::Class,
            SymbolKind::Method => CompletionKind::Method,
            SymbolKind::Function => CompletionKind::Function,
            SymbolKind::Variable => CompletionKind::Variable,
            SymbolKind::Constant => CompletionKind::Constant,
            SymbolKind::Property => CompletionKind::Property,
            SymbolKind::Field => CompletionKind::Field,
            SymbolKind::Constructor => CompletionKind::Constructor,
            SymbolKind::Parameter => CompletionKind::Variable,
            SymbolKind::TypeParameter => CompletionKind::TypeParameter,
            // Not quite sure what to do with this one. I guess
            // in theory the import should be "resolved" to its
            // underlying kind, but that seems expensive.
            SymbolKind::Import => return None,
        })
    }
}

/// Returns a flat list of symbols in the file given.
///
/// The flattened list includes parent/child information and can be
/// converted into a hierarchical collection of symbols.
#[salsa::tracked(returns(ref), heap_size=ruff_memory_usage::heap_size)]
pub(crate) fn symbols_for_file(db: &dyn Db, file: File) -> FlatSymbols {
    let parsed = parsed_module(db, file);
    let module = parsed.load(db);

    let mut visitor = SymbolVisitor {
        symbols: IndexVec::new(),
        symbol_stack: vec![],
        in_function: false,
        global_only: false,
    };
    visitor.visit_body(&module.syntax().body);
    FlatSymbols {
        symbols: visitor.symbols,
    }
}

/// Returns a flat list of *only global* symbols in the file given.
///
/// While callers can convert this into a hierarchical collection of
/// symbols, it won't result in anything meaningful since the flat list
/// returned doesn't include children.
#[salsa::tracked(returns(ref), heap_size=ruff_memory_usage::heap_size)]
pub(crate) fn symbols_for_file_global_only(db: &dyn Db, file: File) -> FlatSymbols {
    let parsed = parsed_module(db, file);
    let module = parsed.load(db);

    let mut visitor = SymbolVisitor {
        symbols: IndexVec::new(),
        symbol_stack: vec![],
        in_function: false,
        global_only: true,
    };
    visitor.visit_body(&module.syntax().body);

    if file
        .path(db)
        .as_system_path()
        .is_none_or(|path| !db.project().is_file_included(db, path))
    {
        // Eagerly clear ASTs of third party files.
        parsed.clear();
    }

    FlatSymbols {
        symbols: visitor.symbols,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, get_size2::GetSize)]
struct SymbolTree {
    parent: Option<SymbolId>,
    name: String,
    kind: SymbolKind,
    name_range: TextRange,
    full_range: TextRange,
}

/// A visitor over all symbols in a single file.
///
/// This guarantees that child symbols have a symbol ID greater
/// than all of its parents.
struct SymbolVisitor {
    symbols: IndexVec<SymbolId, SymbolTree>,
    symbol_stack: Vec<SymbolId>,
    /// Track if we're currently inside a function (to exclude local variables)
    in_function: bool,
    global_only: bool,
}

impl SymbolVisitor {
    fn visit_body(&mut self, body: &[ast::Stmt]) {
        for stmt in body {
            self.visit_stmt(stmt);
        }
    }

    fn add_symbol(&mut self, mut symbol: SymbolTree) -> SymbolId {
        if let Some(&parent_id) = self.symbol_stack.last() {
            symbol.parent = Some(parent_id);
        }
        // It's important that we push the symbol and allocate
        // an ID before visiting its child. This preserves the
        // guarantee that parent IDs are always less than their
        // children IDs.
        let symbol_id = self.symbols.next_index();
        self.symbols.push(symbol);
        symbol_id
    }

    fn add_assignment(&mut self, stmt: &ast::Stmt, name: &ast::ExprName) -> SymbolId {
        let kind = if Self::is_constant_name(name.id.as_str()) {
            SymbolKind::Constant
        } else if self
            .iter_symbol_stack()
            .any(|s| s.kind == SymbolKind::Class)
        {
            SymbolKind::Field
        } else {
            SymbolKind::Variable
        };

        let symbol = SymbolTree {
            parent: None,
            name: name.id.to_string(),
            kind,
            name_range: name.range(),
            full_range: stmt.range(),
        };
        self.add_symbol(symbol)
    }

    fn push_symbol(&mut self, symbol: SymbolTree) {
        let symbol_id = self.add_symbol(symbol);
        self.symbol_stack.push(symbol_id);
    }

    fn pop_symbol(&mut self) {
        self.symbol_stack.pop().unwrap();
    }

    fn iter_symbol_stack(&self) -> impl Iterator<Item = &SymbolTree> {
        self.symbol_stack
            .iter()
            .copied()
            .map(|id| &self.symbols[id])
    }

    fn is_constant_name(name: &str) -> bool {
        name.chars().all(|c| c.is_ascii_uppercase() || c == '_')
    }
}

impl SourceOrderVisitor<'_> for SymbolVisitor {
    fn visit_stmt(&mut self, stmt: &ast::Stmt) {
        match stmt {
            ast::Stmt::FunctionDef(func_def) => {
                let kind = if self
                    .iter_symbol_stack()
                    .any(|s| s.kind == SymbolKind::Class)
                {
                    if func_def.name.as_str() == "__init__" {
                        SymbolKind::Constructor
                    } else {
                        SymbolKind::Method
                    }
                } else {
                    SymbolKind::Function
                };

                let symbol = SymbolTree {
                    parent: None,
                    name: func_def.name.to_string(),
                    kind,
                    name_range: func_def.name.range(),
                    full_range: stmt.range(),
                };

                if self.global_only {
                    self.add_symbol(symbol);
                    // If global_only, don't walk function bodies
                    return;
                }

                self.push_symbol(symbol);

                // Mark that we're entering a function scope
                let was_in_function = self.in_function;
                self.in_function = true;

                source_order::walk_stmt(self, stmt);

                // Restore the previous function scope state
                self.in_function = was_in_function;

                self.pop_symbol();
            }
            ast::Stmt::ClassDef(class_def) => {
                let symbol = SymbolTree {
                    parent: None,
                    name: class_def.name.to_string(),
                    kind: SymbolKind::Class,
                    name_range: class_def.name.range(),
                    full_range: stmt.range(),
                };

                if self.global_only {
                    self.add_symbol(symbol);
                    // If global_only, don't walk class bodies
                    return;
                }

                self.push_symbol(symbol);
                source_order::walk_stmt(self, stmt);
                self.pop_symbol();
            }
            ast::Stmt::Assign(assign) => {
                // Include assignments only when we're in global or class scope
                if self.in_function {
                    return;
                }
                for target in &assign.targets {
                    let ast::Expr::Name(name) = target else {
                        continue;
                    };
                    self.add_assignment(stmt, name);
                }
            }
            ast::Stmt::AnnAssign(ann_assign) => {
                // Include assignments only when we're in global or class scope
                if self.in_function {
                    return;
                }
                let ast::Expr::Name(name) = &*ann_assign.target else {
                    return;
                };
                self.add_assignment(stmt, name);
            }
            _ => {
                source_order::walk_stmt(self, stmt);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use camino::Utf8Component;
    use insta::internals::SettingsBindDropGuard;

    use ruff_db::Db;
    use ruff_db::files::{FileRootKind, system_path_to_file};
    use ruff_db::system::{DbWithWritableSystem, SystemPath, SystemPathBuf};
    use ruff_python_trivia::textwrap::dedent;
    use ty_project::{ProjectMetadata, TestDb};

    use super::symbols_for_file_global_only;

    #[test]
    fn various_yes() {
        assert!(matches("", ""));
        assert!(matches("", "a"));
        assert!(matches("", "abc"));

        assert!(matches("a", "a"));
        assert!(matches("a", "abc"));
        assert!(matches("a", "xaz"));
        assert!(matches("a", "xza"));

        assert!(matches("abc", "abc"));
        assert!(matches("abc", "axbyc"));
        assert!(matches("abc", "waxbycz"));
        assert!(matches("abc", "WAXBYCZ"));
        assert!(matches("ABC", "waxbycz"));
        assert!(matches("ABC", "WAXBYCZ"));
        assert!(matches("aBc", "wAXbyCZ"));

        assert!(matches("δ", "Δ"));
        assert!(matches("δΘπ", "ΔθΠ"));
    }

    #[test]
    fn various_no() {
        assert!(!matches("a", ""));
        assert!(!matches("abc", "bac"));
        assert!(!matches("abcd", "abc"));
        assert!(!matches("δΘπ", "θΔΠ"));
    }

    #[test]
    fn exports_simple() {
        insta::assert_snapshot!(
            public_test("\
FOO = 1
foo = 1
frob: int = 1
class Foo:
    BAR = 1
def quux():
    baz = 1
").exports(),
            @r"
        FOO :: Constant
        foo :: Variable
        frob :: Variable
        Foo :: Class
        quux :: Function
        ",
        );
    }

    #[test]
    fn exports_conditional_true() {
        insta::assert_snapshot!(
            public_test("\
foo = 1
if True:
    bar = 1
").exports(),
            @r"
        foo :: Variable
        bar :: Variable
        ",
        );
    }

    #[test]
    fn exports_conditional_false() {
        // FIXME: This shouldn't include `bar`.
        insta::assert_snapshot!(
            public_test("\
foo = 1
if False:
    bar = 1
").exports(),
            @r"
        foo :: Variable
        bar :: Variable
        ",
        );
    }

    #[test]
    fn exports_conditional_sys_version() {
        // FIXME: This shouldn't include `bar`.
        insta::assert_snapshot!(
            public_test("\
import sys

foo = 1
if sys.version < (3, 5):
    bar = 1
").exports(),
            @r"
        foo :: Variable
        bar :: Variable
        ",
        );
    }

    #[test]
    fn exports_type_checking() {
        insta::assert_snapshot!(
            public_test("\
from typing import TYPE_CHECKING

foo = 1
if TYPE_CHECKING:
    bar = 1
").exports(),
            @r"
        foo :: Variable
        bar :: Variable
        ",
        );
    }

    fn matches(query: &str, symbol: &str) -> bool {
        super::QueryPattern::fuzzy(query).is_match_symbol_name(symbol)
    }

    fn public_test(code: &str) -> PublicTest {
        PublicTestBuilder::default().source("test.py", code).build()
    }

    struct PublicTest {
        db: TestDb,
        _insta_settings_guard: SettingsBindDropGuard,
    }

    impl PublicTest {
        /// Returns the exports from `test.py`.
        ///
        /// This is, conventionally, the default module file path used. For
        /// example, it's used by the `public_test` convenience constructor.
        fn exports(&self) -> String {
            self.exports_for("test.py")
        }

        /// Returns the exports from the module at the given path.
        ///
        /// The path given must have been written to this test's salsa DB.
        fn exports_for(&self, path: impl AsRef<SystemPath>) -> String {
            let file = system_path_to_file(&self.db, path.as_ref()).unwrap();
            let symbols = symbols_for_file_global_only(&self.db, file);
            symbols
                .iter()
                .map(|(_, symbol)| {
                    format!("{name} :: {kind:?}", name = symbol.name, kind = symbol.kind)
                })
                .collect::<Vec<String>>()
                .join("\n")
        }
    }

    #[derive(Default)]
    struct PublicTestBuilder {
        /// A list of source files, corresponding to the
        /// file's path and its contents.
        sources: Vec<Source>,
    }

    impl PublicTestBuilder {
        pub(super) fn build(&self) -> PublicTest {
            let mut db = TestDb::new(ProjectMetadata::new(
                "test".into(),
                SystemPathBuf::from("/"),
            ));

            db.init_program().unwrap();

            for Source { path, contents } in &self.sources {
                db.write_file(path, contents)
                    .expect("write to memory file system to be successful");

                // Add a root for the top-most component.
                let top = path.components().find_map(|c| match c {
                    Utf8Component::Normal(c) => Some(c),
                    _ => None,
                });
                if let Some(top) = top {
                    let top = SystemPath::new(top);
                    if db.system().is_directory(top) {
                        db.files()
                            .try_add_root(&db, top, FileRootKind::LibrarySearchPath);
                    }
                }
            }

            // N.B. We don't set anything custom yet, but we leave
            // this here for when we invevitable add a filter.
            let insta_settings = insta::Settings::clone_current();
            let insta_settings_guard = insta_settings.bind_to_scope();
            PublicTest {
                db,
                _insta_settings_guard: insta_settings_guard,
            }
        }

        pub(super) fn source(
            &mut self,
            path: impl Into<SystemPathBuf>,
            contents: impl AsRef<str>,
        ) -> &mut PublicTestBuilder {
            let path = path.into();
            let contents = dedent(contents.as_ref()).into_owned();
            self.sources.push(Source { path, contents });
            self
        }
    }

    struct Source {
        path: SystemPathBuf,
        contents: String,
    }
}
