use std::ops::Range;

use ruff_db::{files::File, parsed::ParsedModuleRef};
use ruff_index::newtype_index;
use ruff_python_ast::{self as ast, NodeIndex};

use crate::{
    Db, SemanticIndex, ast_node_ref::AstNodeRef, definition::Definition, node_key::NodeKey,
    semantic_index,
};

/// A cross-module identifier of a scope that can be used as a salsa query parameter.
#[salsa::tracked(debug, heap_size=ruff_memory_usage::heap_size)]
pub struct ScopeId<'db> {
    pub file: File,

    pub file_scope_id: FileScopeId,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for ScopeId<'_> {}

impl<'db> ScopeId<'db> {
    pub fn is_annotation(self, db: &'db dyn Db) -> bool {
        self.node(db).scope_kind().is_annotation()
    }

    pub fn node(self, db: &dyn Db) -> &NodeWithScopeKind {
        self.scope(db).node()
    }

    /// Returns `true` if this scope may require type context from its parent scope.
    pub fn accepts_type_context(self, db: &dyn Db) -> bool {
        matches!(
            self.node(db),
            NodeWithScopeKind::Lambda(_)
                | NodeWithScopeKind::ListComprehension(_)
                | NodeWithScopeKind::SetComprehension(_)
                | NodeWithScopeKind::DictComprehension(_)
                | NodeWithScopeKind::GeneratorExpression(_)
        )
    }

    pub fn scope(self, db: &dyn Db) -> &Scope {
        semantic_index(db, self.file(db)).scope(self.file_scope_id(db))
    }

    /// Returns the class definition for the enclosing class if this scope is a method body.
    pub fn class_definition_of_method(self, db: &'db dyn Db) -> Option<Definition<'db>> {
        semantic_index(db, self.file(db)).class_definition_of_method(self.file_scope_id(db))
    }

    pub fn is_method_scope(self, db: &'db dyn Db) -> bool {
        self.class_definition_of_method(db).is_some()
    }

    pub fn name<'ast>(self, db: &'db dyn Db, module: &'ast ParsedModuleRef) -> &'ast str {
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

    pub fn is_generator_function(self, index: &SemanticIndex) -> bool {
        index.generator_functions.contains(&self)
    }
}

#[derive(Debug, salsa::Update, get_size2::GetSize)]
pub struct Scope {
    /// The parent scope, if any.
    parent: Option<FileScopeId>,

    /// The node that introduces this scope.
    node: NodeWithScopeKind,

    /// The range of [`FileScopeId`]s that are descendants of this scope.
    descendants: Range<FileScopeId>,
}

impl Scope {
    pub(super) fn new(
        parent: Option<FileScopeId>,
        node: NodeWithScopeKind,
        descendants: Range<FileScopeId>,
    ) -> Self {
        Scope {
            parent,
            node,
            descendants,
        }
    }

    pub fn parent(&self) -> Option<FileScopeId> {
        self.parent
    }

    pub fn node(&self) -> &NodeWithScopeKind {
        &self.node
    }

    pub fn kind(&self) -> ScopeKind {
        self.node().scope_kind()
    }

    pub fn visibility(&self) -> ScopeVisibility {
        self.kind().visibility()
    }

    pub fn descendants(&self) -> Range<FileScopeId> {
        self.descendants.clone()
    }

    pub(super) fn extend_descendants(&mut self, children_end: FileScopeId) {
        self.descendants = self.descendants.start..children_end;
    }

    pub fn is_eager(&self) -> bool {
        self.kind().is_eager()
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash, get_size2::GetSize)]
pub enum ScopeVisibility {
    /// The scope is private (e.g. function, type alias, comprehension scope).
    Private,
    /// The scope is public (e.g. module, class scope).
    Public,
}

impl ScopeVisibility {
    pub(crate) const fn is_public(self) -> bool {
        matches!(self, ScopeVisibility::Public)
    }

    pub const fn is_private(self) -> bool {
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
pub enum ScopeKind {
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

    pub const fn is_function_like(self) -> bool {
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

    pub const fn is_class(self) -> bool {
        matches!(self, ScopeKind::Class)
    }

    pub const fn is_module(self) -> bool {
        matches!(self, ScopeKind::Module)
    }

    pub const fn is_annotation(self) -> bool {
        matches!(self, ScopeKind::TypeParams | ScopeKind::TypeAlias)
    }

    pub const fn is_non_lambda_function(self) -> bool {
        matches!(self, ScopeKind::Function)
    }
}

/// Reference to a node that introduces a new scope.
#[derive(Copy, Clone, Debug)]
pub enum NodeWithScopeRef<'a> {
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

    pub fn node_key(self) -> NodeWithScopeKey {
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
pub enum NodeWithScopeKind {
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
    pub const fn scope_kind(&self) -> ScopeKind {
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

    pub fn as_class(&self) -> Option<&AstNodeRef<ast::StmtClassDef>> {
        match self {
            Self::Class(class) => Some(class),
            _ => None,
        }
    }

    pub fn expect_class(&self) -> &AstNodeRef<ast::StmtClassDef> {
        self.as_class().expect("expected class")
    }

    pub fn as_function(&self) -> Option<&AstNodeRef<ast::StmtFunctionDef>> {
        match self {
            Self::Function(function) => Some(function),
            _ => None,
        }
    }

    pub fn expect_function(&self) -> &AstNodeRef<ast::StmtFunctionDef> {
        self.as_function().expect("expected function")
    }

    pub fn as_type_alias(&self) -> Option<&AstNodeRef<ast::StmtTypeAlias>> {
        match self {
            Self::TypeAlias(type_alias) => Some(type_alias),
            _ => None,
        }
    }

    pub fn expect_type_alias(&self) -> &AstNodeRef<ast::StmtTypeAlias> {
        self.as_type_alias().expect("expected type alias")
    }

    /// Returns the anchor node index for this scope, or `None` for the module scope.
    ///
    /// This is used to compute relative node indices for expressions within the scope,
    /// providing a stable anchor that only changes when the scope-introducing node changes.
    pub fn node_index(&self) -> Option<NodeIndex> {
        match self {
            Self::Module => None,
            Self::Class(class) => Some(class.index()),
            Self::ClassTypeParameters(class) => Some(class.index()),
            Self::Function(function) => Some(function.index()),
            Self::FunctionTypeParameters(function) => Some(function.index()),
            Self::TypeAlias(type_alias) => Some(type_alias.index()),
            Self::TypeAliasTypeParameters(type_alias) => Some(type_alias.index()),
            Self::Lambda(lambda) => Some(lambda.index()),
            Self::ListComprehension(comp) => Some(comp.index()),
            Self::SetComprehension(comp) => Some(comp.index()),
            Self::DictComprehension(comp) => Some(comp.index()),
            Self::GeneratorExpression(generator) => Some(generator.index()),
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, get_size2::GetSize)]
pub enum NodeWithScopeKey {
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
