#![allow(unreachable_pub)]
#![allow(unused)]

use std::fmt::Formatter;
use std::path::PathBuf;
use std::sync::Arc;

use countme::Count;
use dashmap::mapref::entry::Entry;
use filetime::FileTime;
use rustc_hash::FxHashMap;
use salsa::database::AsSalsaDatabase;
use salsa::{DebugWithDb, Event};
use tracing::{debug, debug_span, warn, Level};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{Layer, Registry};
use tracing_tree::time::Uptime;

use ruff_python_ast::visitor::preorder::{PreorderVisitor, TraversalSignal};
use ruff_python_ast::visitor::{preorder, walk_stmt, Visitor};
use ruff_python_ast::{AnyNodeRef, Expr, Mod, ModModule, Stmt, StmtExpr};
use ruff_python_parser::Mode;
use ruff_text_size::{Ranged, TextRange};

use crate::ast_ids::AstIds;
use crate::files::{FileId, Files};
use crate::module::{ModuleKind, ModuleSearchPath};
use crate::salsa_db::source::{File, SourceText};
use crate::FxDashMap;

use self::source::{Db as SourceDb, Jar as SourceJar};

pub mod source {
    use std::path::PathBuf;
    use std::sync::Arc;

    use countme::Count;
    use dashmap::mapref::entry::Entry;
    use filetime::FileTime;

    use ruff_python_ast::{Mod, ModModule, Stmt, StmtExpr};
    use ruff_python_parser::Mode;
    use ruff_text_size::{Ranged, TextRange};

    use crate::FxDashMap;

    #[salsa::input(jar=Jar)]
    pub struct File {
        #[return_ref]
        pub path: PathBuf,

        pub permissions: u32,
        pub last_modified_time: FileTime,
        _count: Count<File>,
    }

    impl File {
        #[tracing::instrument(level = "debug", skip(db))]
        pub fn touch(&self, db: &mut dyn Db) {
            let path = self.path(db);
            let metadata = path.metadata().unwrap();
            let last_modified = filetime::FileTime::from_last_modification_time(&metadata);

            self.set_last_modified_time(db).to(last_modified);
        }
    }

    #[salsa::tracked(jar=Jar)]
    impl File {
        #[salsa::tracked]
        pub fn source(self, db: &dyn Db) -> SourceText {
            let _ = self.last_modified_time(db); // Read the last modified date to trigger a re-run when the file changes.
            let text = std::fs::read_to_string(self.path(db)).unwrap_or_default();

            SourceText {
                text: Arc::new(text),
                count: Count::default(),
            }
        }
    }

    #[derive(Debug, Clone, Default)]
    pub struct Files {
        inner: Arc<FilesInner>,
    }

    impl Files {
        pub(super) fn resolve(&self, db: &dyn Db, path: PathBuf) -> File {
            match self.inner.by_path.entry(path.clone()) {
                Entry::Occupied(entry) => {
                    let file = entry.get();
                    *file
                }
                Entry::Vacant(entry) => {
                    let metadata = path.metadata();
                    let (last_modified, permissions) = if let Ok(metadata) = metadata {
                        let last_modified =
                            filetime::FileTime::from_last_modification_time(&metadata);
                        #[cfg(unix)]
                        let permissions = if cfg!(unix) {
                            use std::os::unix::fs::PermissionsExt;
                            metadata.permissions().mode()
                        } else {
                            0
                        };

                        (last_modified, permissions)
                    } else {
                        (FileTime::zero(), 0)
                    };

                    // TODO: How to set the durability?

                    let file = File::new(db, path, permissions, last_modified, Count::default());
                    entry.insert(file);

                    file
                }
            }
        }
    }

    #[derive(Debug, Default)]
    struct FilesInner {
        by_path: FxDashMap<PathBuf, File>,
    }

    // TODO salsa recommends to have one jar per crate and call it `Jar`. We're not doing this here
    // because I don't want that many crates just yet.
    #[derive(Debug, Clone, Eq, PartialEq)]
    pub struct SourceText {
        pub text: Arc<String>,
        count: Count<SourceText>,
    }

