#![allow(dead_code)]

use ruff_python_ast as ast;
use ruff_python_ast::AstNode;
use std::fmt::Debug;

use crate::db::{QueryResult, SemanticDb, SemanticJar};

use crate::module::{resolve_module, ModuleName};
use crate::parse::parse;
use crate::symbols::{
    resolve_global_symbol, symbol_table, Definition, GlobalSymbolId, ImportDefinition,
    ImportFromDefinition,
};
use crate::types::{ModuleTypeId, Type};
use crate::{FileId, Name};

// FIXME: Figure out proper dead-lock free synchronisation now that this takes `&db` instead of `&mut db`.
/// Resolve the public-facing type for a symbol (the type seen by other scopes: other modules, or
/// nested functions). Because calls to nested functions and imports can occur anywhere in control
/// flow, this type must be conservative and consider all definitions of the symbol that could
/// possibly be seen by another scope. Currently we take the most conservative approach, which is
/// the union of all definitions. We may be able to narrow this in future to eliminate definitions
/// which can't possibly (or at least likely) be seen by any other scope, so that e.g. we could
/// infer `Literal["1"]` instead of `Literal[1] | Literal["1"]` for `x` in `x = x; x = str(x);`.
#[tracing::instrument(level = "trace", skip(db))]
pub fn infer_symbol_public_type(db: &dyn SemanticDb, symbol: GlobalSymbolId) -> QueryResult<Type> {
    let symbols = symbol_table(db, symbol.file_id)?;
    let defs = symbols.definitions(symbol.symbol_id).to_vec();
    let jar: &SemanticJar = db.jar()?;

    if let Some(ty) = jar.type_store.get_cached_symbol_public_type(symbol) {
        return Ok(ty);
    }

    let ty = infer_type_from_definitions(db, symbol, defs.iter().cloned())?;

    jar.type_store.cache_symbol_public_type(symbol, ty);

    // TODO record dependencies
    Ok(ty)
}

#[tracing::instrument(level = "trace", skip(db))]
pub fn infer_type_from_definitions<T>(
    db: &dyn SemanticDb,
    symbol: GlobalSymbolId,
    definitions: T,
) -> QueryResult<Type>
where
    T: Debug + Iterator<Item = Definition>,
{
    let jar: &SemanticJar = db.jar()?;
    let mut tys = definitions
        .map(|def| infer_definition_type(db, symbol, def.clone()))
        .peekable();
    if let Some(first) = tys.next() {
        if tys.peek().is_some() {
            Ok(Type::Union(jar.type_store.add_union(
                symbol.file_id,
                &Iterator::chain([first].into_iter(), tys).collect::<QueryResult<Vec<_>>>()?,
            )))
        } else {
            first
        }
    } else {
        Ok(Type::Unknown)
    }
}

