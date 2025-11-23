use std::ops::Range;

use ruff_db::{files::File, parsed::ParsedModuleRef};
use ruff_index::newtype_index;
use ruff_python_ast as ast;

use crate::{
    Db,
    ast_node_ref::AstNodeRef,
    node_key::NodeKey,
    semantic_index::{
        SemanticIndex, reachability_constraints::ScopedReachabilityConstraintId, semantic_index,
    },
    types::{GenericContext, binding_type, infer_definition_types},
};

/// A cross-module identifier of a scope that can be used as a salsa query parameter.
#[salsa::tracked(debug, heap_size=ruff_memory_usage::heap_size)]
#[derive(PartialOrd, Ord)]
pub struct ScopeId<'db> {
    pub file: File,

    pub file_scope_id: FileScopeId,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for ScopeId<'_> {}

impl<'db> ScopeId<'db> {
    pub(crate) fn is_annotation(self, db: &'db dyn Db) -> bool {
        self.node(db).scope_kind().is_annotation()
    }

    pub(crate) fn node(self, db: &dyn Db) -> &NodeWithScopeKind {
        self.scope(db).node()
    }

    pub(crate) fn scope(self, db: &dyn Db) -> &Scope {
        semantic_index(db, self.file(db)).scope(self.file_scope_id(db))
    }

    #[cfg(test)]
    pub(crate) fn name<'ast>(self, db: &'db dyn Db, module: &'ast ParsedModuleRef) -> &'ast str {
        match self.node(db) {
            NodeWithScopeKind::Module => "<module>",
            NodeWithScopeKind::Class(class) | NodeWithScopeKind::ClassTypeParameters(class) => {
                class.node(module).name.as_str()
            }
            NodeWithScopeKind::Function(function)
            | NodeWithScopeKind::FunctionTypeParameters(function) => {
                function.node(module).name.as_str()
            }
            NodeWithScopeKind::TypeAlias(type_alias)
            | NodeWithScopeKind::TypeAliasTypeParameters(type_alias) => type_alias
                .node(module)
                .name
                .as_name_expr()
                .map(|name| name.id.as_str())
                .unwrap_or("<type alias>"),
            NodeWithScopeKind::Lambda(_) => "<lambda>",
            NodeWithScopeKind::ListComprehension(_) => "<listcomp>",
            NodeWithScopeKind::SetComprehension(_) => "<setcomp>",
            NodeWithScopeKind::DictComprehension(_) => "<dictcomp>",
            NodeWithScopeKind::GeneratorExpression(_) => "<generator>",
        }
    }
}

/// ID that uniquely identifies a scope inside of a module.
#[newtype_index]
#[derive(salsa::Update, get_size2::GetSize)]
pub struct FileScopeId;

impl FileScopeId {
    /// Returns the scope id of the module-global scope.
    pub fn global() -> Self {
        FileScopeId::from_u32(0)
    }

    pub fn is_global(self) -> bool {
        self == FileScopeId::global()
    }

    pub fn to_scope_id(self, db: &dyn Db, file: File) -> ScopeId<'_> {
        let index = semantic_index(db, file);
        index.scope_ids_by_scope[self]
    }

    pub(crate) fn is_generator_function(self, index: &SemanticIndex) -> bool {
        index.generator_functions.contains(&self)
    }
}

#[derive(Debug, salsa::Update, get_size2::GetSize)]
pub(crate) struct Scope {
    /// The parent scope, if any.
    parent: Option<FileScopeId>,

    /// The node that introduces this scope.
    node: NodeWithScopeKind,

    /// The range of [`FileScopeId`]s that are descendants of this scope.
    descendants: Range<FileScopeId>,

    /// The constraint that determines the reachability of this scope.
    reachability: ScopedReachabilityConstraintId,

    /// Whether this scope is defined inside an `if TYPE_CHECKING:` block.
    in_type_checking_block: bool,
}

impl Scope {
    pub(super) fn new(
        parent: Option<FileScopeId>,
        node: NodeWithScopeKind,
        descendants: Range<FileScopeId>,
        reachability: ScopedReachabilityConstraintId,
        in_type_checking_block: bool,
    ) -> Self {
        Scope {
            parent,
            node,
            descendants,
            reachability,
            in_type_checking_block,
        }
    }

    pub(crate) fn parent(&self) -> Option<FileScopeId> {
        self.parent
    }