    impl SourceText {
        pub fn text(&self) -> &str {
            self.text.as_str()
        }
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct Parsed {
        inner: Arc<ParsedInner>,
    }

    impl Parsed {
        pub fn ast(&self) -> &ModModule {
            &self.inner.ast
        }

        pub fn errors(&self) -> &[ruff_python_parser::ParseError] {
            &self.inner.errors
        }
    }

    #[derive(Debug, PartialEq)]
    struct ParsedInner {
        // TODO should this be an arc to avoid some lifetime awkwardness for call-sites.
        pub ast: ModModule,

        // TODO use an accumulator for this?
        pub errors: Vec<ruff_python_parser::ParseError>,
    }

    #[tracing::instrument(level = "debug", skip(db))]
    #[salsa::tracked(jar=Jar, no_eq)]
    pub fn parse(db: &dyn Db, file: File) -> Parsed {
        let source = file.source(db);
        let text = source.text();

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

        Parsed {
            inner: Arc::new(ParsedInner {
                ast: module,
                errors,
            }),
        }
    }

    #[salsa::jar(db=Db)]
    pub struct Jar(File, File_source, parse);

    pub trait Db: salsa::DbWithJar<Jar> {
        fn file(&self, path: PathBuf) -> File;
    }
}

#[salsa::tracked(jar=SemanticJar)]
pub struct SyntaxCheck {
    #[returned_ref]
    pub diagnostics: Vec<String>,
}

#[salsa::tracked(jar=SemanticJar)]
pub struct PhysicalLinesCheck {
    #[returned_ref]
    pub diagnostics: Vec<String>,
}

#[salsa::input(jar=SemanticJar, singleton)]
pub struct ModuleSearchPaths {
    #[return_ref]
    paths: Vec<ModuleSearchPath>,
}

#[salsa::interned(jar=SemanticJar)]
pub struct ModuleName {
    name: smol_str::SmolStr,
}

impl ModuleName {
    pub fn components(&self, db: &dyn SemanticDb) -> Vec<String> {
        let name = self.name(db);
        name.split(".").map(String::from).collect()
    }
}

// TODO should this be tracked or not?
// I think yes, so that we can reference to it using ids.
#[salsa::tracked(jar=SemanticJar)]
pub struct Module {
    name: ModuleName,
    file: File,
}

#[salsa::tracked(jar=SemanticJar)]
pub struct Symbol {
    #[id]
    #[returned_ref]
    pub name: smol_str::SmolStr,

    count: Count<Symbol>,
}

#[derive(Eq, PartialEq, Default)]
pub struct SymbolTable {
    symbols: FxHashMap<smol_str::SmolStr, Symbol>,
}

impl SymbolTable {
    fn insert(&mut self, name: smol_str::SmolStr, symbol: Symbol) {
        self.symbols.insert(name, symbol);
    }
}

impl std::fmt::Debug for SymbolTable {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.symbols.fmt(f)
    }
}

impl<Db> DebugWithDb<Db> for SymbolTable
where
    Db: AsSalsaDatabase + SemanticDb,
{
    fn fmt(&self, f: &mut Formatter<'_>, db: &Db) -> std::fmt::Result {
        let mut map = f.debug_map();

        for (name, symbol) in &self.symbols {
            map.entry(name, &symbol.debug(db));
        }
        map.finish()
    }
}

#[salsa::jar(db=SemanticDb)]
pub struct SemanticJar(
    SyntaxCheck,
    PhysicalLinesCheck,
    ModuleSearchPaths,
    Symbol,
    Module,
    ModuleName,
    dependencies,
    symbol_table,
    check_syntax,
    check_physical_lines,
    ast_ids,
    resolve_module,
);

pub trait SemanticDb: source::Db + salsa::DbWithJar<SemanticJar> {
    fn upcast(&self) -> &dyn source::Db;
}

pub trait Db: SemanticDb {
    fn upcast(&self) -> &dyn SemanticDb;
}