#[tracing::instrument(level = "trace", skip(db))]
pub fn infer_definition_type(
    db: &dyn SemanticDb,
    symbol: GlobalSymbolId,
    definition: Definition,
) -> QueryResult<Type> {
    let jar: &SemanticJar = db.jar()?;
    let type_store = &jar.type_store;
    let file_id = symbol.file_id;

    match definition {
        Definition::Import(ImportDefinition {
            module: module_name,
        }) => {
            if let Some(module) = resolve_module(db, module_name.clone())? {
                Ok(Type::Module(ModuleTypeId { module, file_id }))
            } else {
                Ok(Type::Unknown)
            }
        }
        Definition::ImportFrom(ImportFromDefinition {
            module,
            name,
            level,
        }) => {
            // TODO relative imports
            assert!(matches!(level, 0));
            let module_name = ModuleName::new(module.as_ref().expect("TODO relative imports"));
            if let Some(remote_symbol) = resolve_global_symbol(db, module_name, &name)? {
                infer_symbol_public_type(db, remote_symbol)
            } else {
                Ok(Type::Unknown)
            }
        }
        Definition::ClassDef(node_key) => {
            if let Some(ty) = type_store.get_cached_node_type(file_id, node_key.erased()) {
                Ok(ty)
            } else {
                let parsed = parse(db.upcast(), file_id)?;
                let ast = parsed.syntax();
                let table = symbol_table(db, file_id)?;
                let node = node_key.resolve_unwrap(ast.as_any_node_ref());

                let mut bases = Vec::with_capacity(node.bases().len());

                for base in node.bases() {
                    bases.push(infer_expr_type(db, file_id, base)?);
                }
                let scope_id = table.scope_id_for_node(node_key.erased());
                let ty = Type::Class(type_store.add_class(file_id, &node.name.id, scope_id, bases));
                type_store.cache_node_type(file_id, *node_key.erased(), ty);
                Ok(ty)
            }
        }
        Definition::FunctionDef(node_key) => {
            if let Some(ty) = type_store.get_cached_node_type(file_id, node_key.erased()) {
                Ok(ty)
            } else {
                let parsed = parse(db.upcast(), file_id)?;
                let ast = parsed.syntax();
                let table = symbol_table(db, file_id)?;
                let node = node_key
                    .resolve(ast.as_any_node_ref())
                    .expect("node key should resolve");

                let decorator_tys = node
                    .decorator_list
                    .iter()
                    .map(|decorator| infer_expr_type(db, file_id, &decorator.expression))
                    .collect::<QueryResult<_>>()?;
                let scope_id = table.scope_id_for_node(node_key.erased());
                let ty = type_store
                    .add_function(
                        file_id,
                        &node.name.id,
                        symbol.symbol_id,
                        scope_id,
                        decorator_tys,
                    )
                    .into();
                type_store.cache_node_type(file_id, *node_key.erased(), ty);
                Ok(ty)
            }
        }
        Definition::Assignment(node_key) => {
            let parsed = parse(db.upcast(), file_id)?;
            let ast = parsed.syntax();
            let node = node_key.resolve_unwrap(ast.as_any_node_ref());
            // TODO handle unpacking assignment correctly (here and for AnnotatedAssignment case, below)
            infer_expr_type(db, file_id, &node.value)
        }
        Definition::AnnotatedAssignment(node_key) => {
            let parsed = parse(db.upcast(), file_id)?;
            let ast = parsed.syntax();
            let node = node_key.resolve_unwrap(ast.as_any_node_ref());
            // TODO actually look at the annotation
            let Some(value) = &node.value else {
                return Ok(Type::Unknown);
            };
            // TODO handle unpacking assignment correctly (here and for Assignment case, above)
            infer_expr_type(db, file_id, value)
        }
    }
}

fn infer_expr_type(db: &dyn SemanticDb, file_id: FileId, expr: &ast::Expr) -> QueryResult<Type> {
    // TODO cache the resolution of the type on the node
    let symbols = symbol_table(db, file_id)?;
    match expr {
        ast::Expr::NumberLiteral(ast::ExprNumberLiteral { value, .. }) => {
            match value {
                ast::Number::Int(n) => {
                    // TODO support big int literals
                    Ok(n.as_i64().map(Type::IntLiteral).unwrap_or(Type::Unknown))
                }
                // TODO builtins.float or builtins.complex
                _ => Ok(Type::Unknown),
            }
        }
        ast::Expr::Name(name) => {
            // TODO look up in the correct scope, don't assume global
            if let Some(symbol_id) = symbols.root_symbol_id_by_name(&name.id) {
                // TODO should use only reachable definitions, not public type
                infer_type_from_definitions(
                    db,
                    GlobalSymbolId { file_id, symbol_id },
                    symbols.reachable_definitions(symbol_id, expr),
                )
            } else {
                Ok(Type::Unknown)
            }
        }
        ast::Expr::Attribute(ast::ExprAttribute { value, attr, .. }) => {
            let value_type = infer_expr_type(db, file_id, value)?;
            let attr_name = &Name::new(&attr.id);
            value_type
                .get_member(db, attr_name)
                .map(|ty| ty.unwrap_or(Type::Unknown))
        }
        _ => todo!("full expression type resolution"),
    }
}

#[cfg(test)]
mod tests {

    use crate::db::tests::TestDb;
    use crate::db::{HasJar, SemanticJar};
    use crate::module::{
        resolve_module, set_module_search_paths, ModuleName, ModuleSearchPath, ModuleSearchPathKind,
    };
    use crate::symbols::{resolve_global_symbol, symbol_table, GlobalSymbolId};
    use crate::types::{infer_symbol_public_type, Type};
    use crate::Name;

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
        set_module_search_paths(&mut db, roots);

