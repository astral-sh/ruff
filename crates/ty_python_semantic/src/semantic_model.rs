use ruff_db::files::{File, FilePath};
use ruff_db::source::{line_index, source_text};
use ruff_python_ast::{self as ast, ExprStringLiteral, ModExpression};
use ruff_python_ast::{Expr, ExprRef, HasNodeIndex, name::Name};
use ruff_python_parser::Parsed;
use ruff_source_file::LineIndex;
use rustc_hash::FxHashMap;
use ty_module_resolver::{
    KnownModule, Module, ModuleName, list_modules, resolve_module, resolve_real_shadowable_module,
};

use crate::Db;
use crate::semantic_index::definition::Definition;
use crate::semantic_index::scope::FileScopeId;
use crate::semantic_index::semantic_index;
use crate::types::list_members::{Member, all_members, all_reachable_members};
use crate::types::{Type, binding_type, infer_scope_types};

/// The primary interface the LSP should use for querying semantic information about a [`File`].
///
/// Although you can in principle freely construct this type given a `db` and `file`, you should
/// try to construct this at the start of your analysis and thread the same instance through
/// the full analysis.
///
/// The primary reason for this is that it manages traversing into the sub-ASTs of string
/// annotations (see [`Self::enter_string_annotation`]). When you do this you will be handling
/// AST nodes that don't belong to the file's AST (or *any* file's AST). These kinds of nodes
/// will result in panics and confusing results if handed to the wrong subsystem. `SemanticModel`
/// methods will automatically handle using the string literal's AST node when necessary.
pub struct SemanticModel<'db> {
    db: &'db dyn Db,
    file: File,
    /// If `Some` then this `SemanticModel` is for analyzing the sub-AST of a string annotation.
    /// This expression will be used as a witness to the scope/location we're analyzing.
    in_string_annotation_expr: Option<Box<Expr>>,
}

impl<'db> SemanticModel<'db> {
    pub fn new(db: &'db dyn Db, file: File) -> Self {
        Self {
            db,
            file,
            in_string_annotation_expr: None,
        }
    }