#[salsa::db(self::SourceJar, self::SemanticJar)]
pub struct Database {
    storage: salsa::Storage<Self>,

    files: source::Files,
}

impl Database {
    pub fn new() -> Self {
        Self {
            files: source::Files::default(),
            storage: Default::default(),
        }
    }
}

impl SourceDb for Database {
    #[tracing::instrument(level = "debug", skip(self))]
    fn file(&self, path: PathBuf) -> File {
        self.files.resolve(self, path)
    }
}

impl SemanticDb for Database {
    fn upcast(&self) -> &dyn SourceDb {
        self
    }
}

impl Db for Database {
    fn upcast(&self) -> &dyn SemanticDb {
        self
    }
}

impl salsa::Database for Database {
    fn salsa_event(&self, event: Event) {
        let _ = debug_span!("event", "{:?}", event.debug(self));
    }
}

impl salsa::ParallelDatabase for Database {
    fn snapshot(&self) -> salsa::Snapshot<Self> {
        salsa::Snapshot::new(Database {
            storage: self.storage.snapshot(),

            // This is ok, because files is an arc
            files: self.files.clone(),
        })
    }
}

#[tracing::instrument(level = "debug", skip(db))]
#[salsa::tracked(jar=SemanticJar)]
pub fn dependencies(db: &dyn SemanticDb, file: File) -> Arc<Vec<Module>> {
    struct DependenciesVisitor<'a> {
        db: &'a dyn SemanticDb,
        module_path: PathBuf,
        dependencies: Vec<Module>,
    }

    // TODO support package imports
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
                        if let Some(module) = resolve_module_by_name(self.db, &alias.name) {
                            self.dependencies.push(module);
                        }
                    }
                }

                Stmt::ImportFrom(from) => {
                    if let Some(module) = &from.module {
                        assert_eq!(from.level, 0, "Relative imports not supported");

                        if let Some(module) = resolve_module_by_name(self.db, module) {
                            self.dependencies.push(module);
                        }
                    } else {
                        warn!("Relative imports are not supported");
                    }
                }
                _ => {}
            }
            preorder::walk_stmt(self, stmt);
        }
    }

    let parsed = source::parse(db.upcast(), file);

    let mut visitor = DependenciesVisitor {
        db,
        module_path: file
            .path(db.upcast())
            .parent()
            .map_or_else(PathBuf::new, std::borrow::ToOwned::to_owned),
        dependencies: Vec::new(),
    };

    // TODO change the visitor so that `visit_mod` accepts a `ModRef` node that we can construct from module.
    visitor.visit_body(&parsed.ast().body);

    Arc::new(visitor.dependencies)

    // TODO we should extract the names of dependencies during parsing to avoid an extra traversal here.
}

#[tracing::instrument(level = "debug", skip(db))]
#[salsa::tracked(jar=SemanticJar)]
fn symbol_table(db: &dyn SemanticDb, file_id: File) -> Arc<SymbolTable> {
    struct SymbolTableVisitor<'db> {
        symbols: SymbolTable,
        db: &'db dyn SemanticDb,
    }

    impl PreorderVisitor<'_> for SymbolTableVisitor<'_> {
        fn visit_stmt(&mut self, stmt: &'_ Stmt) {
            match stmt {
                Stmt::Assign(assign) => {
                    for target in &assign.targets {
                        if let Expr::Name(name) = &target {
                            let name = smol_str::SmolStr::new(&name.id);
                            self.symbols
                                .insert(name.clone(), Symbol::new(self.db, name, Count::default()));
                        }
                    }
                }
                _ => {}
            }

            preorder::walk_stmt(self, stmt);
        }
    }

    let parsed = source::parse(db.upcast(), file_id);
    let mut visitor = SymbolTableVisitor {
        db,
        symbols: SymbolTable::default(),
    };

    visitor.visit_body(&parsed.ast().body);

    Arc::new(visitor.symbols)
}

fn resolve_module_by_name(db: &dyn SemanticDb, name: &str) -> Option<Module> {
    let module_name = ModuleName::new(db, name.into());

    resolve_module(db, module_name)
}

