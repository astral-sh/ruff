#![allow(dead_code)]

use ruff_python_ast as ast;
use ruff_python_ast::AstNode;
use std::fmt::Debug;

use crate::db::{QueryResult, SemanticDb, SemanticJar};

use crate::module::{resolve_module, ModuleName};
use crate::parse::parse;
use crate::semantic::types::{ModuleTypeId, Type};
use crate::semantic::{
    resolve_global_symbol, semantic_index, ConstrainedDefinition, Definition, ExpressionId,
    GlobalSymbolId, ImportDefinition, ImportFromDefinition,
};
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
    let index = semantic_index(db, symbol.file_id)?;
    let defs = index.symbol_table().definitions(symbol.symbol_id).to_vec();
    let jar: &SemanticJar = db.jar()?;

    if let Some(ty) = jar.type_store.get_cached_symbol_public_type(symbol) {
        return Ok(ty);
    }

    let ty = infer_type_from_definitions(db, symbol, defs.iter().cloned())?;

    jar.type_store.cache_symbol_public_type(symbol, ty);

    // TODO record dependencies
    Ok(ty)
}

/// Infer type of a symbol as union of the given Definitions.
#[tracing::instrument(level = "trace", skip(db))]
pub fn infer_type_from_definitions<T>(
    db: &dyn SemanticDb,
    symbol: GlobalSymbolId,
    definitions: T,
) -> QueryResult<Type>
where
    T: Debug + Iterator<Item = Definition>,
{
    infer_type_from_constrained_definitions(
        db,
        symbol,
        definitions.map(|definition| ConstrainedDefinition {
            definition,
            constraints: vec![],
        }),
    )
}

/// Infer type of a symbol as union of the given ConstrainedDefinitions.
#[tracing::instrument(level = "trace", skip(db))]
pub fn infer_type_from_constrained_definitions<T>(
    db: &dyn SemanticDb,
    symbol: GlobalSymbolId,
    constrained_definitions: T,
) -> QueryResult<Type>
where
    T: Debug + Iterator<Item = ConstrainedDefinition>,
{
    let jar: &SemanticJar = db.jar()?;
    let mut tys = constrained_definitions
        .map(|def| infer_constrained_definition_type(db, symbol, def.clone()))
        .peekable();
    if let Some(first) = tys.next() {
        if tys.peek().is_some() {
            Ok(jar.type_store.add_union(
                symbol.file_id,
                &Iterator::chain(std::iter::once(first), tys).collect::<QueryResult<Vec<_>>>()?,
            ))
        } else {
            first
        }
    } else {
        Ok(Type::Unknown)
    }
}

/// Infer type for a ConstrainedDefinition (intersection of the definition type and the
/// constraints)
#[tracing::instrument(level = "trace", skip(db))]
pub fn infer_constrained_definition_type(
    db: &dyn SemanticDb,
    symbol: GlobalSymbolId,
    constrained_definition: ConstrainedDefinition,
) -> QueryResult<Type> {
    let ConstrainedDefinition {
        definition,
        constraints,
    } = constrained_definition;
    let mut intersected_types = vec![infer_definition_type(db, symbol, definition)?];
    for constraint in constraints {
        if let Some(constraint_type) = infer_constraint_type(db, symbol, constraint)? {
            intersected_types.push(constraint_type);
        }
    }
    let jar: &SemanticJar = db.jar()?;
    Ok(jar
        .type_store
        .add_intersection(symbol.file_id, &intersected_types, &[]))
}

/// Infer a type for a Definition
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
        Definition::Unbound => Ok(Type::Unbound),
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
            let Some(module) = resolve_module(db, module_name.clone())? else {
                return Ok(Type::Unknown);
            };

            if let Some(remote_symbol) = resolve_global_symbol(db, module, &name)? {
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
                let index = semantic_index(db, file_id)?;
                let node = node_key.resolve_unwrap(ast.as_any_node_ref());

                let mut bases = Vec::with_capacity(node.bases().len());

                for base in node.bases() {
                    bases.push(infer_expr_type(db, file_id, base)?);
                }
                let scope_id = index.symbol_table().scope_id_for_node(node_key.erased());
                let ty = type_store.add_class(file_id, &node.name.id, scope_id, bases);
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
                let index = semantic_index(db, file_id)?;
                let node = node_key
                    .resolve(ast.as_any_node_ref())
                    .expect("node key should resolve");

                let decorator_tys = node
                    .decorator_list
                    .iter()
                    .map(|decorator| infer_expr_type(db, file_id, &decorator.expression))
                    .collect::<QueryResult<_>>()?;
                let scope_id = index.symbol_table().scope_id_for_node(node_key.erased());
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
            // TODO handle unpacking assignment
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
            // TODO handle unpacking assignment
            infer_expr_type(db, file_id, value)
        }
        Definition::NamedExpr(node_key) => {
            let parsed = parse(db.upcast(), file_id)?;
            let ast = parsed.syntax();
            let node = node_key.resolve_unwrap(ast.as_any_node_ref());
            infer_expr_type(db, file_id, &node.value)
        }
    }
}