    pub(crate) fn node(&self) -> &NodeWithScopeKind {
        &self.node
    }

    pub(crate) fn kind(&self) -> ScopeKind {
        self.node().scope_kind()
    }

    pub(crate) fn visibility(&self) -> ScopeVisibility {
        self.kind().visibility()
    }

    pub(crate) fn descendants(&self) -> Range<FileScopeId> {
        self.descendants.clone()
    }

    pub(super) fn extend_descendants(&mut self, children_end: FileScopeId) {
        self.descendants = self.descendants.start..children_end;
    }

    pub(crate) fn is_eager(&self) -> bool {
        self.kind().is_eager()
    }

    pub(crate) fn reachability(&self) -> ScopedReachabilityConstraintId {
        self.reachability
    }

    pub(crate) fn in_type_checking_block(&self) -> bool {
        self.in_type_checking_block
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash, get_size2::GetSize)]
pub(crate) enum ScopeVisibility {
    /// The scope is private (e.g. function, type alias, comprehension scope).
    Private,
    /// The scope is public (e.g. module, class scope).
    Public,
}

impl ScopeVisibility {
    pub(crate) const fn is_public(self) -> bool {
        matches!(self, ScopeVisibility::Public)
    }

    pub(crate) const fn is_private(self) -> bool {
        matches!(self, ScopeVisibility::Private)
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash, get_size2::GetSize)]
pub(crate) enum ScopeLaziness {
    /// The scope is evaluated lazily (e.g. function, type alias scope).
    Lazy,
    /// The scope is evaluated eagerly (e.g. module, class, comprehension scope).
    Eager,
}

impl ScopeLaziness {
    pub(crate) const fn is_eager(self) -> bool {
        matches!(self, ScopeLaziness::Eager)
    }

