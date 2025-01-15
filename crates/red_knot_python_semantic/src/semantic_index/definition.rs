use ruff_db::files::File;
use ruff_db::parsed::ParsedModule;
use ruff_python_ast as ast;
use ruff_text_size::{Ranged, TextRange};

use crate::ast_node_ref::AstNodeRef;
use crate::node_key::NodeKey;
use crate::semantic_index::symbol::{FileScopeId, ScopeId, ScopedSymbolId};
use crate::unpack::Unpack;
use crate::Db;

/// A definition of a symbol.
///
/// ## Module-local type
/// This type should not be used as part of any cross-module API because
/// it holds a reference to the AST node. Range-offset changes
/// then propagate through all usages, and deserialization requires
/// reparsing the entire module.
///
/// E.g. don't use this type in:
///
/// * a return type of a cross-module query
/// * a field of a type that is a return type of a cross-module query
/// * an argument of a cross-module query
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
    pub(crate) kind: DefinitionKind<'db>,

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
}

#[derive(Copy, Clone, Debug)]
pub(crate) enum DefinitionNodeRef<'a> {
    Import(&'a ast::Alias),
    ImportFrom(ImportFromDefinitionNodeRef<'a>),
    For(ForStmtDefinitionNodeRef<'a>),
    Function(&'a ast::StmtFunctionDef),
    Class(&'a ast::StmtClassDef),
    TypeAlias(&'a ast::StmtTypeAlias),
    NamedExpression(&'a ast::ExprNamed),
    Assignment(AssignmentDefinitionNodeRef<'a>),
    AnnotatedAssignment(&'a ast::StmtAnnAssign),
    AugmentedAssignment(&'a ast::StmtAugAssign),
    Comprehension(ComprehensionDefinitionNodeRef<'a>),
    VariadicPositionalParameter(&'a ast::Parameter),
    VariadicKeywordParameter(&'a ast::Parameter),
    Parameter(&'a ast::ParameterWithDefault),
    WithItem(WithItemDefinitionNodeRef<'a>),
    MatchPattern(MatchPatternDefinitionNodeRef<'a>),
    ExceptHandler(ExceptHandlerDefinitionNodeRef<'a>),
    TypeVar(&'a ast::TypeParamTypeVar),
    ParamSpec(&'a ast::TypeParamParamSpec),
    TypeVarTuple(&'a ast::TypeParamTypeVarTuple),
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

impl<'a> From<&'a ast::StmtTypeAlias> for DefinitionNodeRef<'a> {
    fn from(node: &'a ast::StmtTypeAlias) -> Self {
        Self::TypeAlias(node)
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

impl<'a> From<&'a ast::TypeParamTypeVar> for DefinitionNodeRef<'a> {
    fn from(value: &'a ast::TypeParamTypeVar) -> Self {
        Self::TypeVar(value)
    }
}

impl<'a> From<&'a ast::TypeParamParamSpec> for DefinitionNodeRef<'a> {
    fn from(value: &'a ast::TypeParamParamSpec) -> Self {
        Self::ParamSpec(value)
    }
}

impl<'a> From<&'a ast::TypeParamTypeVarTuple> for DefinitionNodeRef<'a> {
    fn from(value: &'a ast::TypeParamTypeVarTuple) -> Self {
        Self::TypeVarTuple(value)
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

impl<'a> From<&'a ast::ParameterWithDefault> for DefinitionNodeRef<'a> {
    fn from(node: &'a ast::ParameterWithDefault) -> Self {
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
    pub(crate) unpack: Option<Unpack<'a>>,
    pub(crate) value: &'a ast::Expr,
    pub(crate) name: &'a ast::ExprName,
    pub(crate) first: bool,
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct WithItemDefinitionNodeRef<'a> {
    pub(crate) node: &'a ast::WithItem,
    pub(crate) target: &'a ast::ExprName,
    pub(crate) is_async: bool,
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct ForStmtDefinitionNodeRef<'a> {
    pub(crate) unpack: Option<Unpack<'a>>,
    pub(crate) iterable: &'a ast::Expr,
    pub(crate) name: &'a ast::ExprName,
    pub(crate) first: bool,
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

impl<'db> DefinitionNodeRef<'db> {
    #[allow(unsafe_code)]
    pub(super) unsafe fn into_owned(self, parsed: ParsedModule) -> DefinitionKind<'db> {
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
            DefinitionNodeRef::TypeAlias(type_alias) => {
                DefinitionKind::TypeAlias(AstNodeRef::new(parsed, type_alias))
            }
            DefinitionNodeRef::NamedExpression(named) => {
                DefinitionKind::NamedExpression(AstNodeRef::new(parsed, named))
            }
            DefinitionNodeRef::Assignment(AssignmentDefinitionNodeRef {
                unpack,
                value,
                name,
                first,
            }) => DefinitionKind::Assignment(AssignmentDefinitionKind {
                target: TargetKind::from(unpack),
                value: AstNodeRef::new(parsed.clone(), value),
                name: AstNodeRef::new(parsed, name),
                first,
            }),
            DefinitionNodeRef::AnnotatedAssignment(assign) => {
                DefinitionKind::AnnotatedAssignment(AstNodeRef::new(parsed, assign))
            }
            DefinitionNodeRef::AugmentedAssignment(augmented_assignment) => {
                DefinitionKind::AugmentedAssignment(AstNodeRef::new(parsed, augmented_assignment))
            }
            DefinitionNodeRef::For(ForStmtDefinitionNodeRef {
                unpack,
                iterable,
                name,
                first,
                is_async,
            }) => DefinitionKind::For(ForStmtDefinitionKind {
                target: TargetKind::from(unpack),
                iterable: AstNodeRef::new(parsed.clone(), iterable),
                name: AstNodeRef::new(parsed, name),
                first,
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
            DefinitionNodeRef::VariadicPositionalParameter(parameter) => {
                DefinitionKind::VariadicPositionalParameter(AstNodeRef::new(parsed, parameter))
            }
            DefinitionNodeRef::VariadicKeywordParameter(parameter) => {
                DefinitionKind::VariadicKeywordParameter(AstNodeRef::new(parsed, parameter))
            }
            DefinitionNodeRef::Parameter(parameter) => {
                DefinitionKind::Parameter(AstNodeRef::new(parsed, parameter))
            }
            DefinitionNodeRef::WithItem(WithItemDefinitionNodeRef {
                node,
                target,
                is_async,
            }) => DefinitionKind::WithItem(WithItemDefinitionKind {
                node: AstNodeRef::new(parsed.clone(), node),
                target: AstNodeRef::new(parsed, target),
                is_async,
            }),
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
                handler: AstNodeRef::new(parsed, handler),
                is_star,
            }),
            DefinitionNodeRef::TypeVar(node) => {
                DefinitionKind::TypeVar(AstNodeRef::new(parsed, node))
            }
            DefinitionNodeRef::ParamSpec(node) => {
                DefinitionKind::ParamSpec(AstNodeRef::new(parsed, node))
            }
            DefinitionNodeRef::TypeVarTuple(node) => {
                DefinitionKind::TypeVarTuple(AstNodeRef::new(parsed, node))
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
            Self::TypeAlias(node) => node.into(),
            Self::NamedExpression(node) => node.into(),
            Self::Assignment(AssignmentDefinitionNodeRef {
                value: _,
                unpack: _,
                name,
                first: _,
            }) => name.into(),
            Self::AnnotatedAssignment(node) => node.into(),
            Self::AugmentedAssignment(node) => node.into(),
            Self::For(ForStmtDefinitionNodeRef {
                unpack: _,
                iterable: _,
                name,
                first: _,
                is_async: _,
            }) => name.into(),
            Self::Comprehension(ComprehensionDefinitionNodeRef { target, .. }) => target.into(),
            Self::VariadicPositionalParameter(node) => node.into(),
            Self::VariadicKeywordParameter(node) => node.into(),
            Self::Parameter(node) => node.into(),
            Self::WithItem(WithItemDefinitionNodeRef {
                node: _,
                target,
                is_async: _,
            }) => target.into(),
            Self::MatchPattern(MatchPatternDefinitionNodeRef { identifier, .. }) => {
                identifier.into()
            }
            Self::ExceptHandler(ExceptHandlerDefinitionNodeRef { handler, .. }) => handler.into(),
            Self::TypeVar(node) => node.into(),
            Self::ParamSpec(node) => node.into(),
            Self::TypeVarTuple(node) => node.into(),
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
pub enum DefinitionKind<'db> {
    Import(AstNodeRef<ast::Alias>),
    ImportFrom(ImportFromDefinitionKind),
    Function(AstNodeRef<ast::StmtFunctionDef>),
    Class(AstNodeRef<ast::StmtClassDef>),
    TypeAlias(AstNodeRef<ast::StmtTypeAlias>),
    NamedExpression(AstNodeRef<ast::ExprNamed>),
    Assignment(AssignmentDefinitionKind<'db>),
    AnnotatedAssignment(AstNodeRef<ast::StmtAnnAssign>),
    AugmentedAssignment(AstNodeRef<ast::StmtAugAssign>),
    For(ForStmtDefinitionKind<'db>),
    Comprehension(ComprehensionDefinitionKind),
    VariadicPositionalParameter(AstNodeRef<ast::Parameter>),
    VariadicKeywordParameter(AstNodeRef<ast::Parameter>),
    Parameter(AstNodeRef<ast::ParameterWithDefault>),
    WithItem(WithItemDefinitionKind),
    MatchPattern(MatchPatternDefinitionKind),
    ExceptHandler(ExceptHandlerDefinitionKind),
    TypeVar(AstNodeRef<ast::TypeParamTypeVar>),
    ParamSpec(AstNodeRef<ast::TypeParamParamSpec>),
    TypeVarTuple(AstNodeRef<ast::TypeParamTypeVarTuple>),
}

impl DefinitionKind<'_> {
    /// Returns the [`TextRange`] of the definition target.
    ///
    /// A definition target would mainly be the node representing the symbol being defined i.e.,
    /// [`ast::ExprName`] or [`ast::Identifier`] but could also be other nodes.
    ///
    /// This is mainly used for logging and debugging purposes.
    pub(crate) fn target_range(&self) -> TextRange {
        match self {
            DefinitionKind::Import(alias) => alias.range(),
            DefinitionKind::ImportFrom(import) => import.alias().range(),
            DefinitionKind::Function(function) => function.name.range(),
            DefinitionKind::Class(class) => class.name.range(),
            DefinitionKind::TypeAlias(type_alias) => type_alias.name.range(),
            DefinitionKind::NamedExpression(named) => named.target.range(),
            DefinitionKind::Assignment(assignment) => assignment.name().range(),
            DefinitionKind::AnnotatedAssignment(assign) => assign.target.range(),
            DefinitionKind::AugmentedAssignment(aug_assign) => aug_assign.target.range(),
            DefinitionKind::For(for_stmt) => for_stmt.name().range(),
            DefinitionKind::Comprehension(comp) => comp.target().range(),
            DefinitionKind::VariadicPositionalParameter(parameter) => parameter.name.range(),
            DefinitionKind::VariadicKeywordParameter(parameter) => parameter.name.range(),
            DefinitionKind::Parameter(parameter) => parameter.parameter.name.range(),
            DefinitionKind::WithItem(with_item) => with_item.target().range(),
            DefinitionKind::MatchPattern(match_pattern) => match_pattern.identifier.range(),
            DefinitionKind::ExceptHandler(handler) => handler.node().range(),
            DefinitionKind::TypeVar(type_var) => type_var.name.range(),
            DefinitionKind::ParamSpec(param_spec) => param_spec.name.range(),
            DefinitionKind::TypeVarTuple(type_var_tuple) => type_var_tuple.name.range(),
        }
    }

    pub(crate) fn category(&self) -> DefinitionCategory {
        match self {
            // functions, classes, and imports always bind, and we consider them declarations
            DefinitionKind::Function(_)
            | DefinitionKind::Class(_)
            | DefinitionKind::TypeAlias(_)
            | DefinitionKind::Import(_)
            | DefinitionKind::ImportFrom(_)
            | DefinitionKind::TypeVar(_)
            | DefinitionKind::ParamSpec(_)
            | DefinitionKind::TypeVarTuple(_) => DefinitionCategory::DeclarationAndBinding,
            // a parameter always binds a value, but is only a declaration if annotated
            DefinitionKind::VariadicPositionalParameter(parameter)
            | DefinitionKind::VariadicKeywordParameter(parameter) => {
                if parameter.annotation.is_some() {
                    DefinitionCategory::DeclarationAndBinding
                } else {
                    DefinitionCategory::Binding
                }
            }
            // presence of a default is irrelevant, same logic as for a no-default parameter
            DefinitionKind::Parameter(parameter_with_default) => {
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

#[derive(Copy, Clone, Debug, PartialEq)]
pub(crate) enum TargetKind<'db> {
    Sequence(Unpack<'db>),
    Name,
}

impl<'db> From<Option<Unpack<'db>>> for TargetKind<'db> {
    fn from(value: Option<Unpack<'db>>) -> Self {
        match value {
            Some(unpack) => TargetKind::Sequence(unpack),
            None => TargetKind::Name,
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
pub struct AssignmentDefinitionKind<'db> {
    target: TargetKind<'db>,
    value: AstNodeRef<ast::Expr>,
    name: AstNodeRef<ast::ExprName>,
    first: bool,
}

impl<'db> AssignmentDefinitionKind<'db> {
    pub(crate) fn target(&self) -> TargetKind<'db> {
        self.target
    }

    pub(crate) fn value(&self) -> &ast::Expr {
        self.value.node()
    }

    pub(crate) fn name(&self) -> &ast::ExprName {
        self.name.node()
    }

    pub(crate) fn is_first(&self) -> bool {
        self.first
    }
}

#[derive(Clone, Debug)]
pub struct WithItemDefinitionKind {
    node: AstNodeRef<ast::WithItem>,
    target: AstNodeRef<ast::ExprName>,
    is_async: bool,
}

impl WithItemDefinitionKind {
    pub(crate) fn node(&self) -> &ast::WithItem {
        self.node.node()
    }

    pub(crate) fn target(&self) -> &ast::ExprName {
        self.target.node()
    }

    pub(crate) const fn is_async(&self) -> bool {
        self.is_async
    }
}

#[derive(Clone, Debug)]
pub struct ForStmtDefinitionKind<'db> {
    target: TargetKind<'db>,
    iterable: AstNodeRef<ast::Expr>,
    name: AstNodeRef<ast::ExprName>,
    first: bool,
    is_async: bool,
}

impl<'db> ForStmtDefinitionKind<'db> {
    pub(crate) fn iterable(&self) -> &ast::Expr {
        self.iterable.node()
    }

    pub(crate) fn target(&self) -> TargetKind<'db> {
        self.target
    }

    pub(crate) fn name(&self) -> &ast::ExprName {
        self.name.node()
    }

    pub(crate) const fn is_first(&self) -> bool {
        self.first
    }

    pub(crate) const fn is_async(&self) -> bool {
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

impl From<&ast::StmtTypeAlias> for DefinitionNodeKey {
    fn from(node: &ast::StmtTypeAlias) -> Self {
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

impl From<ast::AnyParameterRef<'_>> for DefinitionNodeKey {
    fn from(value: ast::AnyParameterRef) -> Self {
        Self(match value {
            ast::AnyParameterRef::Variadic(node) => NodeKey::from_node(node),
            ast::AnyParameterRef::NonVariadic(node) => NodeKey::from_node(node),
        })
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

impl From<&ast::TypeParamTypeVar> for DefinitionNodeKey {
    fn from(value: &ast::TypeParamTypeVar) -> Self {
        Self(NodeKey::from_node(value))
    }
}

impl From<&ast::TypeParamParamSpec> for DefinitionNodeKey {
    fn from(value: &ast::TypeParamParamSpec) -> Self {
        Self(NodeKey::from_node(value))
    }
}

impl From<&ast::TypeParamTypeVarTuple> for DefinitionNodeKey {
    fn from(value: &ast::TypeParamTypeVarTuple) -> Self {
        Self(NodeKey::from_node(value))
    }
}
