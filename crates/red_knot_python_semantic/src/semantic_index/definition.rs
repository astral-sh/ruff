use ruff_db::files::File;
use ruff_db::parsed::ParsedModule;
use ruff_python_ast as ast;

use crate::ast_node_ref::AstNodeRef;
use crate::node_key::NodeKey;
use crate::semantic_index::symbol::{FileScopeId, ScopeId, ScopedSymbolId};
use crate::Db;

#[salsa::tracked]
pub struct Definition<'db> {
    /// The file in which the definition occurs.
    #[id]
    pub(crate) file: File,

    /// The scope in which the definition occurs.
    #[id]
    pub(crate) file_scope: FileScopeId,

    /// The symbol defined.
    #[id]
    pub(crate) symbol: ScopedSymbolId,

    #[no_eq]
    #[return_ref]
    pub(crate) node: DefinitionKind<'db>,

    #[no_eq]
    count: countme::Count<Definition<'static>>,
}

impl<'db> Definition<'db> {
    pub(crate) fn scope(self, db: &'db dyn Db) -> ScopeId<'db> {
        self.file_scope(db).to_scope_id(db, self.file(db))
    }
}

#[derive(Copy, Clone, Debug)]
pub(crate) enum DefinitionNodeRef<'a, 'ast> {
    Import(&'a ast::Alias<'ast>),
    ImportFrom(ImportFromDefinitionNodeRef<'a, 'ast>),
    Function(&'a ast::StmtFunctionDef<'ast>),
    Class(&'a ast::StmtClassDef<'ast>),
    NamedExpression(&'a ast::ExprNamed<'ast>),
    Assignment(AssignmentDefinitionNodeRef<'a, 'ast>),
    AnnotatedAssignment(&'a ast::StmtAnnAssign<'ast>),
}

impl<'a, 'ast> From<&'a ast::StmtFunctionDef<'ast>> for DefinitionNodeRef<'a, 'ast> {
    fn from(node: &'a ast::StmtFunctionDef<'ast>) -> Self {
        Self::Function(node)
    }
}

impl<'a, 'ast> From<&'a ast::StmtClassDef<'ast>> for DefinitionNodeRef<'a, 'ast> {
    fn from(node: &'a ast::StmtClassDef<'ast>) -> Self {
        Self::Class(node)
    }
}

impl<'a, 'ast> From<&'a ast::ExprNamed<'ast>> for DefinitionNodeRef<'a, 'ast> {
    fn from(node: &'a ast::ExprNamed<'ast>) -> Self {
        Self::NamedExpression(node)
    }
}

impl<'a, 'ast> From<&'a ast::StmtAnnAssign<'ast>> for DefinitionNodeRef<'a, 'ast> {
    fn from(node: &'a ast::StmtAnnAssign<'ast>) -> Self {
        Self::AnnotatedAssignment(node)
    }
}

impl<'a, 'ast> From<&'a ast::Alias<'ast>> for DefinitionNodeRef<'a, 'ast> {
    fn from(node_ref: &'a ast::Alias<'ast>) -> Self {
        Self::Import(node_ref)
    }
}

impl<'a, 'ast> From<ImportFromDefinitionNodeRef<'a, 'ast>> for DefinitionNodeRef<'a, 'ast> {
    fn from(node_ref: ImportFromDefinitionNodeRef<'a, 'ast>) -> Self {
        Self::ImportFrom(node_ref)
    }
}

impl<'a, 'ast> From<AssignmentDefinitionNodeRef<'a, 'ast>> for DefinitionNodeRef<'a, 'ast> {
    fn from(node_ref: AssignmentDefinitionNodeRef<'a, 'ast>) -> Self {
        Self::Assignment(node_ref)
    }
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct ImportFromDefinitionNodeRef<'a, 'ast> {
    pub(crate) node: &'a ast::StmtImportFrom<'ast>,
    pub(crate) alias_index: usize,
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct AssignmentDefinitionNodeRef<'a, 'ast> {
    pub(crate) assignment: &'a ast::StmtAssign<'ast>,
    pub(crate) target: &'a ast::ExprName<'ast>,
}

impl<'ast> DefinitionNodeRef<'_, 'ast> {
    #[allow(unsafe_code)]
    pub(super) unsafe fn into_owned(self, parsed: ParsedModule) -> DefinitionKind<'ast> {
        match self {
            DefinitionNodeRef::Import(alias) => {
                DefinitionKind::Import(AstNodeRef::new(parsed, alias))
            }
            DefinitionNodeRef::ImportFrom(ImportFromDefinitionNodeRef { node, alias_index }) => {
                DefinitionKind::ImportFrom(ImportFromDefinitionKind {
                    node: AstNodeRef::new(parsed, node),
                    alias_index,
                })
            }
            DefinitionNodeRef::Function(function) => {
                DefinitionKind::Function(AstNodeRef::new(parsed, function))
            }
            DefinitionNodeRef::Class(class) => {
                DefinitionKind::Class(AstNodeRef::new(parsed, class))
            }
            DefinitionNodeRef::NamedExpression(named) => {
                DefinitionKind::NamedExpression(AstNodeRef::new(parsed, named))
            }
            DefinitionNodeRef::Assignment(AssignmentDefinitionNodeRef { assignment, target }) => {
                DefinitionKind::Assignment(AssignmentDefinitionKind {
                    assignment: AstNodeRef::new(parsed.clone(), assignment),
                    target: AstNodeRef::new(parsed, target),
                })
            }
            DefinitionNodeRef::AnnotatedAssignment(assign) => {
                DefinitionKind::AnnotatedAssignment(AstNodeRef::new(parsed, assign))
            }
        }
    }