    pub(crate) const fn is_lazy(self) -> bool {
        matches!(self, ScopeLaziness::Lazy)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum ScopeKind {
    Module,
    TypeParams,
    Class,
    Function,
    Lambda,
    Comprehension,
    TypeAlias,
}

impl ScopeKind {
    pub(crate) const fn is_eager(self) -> bool {
        self.laziness().is_eager()
    }

    pub(crate) const fn laziness(self) -> ScopeLaziness {
        match self {
            ScopeKind::Module
            | ScopeKind::Class
            | ScopeKind::Comprehension
            | ScopeKind::TypeParams => ScopeLaziness::Eager,
            ScopeKind::Function | ScopeKind::Lambda | ScopeKind::TypeAlias => ScopeLaziness::Lazy,
        }
    }

    pub(crate) const fn visibility(self) -> ScopeVisibility {
        match self {
            ScopeKind::Module | ScopeKind::Class => ScopeVisibility::Public,
            ScopeKind::TypeParams
            | ScopeKind::TypeAlias
            | ScopeKind::Function
            | ScopeKind::Lambda
            | ScopeKind::Comprehension => ScopeVisibility::Private,
        }
    }

    pub(crate) const fn is_function_like(self) -> bool {
        // Type parameter scopes behave like function scopes in terms of name resolution; CPython
        // symbol table also uses the term "function-like" for these scopes.
        matches!(
            self,
            ScopeKind::TypeParams
                | ScopeKind::Function
                | ScopeKind::Lambda
                | ScopeKind::TypeAlias
                | ScopeKind::Comprehension
        )
    }

    pub(crate) const fn is_class(self) -> bool {
        matches!(self, ScopeKind::Class)
    }

    pub(crate) const fn is_module(self) -> bool {
        matches!(self, ScopeKind::Module)
    }

    pub(crate) const fn is_annotation(self) -> bool {
        matches!(self, ScopeKind::TypeParams | ScopeKind::TypeAlias)
    }

    pub(crate) const fn is_non_lambda_function(self) -> bool {
        matches!(self, ScopeKind::Function)
    }
}

/// Reference to a node that introduces a new scope.
#[derive(Copy, Clone, Debug)]
pub(crate) enum NodeWithScopeRef<'a> {
    Module,
    Class(&'a ast::StmtClassDef),
    Function(&'a ast::StmtFunctionDef),
    Lambda(&'a ast::ExprLambda),
    FunctionTypeParameters(&'a ast::StmtFunctionDef),
    ClassTypeParameters(&'a ast::StmtClassDef),
    TypeAlias(&'a ast::StmtTypeAlias),
    TypeAliasTypeParameters(&'a ast::StmtTypeAlias),
    ListComprehension(&'a ast::ExprListComp),
    SetComprehension(&'a ast::ExprSetComp),
    DictComprehension(&'a ast::ExprDictComp),
    GeneratorExpression(&'a ast::ExprGenerator),
}

impl NodeWithScopeRef<'_> {
    /// Converts the unowned reference to an owned [`NodeWithScopeKind`].
    ///
    /// Note that node wrapped by `self` must be a child of `module`.
    pub(super) fn to_kind(self, module: &ParsedModuleRef) -> NodeWithScopeKind {
        match self {
            NodeWithScopeRef::Module => NodeWithScopeKind::Module,
            NodeWithScopeRef::Class(class) => {
                NodeWithScopeKind::Class(AstNodeRef::new(module, class))
            }
            NodeWithScopeRef::Function(function) => {
                NodeWithScopeKind::Function(AstNodeRef::new(module, function))
            }
            NodeWithScopeRef::TypeAlias(type_alias) => {
                NodeWithScopeKind::TypeAlias(AstNodeRef::new(module, type_alias))
            }
            NodeWithScopeRef::TypeAliasTypeParameters(type_alias) => {
                NodeWithScopeKind::TypeAliasTypeParameters(AstNodeRef::new(module, type_alias))
            }
            NodeWithScopeRef::Lambda(lambda) => {
                NodeWithScopeKind::Lambda(AstNodeRef::new(module, lambda))
            }
            NodeWithScopeRef::FunctionTypeParameters(function) => {
                NodeWithScopeKind::FunctionTypeParameters(AstNodeRef::new(module, function))
            }
            NodeWithScopeRef::ClassTypeParameters(class) => {
                NodeWithScopeKind::ClassTypeParameters(AstNodeRef::new(module, class))
            }
            NodeWithScopeRef::ListComprehension(comprehension) => {
                NodeWithScopeKind::ListComprehension(AstNodeRef::new(module, comprehension))
            }
            NodeWithScopeRef::SetComprehension(comprehension) => {
                NodeWithScopeKind::SetComprehension(AstNodeRef::new(module, comprehension))
            }
            NodeWithScopeRef::DictComprehension(comprehension) => {
                NodeWithScopeKind::DictComprehension(AstNodeRef::new(module, comprehension))
            }
            NodeWithScopeRef::GeneratorExpression(generator) => {
                NodeWithScopeKind::GeneratorExpression(AstNodeRef::new(module, generator))
            }
        }
    }

    pub(crate) fn node_key(self) -> NodeWithScopeKey {
        match self {
            NodeWithScopeRef::Module => NodeWithScopeKey::Module,
            NodeWithScopeRef::Class(class) => NodeWithScopeKey::Class(NodeKey::from_node(class)),
            NodeWithScopeRef::Function(function) => {
                NodeWithScopeKey::Function(NodeKey::from_node(function))
            }
            NodeWithScopeRef::Lambda(lambda) => {
                NodeWithScopeKey::Lambda(NodeKey::from_node(lambda))
            }
            NodeWithScopeRef::FunctionTypeParameters(function) => {
                NodeWithScopeKey::FunctionTypeParameters(NodeKey::from_node(function))
            }
            NodeWithScopeRef::ClassTypeParameters(class) => {
                NodeWithScopeKey::ClassTypeParameters(NodeKey::from_node(class))
            }
            NodeWithScopeRef::TypeAlias(type_alias) => {
                NodeWithScopeKey::TypeAlias(NodeKey::from_node(type_alias))
            }
            NodeWithScopeRef::TypeAliasTypeParameters(type_alias) => {
                NodeWithScopeKey::TypeAliasTypeParameters(NodeKey::from_node(type_alias))
            }
            NodeWithScopeRef::ListComprehension(comprehension) => {
                NodeWithScopeKey::ListComprehension(NodeKey::from_node(comprehension))
            }
            NodeWithScopeRef::SetComprehension(comprehension) => {
                NodeWithScopeKey::SetComprehension(NodeKey::from_node(comprehension))
            }
            NodeWithScopeRef::DictComprehension(comprehension) => {
                NodeWithScopeKey::DictComprehension(NodeKey::from_node(comprehension))
            }
            NodeWithScopeRef::GeneratorExpression(generator) => {
                NodeWithScopeKey::GeneratorExpression(NodeKey::from_node(generator))
            }
        }
    }
}

/// Node that introduces a new scope.
#[derive(Clone, Debug, salsa::Update, get_size2::GetSize)]
pub(crate) enum NodeWithScopeKind {
    Module,
    Class(AstNodeRef<ast::StmtClassDef>),
    ClassTypeParameters(AstNodeRef<ast::StmtClassDef>),
    Function(AstNodeRef<ast::StmtFunctionDef>),
    FunctionTypeParameters(AstNodeRef<ast::StmtFunctionDef>),
    TypeAliasTypeParameters(AstNodeRef<ast::StmtTypeAlias>),
    TypeAlias(AstNodeRef<ast::StmtTypeAlias>),
    Lambda(AstNodeRef<ast::ExprLambda>),
    ListComprehension(AstNodeRef<ast::ExprListComp>),
    SetComprehension(AstNodeRef<ast::ExprSetComp>),
    DictComprehension(AstNodeRef<ast::ExprDictComp>),
    GeneratorExpression(AstNodeRef<ast::ExprGenerator>),
}

impl NodeWithScopeKind {
    pub(crate) const fn scope_kind(&self) -> ScopeKind {
        match self {
            Self::Module => ScopeKind::Module,
            Self::Class(_) => ScopeKind::Class,
            Self::Function(_) => ScopeKind::Function,
            Self::Lambda(_) => ScopeKind::Lambda,
            Self::FunctionTypeParameters(_)
            | Self::ClassTypeParameters(_)
            | Self::TypeAliasTypeParameters(_) => ScopeKind::TypeParams,
            Self::TypeAlias(_) => ScopeKind::TypeAlias,
            Self::ListComprehension(_)
            | Self::SetComprehension(_)
            | Self::DictComprehension(_)
            | Self::GeneratorExpression(_) => ScopeKind::Comprehension,
        }
    }

    pub(crate) fn as_class(&self) -> Option<&AstNodeRef<ast::StmtClassDef>> {
        match self {
            Self::Class(class) => Some(class),
            _ => None,
        }
    }

    pub(crate) fn expect_class(&self) -> &AstNodeRef<ast::StmtClassDef> {
        self.as_class().expect("expected class")
    }

    pub(crate) fn as_function(&self) -> Option<&AstNodeRef<ast::StmtFunctionDef>> {
        match self {
            Self::Function(function) => Some(function),
            _ => None,
        }
    }

    pub(crate) fn expect_function(&self) -> &AstNodeRef<ast::StmtFunctionDef> {
        self.as_function().expect("expected function")
    }

    pub(crate) fn as_type_alias(&self) -> Option<&AstNodeRef<ast::StmtTypeAlias>> {
        match self {
            Self::TypeAlias(type_alias) => Some(type_alias),
            _ => None,
        }
    }

    pub(crate) fn expect_type_alias(&self) -> &AstNodeRef<ast::StmtTypeAlias> {
        self.as_type_alias().expect("expected type alias")
    }

    pub(crate) fn generic_context<'db>(
        &self,
        db: &'db dyn Db,
        index: &SemanticIndex<'db>,
    ) -> Option<GenericContext<'db>> {
        match self {
            NodeWithScopeKind::Class(class) => {
                let definition = index.expect_single_definition(class);
                binding_type(db, definition)
                    .as_class_literal()?
                    .generic_context(db)
            }
            NodeWithScopeKind::Function(function) => {
                let definition = index.expect_single_definition(function);
                infer_definition_types(db, definition)
                    .undecorated_type()
                    .expect("function should have undecorated type")
                    .as_function_literal()?
                    .last_definition_signature(db)
                    .generic_context
            }
            NodeWithScopeKind::TypeAlias(type_alias) => {
                let definition = index.expect_single_definition(type_alias);
                binding_type(db, definition)
                    .as_type_alias()?
                    .as_pep_695_type_alias()?
                    .generic_context(db)
            }
            _ => None,
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, get_size2::GetSize)]
pub(crate) enum NodeWithScopeKey {
    Module,
    Class(NodeKey),
    ClassTypeParameters(NodeKey),
    Function(NodeKey),
    FunctionTypeParameters(NodeKey),
    TypeAlias(NodeKey),
    TypeAliasTypeParameters(NodeKey),
    Lambda(NodeKey),
    ListComprehension(NodeKey),
    SetComprehension(NodeKey),
    DictComprehension(NodeKey),
    GeneratorExpression(NodeKey),
}
