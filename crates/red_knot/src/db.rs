use std::path::PathBuf;
use std::ptr::NonNull;
use std::sync::Arc;

use parking_lot::lock_api::RwLockUpgradableReadGuard;
use parking_lot::RwLock;
use rustc_hash::FxHashMap;
use salsa::Event;

use ruff_index::IndexVec;
use ruff_python_ast::visitor::preorder::{PreorderVisitor, TraversalSignal};
use ruff_python_ast::visitor::{preorder, Visitor};
use ruff_python_ast::{
    AnyNodeRef, AstNode, Expr, Mod, ModModule, NodeKind, Stmt, StmtClassDef, StmtExpr,
    StmtFunctionDef,
};
use ruff_python_parser::Mode;
use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::files::{FileId, Files};

// TODO salsa recommends to have one jar per crate and call it `Jar`. We're not doing this here
// because I don't want that many crates just yet.
#[salsa::input(jar=SourceJar)]
pub struct SourceText {
    file: FileId,

    #[return_ref]
    text: String,
}

#[salsa::tracked(jar=SourceJar)]
pub struct Parsed {
    // TODO should this be an arc to avoid some lifetime awkwardness for call-sites.
    #[return_ref]
    pub ast: ModModule,

    #[return_ref]
    pub imports: Vec<String>,

    // TODO use an accumulator for this?
    #[return_ref]
    pub errors: Vec<ruff_python_parser::ParseError>,
}

#[salsa::tracked(jar=SourceJar)]
pub struct Dependencies {
    #[returned_ref]
    pub files: Vec<FileId>,
}

#[salsa::tracked(jar=SourceJar)]
pub struct SyntaxCheck {
    #[returned_ref]
    pub diagnostics: Vec<String>,
}

#[salsa::tracked(jar=SourceJar)]
pub struct PhysicalLinesCheck {
    #[returned_ref]
    pub diagnostics: Vec<String>,
}

#[salsa::jar(db=Db)]
pub struct SourceJar(
    SourceText,
    Parsed,
    SyntaxCheck,
    PhysicalLinesCheck,
    Dependencies,
    parse,
    dependencies,
    check_syntax,
    check_physical_lines,
);

pub trait Db: salsa::DbWithJar<SourceJar> {
    // TODO: This function makes the source code lazy. However, it's unclear to me how we can let Salsa know
    //   if a source text changed or how to manually set the source text.
    //
    // TODO There's also the problem that the source text will be retained in memory forever?
    fn source_text(&self, file_id: FileId) -> std::io::Result<SourceText>;

    fn files(&self) -> &Files;
}

#[salsa::db(self::SourceJar)]
pub struct Database {
    storage: salsa::Storage<Self>,

    // can define additional fields
    // TODO how to reuse file ids across runs? Do we want this to be part of salsa or shout the id
    //   mapping happen out side because we don't want to read them from disk every time?
    sources: Arc<RwLock<FxHashMap<FileId, SourceText>>>,
    files: Arc<Files>,
}

impl Database {
    pub fn new(files: Arc<Files>) -> Self {
        Self {
            sources: Arc::new(RwLock::new(FxHashMap::default())),
            files,
            storage: Default::default(),
        }
    }
}

impl Db for Database {
    fn source_text(&self, file_id: FileId) -> std::io::Result<SourceText> {
        let lock = self.sources.upgradable_read();

        if let Some(source) = lock.get(&file_id) {
            return Ok(*source);
        }

        let mut upgraded = RwLockUpgradableReadGuard::upgrade(lock);

        let path = self.files.path(file_id);
        let file = SourceText::new(self, file_id, std::fs::read_to_string(path)?);

        upgraded.insert(file_id, file);

        Ok(file)
    }

    fn files(&self) -> &Files {
        &self.files
    }
}

impl salsa::Database for Database {
    fn salsa_event(&self, event: Event) {
        &event;
    }
}

impl salsa::ParallelDatabase for Database {
    fn snapshot(&self) -> salsa::Snapshot<Self> {
        salsa::Snapshot::new(Database {
            storage: self.storage.snapshot(),

            sources: self.sources.clone(),
            // This is ok, because files is an arc
            files: self.files.clone(),
        })
    }
}

