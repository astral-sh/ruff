#![allow(dead_code)]
use crate::db::{HasJar, SemanticDb, SemanticJar};
use crate::module::ModuleName;
use crate::symbols::{Definition, ImportFromDefinition, SymbolId};
use crate::types::Type;
use crate::FileId;
use ruff_python_ast::{AstNode, StmtClassDef};

#[tracing::instrument(level = "trace", skip(db))]
pub fn eval_symbol<Db>(db: &mut Db, file_id: FileId, symbol_id: SymbolId) -> Type
where
    Db: SemanticDb + HasJar<SemanticJar>,
{
    let symbols = db.symbol_table(file_id);
    let defs = symbols.defs(symbol_id);

    if let Some(ty) = db
        .jar()
        .type_store
        .get_cached_symbol_type(file_id, symbol_id)
    {
        return ty;
    }

    // TODO handle multiple defs, conditional defs...
    assert_eq!(defs.len(), 1);

    let ty = match &defs[0] {
        Definition::ImportFrom(ImportFromDefinition {
            module,
            name,
            level,
        }) => {
            // TODO relative imports
            assert!(matches!(level, Some(0)));
            let module_name = ModuleName::new(module.as_ref().expect("TODO relative imports"));
            if let Some(module) = db.resolve_module(module_name) {
                let remote_file_id = module.path(db).file();
                let remote_symbols = db.symbol_table(remote_file_id);
                if let Some(remote_symbol_id) = remote_symbols.root_symbol_id_by_name(name) {
                    db.eval_symbol(remote_file_id, remote_symbol_id)
                } else {
                    Type::Unknown
                }
            } else {
                Type::Unknown
            }
        }
        Definition::ClassDef(node_key) => {
            let node_key = node_key.erased();
            if let Some(ty) = db.jar().type_store.get_cached_node_type(file_id, node_key) {
                ty
            } else {
                let parsed = db.parse(file_id);
                let ast = parsed.ast();
                let node = StmtClassDef::cast_ref(
                    node_key
                        .resolve(ast.as_any_node_ref())
                        .expect("node key should resolve"),
                )
                .expect("node key should cast");

                let store = &mut db.jar_mut().type_store;
                let ty = store.add_class(file_id, &node.name.id);
                store.cache_node_type(file_id, *node_key, ty);
                ty
            }
        }
        _ => todo!("other kinds of definitions"),
    };

    db.jar_mut()
        .type_store
        .cache_symbol_type(file_id, symbol_id, ty);
    // TODO record dependencies
    ty
}

#[cfg(test)]
mod tests {
    use crate::db::tests::TestDb;
    use crate::db::{HasJar, SemanticDb, SemanticJar};
    use crate::module::{ModuleName, ModuleSearchPath, ModuleSearchPathKind};
    use crate::types::Type;

    // TODO with virtual filesystem we shouldn't have to write files to disk for these
    // tests

    struct TestCase {
        temp_dir: tempfile::TempDir,
        db: TestDb,

        src: ModuleSearchPath,
    }

    fn create_test() -> std::io::Result<TestCase> {
        let temp_dir = tempfile::tempdir()?;

        let src = temp_dir.path().join("src");
        std::fs::create_dir(&src)?;
        let src = ModuleSearchPath::new(src.canonicalize()?, ModuleSearchPathKind::FirstParty);

        let roots = vec![src.clone()];

        let mut db = TestDb::default();
        db.set_module_search_paths(roots);

        Ok(TestCase { temp_dir, db, src })
    }

    #[test]
    fn follow_import_to_class() -> std::io::Result<()> {
        let TestCase {
            src,
            mut db,
            temp_dir: _temp_dir,
        } = create_test()?;

        let a_path = src.path().join("a.py");
        let b_path = src.path().join("b.py");
        std::fs::write(a_path, "from b import C as D")?;
        std::fs::write(b_path, "class C: pass")?;
        let a_file = db
            .resolve_module(ModuleName::new("a"))
            .expect("module should be found")
            .path(&db)
            .file();
        let a_syms = db.symbol_table(a_file);
        let d_sym = a_syms
            .root_symbol_id_by_name("D")
            .expect("D symbol should be found");

        let ty = db.eval_symbol(a_file, d_sym);

        let jar = HasJar::<SemanticJar>::jar(&db);

        assert!(matches!(ty, Type::Class(_)));
        assert_eq!(format!("{}", ty.display(&jar.type_store)), "C");
        Ok(())
    }
}
