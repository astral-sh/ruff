use std::path::PathBuf;
use std::sync::Arc;

use log::warn;
use parking_lot::lock_api::RwLockUpgradableReadGuard;
use parking_lot::RwLock;
use rustc_hash::FxHashMap;
use salsa::Event;

use ruff_python_ast::visitor::preorder::{PreorderVisitor, TraversalSignal};
use ruff_python_ast::visitor::{preorder, Visitor};
use ruff_python_ast::{AnyNodeRef, Expr, Mod, ModModule, Stmt, StmtExpr};
use ruff_python_parser::Mode;
use ruff_text_size::{Ranged, TextRange};

use crate::ast_ids::AstIds;
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
    parse,
    dependencies,
    check_syntax,
    check_physical_lines,
    ast_ids,
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
    files: Files,
}

impl Database {
    pub fn new(files: Files) -> Self {
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
        log::debug!("{:#?}", event);
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
pub fn dependencies(db: &dyn Db, source_text: SourceText) -> Arc<Vec<Dependency>> {
    let parsed = parse(db, source_text);

    let mut visitor = DependenciesVisitor {
        module_path: db
            .files()
            .path(source_text.file(db))
            .parent()
            .map_or_else(PathBuf::new, std::borrow::ToOwned::to_owned),
        dependencies: Vec::new(),
    };

    // TODO change the visitor so that `visit_mod` accepts a `ModRef` node that we can construct from module.
    visitor.visit_body(&parsed.ast(db).body);

    Arc::new(visitor.dependencies)

    // TODO we should extract the names of dependencies during parsing to avoid an extra traversal here.
}

struct DependenciesVisitor {
    module_path: PathBuf,
    dependencies: Vec<Dependency>,
}

impl DependenciesVisitor {
    fn push_dependency(&mut self, path: PathBuf) {
        // TODO handle error case by pushing a diagnostic?
        let joined = self.module_path.join(path);
        if let Ok(normalized) = joined.canonicalize() {
            self.dependencies.push(Dependency { path: normalized });
        } else {
            warn!("Could not canonicalize path: {:?}", joined);
        }
    }
}

// TODO support package imports
impl PreorderVisitor<'_> for DependenciesVisitor {
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
                    let path: PathBuf = alias.name.split('.').collect();

                    self.push_dependency(path.with_extension("py"));
                }
            }

            Stmt::ImportFrom(from) => {
                if let Some(module) = &from.module {
                    let path: PathBuf = module.split('.').collect();
                    self.push_dependency(path.with_extension("py"));
                } else if let Some(level) = &from.level {
                    let path: PathBuf = (0..*level).map(|_| "..").collect();
                    self.push_dependency(path.with_extension("py"));
                } else {
                    // Should never happen, let's assume we didn't see it.
                }
            }
            _ => {}
        }
        preorder::walk_stmt(self, stmt);
    }
}

#[derive(Eq, PartialEq, Hash, Clone, Debug)]
pub struct Dependency {
    // A relative path from the current module to the dependency
    pub path: PathBuf,
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

#[salsa::tracked(jar=SourceJar)]
pub fn ast_ids(db: &dyn Db, source: SourceText) -> Arc<AstIds> {
    let parsed = parse(db, source);
    let ast = parsed.ast(db);

    Arc::new(AstIds::from_module(ast))
}
