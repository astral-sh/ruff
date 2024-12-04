use ruff_db::files::{File, FilePath};
use ruff_db::source::line_index;
use ruff_python_ast as ast;
use ruff_python_ast::{Expr, ExpressionRef};
use ruff_source_file::LineIndex;

use crate::module_name::ModuleName;
use crate::module_resolver::{resolve_module, Module};
use crate::semantic_index::ast_ids::HasScopedExpressionId;
use crate::semantic_index::semantic_index;
use crate::types::{binding_ty, infer_scope_types, Type};
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

    pub fn file_path(&self) -> &FilePath {
        self.file.path(self.db)
    }

    pub fn line_index(&self) -> LineIndex {
        line_index(self.db.upcast(), self.file)
    }

    pub fn resolve_module(&self, module_name: &ModuleName) -> Option<Module> {
        resolve_module(self.db, module_name)
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

        let expression_id = self.scoped_expression_id(model.db, scope);
        infer_scope_types(model.db, scope).expression_ty(expression_id)
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

macro_rules! impl_binding_has_ty {
    ($ty: ty) => {
        impl HasTy for $ty {
            #[inline]
            fn ty<'db>(&self, model: &SemanticModel<'db>) -> Type<'db> {
                let index = semantic_index(model.db, model.file);
                let binding = index.definition(self);
                binding_ty(model.db, binding)
            }
        }
    };
}

impl_binding_has_ty!(ast::StmtFunctionDef);
impl_binding_has_ty!(ast::StmtClassDef);
impl_binding_has_ty!(ast::Alias);
impl_binding_has_ty!(ast::Parameter);
impl_binding_has_ty!(ast::ParameterWithDefault);

#[cfg(test)]
mod tests {
    use ruff_db::files::system_path_to_file;
    use ruff_db::parsed::parsed_module;

    use crate::db::tests::TestDbBuilder;
    use crate::{HasTy, SemanticModel};

    #[test]
    fn function_ty() -> anyhow::Result<()> {
        let db = TestDbBuilder::new()
            .with_file("/src/foo.py", "def test(): pass")
            .build()?;

        let foo = system_path_to_file(&db, "/src/foo.py").unwrap();

        let ast = parsed_module(&db, foo);

        let function = ast.suite()[0].as_function_def_stmt().unwrap();
        let model = SemanticModel::new(&db, foo);
        let ty = function.ty(&model);

        assert!(ty.is_function_literal());

        Ok(())
    }

    #[test]
    fn class_ty() -> anyhow::Result<()> {
        let db = TestDbBuilder::new()
            .with_file("/src/foo.py", "class Test: pass")
            .build()?;

        let foo = system_path_to_file(&db, "/src/foo.py").unwrap();

        let ast = parsed_module(&db, foo);

        let class = ast.suite()[0].as_class_def_stmt().unwrap();
        let model = SemanticModel::new(&db, foo);
        let ty = class.ty(&model);

        assert!(ty.is_class_literal());

        Ok(())
    }

    #[test]
    fn alias_ty() -> anyhow::Result<()> {
        let db = TestDbBuilder::new()
            .with_file("/src/foo.py", "class Test: pass")
            .with_file("/src/bar.py", "from foo import Test")
            .build()?;

        let bar = system_path_to_file(&db, "/src/bar.py").unwrap();

        let ast = parsed_module(&db, bar);

        let import = ast.suite()[0].as_import_from_stmt().unwrap();
        let alias = &import.names[0];
        let model = SemanticModel::new(&db, bar);
        let ty = alias.ty(&model);

        assert!(ty.is_class_literal());

        Ok(())
    }
}
