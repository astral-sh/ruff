use ruff_db::files::File;
use ruff_db::parsed::ParsedModule;
use ruff_index::newtype_index;
use ruff_python_ast as ast;

use crate::ast_node_ref::AstNodeRef;
use crate::node_key::NodeKey;
use crate::semantic_index::symbol::{FileScopeId, ScopeId, ScopedSymbolId};
use crate::Db;

#[salsa::tracked]
pub(crate) struct Definition<'db> {
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
    pub(crate) kind: DefinitionKind,

    #[no_eq]
    count: countme::Count<Definition<'static>>,
}

impl<'db> Definition<'db> {
    pub(crate) fn scope(self, db: &'db dyn Db) -> ScopeId<'db> {
        self.file_scope(db).to_scope_id(db, self.file(db))
    }
}

#[derive(Clone, Debug)]
pub(crate) enum DefinitionKind {
    /// Inserted at control-flow merge points, if multiple definitions can reach the merge point.
    ///
    /// Operands are not kept inline, since it's not possible to construct cyclically-referential
    /// Salsa tracked structs; they are kept instead in the
    /// [`UseDefMap`](super::use_def::UseDefMap).
    Phi(ScopedPhiId),

    /// An assignment to the symbol.
    Node(DefinitionNode),
}

#[newtype_index]
pub(crate) struct ScopedPhiId;

#[derive(Copy, Clone, Debug)]
pub(crate) enum DefinitionNodeRef<'a> {
    Import(&'a ast::Alias),
    ImportFrom(ImportFromDefinitionNodeRef<'a>),
    Function(&'a ast::StmtFunctionDef),
    Class(&'a ast::StmtClassDef),
    NamedExpression(&'a ast::ExprNamed),
    Assignment(AssignmentDefinitionNodeRef<'a>),
    AnnotatedAssignment(&'a ast::StmtAnnAssign),
    Comprehension(ComprehensionDefinitionNodeRef<'a>),
}

impl<'a> From<&'a ast::StmtFunctionDef> for DefinitionNodeRef<'a> {
    fn from(node: &'a ast::StmtFunctionDef) -> Self {
        Self::Function(node)
    }
}

impl<'a> From<&'a ast::StmtClassDef> for DefinitionNodeRef<'a> {
    fn from(node: &'a ast::StmtClassDef) -> Self {
        Self::Class(node)
    }
}

impl<'a> From<&'a ast::ExprNamed> for DefinitionNodeRef<'a> {
    fn from(node: &'a ast::ExprNamed) -> Self {
        Self::NamedExpression(node)
    }
}

impl<'a> From<&'a ast::StmtAnnAssign> for DefinitionNodeRef<'a> {
    fn from(node: &'a ast::StmtAnnAssign) -> Self {
        Self::AnnotatedAssignment(node)
    }
}

impl<'a> From<&'a ast::Alias> for DefinitionNodeRef<'a> {
    fn from(node_ref: &'a ast::Alias) -> Self {
        Self::Import(node_ref)
    }
}

impl<'a> From<ImportFromDefinitionNodeRef<'a>> for DefinitionNodeRef<'a> {
    fn from(node_ref: ImportFromDefinitionNodeRef<'a>) -> Self {
        Self::ImportFrom(node_ref)
    }
}

impl<'a> From<AssignmentDefinitionNodeRef<'a>> for DefinitionNodeRef<'a> {
    fn from(node_ref: AssignmentDefinitionNodeRef<'a>) -> Self {
        Self::Assignment(node_ref)
    }
}

impl<'a> From<ComprehensionDefinitionNodeRef<'a>> for DefinitionNodeRef<'a> {
    fn from(node: ComprehensionDefinitionNodeRef<'a>) -> Self {
        Self::Comprehension(node)
    }
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct ImportFromDefinitionNodeRef<'a> {
    pub(crate) node: &'a ast::StmtImportFrom,
    pub(crate) alias_index: usize,
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct AssignmentDefinitionNodeRef<'a> {
    pub(crate) assignment: &'a ast::StmtAssign,
    pub(crate) target: &'a ast::ExprName,
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct ComprehensionDefinitionNodeRef<'a> {
    pub(crate) node: &'a ast::Comprehension,
    pub(crate) first: bool,
}

