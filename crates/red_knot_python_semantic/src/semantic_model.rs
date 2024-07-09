use red_knot_module_resolver::{resolve_module, Module, ModuleName};
use ruff_db::files::File;
use ruff_python_ast as ast;
use ruff_python_ast::{Expr, ExpressionRef, StmtClassDef};

use crate::semantic_index::ast_ids::HasScopedAstId;
use crate::semantic_index::symbol::PublicSymbolId;
use crate::semantic_index::{public_symbol, semantic_index};
use crate::types::{infer_types, public_symbol_ty, Type};
use crate::Db;

pub struct SemanticModel<'db> {
    db: &'db dyn Db,
    file: File,
}

impl<'db> SemanticModel<'db> {
    pub fn new(db: &'db dyn Db, file: File) -> Self {
        Self { db, file }
    }

    // TODO we don't actually want to expose the Db directly to lint rules, but we need to find a
    // solution for exposing information from types
    pub fn db(&self) -> &dyn Db {
        self.db
    }

    pub fn resolve_module(&self, module_name: ModuleName) -> Option<Module> {
        resolve_module(self.db.upcast(), module_name)
    }

    pub fn public_symbol(&self, module: &Module, symbol_name: &str) -> Option<PublicSymbolId<'db>> {
        public_symbol(self.db, module.file(), symbol_name)
    }

    pub fn public_symbol_ty(&self, symbol: PublicSymbolId<'db>) -> Type {
        public_symbol_ty(self.db, symbol)
    }
}

pub trait HasTy {
    /// Returns the inferred type of `self`.
    ///
    /// ## Panics
    /// May panic if `self` is from another file than `model`.
    fn ty<'db>(&self, model: &SemanticModel<'db>) -> Type<'db>;
}

impl HasTy for ast::ExpressionRef<'_> {
    fn ty<'db>(&self, model: &SemanticModel<'db>) -> Type<'db> {
        let index = semantic_index(model.db, model.file);
        let file_scope = index.expression_scope_id(*self);
        let scope = file_scope.to_scope_id(model.db, model.file);

        let expression_id = self.scoped_ast_id(model.db, scope);
        infer_types(model.db, scope).expression_ty(expression_id)
    }
}

macro_rules! impl_expression_has_ty {
    ($ty: ty) => {
        impl HasTy for $ty {
            #[inline]
            fn ty<'db>(&self, model: &SemanticModel<'db>) -> Type<'db> {
                let expression_ref = ExpressionRef::from(self);
                expression_ref.ty(model)
            }
        }
    };
}

impl_expression_has_ty!(ast::ExprBoolOp);
impl_expression_has_ty!(ast::ExprNamed);
impl_expression_has_ty!(ast::ExprBinOp);
impl_expression_has_ty!(ast::ExprUnaryOp);
impl_expression_has_ty!(ast::ExprLambda);
impl_expression_has_ty!(ast::ExprIf);
impl_expression_has_ty!(ast::ExprDict);
impl_expression_has_ty!(ast::ExprSet);
impl_expression_has_ty!(ast::ExprListComp);
impl_expression_has_ty!(ast::ExprSetComp);
impl_expression_has_ty!(ast::ExprDictComp);
impl_expression_has_ty!(ast::ExprGenerator);
impl_expression_has_ty!(ast::ExprAwait);
impl_expression_has_ty!(ast::ExprYield);
impl_expression_has_ty!(ast::ExprYieldFrom);
impl_expression_has_ty!(ast::ExprCompare);
impl_expression_has_ty!(ast::ExprCall);
impl_expression_has_ty!(ast::ExprFString);
impl_expression_has_ty!(ast::ExprStringLiteral);
impl_expression_has_ty!(ast::ExprBytesLiteral);
impl_expression_has_ty!(ast::ExprNumberLiteral);
impl_expression_has_ty!(ast::ExprBooleanLiteral);
impl_expression_has_ty!(ast::ExprNoneLiteral);
impl_expression_has_ty!(ast::ExprEllipsisLiteral);
impl_expression_has_ty!(ast::ExprAttribute);
impl_expression_has_ty!(ast::ExprSubscript);
impl_expression_has_ty!(ast::ExprStarred);
impl_expression_has_ty!(ast::ExprName);
impl_expression_has_ty!(ast::ExprList);
impl_expression_has_ty!(ast::ExprTuple);
impl_expression_has_ty!(ast::ExprSlice);
impl_expression_has_ty!(ast::ExprIpyEscapeCommand);

