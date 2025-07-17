use ruff_db::files::{File, FilePath};
use ruff_db::source::line_index;
use ruff_python_ast as ast;
use ruff_python_ast::{Expr, ExprRef, HasNodeIndex, name::Name};
use ruff_source_file::LineIndex;

use crate::Db;
use crate::module_name::ModuleName;
use crate::module_resolver::{KnownModule, Module, resolve_module};
use crate::semantic_index::place::FileScopeId;
use crate::semantic_index::semantic_index;
use crate::types::ide_support::all_declarations_and_bindings;
use crate::types::{Type, binding_type, infer_scope_types};

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
        line_index(self.db, self.file)
    }

    pub fn resolve_module(&self, module_name: &ModuleName) -> Option<Module> {
        resolve_module(self.db, module_name)
    }

    /// Returns completions for symbols available in a `from module import <CURSOR>` context.
    pub fn import_completions(
        &self,
        import: &ast::StmtImportFrom,
        _name: Option<usize>,
    ) -> Vec<Completion<'db>> {
        let module_name = match ModuleName::from_import_statement(self.db, self.file, import) {
            Ok(module_name) => module_name,
            Err(err) => {
                tracing::debug!(
                    "Could not extract module name from `{module:?}` with level {level}: {err:?}",
                    module = import.module,
                    level = import.level,
                );
                return vec![];
            }
        };
        self.module_completions(&module_name)
    }

    /// Returns completions for symbols available in the given module as if
    /// it were imported by this model's `File`.
    fn module_completions(&self, module_name: &ModuleName) -> Vec<Completion<'db>> {
        let Some(module) = resolve_module(self.db, module_name) else {
            tracing::debug!("Could not resolve module from `{module_name:?}`");
            return vec![];
        };
        let ty = Type::module_literal(self.db, self.file, &module);
        let builtin = module.is_known(KnownModule::Builtins);

        let mut completions = vec![];
        for crate::types::Member { name, ty } in crate::types::all_members(self.db, ty) {
            completions.push(Completion { name, ty, builtin });
        }
        for submodule_basename in module.all_submodules(self.db) {
            let Some(basename) = ModuleName::new(submodule_basename.as_str()) else {
                continue;
            };
            let mut submodule_name = module_name.clone();
            submodule_name.extend(&basename);

            let Some(submodule) = resolve_module(self.db, &submodule_name) else {
                continue;
            };
            let ty = Type::module_literal(self.db, self.file, &submodule);
            completions.push(Completion {
                name: submodule_basename,
                ty,
                builtin,
            });
        }
        completions
    }

    /// Returns completions for symbols available in a `object.<CURSOR>` context.
    pub fn attribute_completions(&self, node: &ast::ExprAttribute) -> Vec<Completion<'db>> {
        let ty = node.value.inferred_type(self);
        crate::types::all_members(self.db, ty)
            .into_iter()
            .map(|member| Completion {
                name: member.name,
                ty: member.ty,
                builtin: false,
            })
            .collect()
    }

    /// Returns completions for symbols available in the scope containing the
    /// given expression.
    ///
    /// If a scope could not be determined, then completions for the global
    /// scope of this model's `File` are returned.
    pub fn scoped_completions(&self, node: ast::AnyNodeRef<'_>) -> Vec<Completion<'db>> {
        let index = semantic_index(self.db, self.file);

        // TODO: We currently use `try_expression_scope_id` here as a hotfix for [1].
        // Revert this to use `expression_scope_id` once a proper fix is in place.
        //
        // [1] https://github.com/astral-sh/ty/issues/572
        let Some(file_scope) = (match node {
            ast::AnyNodeRef::Identifier(identifier) => index.try_expression_scope_id(identifier),
            node => match node.as_expr_ref() {
                // If we couldn't identify a specific
                // expression that we're in, then just
                // fall back to the global scope.
                None => Some(FileScopeId::global()),
                Some(expr) => index.try_expression_scope_id(&expr),
            },
        }) else {
            return vec![];
        };
        let mut completions = vec![];
        for (file_scope, _) in index.ancestor_scopes(file_scope) {
            completions.extend(
                all_declarations_and_bindings(self.db, file_scope.to_scope_id(self.db, self.file))
                    .map(|member| Completion {
                        name: member.name,
                        ty: member.ty,
                        builtin: false,
                    }),
            );
        }
        // Builtins are available in all scopes.
        let builtins = ModuleName::new("builtins").expect("valid module name");
        completions.extend(self.module_completions(&builtins));
        completions
    }
}

/// A classification of symbol names.
///
/// The ordering here is used for sorting completions.
///
/// This sorts "normal" names first, then dunder names and finally
/// single-underscore names. This matches the order of the variants defined for
/// this enum, which is in turn picked up by the derived trait implementation
/// for `Ord`.
#[derive(Clone, Copy, Eq, PartialEq, PartialOrd, Ord)]
pub enum NameKind {
    Normal,
    Dunder,
    Sunder,
}