impl DefinitionNodeRef<'_> {
    #[allow(unsafe_code)]
    pub(super) unsafe fn into_owned(self, parsed: ParsedModule) -> DefinitionNode {
        match self {
            DefinitionNodeRef::Import(alias) => {
                DefinitionNode::Import(AstNodeRef::new(parsed, alias))
            }
            DefinitionNodeRef::ImportFrom(ImportFromDefinitionNodeRef { node, alias_index }) => {
                DefinitionNode::ImportFrom(ImportFromDefinitionNode {
                    node: AstNodeRef::new(parsed, node),
                    alias_index,
                })
            }
            DefinitionNodeRef::Function(function) => {
                DefinitionNode::Function(AstNodeRef::new(parsed, function))
            }
            DefinitionNodeRef::Class(class) => {
                DefinitionNode::Class(AstNodeRef::new(parsed, class))
            }
            DefinitionNodeRef::NamedExpression(named) => {
                DefinitionNode::NamedExpression(AstNodeRef::new(parsed, named))
            }
            DefinitionNodeRef::Assignment(AssignmentDefinitionNodeRef { assignment, target }) => {
                DefinitionNode::Assignment(AssignmentDefinitionNode {
                    assignment: AstNodeRef::new(parsed.clone(), assignment),
                    target: AstNodeRef::new(parsed, target),
                })
            }
            DefinitionNodeRef::AnnotatedAssignment(assign) => {
                DefinitionNode::AnnotatedAssignment(AstNodeRef::new(parsed, assign))
            }
            DefinitionNodeRef::Comprehension(ComprehensionDefinitionNodeRef { node, first }) => {
                DefinitionNode::Comprehension(ComprehensionDefinitionNode {
                    node: AstNodeRef::new(parsed, node),
                    first,
                })
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
            Self::Comprehension(ComprehensionDefinitionNodeRef { node, first: _ }) => node.into(),
        }
    }
}

#[derive(Clone, Debug)]
pub enum DefinitionNode {
    Import(AstNodeRef<ast::Alias>),
    ImportFrom(ImportFromDefinitionNode),
    Function(AstNodeRef<ast::StmtFunctionDef>),
    Class(AstNodeRef<ast::StmtClassDef>),
    NamedExpression(AstNodeRef<ast::ExprNamed>),
    Assignment(AssignmentDefinitionNode),
    AnnotatedAssignment(AstNodeRef<ast::StmtAnnAssign>),
    Comprehension(ComprehensionDefinitionNode),
}

#[derive(Clone, Debug)]
pub struct ComprehensionDefinitionNode {
    node: AstNodeRef<ast::Comprehension>,
    first: bool,
}

impl ComprehensionDefinitionNode {
    pub(crate) fn node(&self) -> &ast::Comprehension {
        self.node.node()
    }

    pub(crate) fn is_first(&self) -> bool {
        self.first
    }
}

#[derive(Clone, Debug)]
pub struct ImportFromDefinitionNode {
    node: AstNodeRef<ast::StmtImportFrom>,
    alias_index: usize,
}

impl ImportFromDefinitionNode {
    pub(crate) fn import(&self) -> &ast::StmtImportFrom {
        self.node.node()
    }

    pub(crate) fn alias(&self) -> &ast::Alias {
        &self.node.node().names[self.alias_index]
    }
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct AssignmentDefinitionNode {
    assignment: AstNodeRef<ast::StmtAssign>,
    target: AstNodeRef<ast::ExprName>,
}

impl AssignmentDefinitionNode {
    pub(crate) fn assignment(&self) -> &ast::StmtAssign {
        self.assignment.node()
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub(crate) struct DefinitionNodeKey(NodeKey);

impl From<&ast::Alias> for DefinitionNodeKey {
    fn from(node: &ast::Alias) -> Self {
        Self(NodeKey::from_node(node))
    }
}

impl From<&ast::StmtFunctionDef> for DefinitionNodeKey {
    fn from(node: &ast::StmtFunctionDef) -> Self {
        Self(NodeKey::from_node(node))
    }
}

impl From<&ast::StmtClassDef> for DefinitionNodeKey {
    fn from(node: &ast::StmtClassDef) -> Self {
        Self(NodeKey::from_node(node))
    }
}

impl From<&ast::ExprName> for DefinitionNodeKey {
    fn from(node: &ast::ExprName) -> Self {
        Self(NodeKey::from_node(node))
    }
}

impl From<&ast::ExprNamed> for DefinitionNodeKey {
    fn from(node: &ast::ExprNamed) -> Self {
        Self(NodeKey::from_node(node))
    }
}

impl From<&ast::StmtAnnAssign> for DefinitionNodeKey {
    fn from(node: &ast::StmtAnnAssign) -> Self {
        Self(NodeKey::from_node(node))
    }
}

impl From<&ast::Comprehension> for DefinitionNodeKey {
    fn from(node: &ast::Comprehension) -> Self {
        Self(NodeKey::from_node(node))
    }
}