#[salsa::tracked(jar=SourceJar)]
pub fn parse(db: &dyn Db, source: SourceText) -> Parsed {
    let text = source.text(db);

    let result = ruff_python_parser::parse(text, Mode::Module);

    let (module, errors) = match result {
        Ok(Mod::Module(module)) => (module, vec![]),
        Ok(Mod::Expression(expression)) => (
            ModModule {
                range: expression.range(),
                body: vec![Stmt::Expr(StmtExpr {
                    range: expression.range(),
                    value: expression.body,
                })],
            },
            vec![],
        ),
        Err(errors) => (
            ModModule {
                range: TextRange::default(),
                body: Vec::new(),
            },
            vec![errors],
        ),
    };

    Parsed::new(db, module, Vec::new(), errors)
}

#[salsa::tracked(jar=SourceJar)]
pub fn dependencies(db: &dyn Db, source_text: SourceText) -> Dependencies {
    let parsed = parse(db, source_text);

    let mut visitor = DependenciesVisitor {
        db,
        // FIXME I think using files here is wrong. It leads to non-deterministic results.
        //  We should change files back to not use internal-mutability and only return the dependency paths from here.
        //  It's up to the caller to resolve the dependencies from path to file-ids.k
        base_path: db
            .files()
            .path(source_text.file(db))
            .parent()
            .map_or_else(PathBuf::new, std::borrow::ToOwned::to_owned),
        dependencies: Vec::new(),
    };

    // TODO change the visitor so that `visit_mod` accepts a `ModRef` node that we can construct from module.
    visitor.visit_body(&parsed.ast(db).body);

    Dependencies::new(db, visitor.dependencies)

    // TODO we should extract the names of dependencies during parsing to avoid an extra traversal here.
}

struct DependenciesVisitor<'a> {
    db: &'a dyn Db,
    base_path: PathBuf,
    dependencies: Vec<FileId>,
}

impl PreorderVisitor<'_> for DependenciesVisitor<'_> {
    fn enter_node(&mut self, node: AnyNodeRef) -> TraversalSignal {
        // Don't traverse into expressions
        if node.is_expression() {
            return TraversalSignal::Skip;
        }

        TraversalSignal::Traverse
    }

    fn visit_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Import(import) => {
                for alias in &import.names {
                    let mut path = self.base_path.clone();
                    for part in alias.name.split('.') {
                        path.push(part);
                    }

                    path = path.with_extension("py");
                    let id = self.db.files().intern(&path);

                    self.dependencies.push(id);
                }
            }

            Stmt::ImportFrom(from) => {
                if let Some(module) = &from.module {
                    let mut path = self.base_path.clone();
                    for part in module.split('.') {
                        path.push(part);
                    }

                    path = path.with_extension("py");
                    let id = self.db.files().intern(&path);

                    self.dependencies.push(id);
                } else if let Some(level) = &from.level {
                    let mut path = self.base_path.clone();
                    for _ in 0..*level {
                        path.pop();
                    }

                    path = path.with_extension("py");
                    let id = self.db.files().intern(&path);

                    self.dependencies.push(id);
                } else {
                    // Should never happen, let's assume we didn't see it.
                }
            }
            _ => {}
        }
        preorder::walk_stmt(self, stmt);
    }
}

// TODO it's unclear to me if the function should accept a parsed or a source text?
//   Is it best practice to inline as many db calls or should we ask the caller to do the db calls?
#[salsa::tracked(jar=SourceJar)]
pub fn check_syntax(db: &dyn Db, parsed: Parsed) -> SyntaxCheck {
    // TODO I haven't looked into how many rules are pure syntax checks.
    //   It may be necessary to at least give access to a simplified semantic model.
    struct SyntaxChecker {
        diagnostics: Vec<String>,
    }

    impl Visitor<'_> for SyntaxChecker {
        fn visit_expr(&mut self, expr: &'_ Expr) {
            if let Expr::Name(name) = expr {
                if &name.id == "a" {
                    self.diagnostics.push("Use of name a".to_string());
                }
            }
        }
    }

    let mut visitor = SyntaxChecker {
        diagnostics: Vec::new(),
    };
    visitor.visit_body(&parsed.ast(db).body);

    SyntaxCheck::new(db, visitor.diagnostics)
}