    pub fn db(&self) -> &'db dyn Db {
        self.db
    }

    pub fn file(&self) -> File {
        self.file
    }

    pub fn file_path(&self) -> &FilePath {
        self.file.path(self.db)
    }

    pub fn line_index(&self) -> LineIndex {
        line_index(self.db, self.file)
    }

    /// Returns a map from symbol name to that symbol's
    /// type and definition site (if available).
    ///
    /// The symbols are the symbols in scope at the given
    /// AST node.
    pub fn members_in_scope_at(
        &self,
        node: ast::AnyNodeRef<'_>,
    ) -> FxHashMap<Name, MemberDefinition<'db>> {
        let index = semantic_index(self.db, self.file);
        let mut members = FxHashMap::default();
        let Some(file_scope) = self.scope(node) else {
            return members;
        };

        for (file_scope, _) in index.ancestor_scopes(file_scope) {
            for memberdef in
                all_reachable_members(self.db, file_scope.to_scope_id(self.db, self.file))
            {
                members.insert(
                    memberdef.member.name,
                    MemberDefinition {
                        ty: memberdef.member.ty,
                        first_reachable_definition: memberdef.first_reachable_definition,
                    },
                );
            }
        }
        members
    }

    /// Resolve the given import made in this file to a Type
    pub fn resolve_module_type(&self, module: Option<&str>, level: u32) -> Option<Type<'db>> {
        let module = self.resolve_module(module, level)?;
        Some(Type::module_literal(self.db, self.file, module))
    }

    /// Resolve the given import made in this file to a Module
    pub fn resolve_module(&self, module: Option<&str>, level: u32) -> Option<Module<'db>> {
        let module_name =
            ModuleName::from_identifier_parts(self.db, self.file, module, level).ok()?;
        resolve_module(self.db, self.file, &module_name)
    }

    /// Returns completions for symbols available in a `import <CURSOR>` context.
    pub fn import_completions(&self) -> Vec<Completion<'db>> {
        let typing_extensions = ModuleName::new_static("typing_extensions").unwrap();
        let is_typing_extensions_available = self.file.is_stub(self.db)
            || resolve_real_shadowable_module(self.db, self.file, &typing_extensions).is_some();
        list_modules(self.db)
            .into_iter()
            .filter(|module| {
                is_typing_extensions_available || module.name(self.db) != &typing_extensions
            })
            .map(|module| {
                let builtin = module.is_known(self.db, KnownModule::Builtins);
                let ty = Type::module_literal(self.db, self.file, module);
                Completion {
                    name: Name::new(module.name(self.db).as_str()),
                    ty: Some(ty),
                    builtin,
                }
            })
            .collect()
    }

    /// Returns completions for symbols available in a `from module import <CURSOR>` context.
    pub fn from_import_completions(&self, import: &ast::StmtImportFrom) -> Vec<Completion<'db>> {
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

    /// Returns submodule-only completions for the given module.
    pub fn import_submodule_completions_for_name(
        &self,
        module_name: &ModuleName,
    ) -> Vec<Completion<'db>> {
        let Some(module) = resolve_module(self.db, self.file, module_name) else {
            tracing::debug!("Could not resolve module from `{module_name:?}`");
            return vec![];
        };
        self.submodule_completions(&module)
    }

    /// Returns completions for symbols available in the given module as if
    /// it were imported by this model's `File`.
    fn module_completions(&self, module_name: &ModuleName) -> Vec<Completion<'db>> {
        let Some(module) = resolve_module(self.db, self.file, module_name) else {
            tracing::debug!("Could not resolve module from `{module_name:?}`");
            return vec![];
        };
        let ty = Type::module_literal(self.db, self.file, module);
        let builtin = module.is_known(self.db, KnownModule::Builtins);

        let mut completions = vec![];
        for Member { name, ty } in all_members(self.db, ty) {
            completions.push(Completion {
                name,
                ty: Some(ty),
                builtin,
            });
        }
        completions.extend(self.submodule_completions(&module));
        completions
    }

    /// Returns completions for submodules of the given module.
    fn submodule_completions(&self, module: &Module<'db>) -> Vec<Completion<'db>> {
        let builtin = module.is_known(self.db, KnownModule::Builtins);

        let mut completions = vec![];
        for submodule in module.all_submodules(self.db) {
            let ty = Type::module_literal(self.db, self.file, *submodule);
            let Some(base) = submodule.name(self.db).components().next_back() else {
                continue;
            };
            completions.push(Completion {
                name: Name::new(base),
                ty: Some(ty),
                builtin,
            });
        }
        completions
    }

    /// Returns completions for symbols available in a `object.<CURSOR>` context.
    pub fn attribute_completions(&self, node: &ast::ExprAttribute) -> Vec<Completion<'db>> {
        let Some(ty) = node.value.inferred_type(self) else {
            return Vec::new();
        };

        all_members(self.db, ty)
            .into_iter()
            .map(|member| Completion {
                name: member.name,
                ty: Some(member.ty),
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

        let Some(file_scope) = self.scope(node) else {
            return vec![];
        };
        let mut completions = vec![];
        for (file_scope, _) in index.ancestor_scopes(file_scope) {
            completions.extend(
                all_reachable_members(self.db, file_scope.to_scope_id(self.db, self.file)).map(
                    |memberdef| Completion {
                        name: memberdef.member.name,
                        ty: Some(memberdef.member.ty),
                        builtin: false,
                    },
                ),
            );
        }
        // Builtins are available in all scopes.
        let builtins = ModuleName::new_static("builtins").expect("valid module name");
        completions.extend(self.module_completions(&builtins));
        completions
    }

    /// Returns the scope in which `node` is defined (handles string annotations).
    pub fn scope(&self, node: ast::AnyNodeRef<'_>) -> Option<FileScopeId> {
        let index = semantic_index(self.db, self.file);
        match self.node_in_ast(node) {
            ast::AnyNodeRef::Identifier(identifier) => index.try_expression_scope_id(identifier),

            // Nodes implementing `HasDefinition`
            ast::AnyNodeRef::StmtFunctionDef(function) => Some(
                function
                    .definition(self)
                    .scope(self.db)
                    .file_scope_id(self.db),
            ),
            ast::AnyNodeRef::StmtClassDef(class) => {
                Some(class.definition(self).scope(self.db).file_scope_id(self.db))
            }
            ast::AnyNodeRef::Parameter(parameter) => Some(
                parameter
                    .definition(self)
                    .scope(self.db)
                    .file_scope_id(self.db),
            ),
            ast::AnyNodeRef::ParameterWithDefault(parameter) => Some(
                parameter
                    .definition(self)
                    .scope(self.db)
                    .file_scope_id(self.db),
            ),
            ast::AnyNodeRef::ExceptHandlerExceptHandler(handler) => Some(
                handler
                    .definition(self)
                    .scope(self.db)
                    .file_scope_id(self.db),
            ),
            ast::AnyNodeRef::TypeParamTypeVar(var) => {
                Some(var.definition(self).scope(self.db).file_scope_id(self.db))
            }

            // Fallback
            node => match node.as_expr_ref() {
                // If we couldn't identify a specific
                // expression that we're in, then just
                // fall back to the global scope.
                None => Some(FileScopeId::global()),
                Some(expr) => index.try_expression_scope_id(&expr),
            },
        }
    }

    /// Get a "safe" [`ast::AnyNodeRef`] to use for referring to the given (sub-)AST node.
    ///
    /// If we're analyzing a string annotation, it will return the string literal's node.
    /// Otherwise it will return the input.
    pub fn node_in_ast<'a>(&'a self, node: ast::AnyNodeRef<'a>) -> ast::AnyNodeRef<'a> {
        if let Some(string_annotation) = &self.in_string_annotation_expr {
            (&**string_annotation).into()
        } else {
            node
        }
    }

    /// Get a "safe" [`Expr`] to use for referring to the given (sub-)expression.
    ///
    /// If we're analyzing a string annotation, it will return the string literal's expression.
    /// Otherwise it will return the input.
    pub fn expr_in_ast<'a>(&'a self, expr: &'a Expr) -> &'a Expr {
        if let Some(string_annotation) = &self.in_string_annotation_expr {
            string_annotation
        } else {
            expr
        }
    }

    /// Get a "safe" [`ExprRef`] to use for referring to the given (sub-)expression.
    ///
    /// If we're analyzing a string annotation, it will return the string literal's expression.
    /// Otherwise it will return the input.
    pub fn expr_ref_in_ast<'a>(&'a self, expr: ExprRef<'a>) -> ExprRef<'a> {
        if let Some(string_annotation) = &self.in_string_annotation_expr {
            ExprRef::from(string_annotation)
        } else {
            expr
        }
    }

    /// Given a string expression, determine if it's a string annotation, and if it is,
    /// yield the parsed sub-AST and a sub-model that knows it's analyzing a sub-AST.
    ///
    /// Analysis of the sub-AST should only be done with the sub-model, or else things
    /// may return nonsense results or even panic!
    pub fn enter_string_annotation(
        &self,
        string_expr: &ExprStringLiteral,
    ) -> Option<(Parsed<ModExpression>, Self)> {
        // String annotations can't contain string annotations
        if self.in_string_annotation_expr.is_some() {
            return None;
        }

        // Ask the inference engine whether this is actually a string annotation
        let expr = ExprRef::StringLiteral(string_expr);
        let index = semantic_index(self.db, self.file);
        let file_scope = index.expression_scope_id(&expr);
        let scope = file_scope.to_scope_id(self.db, self.file);
        if !infer_scope_types(self.db, scope).is_string_annotation(expr) {
            return None;
        }

        // Parse the sub-AST and create a semantic model that knows it's in a sub-AST
        //
        // The string_annotation will be used as the expr/node for any query that needs
        // to look up a node in the AST to prevent panics, because these sub-AST nodes
        // are not in the File's AST!
        let source = source_text(self.db, self.file);
        let string_literal = string_expr.as_single_part_string()?;
        let ast =
            ruff_python_parser::parse_string_annotation(source.as_str(), string_literal).ok()?;
        let model = Self {
            db: self.db,
            file: self.file,
            in_string_annotation_expr: Some(Box::new(Expr::StringLiteral(string_expr.clone()))),
        };
        Some((ast, model))
    }
}

/// The type and definition of a symbol.
#[derive(Clone, Debug)]
pub struct MemberDefinition<'db> {
    pub ty: Type<'db>,
    pub first_reachable_definition: Definition<'db>,
}

