#![allow(unreachable_pub)]
#![allow(clippy::used_underscore_binding)]

use std::path::PathBuf;

use salsa::{DebugWithDb, Event, EventKind, Storage};
use tracing::{debug, debug_span, trace, warn};

use crate::db::Upcast;
use crate::salsa_db::source::File;

use self::source::Db as SourceDb;

pub mod lint;
pub mod semantic;
pub mod source;

#[salsa::db(source::Jar, lint::Jar, semantic::Jar)]
pub struct Database {
    storage: Storage<Self>,

    files: source::Files,
}

impl Database {
    #[allow(unused)]
    pub fn new() -> Self {
        Self {
            files: source::Files::default(),
            storage: Storage::default(),
        }
    }
}

impl SourceDb for Database {
    #[tracing::instrument(level = "debug", skip(self))]
    fn file(&self, path: PathBuf) -> File {
        self.files.resolve(self, path)
    }
}

impl semantic::Db for Database {}

impl Upcast<dyn source::Db> for Database {
    fn upcast(&self) -> &(dyn source::Db + 'static) {
        self
    }
}

impl Upcast<dyn semantic::Db> for Database {
    fn upcast(&self) -> &(dyn semantic::Db + 'static) {
        self
    }
}

impl lint::Db for Database {}

impl salsa::Database for Database {
    fn salsa_event(&self, event: Event) {
        if matches!(event.kind, EventKind::WillCheckCancellation) {
            return;
        }

        trace!("event: {:?}", event.debug(self));
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

#[cfg(test)]
mod tests {
    use salsa::{Event, Storage};
    use std::path::PathBuf;
    use tracing::{debug, Level};
    use tracing_subscriber::fmt::time;

    use crate::db::Upcast;
    use crate::salsa_db::semantic::ast_ids::AstIdNode;
    use crate::salsa_db::semantic::module::file_to_module;
    use crate::salsa_db::semantic::types::infer::infer_expression_type;
    use crate::salsa_db::semantic::{
        dependencies, global_symbol_type, resolve_global_symbol, GlobalId,
    };
    use crate::salsa_db::source::{parse, Db, File};

    use super::lint;
    use super::semantic;
    use super::semantic::module::{
        set_module_search_paths, ModuleSearchPath, ModuleSearchPathKind,
    };
    use super::source;
    use super::Database;

    #[salsa::db(source::Jar, lint::Jar, semantic::Jar)]
    pub struct TestDb {
        storage: salsa::Storage<Self>,

        files: source::Files,
        events: std::sync::Mutex<Vec<salsa::Event>>,
    }

    impl TestDb {
        pub fn new() -> Self {
            Self {
                files: source::Files::default(),
                storage: Storage::default(),
                events: std::sync::Mutex::new(Vec::new()),
            }
        }
    }

    impl source::Db for TestDb {
        #[tracing::instrument(level = "debug", skip(self))]
        fn file(&self, path: PathBuf) -> File {
            self.files.resolve(self, path)
        }
    }

    impl semantic::Db for TestDb {}

    impl Upcast<dyn source::Db> for TestDb {
        fn upcast(&self) -> &(dyn source::Db + 'static) {
            self
        }
    }

    impl Upcast<dyn semantic::Db> for TestDb {
        fn upcast(&self) -> &(dyn semantic::Db + 'static) {
            self
        }
    }

    impl lint::Db for TestDb {}

    impl salsa::Database for TestDb {
        fn salsa_event(&self, event: Event) {
            self.events.lock().unwrap().push(event);
        }
    }

    #[allow(clippy::print_stderr)]
    #[test]
    fn inputs() {
        countme::enable(true);
        setup_tracing();

        let tempdir = tempfile::tempdir().unwrap();
        let main = tempdir.path().join("main.py");
        let foo = tempdir.path().join("foo.py");

        std::fs::write(
            &main,
            r#"
import foo;

x = foo.x


def test():
    return x;

def test():
    return y
"#,
        )
        .unwrap();
        std::fs::write(&foo, "x = 10\ndef foo(): y = 10").unwrap();

        let mut db = Database::new();
        set_module_search_paths(
            &mut db,
            vec![ModuleSearchPath::new(
                tempdir.path().to_owned(),
                ModuleSearchPathKind::FirstParty,
            )],
        );

        let main_file = db.file(main.clone());
        let mut foo_file = db.file(foo.clone());

        let parsed = parse(&db, main_file);

        dependencies(&db, main_file);
        let main_module = file_to_module(&db, main_file).unwrap();
        let x = resolve_global_symbol(&db, main_module.file(), "x").unwrap();
        debug!("{:#?}", global_symbol_type(&db, x));

        std::fs::write(&foo, "x = 10\ndef foo():\n  y = x").unwrap();
        foo_file.touch(&mut db);

        let x = resolve_global_symbol(&db, main_module.file(), "x").unwrap();
        debug!("{:#?}", global_symbol_type(&db, x));

        // let foo = resolve_global_symbol(&db, main_module.file(), "foo").unwrap();
        //
        // tracing::debug!("{:?}", foo);

        // Make a change that doesn't impact the symbol table
        // main_file.touch(&mut db);

        // dbg!(infer_expression_type(
        //     &db,
        //     GlobalId::new(main_module.file(), x_id)
        // ));
        // tracing::debug!("{:?}", foo);

        eprintln!("{}", countme::get_all());
    }

    fn setup_tracing() {
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
            .with_max_level(Level::DEBUG)
            .with_writer(std::io::stderr)
            // Build the subscriber
            .finish();

        tracing::subscriber::set_global_default(subscriber).unwrap();
    }
}