impl HasTy for ast::Expr {
    fn ty<'db>(&self, model: &SemanticModel<'db>) -> Type<'db> {
        match self {
            Expr::BoolOp(inner) => inner.ty(model),
            Expr::Named(inner) => inner.ty(model),
            Expr::BinOp(inner) => inner.ty(model),
            Expr::UnaryOp(inner) => inner.ty(model),
            Expr::Lambda(inner) => inner.ty(model),
            Expr::If(inner) => inner.ty(model),
            Expr::Dict(inner) => inner.ty(model),
            Expr::Set(inner) => inner.ty(model),
            Expr::ListComp(inner) => inner.ty(model),
            Expr::SetComp(inner) => inner.ty(model),
            Expr::DictComp(inner) => inner.ty(model),
            Expr::Generator(inner) => inner.ty(model),
            Expr::Await(inner) => inner.ty(model),
            Expr::Yield(inner) => inner.ty(model),
            Expr::YieldFrom(inner) => inner.ty(model),
            Expr::Compare(inner) => inner.ty(model),
            Expr::Call(inner) => inner.ty(model),
            Expr::FString(inner) => inner.ty(model),
            Expr::StringLiteral(inner) => inner.ty(model),
            Expr::BytesLiteral(inner) => inner.ty(model),
            Expr::NumberLiteral(inner) => inner.ty(model),
            Expr::BooleanLiteral(inner) => inner.ty(model),
            Expr::NoneLiteral(inner) => inner.ty(model),
            Expr::EllipsisLiteral(inner) => inner.ty(model),
            Expr::Attribute(inner) => inner.ty(model),
            Expr::Subscript(inner) => inner.ty(model),
            Expr::Starred(inner) => inner.ty(model),
            Expr::Name(inner) => inner.ty(model),
            Expr::List(inner) => inner.ty(model),
            Expr::Tuple(inner) => inner.ty(model),
            Expr::Slice(inner) => inner.ty(model),
            Expr::IpyEscapeCommand(inner) => inner.ty(model),
        }
    }
}

impl HasTy for ast::StmtFunctionDef {
    fn ty<'db>(&self, model: &SemanticModel<'db>) -> Type<'db> {
        let index = semantic_index(model.db, model.file);
        let definition = index.definition(self);

        let scope = definition.scope(model.db).to_scope_id(model.db, model.file);
        let types = infer_types(model.db, scope);

        types.definition_ty(definition)
    }
}

impl HasTy for StmtClassDef {
    fn ty<'db>(&self, model: &SemanticModel<'db>) -> Type<'db> {
        let index = semantic_index(model.db, model.file);
        let definition = index.definition(self);

        let scope = definition.scope(model.db).to_scope_id(model.db, model.file);
        let types = infer_types(model.db, scope);

        types.definition_ty(definition)
    }
}

impl HasTy for ast::Alias {
    fn ty<'db>(&self, model: &SemanticModel<'db>) -> Type<'db> {
        let index = semantic_index(model.db, model.file);
        let definition = index.definition(self);

        let scope = definition.scope(model.db).to_scope_id(model.db, model.file);
        let types = infer_types(model.db, scope);

        types.definition_ty(definition)
    }
}

#[cfg(test)]
mod tests {
    use red_knot_module_resolver::{
        set_module_resolution_settings, RawModuleResolutionSettings, TargetVersion,
    };
    use ruff_db::files::system_path_to_file;
    use ruff_db::parsed::parsed_module;
    use ruff_db::system::{DbWithTestSystem, SystemPathBuf};

    use crate::db::tests::TestDb;
    use crate::types::Type;
    use crate::{HasTy, SemanticModel};

    fn setup_db() -> TestDb {
        let mut db = TestDb::new();
        set_module_resolution_settings(
            &mut db,
            RawModuleResolutionSettings {
                extra_paths: vec![],
                workspace_root: SystemPathBuf::from("/src"),
                site_packages: None,
                custom_typeshed: None,
                target_version: TargetVersion::Py38,
            },
        );

        db
    }

    #[test]
    fn function_ty() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_file("/src/foo.py", "def test(): pass")?;
        let foo = system_path_to_file(&db, "/src/foo.py").unwrap();

        let ast = parsed_module(&db, foo);

        let function = ast.suite()[0].as_function_def_stmt().unwrap();
        let model = SemanticModel::new(&db, foo);
        let ty = function.ty(&model);

        assert!(matches!(ty, Type::Function(_)));

        Ok(())
    }

    #[test]
    fn class_ty() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_file("/src/foo.py", "class Test: pass")?;
        let foo = system_path_to_file(&db, "/src/foo.py").unwrap();

        let ast = parsed_module(&db, foo);

        let class = ast.suite()[0].as_class_def_stmt().unwrap();
        let model = SemanticModel::new(&db, foo);
        let ty = class.ty(&model);

        assert!(matches!(ty, Type::Class(_)));

        Ok(())
    }

    #[test]
    fn alias_ty() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_files([
            ("/src/foo.py", "class Test: pass"),
            ("/src/bar.py", "from foo import Test"),
        ])?;
        let bar = system_path_to_file(&db, "/src/bar.py").unwrap();

        let ast = parsed_module(&db, bar);

        let import = ast.suite()[0].as_import_from_stmt().unwrap();
        let alias = &import.names[0];
        let model = SemanticModel::new(&db, bar);
        let ty = alias.ty(&model);

        assert!(matches!(ty, Type::Class(_)));

        Ok(())
    }
}