#[salsa::tracked(jar=SourceJar)]
pub fn check_physical_lines(db: &dyn Db, source_text: SourceText) -> PhysicalLinesCheck {
    let text = source_text.text(db);

    let mut diagnostics = Vec::new();
    let mut line_number = 0u32;
    for line in text.lines() {
        if line.chars().count() > 88 {
            diagnostics.push(format!("Line {} too long", line_number + 1));
        }
        line_number += 1;
    }

    PhysicalLinesCheck::new(db, diagnostics)
}

#[derive(Copy, Clone, Eq, PartialEq, Hash)]
pub struct HirAstId {
    file_id: FileId,
    node_id: AstId,
}

#[ruff_index::newtype_index]
pub struct AstId;

// TODO THis is now something that doesn't work well with Ruff's AST because the reverse map requires lifetimes because
//  cloning the nodes would be silly.
pub struct AstIds {
    ids: IndexVec<AstId, NodeKey>,
    reverse: FxHashMap<NodeKey, AstId>,
}

impl AstIds {
    pub fn from_module(module: &ModModule) -> Self {
        let mut visitor = AstIdsVisitor::default();

        // TODO: visit_module?
        visitor.visit_body(&module.body);

        while let Some(deferred) = visitor.deferred.pop() {
            match deferred {
                DeferredNode::FunctionDefinition(def) => {
                    def.visit_preorder(&mut visitor);
                }
                DeferredNode::ClassDefinition(def) => def.visit_preorder(&mut visitor),
            }
        }

        AstIds {
            ids: visitor.ids,
            reverse: visitor.reverse,
        }
    }

    pub fn get(&self, node: &NodeKey) -> Option<AstId> {
        self.reverse.get(node).copied()
    }
}

#[derive(Default)]
struct AstIdsVisitor<'a> {
    ids: IndexVec<AstId, NodeKey>,
    reverse: FxHashMap<NodeKey, AstId>,
    deferred: Vec<DeferredNode<'a>>,
}

impl<'a> AstIdsVisitor<'a> {
    fn push<A: Into<AnyNodeRef<'a>>>(&mut self, node: A) {
        let node = node.into();
        let node_key = NodeKey {
            kind: node.kind(),
            range: node.range(),
        };
        let id = self.ids.push(node_key);
        self.reverse.insert(node_key, id);
    }
}

impl<'a> PreorderVisitor<'a> for AstIdsVisitor<'a> {
    fn enter_node(&mut self, node: AnyNodeRef<'a>) -> TraversalSignal {
        self.push(node);
        TraversalSignal::Traverse
    }
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::FunctionDef(def) => {
                self.deferred.push(DeferredNode::FunctionDefinition(def));
            }
            // TODO defer visiting the assignment body, type alias parameters etc?
            Stmt::ClassDef(def) => {
                self.deferred.push(DeferredNode::ClassDefinition(def));
            }
            _ => preorder::walk_stmt(self, stmt),
        }
    }
}

enum DeferredNode<'a> {
    FunctionDefinition(&'a StmtFunctionDef),
    ClassDefinition(&'a StmtClassDef),
}

// TODO an alternative to this is to have a `NodeId` on each node (in increasing order depending on the position).
//  This would allow to reduce the size of this to a u32.
//  What would be nice if we could use an `Arc::weak_ref` here but that only works if we use
//   `Arc` internally
// TODO: Implement the logic to resolve a node, given a db (and the correct file).
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct NodeKey {
    kind: NodeKind,
    range: TextRange,
}

// ref counted
// struct GreenNode {
//     len: TextSize,
//     kind: NodeKind,
//     children: Vec<GreenElement> // GreenElement which can either be a Token or Node
// }

// enum GreenElement {
//     Node(GreenNode),
//     Token(GreenToken)
// }

// struct GreenToken {
//     len: TextSize,
//     kind: TokenKind,
//     content: String,
// }

// // ref counted, red nodes
// struct SyntaxNode {
//     offset: TextSize,
//     parent: Option<GreenNode>, // upward pointer
//     node: GreenNode,
// }