/// A classification of symbol names.
///
/// The ordering here is used for sorting completions.
///
/// This sorts "normal" names first, then dunder names and finally
/// single-underscore names. This matches the order of the variants defined for
/// this enum, which is in turn picked up by the derived trait implementation
/// for `Ord`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
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
    /// The type of this completion, if available.
    ///
    /// Generally speaking, this is always available
    /// *unless* this was a completion corresponding to
    /// an unimported symbol. In that case, computing the
    /// type of all such symbols could be quite expensive.
    pub ty: Option<Type<'db>>,
    /// Whether this suggestion came from builtins or not.
    ///
    /// At time of writing (2025-06-26), this information
    /// doesn't make it into the LSP response. Instead, we
    /// use it mainly in tests so that we can write less
    /// noisy tests.
    pub builtin: bool,
}

impl<'db> Completion<'db> {
    pub fn is_type_check_only(&self, db: &'db dyn Db) -> bool {
        self.ty.is_some_and(|ty| ty.is_type_check_only(db))
    }
}

pub trait HasType {
    /// Returns the inferred type of `self`.
    ///
    /// ## Panics
    /// May panic if `self` is from another file than `model`.
    fn inferred_type<'db>(&self, model: &SemanticModel<'db>) -> Option<Type<'db>>;
}