    pub(super) fn key(self) -> DefinitionNodeKey {
        match self {
            Self::Import(node) => node.into(),
            Self::ImportFrom(ImportFromDefinitionNodeRef { node, alias_index }) => {
                (&node.names[alias_index]).into()
            }
            Self::Function(node) => node.into(),
            Self::Class(node) => node.into(),
            Self::NamedExpression(node) => node.into(),
            Self::Assignment(AssignmentDefinitionNodeRef {
                assignment: _,
                target,
            }) => target.into(),
            Self::AnnotatedAssignment(node) => node.into(),
        }
    }
}

#[derive(Clone, Debug)]
pub enum DefinitionKind<'ast> {
    Import(AstNodeRef<ast::Alias<'ast>>),
    ImportFrom(ImportFromDefinitionKind<'ast>),
    Function(AstNodeRef<ast::StmtFunctionDef<'ast>>),
    Class(AstNodeRef<ast::StmtClassDef<'ast>>),
    NamedExpression(AstNodeRef<ast::ExprNamed<'ast>>),
    Assignment(AssignmentDefinitionKind<'ast>),
    AnnotatedAssignment(AstNodeRef<ast::StmtAnnAssign<'ast>>),
}

#[derive(Clone, Debug)]
pub struct ImportFromDefinitionKind<'ast> {
    node: AstNodeRef<ast::StmtImportFrom<'ast>>,
    alias_index: usize,
}

impl<'ast> ImportFromDefinitionKind<'ast> {
    pub(crate) fn import(&self) -> &ast::StmtImportFrom<'ast> {
        self.node.node()
    }

    pub(crate) fn alias(&self) -> &ast::Alias<'ast> {
        &self.node.node().names[self.alias_index]
    }
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct AssignmentDefinitionKind<'ast> {
    assignment: AstNodeRef<ast::StmtAssign<'ast>>,
    target: AstNodeRef<ast::ExprName<'ast>>,
}

impl<'ast> AssignmentDefinitionKind<'ast> {
    pub(crate) fn assignment(&self) -> &ast::StmtAssign<'ast> {
        self.assignment.node()
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub(crate) struct DefinitionNodeKey(NodeKey);

impl From<&ast::Alias<'_>> for DefinitionNodeKey {
    fn from(node: &ast::Alias) -> Self {
        Self(NodeKey::from_node(node))
    }
}

impl From<&ast::StmtFunctionDef<'_>> for DefinitionNodeKey {
    fn from(node: &ast::StmtFunctionDef) -> Self {
        Self(NodeKey::from_node(node))
    }
}

impl From<&ast::StmtClassDef<'_>> for DefinitionNodeKey {
    fn from(node: &ast::StmtClassDef) -> Self {
        Self(NodeKey::from_node(node))
    }
}

impl From<&ast::ExprName<'_>> for DefinitionNodeKey {
    fn from(node: &ast::ExprName) -> Self {
        Self(NodeKey::from_node(node))
    }
}

impl From<&ast::ExprNamed<'_>> for DefinitionNodeKey {
    fn from(node: &ast::ExprNamed) -> Self {
        Self(NodeKey::from_node(node))
    }
}

impl From<&ast::StmtAnnAssign<'_>> for DefinitionNodeKey {
    fn from(node: &ast::StmtAnnAssign) -> Self {
        Self(NodeKey::from_node(node))
    }
}