#[tracing::instrument(level = "debug", skip(db))]
#[salsa::tracked(jar=SemanticJar)]
fn resolve_module(db: &dyn SemanticDb, name: ModuleName) -> Option<Module> {
    let search_paths = ModuleSearchPaths::get(db);

    for search_path in search_paths.paths(db) {
        let mut components = name.components(db).into_iter();
        let module_name = components.next_back()?;

        match resolve_package(db, search_path, components) {
            Ok(resolved_package) => {
                let mut package_path = resolved_package.path;

                package_path.push(module_name);

                // Must be a `__init__.pyi` or `__init__.py` or it isn't a package.
                let kind = if package_path.is_dir() {
                    package_path.push("__init__");
                    ModuleKind::Package
                } else {
                    ModuleKind::Module
                };

                // TODO Implement full https://peps.python.org/pep-0561/#type-checker-module-resolution-order resolution
                let stub = package_path.with_extension("pyi");
                let stub_file = db.file(stub.clone());

                if stub.is_file() {
                    return Some(Module::new(db, name, stub_file));
                }

                let module = package_path.with_extension("py");
                let module_file = db.file(module.clone());

                if module.is_file() {
                    return Some(Module::new(db, name, module_file));
                }

                // For regular packages, don't search the next search path. All files of that
                // package must be in the same location
                if resolved_package.kind.is_regular_package() {
                    return None;
                }
            }
            Err(parent_kind) => {
                if parent_kind.is_regular_package() {
                    // For regular packages, don't search the next search path.
                    return None;
                }
            }
        }
    }
    None
}

fn resolve_package<'a, I>(
    db: &dyn SemanticDb,
    module_search_path: &ModuleSearchPath,
    components: I,
) -> Result<crate::module::ResolvedPackage, crate::module::PackageKind>
where
    I: Iterator<Item = String>,
{
    let mut package_path = module_search_path.path().to_path_buf();

    // `true` if inside a folder that is a namespace package (has no `__init__.py`).
    // Namespace packages are special because they can be spread across multiple search paths.
    // https://peps.python.org/pep-0420/
    let mut in_namespace_package = false;

    // `true` if resolving a sub-package. For example, `true` when resolving `bar` of `foo.bar`.
    let mut in_sub_package = false;

    // For `foo.bar.baz`, test that `foo` and `baz` both contain a `__init__.py`.
    for folder in components {
        package_path.push(folder);

        let has_init_py = package_path.join("__init__.py").is_file()
            || package_path.join("__init__.pyi").is_file();

        if has_init_py {
            in_namespace_package = false;
        } else if package_path.is_dir() {
            // A directory without an `__init__.py` is a namespace package, continue with the next folder.
            in_namespace_package = true;
        } else if in_namespace_package {
            // Package not found but it is part of a namespace package.
            return Err(crate::module::PackageKind::Namespace);
        } else if in_sub_package {
            // A regular sub package wasn't found.
            return Err(crate::module::PackageKind::Regular);
        } else {
            // We couldn't find `foo` for `foo.bar.baz`, search the next search path.
            return Err(crate::module::PackageKind::Root);
        }

        in_sub_package = true;
    }

    let kind = if in_namespace_package {
        crate::module::PackageKind::Namespace
    } else if in_sub_package {
        crate::module::PackageKind::Regular
    } else {
        crate::module::PackageKind::Root
    };

    Ok(crate::module::ResolvedPackage {
        kind,
        path: package_path,
    })
}

#[tracing::instrument(level = "debug", skip(db))]
#[salsa::tracked(jar=SemanticJar)]
pub fn check_syntax(db: &dyn SemanticDb, file: File) -> SyntaxCheck {
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

    let parsed = source::parse(db.upcast(), file);

    let mut visitor = SyntaxChecker {
        diagnostics: Vec::new(),
    };

    visitor.visit_body(&parsed.ast().body);

    SyntaxCheck::new(db, visitor.diagnostics)
}

