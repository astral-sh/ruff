use ruff_db::files::{File, FilePath};
use ruff_db::source::line_index;
use ruff_python_ast as ast;
use ruff_python_ast::{Expr, ExprRef};
use ruff_source_file::LineIndex;

use crate::module_name::ModuleName;
use crate::module_resolver::{resolve_module, Module};
use crate::semantic_index::ast_ids::HasScopedExpressionId;
use crate::semantic_index::semantic_index;
use crate::types::{binding_type, infer_scope_types, Type};
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

pub trait HasType {
    /// Returns the inferred type of `self`.
    ///
    /// ## Panics
    /// May panic if `self` is from another file than `model`.
    fn inferred_type<'db>(&self, model: &SemanticModel<'db>) -> Type<'db>;
}

impl HasType for ast::ExprRef<'_> {
    fn inferred_type<'db>(&self, model: &SemanticModel<'db>) -> Type<'db> {
        let index = semantic_index(model.db, model.file);
        let file_scope = index.expression_scope_id(*self);
        let scope = file_scope.to_scope_id(model.db, model.file);

        let expression_id = self.scoped_expression_id(model.db, scope);
        infer_scope_types(model.db, scope).expression_type(expression_id)
    }
}

macro_rules! impl_expression_has_type {
    ($ty: ty) => {
        impl HasType for $ty {
            #[inline]
            fn inferred_type<'db>(&self, model: &SemanticModel<'db>) -> Type<'db> {
                let expression_ref = ExprRef::from(self);
                expression_ref.inferred_type(model)
            }
        }
    };
}

impl_expression_has_type!(ast::ExprBoolOp);
impl_expression_has_type!(ast::ExprNamed);
impl_expression_has_type!(ast::ExprBinOp);
impl_expression_has_type!(ast::ExprUnaryOp);
impl_expression_has_type!(ast::ExprLambda);
impl_expression_has_type!(ast::ExprIf);
impl_expression_has_type!(ast::ExprDict);
impl_expression_has_type!(ast::ExprSet);
impl_expression_has_type!(ast::ExprListComp);
impl_expression_has_type!(ast::ExprSetComp);
impl_expression_has_type!(ast::ExprDictComp);
impl_expression_has_type!(ast::ExprGenerator);
impl_expression_has_type!(ast::ExprAwait);
impl_expression_has_type!(ast::ExprYield);
impl_expression_has_type!(ast::ExprYieldFrom);
impl_expression_has_type!(ast::ExprCompare);
impl_expression_has_type!(ast::ExprCall);
impl_expression_has_type!(ast::ExprFString);
impl_expression_has_type!(ast::ExprStringLiteral);
impl_expression_has_type!(ast::ExprBytesLiteral);
impl_expression_has_type!(ast::ExprNumberLiteral);
impl_expression_has_type!(ast::ExprBooleanLiteral);
impl_expression_has_type!(ast::ExprNoneLiteral);
impl_expression_has_type!(ast::ExprEllipsisLiteral);
impl_expression_has_type!(ast::ExprAttribute);
impl_expression_has_type!(ast::ExprSubscript);
impl_expression_has_type!(ast::ExprStarred);
impl_expression_has_type!(ast::ExprName);
impl_expression_has_type!(ast::ExprList);
impl_expression_has_type!(ast::ExprTuple);
impl_expression_has_type!(ast::ExprSlice);
impl_expression_has_type!(ast::ExprIpyEscapeCommand);

