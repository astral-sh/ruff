#![cfg(feature = "inventory")]

/// Test that high durabilities can't cause "access tracked struct from previous revision" panic.
///
/// The test models a situation where we have two File inputs (0, 1), where `File(0)` has LOW
/// durability and `File(1)` has HIGH durability. We can query an `index` for each file, and a
/// `definitions` from that index (just a sub-part of the index), and we can `infer` each file. The
/// `index` and `definitions` queries depend only on the `File` they operate on, but the `infer`
/// query has some other dependencies: `infer(0)` depends on `infer(1)`, and `infer(1)` also
/// depends directly on `File(0)`.
///
/// The panic occurs (in versions of Salsa without a fix) because `definitions(1)` is high
/// durability, and depends on `index(1)` which is also high durability. `index(1)` creates the
/// tracked struct `Definition(1)`, and `infer(1)` (which is low durability) depends on
/// `Definition.file(1)`.
///
/// After a change to `File(0)` (low durability), we only shallowly verify `definitions(1)` -- it
/// passes shallow verification due to durability. We take care to mark-validated the outputs of
/// `definitions(1)`, but we never verify `index(1)` at all (deeply or shallowly), which means we
/// never mark `Definition(1)` validated. So when we deep-verify `infer(1)`, we try to access its
/// dependency `Definition.file(1)`, and hit the panic because we are accessing a tracked struct
/// that has never been re-validated or re-recreated in R2.
use salsa::{Durability, Setter};

#[salsa::db]
trait Db: salsa::Database {
    fn file(&self, idx: usize) -> File;
}

#[salsa::input]
struct File {
    #[returns(copy)]
    field: usize,
}

#[salsa::tracked]
struct Definition<'db> {
    #[tracked]
    #[returns(copy)]
    file: File,
}

#[salsa::tracked]
struct Index<'db> {
    #[tracked]
    #[returns(copy)]
    definitions: Definitions<'db>,
}

#[salsa::tracked]
struct Definitions<'db> {
    #[tracked]
    #[returns(copy)]
    definition: Definition<'db>,
}

#[salsa::tracked]
struct Inference<'db> {
    #[tracked]
    #[returns(copy)]
    definition: Definition<'db>,
}

#[salsa::tracked(returns(copy))]
fn index(db: &dyn Db, file: File) -> Index<'_> {
    let _ = file.field(db);
    Index::new(db, Definitions::new(db, Definition::new(db, file)))
}

#[salsa::tracked(returns(copy))]
fn definitions(db: &dyn Db, file: File) -> Definitions<'_> {
    index(db, file).definitions(db)
}

#[salsa::tracked(returns(copy))]
fn infer<'db>(db: &'db dyn Db, definition: Definition<'db>) -> Inference<'db> {
    let file = definition.file(db);
    if file.field(db) < 1 {
        let dependent_file = db.file(1);
        infer(db, definitions(db, dependent_file).definition(db))
    } else {
        db.file(0).field(db);
        index(db, file);
        Inference::new(db, definition)
    }
}

#[salsa::tracked(returns(copy))]
fn check(db: &dyn Db, file: File) -> Inference<'_> {
    let defs = definitions(db, file);
    infer(db, defs.definition(db))
}

#[test]
fn execute() {
    #[salsa::db]
    #[derive(Default, Clone)]
    struct Database {
        storage: salsa::Storage<Self>,
        files: Vec<File>,
    }

    #[salsa::db]
    impl salsa::Database for Database {}

    #[salsa::db]
    impl Db for Database {
        fn file(&self, idx: usize) -> File {
            self.files[idx]
        }
    }

    let mut db = Database::default();
    // Create a file 0 with low durability, and a file 1 with high durability.

    let file0 = File::new(&db, 0);
    db.files.push(file0);

    let file1 = File::new(&db, 1);
    file1
        .set_field(&mut db)
        .with_durability(Durability::HIGH)
        .to(1);
    db.files.push(file1);

    // check(0) -> infer(0) -> definitions(0) -> index(0)
    //                     \-> infer(1) -> definitions(1) -> index(1)

    assert_eq!(check(&db, file0).definition(&db).file(&db).field(&db), 1);

    // update the low durability file 0
    file0.set_field(&mut db).to(0);

    // Re-query check(0). definitions(1) is high durability so it short-circuits in shallow-verify,
    // meaning we never verify index(1) at all, but index(1) created the tracked struct
    // Definition(1), so we never validate Definition(1) in R2, so when we try to verify
    // Definition.file(1) (as an input of infer(1) ) we hit a panic for trying to use a struct that
    // isn't validated in R2.
    check(&db, file0);
}