#[tracing::instrument(level = "debug", skip(db))]
#[salsa::tracked(jar=SemanticJar)]
pub fn check_physical_lines(db: &dyn SemanticDb, file: File) -> PhysicalLinesCheck {
    let source = file.source(db.upcast());
    let text = source.text();

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

#[tracing::instrument(level = "debug", skip(db))]
#[salsa::tracked(jar=SemanticJar)]
pub fn ast_ids(db: &dyn SemanticDb, file: File) -> Arc<AstIds> {
    let parsed = source::parse(db.upcast(), file);

    Arc::new(AstIds::from_module(parsed.ast()))
}

#[cfg(test)]
mod tests {
    use salsa::storage::HasJar;
    use salsa::DebugWithDb;
    use tracing::{debug, Level};
    use tracing_subscriber::fmt::time;
    use tracing_subscriber::fmt::writer::MakeWriterExt;
    use tracing_subscriber::prelude::__tracing_subscriber_SubscriberExt;
    use tracing_subscriber::Registry;
    use tracing_tree::time::Uptime;

    use crate::module::ModuleSearchPathKind;
    use crate::salsa_db::source::Db;

    use super::{
        dependencies, source, symbol_table, Database, ModuleSearchPath, ModuleSearchPaths, Symbol,
    };

    #[test]
    fn inputs() {
        countme::enable(true);
        setup_tracing();
        // log::set_max_level(LevelFilter::Trace);
        // log::set_logger()

        let tempdir = tempfile::tempdir().unwrap();
        let main = tempdir.path().join("main.py");
        let foo = tempdir.path().join("foo.py");

        std::fs::write(&main, "import foo;\nx = 1").unwrap();
        std::fs::write(&foo, "x = 10").unwrap();

        let mut db = Database::new();
        ModuleSearchPaths::new(
            &mut db,
            vec![ModuleSearchPath::new(
                tempdir.path().to_owned(),
                ModuleSearchPathKind::FirstParty,
            )],
        );

        let main_file = db.file(main.clone());

        dependencies(&db, main_file);
        debug!("{:#?}", &symbol_table(&db, main_file).debug(&db));

        std::fs::write(&main, "print('Hello, Micha!')").unwrap();

        main_file.touch(&mut db);

        // let (source_jar, _): (&mut SourceJar, _) = db.jar_mut();
        // source_jar.0.reset()

        assert_eq!("print('Hello, Micha!')", main_file.source(&db).text());

        debug!("{:#?}", &symbol_table(&db, main_file).debug(&db));

        // The file never gets collected.
        main_file.touch(&mut db);

        // TODO: Is there a way to remove a file?

        // There's only one source alive. I guess that makes sense because we never read the content of `foo.py`.

        eprintln!("{}", countme::get_all());
    }

    fn setup_tracing() {
        // tracing_log::LogTracer::init().unwrap();

        // let subscriber = Registry::default().with(
        //     tracing_tree::HierarchicalLayer::default()
        //         .with_indent_lines(true)
        //         .with_indent_amount(2)
        //         .with_bracketed_fields(true)
        //         .with_thread_ids(true)
        //         .with_targets(true)
        //         // .with_writer(|| Box::new(std::io::stderr()))
        //         .with_timer(Uptime::default()),
        // );
        //
        let subscriber = tracing_subscriber::fmt()
            // Use a more compact, abbreviated log format
            .compact()
            .with_span_events(
                tracing_subscriber::fmt::format::FmtSpan::ENTER
                    | tracing_subscriber::fmt::format::FmtSpan::CLOSE,
            )
            // Display source code file paths
            .with_file(false)
            // Display source code line numbers
            .with_line_number(true)
            // Display the thread ID an event was recorded on
            .with_thread_ids(false)
            .with_timer(time())
            // Don't display the event's target (module path)
            .with_target(true)
            .with_max_level(Level::TRACE)
            .with_writer(std::io::stderr)
            // Build the subscriber
            .finish();

        tracing::subscriber::set_global_default(subscriber).unwrap();
    }
}