impl NameKind {
    pub fn classify(name: &Name) -> NameKind {
        // Dunder needs a prefix and suffix double underscore.
        // When there's only a prefix double underscore, this
        // results in explicit name mangling. We let that be
        // classified as-if they were single underscore names.
        //
        // Ref: <https://docs.python.org/3/reference/lexical_analysis.html#reserved-classes-of-identifiers>
        if name.starts_with("__") && name.ends_with("__") {
            NameKind::Dunder
        } else if name.starts_with('_') {
            NameKind::Sunder
        } else {
            NameKind::Normal
        }
    }
}

/// A suggestion for code completion.
#[derive(Clone, Debug)]
pub struct Completion<'db> {
    /// The label shown to the user for this suggestion.
    pub name: Name,
    /// The type of this completion.
    pub ty: Type<'db>,
    /// Whether this suggestion came from builtins or not.
    ///
    /// At time of writing (2025-06-26), this information
    /// doesn't make it into the LSP response. Instead, we
    /// use it mainly in tests so that we can write less
    /// noisy tests.
    pub builtin: bool,
}

impl<'db> Completion<'db> {
    /// Returns the "kind" of this completion.
    ///
    /// This is meant to be a very general classification of this completion.
    /// Typically, this is communicated from the LSP server to a client, and
    /// the client uses this information to help improve the UX (perhaps by
    /// assigning an icon of some kind to the completion).
    pub fn kind(&self, db: &'db dyn Db) -> Option<CompletionKind> {
        fn imp<'db>(db: &'db dyn Db, ty: Type<'db>) -> Option<CompletionKind> {
            Some(match ty {
                Type::FunctionLiteral(_)
                | Type::DataclassDecorator(_)
                | Type::WrapperDescriptor(_)
                | Type::DataclassTransformer(_)
                | Type::Callable(_) => CompletionKind::Function,
                Type::BoundMethod(_) | Type::MethodWrapper(_) => CompletionKind::Method,
                Type::ModuleLiteral(_) => CompletionKind::Module,
                Type::ClassLiteral(_) | Type::GenericAlias(_) | Type::SubclassOf(_) => {
                    CompletionKind::Class
                }
                // This is a little weird for "struct." I'm mostly interpreting
                // "struct" here as a more general "object." ---AG
                Type::NominalInstance(_)
                | Type::PropertyInstance(_)
                | Type::Tuple(_)
                | Type::BoundSuper(_) => CompletionKind::Struct,
                Type::IntLiteral(_)
                | Type::BooleanLiteral(_)
                | Type::TypeIs(_)
                | Type::StringLiteral(_)
                | Type::LiteralString
                | Type::BytesLiteral(_) => CompletionKind::Value,
                Type::EnumLiteral(_) => CompletionKind::Enum,
                Type::ProtocolInstance(_) => CompletionKind::Interface,
                Type::TypeVar(_) => CompletionKind::TypeParameter,
                Type::Union(union) => union.elements(db).iter().find_map(|&ty| imp(db, ty))?,
                Type::Intersection(intersection) => {
                    intersection.iter_positive(db).find_map(|ty| imp(db, ty))?
                }
                Type::Dynamic(_)
                | Type::Never
                | Type::SpecialForm(_)
                | Type::KnownInstance(_)
                | Type::AlwaysTruthy
                | Type::AlwaysFalsy => return None,
            })
        }
        imp(db, self.ty)
    }
}

/// The "kind" of a completion.
///
/// This is taken directly from the LSP completion specification:
/// <https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#completionItemKind>
///
/// The idea here is that `Completion::kind` defines the mapping to this from
/// `Type` (and possibly other information), which might be interesting and
/// contentious. Then the outer edges map this to the LSP types, which is
/// expected to be mundane and boring.
#[derive(Clone, Copy, Debug)]
pub enum CompletionKind {
    Text,
    Method,
    Function,
    Constructor,
    Field,
    Variable,
    Class,
    Interface,
    Module,
    Property,
    Unit,
    Value,
    Enum,
    Keyword,
    Snippet,
    Color,
    File,
    Reference,
    Folder,
    EnumMember,
    Constant,
    Struct,
    Event,
    Operator,
    TypeParameter,
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
        let file_scope = index.expression_scope_id(self);
        let scope = file_scope.to_scope_id(model.db, model.file);

        infer_scope_types(model.db, scope).expression_type(*self)
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
impl_expression_has_type!(ast::ExprTString);
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
            Expr::TString(inner) => inner.inferred_type(model),
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

/// Implemented by types for which the semantic index tracks their scope.
pub(crate) trait HasTrackedScope: HasNodeIndex {}

impl HasTrackedScope for ast::Expr {}

impl HasTrackedScope for ast::ExprRef<'_> {}
impl HasTrackedScope for &ast::ExprRef<'_> {}

// See https://github.com/astral-sh/ty/issues/572 why this implementation exists
// even when we never register identifiers during semantic index building.
impl HasTrackedScope for ast::Identifier {}

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

        let ast = parsed_module(&db, foo).load(&db);

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

        let ast = parsed_module(&db, foo).load(&db);

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

        let ast = parsed_module(&db, bar).load(&db);

        let import = ast.suite()[0].as_import_from_stmt().unwrap();
        let alias = &import.names[0];
        let model = SemanticModel::new(&db, bar);
        let ty = alias.inferred_type(&model);

        assert!(ty.is_class_literal());

        Ok(())
    }
}