impl HasType for ast::Expr {
    fn inferred_type<'db>(&self, model: &SemanticModel<'db>) -> Type<'db> {
        match self {
            Expr::BoolOp(inner) => inner.inferred_type(model),
            Expr::Named(inner) => inner.inferred_type(model),
            Expr::BinOp(inner) => inner.inferred_type(model),
            Expr::UnaryOp(inner) => inner.inferred_type(model),
            Expr::Lambda(inner) => inner.inferred_type(model),
            Expr::If(inner) => inner.inferred_type(model),
            Expr::Dict(inner) => inner.inferred_type(model),
            Expr::Set(inner) => inner.inferred_type(model),
            Expr::ListComp(inner) => inner.inferred_type(model),
            Expr::SetComp(inner) => inner.inferred_type(model),
            Expr::DictComp(inner) => inner.inferred_type(model),
            Expr::Generator(inner) => inner.inferred_type(model),
            Expr::Await(inner) => inner.inferred_type(model),
            Expr::Yield(inner) => inner.inferred_type(model),
            Expr::YieldFrom(inner) => inner.inferred_type(model),
            Expr::Compare(inner) => inner.inferred_type(model),
            Expr::Call(inner) => inner.inferred_type(model),
            Expr::FString(inner) => inner.inferred_type(model),
            Expr::StringLiteral(inner) => inner.inferred_type(model),
            Expr::BytesLiteral(inner) => inner.inferred_type(model),
            Expr::NumberLiteral(inner) => inner.inferred_type(model),
            Expr::BooleanLiteral(inner) => inner.inferred_type(model),
            Expr::NoneLiteral(inner) => inner.inferred_type(model),
            Expr::EllipsisLiteral(inner) => inner.inferred_type(model),
            Expr::Attribute(inner) => inner.inferred_type(model),
            Expr::Subscript(inner) => inner.inferred_type(model),
            Expr::Starred(inner) => inner.inferred_type(model),
            Expr::Name(inner) => inner.inferred_type(model),
            Expr::List(inner) => inner.inferred_type(model),
            Expr::Tuple(inner) => inner.inferred_type(model),
            Expr::Slice(inner) => inner.inferred_type(model),
            Expr::IpyEscapeCommand(inner) => inner.inferred_type(model),
        }
    }
}

macro_rules! impl_binding_has_ty {
    ($ty: ty) => {
        impl HasType for $ty {
            #[inline]
            fn inferred_type<'db>(&self, model: &SemanticModel<'db>) -> Type<'db> {
                let index = semantic_index(model.db, model.file);
                let binding = index.expect_single_definition(self);
                binding_type(model.db, binding)
            }
        }
    };
}

impl_binding_has_ty!(ast::StmtFunctionDef);
impl_binding_has_ty!(ast::StmtClassDef);
impl_binding_has_ty!(ast::Parameter);
impl_binding_has_ty!(ast::ParameterWithDefault);
impl_binding_has_ty!(ast::ExceptHandlerExceptHandler);

impl HasType for ast::Alias {
    fn inferred_type<'db>(&self, model: &SemanticModel<'db>) -> Type<'db> {
        if &self.name == "*" {
            return Type::Never;
        }
        let index = semantic_index(model.db, model.file);
        binding_type(model.db, index.expect_single_definition(self))
    }
}

#[cfg(test)]
mod tests {
    use ruff_db::files::system_path_to_file;
    use ruff_db::parsed::parsed_module;

    use crate::db::tests::TestDbBuilder;
    use crate::{HasType, SemanticModel};

    #[test]
    fn function_type() -> anyhow::Result<()> {
        let db = TestDbBuilder::new()
            .with_file("/src/foo.py", "def test(): pass")
            .build()?;

        let foo = system_path_to_file(&db, "/src/foo.py").unwrap();

        let ast = parsed_module(&db, foo);

        let function = ast.suite()[0].as_function_def_stmt().unwrap();
        let model = SemanticModel::new(&db, foo);
        let ty = function.inferred_type(&model);

        assert!(ty.is_function_literal());

        Ok(())
    }

    #[test]
    fn class_type() -> anyhow::Result<()> {
        let db = TestDbBuilder::new()
            .with_file("/src/foo.py", "class Test: pass")
            .build()?;

        let foo = system_path_to_file(&db, "/src/foo.py").unwrap();

        let ast = parsed_module(&db, foo);

        let class = ast.suite()[0].as_class_def_stmt().unwrap();
        let model = SemanticModel::new(&db, foo);
        let ty = class.inferred_type(&model);

        assert!(ty.is_class_literal());

        Ok(())
    }

    #[test]
    fn alias_type() -> anyhow::Result<()> {
        let db = TestDbBuilder::new()
            .with_file("/src/foo.py", "class Test: pass")
            .with_file("/src/bar.py", "from foo import Test")
            .build()?;

        let bar = system_path_to_file(&db, "/src/bar.py").unwrap();

        let ast = parsed_module(&db, bar);

        let import = ast.suite()[0].as_import_from_stmt().unwrap();
        let alias = &import.names[0];
        let model = SemanticModel::new(&db, bar);
        let ty = alias.inferred_type(&model);

        assert!(ty.is_class_literal());

        Ok(())
    }
}