pub trait HasDefinition {
    /// Returns the definition of `self`.
    ///
    /// ## Panics
    /// May panic if `self` is from another file than `model`.
    fn definition<'db>(&self, model: &SemanticModel<'db>) -> Definition<'db>;
}

impl HasType for ast::ExprRef<'_> {
    fn inferred_type<'db>(&self, model: &SemanticModel<'db>) -> Option<Type<'db>> {
        let index = semantic_index(model.db, model.file);
        // TODO(#1637): semantic tokens is making this crash even with
        // `try_expr_ref_in_ast` guarding this, for now just use `try_expression_scope_id`.
        // The problematic input is `x: "float` (with a dangling quote). I imagine the issue
        // is we're too eagerly setting `is_string_annotation` in inference.
        let file_scope = index.try_expression_scope_id(&model.expr_ref_in_ast(*self))?;
        let scope = file_scope.to_scope_id(model.db, model.file);

        infer_scope_types(model.db, scope).try_expression_type(*self)
    }
}

macro_rules! impl_expression_has_type {
    ($ty: ty) => {
        impl HasType for $ty {
            #[inline]
            fn inferred_type<'db>(&self, model: &SemanticModel<'db>) -> Option<Type<'db>> {
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
    fn inferred_type<'db>(&self, model: &SemanticModel<'db>) -> Option<Type<'db>> {
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

macro_rules! impl_binding_has_ty_def {
    ($ty: ty) => {
        impl HasDefinition for $ty {
            #[inline]
            fn definition<'db>(&self, model: &SemanticModel<'db>) -> Definition<'db> {
                let index = semantic_index(model.db, model.file);
                index.expect_single_definition(self)
            }
        }

        impl HasType for $ty {
            #[inline]
            fn inferred_type<'db>(&self, model: &SemanticModel<'db>) -> Option<Type<'db>> {
                let binding = HasDefinition::definition(self, model);
                Some(binding_type(model.db, binding))
            }
        }
    };
}

impl_binding_has_ty_def!(ast::StmtFunctionDef);
impl_binding_has_ty_def!(ast::StmtClassDef);
impl_binding_has_ty_def!(ast::Parameter);
impl_binding_has_ty_def!(ast::ParameterWithDefault);
impl_binding_has_ty_def!(ast::ExceptHandlerExceptHandler);
impl_binding_has_ty_def!(ast::TypeParamTypeVar);

impl HasType for ast::Alias {
    fn inferred_type<'db>(&self, model: &SemanticModel<'db>) -> Option<Type<'db>> {
        if &self.name == "*" {
            return Some(Type::Never);
        }
        let index = semantic_index(model.db, model.file);
        Some(binding_type(model.db, index.expect_single_definition(self)))
    }
}

/// Implemented by types for which the semantic index tracks their scope.
pub(crate) trait HasTrackedScope: HasNodeIndex {}

impl HasTrackedScope for ast::Expr {}

impl HasTrackedScope for ast::ExprRef<'_> {}
impl HasTrackedScope for &ast::ExprRef<'_> {}

// We never explicitly register the scope of an `Identifier`.
// However, `ExpressionsScopeMap` stores the text ranges of each scope.
// That allows us to look up the identifier's scope for as long as it's
// inside an expression (because the ranges overlap).
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
        let ty = function.inferred_type(&model).unwrap();

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
        let ty = class.inferred_type(&model).unwrap();

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
        let ty = alias.inferred_type(&model).unwrap();

        assert!(ty.is_class_literal());

        Ok(())
    }
}