        Ok(TestCase { temp_dir, db, src })
    }

    fn write_to_path(case: &TestCase, relpath: &str, contents: &str) -> anyhow::Result<()> {
        let path = case.src.path().join(relpath);
        std::fs::write(path, contents)?;
        Ok(())
    }

    fn get_public_type(case: &TestCase, modname: &str, varname: &str) -> anyhow::Result<Type> {
        let db = &case.db;
        let symbol =
            resolve_global_symbol(db, ModuleName::new(modname), varname)?.expect("symbol to exist");

        Ok(infer_symbol_public_type(db, symbol)?)
    }

    fn assert_public_type(
        case: &TestCase,
        modname: &str,
        varname: &str,
        tyname: &str,
    ) -> anyhow::Result<()> {
        let ty = get_public_type(case, modname, varname)?;

        let jar = HasJar::<SemanticJar>::jar(&case.db)?;
        assert_eq!(format!("{}", ty.display(&jar.type_store)), tyname);
        Ok(())
    }

    #[test]
    fn follow_import_to_class() -> anyhow::Result<()> {
        let case = create_test()?;

        write_to_path(&case, "a.py", "from b import C as D; E = D")?;
        write_to_path(&case, "b.py", "class C: pass")?;

        assert_public_type(&case, "a", "E", "Literal[C]")
    }

    #[test]
    fn resolve_base_class_by_name() -> anyhow::Result<()> {
        let case = create_test()?;

        write_to_path(
            &case,
            "mod.py",
            "
                class Base: pass
                class Sub(Base): pass
            ",
        )?;

        let ty = get_public_type(&case, "mod", "Sub")?;

        let Type::Class(class_id) = ty else {
            panic!("Sub is not a Class")
        };
        let jar = HasJar::<SemanticJar>::jar(&case.db)?;
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

    #[test]
    fn resolve_method() -> anyhow::Result<()> {
        let case = create_test()?;

        write_to_path(
            &case,
            "mod.py",
            "
                class C:
                    def f(self): pass
            ",
        )?;

        let ty = get_public_type(&case, "mod", "C")?;

        let Type::Class(class_id) = ty else {
            panic!("C is not a Class");
        };

        let member_ty = class_id
            .get_own_class_member(&case.db, &Name::new("f"))
            .expect("C.f to resolve");

        let Some(Type::Function(func_id)) = member_ty else {
            panic!("C.f is not a Function");
        };

        let jar = HasJar::<SemanticJar>::jar(&case.db)?;
        let function = jar.type_store.get_function(func_id);
        assert_eq!(function.name(), "f");

        Ok(())
    }

    #[test]
    fn resolve_module_member() -> anyhow::Result<()> {
        let case = create_test()?;

        write_to_path(&case, "a.py", "import b; D = b.C")?;
        write_to_path(&case, "b.py", "class C: pass")?;

        assert_public_type(&case, "a", "D", "Literal[C]")
    }

    #[test]
    fn resolve_literal() -> anyhow::Result<()> {
        let case = create_test()?;

        write_to_path(&case, "a.py", "x = 1")?;

        assert_public_type(&case, "a", "x", "Literal[1]")
    }

    #[test]
    fn resolve_union() -> anyhow::Result<()> {
        let case = create_test()?;

        write_to_path(
            &case,
            "a.py",
            "
                if flag:
                    x = 1
                else:
                    x = 2
            ",
        )?;

        assert_public_type(&case, "a", "x", "(Literal[1] | Literal[2])")
    }

    #[test]
    fn resolve_visible_def() -> anyhow::Result<()> {
        let case = create_test()?;
        let db = &case.db;

        let path = case.src.path().join("a.py");
        std::fs::write(path, "y = 1; y = 2; x = y")?;
        let file = resolve_module(db, ModuleName::new("a"))?
            .expect("module should be found")
            .path(db)?
            .file();
        let syms = symbol_table(db, file)?;
        let x_sym = syms
            .root_symbol_id_by_name("x")
            .expect("x symbol should be found");

        let ty = infer_symbol_public_type(
            db,
            GlobalSymbolId {
                file_id: file,
                symbol_id: x_sym,
            },
        )?;

        let jar = HasJar::<SemanticJar>::jar(db)?;
        assert!(matches!(ty, Type::IntLiteral(_)));
        assert_eq!(format!("{}", ty.display(&jar.type_store)), "Literal[2]");
        Ok(())
    }
}
