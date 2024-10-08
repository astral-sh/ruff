use ruff_db::files::File;
use ruff_db::parsed::ParsedModule;
use ruff_python_ast as ast;

use crate::ast_node_ref::AstNodeRef;
use crate::module_resolver::file_to_module;
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
    pub(crate) kind: DefinitionKind,

    #[no_eq]
    count: countme::Count<Definition<'static>>,
}

impl<'db> Definition<'db> {
    pub(crate) fn scope(self, db: &'db dyn Db) -> ScopeId<'db> {
        self.file_scope(db).to_scope_id(db, self.file(db))
    }

    pub(crate) fn category(self, db: &'db dyn Db) -> DefinitionCategory {
        self.kind(db).category()
    }

    pub(crate) fn is_declaration(self, db: &'db dyn Db) -> bool {
        self.kind(db).category().is_declaration()
    }

    pub(crate) fn is_binding(self, db: &'db dyn Db) -> bool {
        self.kind(db).category().is_binding()
    }

    /// Return true if this is a symbol was defined in the `typing` or `typing_extensions` modules
    pub(crate) fn is_typing_definition(self, db: &'db dyn Db) -> bool {
        file_to_module(db, self.file(db)).is_some_and(|module| {
            module.search_path().is_standard_library()
                && matches!(&**module.name(), "typing" | "typing_extensions")
        })
    }
}

#[derive(Copy, Clone, Debug)]
pub(crate) enum DefinitionNodeRef<'a> {
    Import(&'a ast::Alias),
    ImportFrom(ImportFromDefinitionNodeRef<'a>),
    For(ForStmtDefinitionNodeRef<'a>),
    Function(&'a ast::StmtFunctionDef),
    Class(&'a ast::StmtClassDef),
    NamedExpression(&'a ast::ExprNamed),
    Assignment(AssignmentDefinitionNodeRef<'a>),
    AnnotatedAssignment(&'a ast::StmtAnnAssign),
    AugmentedAssignment(&'a ast::StmtAugAssign),
    Comprehension(ComprehensionDefinitionNodeRef<'a>),
    Parameter(ast::AnyParameterRef<'a>),
    WithItem(WithItemDefinitionNodeRef<'a>),
    MatchPattern(MatchPatternDefinitionNodeRef<'a>),
    ExceptHandler(ExceptHandlerDefinitionNodeRef<'a>),
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

impl<'a> From<&'a ast::StmtAugAssign> for DefinitionNodeRef<'a> {
    fn from(node: &'a ast::StmtAugAssign) -> Self {
        Self::AugmentedAssignment(node)
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

impl<'a> From<ForStmtDefinitionNodeRef<'a>> for DefinitionNodeRef<'a> {
    fn from(value: ForStmtDefinitionNodeRef<'a>) -> Self {
        Self::For(value)
    }
}

impl<'a> From<AssignmentDefinitionNodeRef<'a>> for DefinitionNodeRef<'a> {
    fn from(node_ref: AssignmentDefinitionNodeRef<'a>) -> Self {
        Self::Assignment(node_ref)
    }
}

impl<'a> From<WithItemDefinitionNodeRef<'a>> for DefinitionNodeRef<'a> {
    fn from(node_ref: WithItemDefinitionNodeRef<'a>) -> Self {
        Self::WithItem(node_ref)
    }
}

impl<'a> From<ComprehensionDefinitionNodeRef<'a>> for DefinitionNodeRef<'a> {
    fn from(node: ComprehensionDefinitionNodeRef<'a>) -> Self {
        Self::Comprehension(node)
    }
}

impl<'a> From<ast::AnyParameterRef<'a>> for DefinitionNodeRef<'a> {
    fn from(node: ast::AnyParameterRef<'a>) -> Self {
        Self::Parameter(node)
    }
}

impl<'a> From<MatchPatternDefinitionNodeRef<'a>> for DefinitionNodeRef<'a> {
    fn from(node: MatchPatternDefinitionNodeRef<'a>) -> Self {
        Self::MatchPattern(node)
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
pub(crate) struct WithItemDefinitionNodeRef<'a> {
    pub(crate) node: &'a ast::WithItem,
    pub(crate) target: &'a ast::ExprName,
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct ForStmtDefinitionNodeRef<'a> {
    pub(crate) iterable: &'a ast::Expr,
    pub(crate) target: &'a ast::ExprName,
    pub(crate) is_async: bool,
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct ExceptHandlerDefinitionNodeRef<'a> {
    pub(crate) handler: &'a ast::ExceptHandlerExceptHandler,
    pub(crate) is_star: bool,
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct ComprehensionDefinitionNodeRef<'a> {
    pub(crate) iterable: &'a ast::Expr,
    pub(crate) target: &'a ast::ExprName,
    pub(crate) first: bool,
    pub(crate) is_async: bool,
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct MatchPatternDefinitionNodeRef<'a> {
    /// The outermost pattern node in which the identifier being defined occurs.
    pub(crate) pattern: &'a ast::Pattern,
    /// The identifier being defined.
    pub(crate) identifier: &'a ast::Identifier,
    /// The index of the identifier in the pattern when visiting the `pattern` node in evaluation
    /// order.
    pub(crate) index: u32,
}

impl DefinitionNodeRef<'_> {
    #[allow(unsafe_code)]
    pub(super) unsafe fn into_owned(self, parsed: ParsedModule) -> DefinitionKind {
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
            DefinitionNodeRef::AugmentedAssignment(augmented_assignment) => {
                DefinitionKind::AugmentedAssignment(AstNodeRef::new(parsed, augmented_assignment))
            }
            DefinitionNodeRef::For(ForStmtDefinitionNodeRef {
                iterable,
                target,
                is_async,
            }) => DefinitionKind::For(ForStmtDefinitionKind {
                iterable: AstNodeRef::new(parsed.clone(), iterable),
                target: AstNodeRef::new(parsed, target),
                is_async,
            }),
            DefinitionNodeRef::Comprehension(ComprehensionDefinitionNodeRef {
                iterable,
                target,
                first,
                is_async,
            }) => DefinitionKind::Comprehension(ComprehensionDefinitionKind {
                iterable: AstNodeRef::new(parsed.clone(), iterable),
                target: AstNodeRef::new(parsed, target),
                first,
                is_async,
            }),
            DefinitionNodeRef::Parameter(parameter) => match parameter {
                ast::AnyParameterRef::Variadic(parameter) => {
                    DefinitionKind::Parameter(AstNodeRef::new(parsed, parameter))
                }
                ast::AnyParameterRef::NonVariadic(parameter) => {
                    DefinitionKind::ParameterWithDefault(AstNodeRef::new(parsed, parameter))
                }
            },
            DefinitionNodeRef::WithItem(WithItemDefinitionNodeRef { node, target }) => {
                DefinitionKind::WithItem(WithItemDefinitionKind {
                    node: AstNodeRef::new(parsed.clone(), node),
                    target: AstNodeRef::new(parsed, target),
                })
            }
            DefinitionNodeRef::MatchPattern(MatchPatternDefinitionNodeRef {
                pattern,
                identifier,
                index,
            }) => DefinitionKind::MatchPattern(MatchPatternDefinitionKind {
                pattern: AstNodeRef::new(parsed.clone(), pattern),
                identifier: AstNodeRef::new(parsed, identifier),
                index,
            }),
            DefinitionNodeRef::ExceptHandler(ExceptHandlerDefinitionNodeRef {
                handler,
                is_star,
            }) => DefinitionKind::ExceptHandler(ExceptHandlerDefinitionKind {
                handler: AstNodeRef::new(parsed.clone(), handler),
                is_star,
            }),
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
            Self::AugmentedAssignment(node) => node.into(),
            Self::For(ForStmtDefinitionNodeRef {
                iterable: _,
                target,
                is_async: _,
            }) => target.into(),
            Self::Comprehension(ComprehensionDefinitionNodeRef { target, .. }) => target.into(),
            Self::Parameter(node) => match node {
                ast::AnyParameterRef::Variadic(parameter) => parameter.into(),
                ast::AnyParameterRef::NonVariadic(parameter) => parameter.into(),
            },
            Self::WithItem(WithItemDefinitionNodeRef { node: _, target }) => target.into(),
            Self::MatchPattern(MatchPatternDefinitionNodeRef { identifier, .. }) => {
                identifier.into()
            }
            Self::ExceptHandler(ExceptHandlerDefinitionNodeRef { handler, .. }) => handler.into(),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum DefinitionCategory {
    /// A Definition which binds a value to a name (e.g. `x = 1`).
    Binding,
    /// A Definition which declares the upper-bound of acceptable types for this name (`x: int`).
    Declaration,
    /// A Definition which both declares a type and binds a value (e.g. `x: int = 1`).
    DeclarationAndBinding,
}

impl DefinitionCategory {
    /// True if this definition establishes a "declared type" for the symbol.
    ///
    /// If so, any assignments reached by this definition are in error if they assign a value of a
    /// type not assignable to the declared type.
    ///
    /// Annotations establish a declared type. So do function and class definitions, and imports.
    pub(crate) fn is_declaration(self) -> bool {
        matches!(
            self,
            DefinitionCategory::Declaration | DefinitionCategory::DeclarationAndBinding
        )
    }

    /// True if this definition assigns a value to the symbol.
    ///
    /// False only for annotated assignments without a RHS.
    pub(crate) fn is_binding(self) -> bool {
        matches!(
            self,
            DefinitionCategory::Binding | DefinitionCategory::DeclarationAndBinding
        )
    }
}

#[derive(Clone, Debug)]
pub enum DefinitionKind {
    Import(AstNodeRef<ast::Alias>),
    ImportFrom(ImportFromDefinitionKind),
    Function(AstNodeRef<ast::StmtFunctionDef>),
    Class(AstNodeRef<ast::StmtClassDef>),
    NamedExpression(AstNodeRef<ast::ExprNamed>),
    Assignment(AssignmentDefinitionKind),
    AnnotatedAssignment(AstNodeRef<ast::StmtAnnAssign>),
    AugmentedAssignment(AstNodeRef<ast::StmtAugAssign>),
    For(ForStmtDefinitionKind),
    Comprehension(ComprehensionDefinitionKind),
    Parameter(AstNodeRef<ast::Parameter>),
    ParameterWithDefault(AstNodeRef<ast::ParameterWithDefault>),
    WithItem(WithItemDefinitionKind),
    MatchPattern(MatchPatternDefinitionKind),
    ExceptHandler(ExceptHandlerDefinitionKind),
}

impl DefinitionKind {
    pub(crate) fn category(&self) -> DefinitionCategory {
        match self {
            // functions, classes, and imports always bind, and we consider them declarations
            DefinitionKind::Function(_)
            | DefinitionKind::Class(_)
            | DefinitionKind::Import(_)
            | DefinitionKind::ImportFrom(_) => DefinitionCategory::DeclarationAndBinding,
            // a parameter always binds a value, but is only a declaration if annotated
            DefinitionKind::Parameter(parameter) => {
                if parameter.annotation.is_some() {
                    DefinitionCategory::DeclarationAndBinding
                } else {
                    DefinitionCategory::Binding
                }
            }
            // presence of a default is irrelevant, same logic as for a no-default parameter
            DefinitionKind::ParameterWithDefault(parameter_with_default) => {
                if parameter_with_default.parameter.annotation.is_some() {
                    DefinitionCategory::DeclarationAndBinding
                } else {
                    DefinitionCategory::Binding
                }
            }
            // annotated assignment is always a declaration, only a binding if there is a RHS
            DefinitionKind::AnnotatedAssignment(ann_assign) => {
                if ann_assign.value.is_some() {
                    DefinitionCategory::DeclarationAndBinding
                } else {
                    DefinitionCategory::Declaration
                }
            }
            // all of these bind values without declaring a type
            DefinitionKind::NamedExpression(_)
            | DefinitionKind::Assignment(_)
            | DefinitionKind::AugmentedAssignment(_)
            | DefinitionKind::For(_)
            | DefinitionKind::Comprehension(_)
            | DefinitionKind::WithItem(_)
            | DefinitionKind::MatchPattern(_)
            | DefinitionKind::ExceptHandler(_) => DefinitionCategory::Binding,
        }
    }
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct MatchPatternDefinitionKind {
    pattern: AstNodeRef<ast::Pattern>,
    identifier: AstNodeRef<ast::Identifier>,
    index: u32,
}

impl MatchPatternDefinitionKind {
    pub(crate) fn pattern(&self) -> &ast::Pattern {
        self.pattern.node()
    }

    pub(crate) fn index(&self) -> u32 {
        self.index
    }
}

#[derive(Clone, Debug)]
pub struct ComprehensionDefinitionKind {
    iterable: AstNodeRef<ast::Expr>,
    target: AstNodeRef<ast::ExprName>,
    first: bool,
    is_async: bool,
}

impl ComprehensionDefinitionKind {
    pub(crate) fn iterable(&self) -> &ast::Expr {
        self.iterable.node()
    }

    pub(crate) fn target(&self) -> &ast::ExprName {
        self.target.node()
    }

    pub(crate) fn is_first(&self) -> bool {
        self.first
    }

    pub(crate) fn is_async(&self) -> bool {
        self.is_async
    }
}

#[derive(Clone, Debug)]
pub struct ImportFromDefinitionKind {
    node: AstNodeRef<ast::StmtImportFrom>,
    alias_index: usize,
}

impl ImportFromDefinitionKind {
    pub(crate) fn import(&self) -> &ast::StmtImportFrom {
        self.node.node()
    }

    pub(crate) fn alias(&self) -> &ast::Alias {
        &self.node.node().names[self.alias_index]
    }
}

#[derive(Clone, Debug)]
pub struct AssignmentDefinitionKind {
    assignment: AstNodeRef<ast::StmtAssign>,
    target: AstNodeRef<ast::ExprName>,
}

impl AssignmentDefinitionKind {
    pub(crate) fn assignment(&self) -> &ast::StmtAssign {
        self.assignment.node()
    }

    pub(crate) fn target(&self) -> &ast::ExprName {
        self.target.node()
    }
}

#[derive(Clone, Debug)]
pub struct WithItemDefinitionKind {
    node: AstNodeRef<ast::WithItem>,
    target: AstNodeRef<ast::ExprName>,
}

impl WithItemDefinitionKind {
    pub(crate) fn node(&self) -> &ast::WithItem {
        self.node.node()
    }

    pub(crate) fn target(&self) -> &ast::ExprName {
        self.target.node()
    }
}

#[derive(Clone, Debug)]
pub struct ForStmtDefinitionKind {
    iterable: AstNodeRef<ast::Expr>,
    target: AstNodeRef<ast::ExprName>,
    is_async: bool,
}

impl ForStmtDefinitionKind {
    pub(crate) fn iterable(&self) -> &ast::Expr {
        self.iterable.node()
    }

    pub(crate) fn target(&self) -> &ast::ExprName {
        self.target.node()
    }

    pub(crate) fn is_async(&self) -> bool {
        self.is_async
    }
}

#[derive(Clone, Debug)]
pub struct ExceptHandlerDefinitionKind {
    handler: AstNodeRef<ast::ExceptHandlerExceptHandler>,
    is_star: bool,
}

impl ExceptHandlerDefinitionKind {
    pub(crate) fn node(&self) -> &ast::ExceptHandlerExceptHandler {
        self.handler.node()
    }

    pub(crate) fn handled_exceptions(&self) -> Option<&ast::Expr> {
        self.node().type_.as_deref()
    }

    pub(crate) fn is_star(&self) -> bool {
        self.is_star
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

impl From<&ast::StmtAugAssign> for DefinitionNodeKey {
    fn from(node: &ast::StmtAugAssign) -> Self {
        Self(NodeKey::from_node(node))
    }
}

impl From<&ast::StmtFor> for DefinitionNodeKey {
    fn from(value: &ast::StmtFor) -> Self {
        Self(NodeKey::from_node(value))
    }
}

impl From<&ast::Parameter> for DefinitionNodeKey {
    fn from(node: &ast::Parameter) -> Self {
        Self(NodeKey::from_node(node))
    }
}

impl From<&ast::ParameterWithDefault> for DefinitionNodeKey {
    fn from(node: &ast::ParameterWithDefault) -> Self {
        Self(NodeKey::from_node(node))
    }
}

impl From<&ast::Identifier> for DefinitionNodeKey {
    fn from(identifier: &ast::Identifier) -> Self {
        Self(NodeKey::from_node(identifier))
    }
}

impl From<&ast::ExceptHandlerExceptHandler> for DefinitionNodeKey {
    fn from(handler: &ast::ExceptHandlerExceptHandler) -> Self {
        Self(NodeKey::from_node(handler))
    }
}
