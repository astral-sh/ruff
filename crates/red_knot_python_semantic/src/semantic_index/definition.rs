use ruff_db::files::File;
use ruff_db::parsed::ParsedModule;
use ruff_python_ast as ast;

use crate::ast_node_ref::AstNodeRef;
use crate::node_key::NodeKey;
use crate::semantic_index::symbol::{FileScopeId, ScopedSymbolId};

#[salsa::tracked]
pub struct Definition<'db> {
    /// The file in which the definition is defined.
    #[id]
    pub(super) file: File,

    /// The scope in which the definition is defined.
    #[id]
    pub(crate) scope: FileScopeId,

    /// The id of the corresponding symbol. Mainly used as ID.
    #[id]
    symbol_id: ScopedSymbolId,

    #[no_eq]
    #[return_ref]
    pub(crate) node: DefinitionKind,
}

#[derive(Copy, Clone, Debug)]
pub(crate) enum DefinitionNodeRef<'a> {
    Alias(&'a ast::Alias),
    Function(&'a ast::StmtFunctionDef),
    Class(&'a ast::StmtClassDef),
    NamedExpression(&'a ast::ExprNamed),
    Target(&'a ast::Expr),
}

impl<'a> From<&'a ast::Alias> for DefinitionNodeRef<'a> {
    fn from(node: &'a ast::Alias) -> Self {
        Self::Alias(node)
    }
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

impl DefinitionNodeRef<'_> {
    #[allow(unsafe_code)]
    pub(super) unsafe fn into_owned(self, parsed: ParsedModule) -> DefinitionKind {
        match self {
            DefinitionNodeRef::Alias(alias) => {
                DefinitionKind::Alias(AstNodeRef::new(parsed, alias))
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
            DefinitionNodeRef::Target(target) => {
                DefinitionKind::Target(AstNodeRef::new(parsed, target))
            }
        }
    }
}

impl DefinitionNodeRef<'_> {
    pub(super) fn key(self) -> DefinitionNodeKey {
        match self {
            Self::Alias(node) => DefinitionNodeKey(NodeKey::from_node(node)),
            Self::Function(node) => DefinitionNodeKey(NodeKey::from_node(node)),
            Self::Class(node) => DefinitionNodeKey(NodeKey::from_node(node)),
            Self::NamedExpression(node) => DefinitionNodeKey(NodeKey::from_node(node)),
            Self::Target(node) => DefinitionNodeKey(NodeKey::from_node(node)),
        }
    }
}

#[derive(Clone, Debug)]
pub enum DefinitionKind {
    Alias(AstNodeRef<ast::Alias>),
    Function(AstNodeRef<ast::StmtFunctionDef>),
    Class(AstNodeRef<ast::StmtClassDef>),
    NamedExpression(AstNodeRef<ast::ExprNamed>),
    Target(AstNodeRef<ast::Expr>),
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub(super) struct DefinitionNodeKey(NodeKey);
