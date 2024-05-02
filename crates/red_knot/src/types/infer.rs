#![allow(dead_code)]

use ruff_python_ast::AstNode;

use crate::db::{HasJar, QueryResult, SemanticDb, SemanticJar};
use crate::module::ModuleName;
use crate::symbols::{Definition, ImportFromDefinition, SymbolId};
use crate::types::Type;
use crate::FileId;
use ruff_python_ast as ast;

// FIXME: Figure out proper dead-lock free synchronisation now that this takes `&db` instead of `&mut db`.
#[tracing::instrument(level = "trace", skip(db))]
pub fn infer_symbol_type<Db>(db: &Db, file_id: FileId, symbol_id: SymbolId) -> QueryResult<Type>
where
    Db: SemanticDb + HasJar<SemanticJar>,
{
    let symbols = db.symbol_table(file_id)?;
    let defs = symbols.definitions(symbol_id);

    if let Some(ty) = db
        .jar()?
        .type_store
        .get_cached_symbol_type(file_id, symbol_id)
    {
        return Ok(ty);
    }

    // TODO handle multiple defs, conditional defs...
    assert_eq!(defs.len(), 1);
    let type_store = &db.jar()?.type_store;

    let ty = match &defs[0] {
        Definition::ImportFrom(ImportFromDefinition {
            module,
            name,
            level,
        }) => {
            // TODO relative imports
            assert!(matches!(level, 0));
            let module_name = ModuleName::new(module.as_ref().expect("TODO relative imports"));
            if let Some(module) = db.resolve_module(module_name)? {
                let remote_file_id = module.path(db)?.file();
                let remote_symbols = db.symbol_table(remote_file_id)?;
                if let Some(remote_symbol_id) = remote_symbols.root_symbol_id_by_name(name) {
                    db.infer_symbol_type(remote_file_id, remote_symbol_id)?
                } else {
                    Type::Unknown
                }
            } else {
                Type::Unknown
            }
        }
        Definition::ClassDef(node_key) => {
            if let Some(ty) = type_store.get_cached_node_type(file_id, node_key.erased()) {
                ty
            } else {
                let parsed = db.parse(file_id)?;
                let ast = parsed.ast();
                let node = node_key.resolve_unwrap(ast.as_any_node_ref());

                let mut bases = Vec::with_capacity(node.bases().len());

                for base in node.bases() {
                    bases.push(infer_expr_type(db, file_id, base)?);
                }

                let ty = Type::Class(type_store.add_class(file_id, &node.name.id, bases));
                type_store.cache_node_type(file_id, *node_key.erased(), ty);
                ty
            }
        }
        Definition::FunctionDef(node_key) => {
            if let Some(ty) = type_store.get_cached_node_type(file_id, node_key.erased()) {
                ty
            } else {
                let parsed = db.parse(file_id)?;
                let ast = parsed.ast();
                let node = node_key
                    .resolve(ast.as_any_node_ref())
                    .expect("node key should resolve");

                let decorator_tys: Vec<_> = node
                    .decorator_list
                    .iter()
                    .map(|decorator| {
                        infer_expr_type(db, file_id, &decorator.expression)
                            .expect("decorator expression type should be inferrable")
                    })
                    .collect();

                let ty = type_store
                    .add_function(file_id, &node.name.id, decorator_tys)
                    .into();
                type_store.cache_node_type(file_id, *node_key.erased(), ty);
                ty
            }
        }
        Definition::Assignment(node_key) => {
            let parsed = db.parse(file_id)?;
            let ast = parsed.ast();
            let node = node_key.resolve_unwrap(ast.as_any_node_ref());
            // TODO handle unpacking assignment correctly
            infer_expr_type(db, file_id, &node.value)?
        }
        _ => todo!("other kinds of definitions"),
    };

    type_store.cache_symbol_type(file_id, symbol_id, ty);

    // TODO record dependencies
    Ok(ty)
}

fn infer_expr_type<Db>(db: &Db, file_id: FileId, expr: &ast::Expr) -> QueryResult<Type>
where
    Db: SemanticDb + HasJar<SemanticJar>,
{
    // TODO cache the resolution of the type on the node
    let symbols = db.symbol_table(file_id)?;
    match expr {
        ast::Expr::Name(name) => {
            if let Some(symbol_id) = symbols.root_symbol_id_by_name(&name.id) {
                db.infer_symbol_type(file_id, symbol_id)
            } else {
                Ok(Type::Unknown)
            }
        }
        _ => todo!("full expression type resolution"),
    }
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
    fn follow_import_to_class() -> anyhow::Result<()> {
        let case = create_test()?;
        let db = &case.db;

        let a_path = case.src.path().join("a.py");
        let b_path = case.src.path().join("b.py");
        std::fs::write(a_path, "from b import C as D; E = D")?;
        std::fs::write(b_path, "class C: pass")?;
        let a_file = db
            .resolve_module(ModuleName::new("a"))?
            .expect("module should be found")
            .path(db)?
            .file();
        let a_syms = db.symbol_table(a_file)?;
        let e_sym = a_syms
            .root_symbol_id_by_name("E")
            .expect("E symbol should be found");

        let ty = db.infer_symbol_type(a_file, e_sym)?;

        let jar = HasJar::<SemanticJar>::jar(db)?;
        assert!(matches!(ty, Type::Class(_)));
        assert_eq!(format!("{}", ty.display(&jar.type_store)), "Literal[C]");

        Ok(())
    }

    #[test]
    fn resolve_base_class_by_name() -> anyhow::Result<()> {
        let case = create_test()?;
        let db = &case.db;

        let path = case.src.path().join("mod.py");
        std::fs::write(path, "class Base: pass\nclass Sub(Base): pass")?;
        let file = db
            .resolve_module(ModuleName::new("mod"))?
            .expect("module should be found")
            .path(db)?
            .file();
        let syms = db.symbol_table(file)?;
        let sym = syms
            .root_symbol_id_by_name("Sub")
            .expect("Sub symbol should be found");

        let ty = db.infer_symbol_type(file, sym)?;

        let Type::Class(class_id) = ty else {
            panic!("Sub is not a Class")
        };
        let jar = HasJar::<SemanticJar>::jar(db)?;
        let base_names: Vec<_> = jar
            .type_store
            .get_class(class_id)
            .bases()
            .iter()
            .map(|base_ty| format!("{}", base_ty.display(&jar.type_store)))
            .collect();

        assert_eq!(base_names, vec!["Literal[Base]"]);

        Ok(())
    }
}