/// Return the type that the given constraint (an expression from a control-flow test) requires the
/// given symbol to have. For example, returns ~None as the constraint type if given the symbol ID
/// for x and the expression ID for `x is not None`. Returns None if the given expression applies
/// no constraints on the given symbol.
#[tracing::instrument(level = "trace", skip(db))]
fn infer_constraint_type(
    db: &dyn SemanticDb,
    symbol_id: GlobalSymbolId,
    constraint: ExpressionId,
) -> QueryResult<Option<Type>> {
    let index = semantic_index(db, symbol_id.file_id)?;
    // TODO actually infer constraints
    Ok(None)
}

/// Infer type of the given expression.
fn infer_expr_type(db: &dyn SemanticDb, file_id: FileId, expr: &ast::Expr) -> QueryResult<Type> {
    // TODO cache the resolution of the type on the node
    let index = semantic_index(db, file_id)?;
    match expr {
        ast::Expr::NoneLiteral(_) => Ok(Type::None),
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
            if let Some(symbol_id) = index.symbol_table().root_symbol_id_by_name(&name.id) {
                infer_type_from_constrained_definitions(
                    db,
                    GlobalSymbolId { file_id, symbol_id },
                    index.reachable_definitions(symbol_id, expr),
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
        ast::Expr::BinOp(ast::ExprBinOp {
            left, op, right, ..
        }) => {
            let left_ty = infer_expr_type(db, file_id, left)?;
            let right_ty = infer_expr_type(db, file_id, right)?;
            // TODO add reverse bin op support if right <: left
            left_ty.resolve_bin_op(db, *op, right_ty)
        }
        ast::Expr::Named(ast::ExprNamed { value, .. }) => infer_expr_type(db, file_id, value),
        ast::Expr::If(ast::ExprIf { body, orelse, .. }) => {
            // TODO detect statically known truthy or falsy test
            let body_ty = infer_expr_type(db, file_id, body)?;
            let else_ty = infer_expr_type(db, file_id, orelse)?;
            let jar: &SemanticJar = db.jar()?;
            Ok(jar.type_store.add_union(file_id, &[body_ty, else_ty]))
        }
        _ => todo!("expression type resolution for {:?}", expr),
    }
}

#[cfg(test)]
mod tests {

    use std::path::PathBuf;

    use crate::db::tests::TestDb;
    use crate::db::{HasJar, SemanticJar};
    use crate::module::{
        resolve_module, set_module_search_paths, ModuleName, ModuleResolutionInputs,
    };
    use crate::semantic::{infer_symbol_public_type, resolve_global_symbol, Type};
    use crate::Name;

    // TODO with virtual filesystem we shouldn't have to write files to disk for these
    // tests

    struct TestCase {
        temp_dir: tempfile::TempDir,
        db: TestDb,

        src: PathBuf,
    }

    fn create_test() -> std::io::Result<TestCase> {
        let temp_dir = tempfile::tempdir()?;

        let src = temp_dir.path().join("src");
        std::fs::create_dir(&src)?;
        let src = src.canonicalize()?;

        let search_paths = ModuleResolutionInputs {
            extra_paths: vec![],
            workspace_root: src.clone(),
            site_packages: None,
            custom_typeshed: None,
        };

        let mut db = TestDb::default();
        set_module_search_paths(&mut db, search_paths);

        Ok(TestCase { temp_dir, db, src })
    }

    fn write_to_path(case: &TestCase, relative_path: &str, contents: &str) -> anyhow::Result<()> {
        let path = case.src.join(relative_path);
        std::fs::write(path, contents)?;
        Ok(())
    }

    fn get_public_type(
        case: &TestCase,
        module_name: &str,
        variable_name: &str,
    ) -> anyhow::Result<Type> {
        let db = &case.db;
        let module = resolve_module(db, ModuleName::new(module_name))?.expect("Module to exist");
        let symbol = resolve_global_symbol(db, module, variable_name)?.expect("symbol to exist");

        Ok(infer_symbol_public_type(db, symbol)?)
    }

    fn assert_public_type(
        case: &TestCase,
        module_name: &str,
        variable_name: &str,
        type_name: &str,
    ) -> anyhow::Result<()> {
        let ty = get_public_type(case, module_name, variable_name)?;

        let jar = HasJar::<SemanticJar>::jar(&case.db)?;
        assert_eq!(format!("{}", ty.display(&jar.type_store)), type_name);
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

        assert_public_type(&case, "a", "x", "Literal[1, 2]")
    }

    #[test]
    fn resolve_visible_def() -> anyhow::Result<()> {
        let case = create_test()?;
        write_to_path(&case, "a.py", "y = 1; y = 2; x = y")?;
        assert_public_type(&case, "a", "x", "Literal[2]")
    }

    #[test]
    fn join_paths() -> anyhow::Result<()> {
        let case = create_test()?;

        write_to_path(
            &case,
            "a.py",
            "
                y = 1
                y = 2
                if flag:
                    y = 3
                x = y
            ",
        )?;

        assert_public_type(&case, "a", "x", "Literal[2, 3]")
    }

    #[test]
    fn maybe_unbound() -> anyhow::Result<()> {
        let case = create_test()?;

        write_to_path(
            &case,
            "a.py",
            "
                if flag:
                    y = 1
                x = y
            ",
        )?;

        assert_public_type(&case, "a", "x", "Literal[1] | Unbound")
    }

    #[test]
    fn if_elif_else() -> anyhow::Result<()> {
        let case = create_test()?;

        write_to_path(
            &case,
            "a.py",
            "
                y = 1
                y = 2
                if flag:
                    y = 3
                elif flag2:
                    y = 4
                else:
                    r = y
                    y = 5
                    s = y
                x = y
            ",
        )?;

        assert_public_type(&case, "a", "x", "Literal[3, 4, 5]")?;
        assert_public_type(&case, "a", "r", "Literal[2]")?;
        assert_public_type(&case, "a", "s", "Literal[5]")
    }

    #[test]
    fn if_elif() -> anyhow::Result<()> {
        let case = create_test()?;

        write_to_path(
            &case,
            "a.py",
            "
                y = 1
                y = 2
                if flag:
                    y = 3
                elif flag2:
                    y = 4
                x = y
            ",
        )?;

        assert_public_type(&case, "a", "x", "Literal[2, 3, 4]")
    }

    #[test]
    fn literal_int_arithmetic() -> anyhow::Result<()> {
        let case = create_test()?;

        write_to_path(
            &case,
            "a.py",
            "
                a = 2 + 1
                b = a - 4
                c = a * b
                d = c / 3
                e = 5 % 3
            ",
        )?;

        assert_public_type(&case, "a", "a", "Literal[3]")?;
        assert_public_type(&case, "a", "b", "Literal[-1]")?;
        assert_public_type(&case, "a", "c", "Literal[-3]")?;
        assert_public_type(&case, "a", "d", "Literal[-1]")?;
        assert_public_type(&case, "a", "e", "Literal[2]")
    }

    #[test]
    fn walrus() -> anyhow::Result<()> {
        let case = create_test()?;

        write_to_path(
            &case,
            "a.py",
            "
                x = (y := 1) + 1
            ",
        )?;

        assert_public_type(&case, "a", "x", "Literal[2]")?;
        assert_public_type(&case, "a", "y", "Literal[1]")
    }

    #[test]
    fn ifexpr() -> anyhow::Result<()> {
        let case = create_test()?;

        write_to_path(
            &case,
            "a.py",
            "
                x = 1 if flag else 2
            ",
        )?;

        assert_public_type(&case, "a", "x", "Literal[1, 2]")
    }

    #[test]
    fn ifexpr_walrus() -> anyhow::Result<()> {
        let case = create_test()?;

        write_to_path(
            &case,
            "a.py",
            "
                y = z = 0
                x = (y := 1) if flag else (z := 2)
                a = y
                b = z
            ",
        )?;

        assert_public_type(&case, "a", "x", "Literal[1, 2]")?;
        assert_public_type(&case, "a", "a", "Literal[0, 1]")?;
        assert_public_type(&case, "a", "b", "Literal[0, 2]")
    }

    #[test]
    fn ifexpr_walrus_2() -> anyhow::Result<()> {
        let case = create_test()?;

        write_to_path(
            &case,
            "a.py",
            "
                y = 0
                (y := 1) if flag else (y := 2)
                a = y
            ",
        )?;

        assert_public_type(&case, "a", "a", "Literal[1, 2]")
    }

    #[test]
    fn ifexpr_nested() -> anyhow::Result<()> {
        let case = create_test()?;

        write_to_path(
            &case,
            "a.py",
            "
                x = 1 if flag else 2 if flag2 else 3
            ",
        )?;

        assert_public_type(&case, "a", "x", "Literal[1, 2, 3]")
    }

    #[test]
    fn none() -> anyhow::Result<()> {
        let case = create_test()?;

        write_to_path(
            &case,
            "a.py",
            "
                x = 1 if flag else None
            ",
        )?;

        assert_public_type(&case, "a", "x", "Literal[1] | None")
    }

    #[test]
    fn narrow_none() -> anyhow::Result<()> {
        let case = create_test()?;

        write_to_path(
            &case,
            "a.py",
            "
                x = 1 if flag else None
                y = 0
                if x is not None:
                    y = x
                z = y
            ",
        )?;

        // TODO normalization of unions and intersections
        assert_public_type(&case, "a", "z", "Literal[0] | Literal[1] | None & ~None")
    }
}
