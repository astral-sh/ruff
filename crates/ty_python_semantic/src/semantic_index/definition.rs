use std::ops::Deref;

use ruff_db::files::{File, FileRange};
use ruff_db::parsed::ParsedModuleRef;
use ruff_python_ast as ast;
use ruff_text_size::{Ranged, TextRange};

use crate::Db;
use crate::ast_node_ref::AstNodeRef;
use crate::node_key::NodeKey;
use crate::semantic_index::place::{FileScopeId, ScopeId, ScopedPlaceId};
use crate::unpack::{Unpack, UnpackPosition};

/// A definition of a place.
///
/// ## ID stability
/// The `Definition`'s ID is stable when the only field that change is its `kind` (AST node).
///
/// The `Definition` changes when the `file`, `scope`, or `place` change. This can be
/// because a new scope gets inserted before the `Definition` or a new place is inserted
/// before this `Definition`. However, the ID can be considered stable and it is okay to use
/// `Definition` in cross-module` salsa queries or as a field on other salsa tracked structs.
#[salsa::tracked(debug)]
pub struct Definition<'db> {
    /// The file in which the definition occurs.
    pub(crate) file: File,

    /// The scope in which the definition occurs.
    pub(crate) file_scope: FileScopeId,

    /// The place ID of the definition.
    pub(crate) place: ScopedPlaceId,

    /// WARNING: Only access this field when doing type inference for the same
    /// file as where `Definition` is defined to avoid cross-file query dependencies.
    #[no_eq]
    #[returns(ref)]
    #[tracked]
    pub(crate) kind: DefinitionKind<'db>,

    /// This is a dedicated field to avoid accessing `kind` to compute this value.
    pub(crate) is_reexported: bool,

    count: countme::Count<Definition<'static>>,
}

impl<'db> Definition<'db> {
    pub(crate) fn scope(self, db: &'db dyn Db) -> ScopeId<'db> {
        self.file_scope(db).to_scope_id(db, self.file(db))
    }

    pub fn full_range(self, db: &'db dyn Db, module: &ParsedModuleRef) -> FileRange {
        FileRange::new(self.file(db), self.kind(db).full_range(module))
    }

    pub fn focus_range(self, db: &'db dyn Db, module: &ParsedModuleRef) -> FileRange {
        FileRange::new(self.file(db), self.kind(db).target_range(module))
    }
}

/// One or more [`Definition`]s.
#[derive(Debug, Default, PartialEq, Eq, salsa::Update)]
pub struct Definitions<'db>(smallvec::SmallVec<[Definition<'db>; 1]>);

impl<'db> Definitions<'db> {
    pub(crate) fn single(definition: Definition<'db>) -> Self {
        Self(smallvec::smallvec![definition])
    }

    pub(crate) fn push(&mut self, definition: Definition<'db>) {
        self.0.push(definition);
    }
}

impl<'db> Deref for Definitions<'db> {
    type Target = [Definition<'db>];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'a, 'db> IntoIterator for &'a Definitions<'db> {
    type Item = &'a Definition<'db>;
    type IntoIter = std::slice::Iter<'a, Definition<'db>>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, salsa::Update)]
pub(crate) enum DefinitionState<'db> {
    Defined(Definition<'db>),
    /// Represents the implicit "unbound"/"undeclared" definition of every place.
    Undefined,
    /// Represents a definition that has been deleted.
    /// This used when an attribute/subscript definition (such as `x.y = ...`, `x[0] = ...`) becomes obsolete due to a reassignment of the root place.
    Deleted,
}

impl<'db> DefinitionState<'db> {
    pub(crate) fn is_defined_and(self, f: impl Fn(Definition<'db>) -> bool) -> bool {
        matches!(self, DefinitionState::Defined(def) if f(def))
    }

    pub(crate) fn is_undefined_or(self, f: impl Fn(Definition<'db>) -> bool) -> bool {
        matches!(self, DefinitionState::Undefined)
            || matches!(self, DefinitionState::Defined(def) if f(def))
    }

    pub(crate) fn is_undefined(self) -> bool {
        matches!(self, DefinitionState::Undefined)
    }

    #[allow(unused)]
    pub(crate) fn definition(self) -> Option<Definition<'db>> {
        match self {
            DefinitionState::Defined(def) => Some(def),
            DefinitionState::Deleted | DefinitionState::Undefined => None,
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub(crate) enum DefinitionNodeRef<'ast, 'db> {
    Import(ImportDefinitionNodeRef<'ast>),
    ImportFrom(ImportFromDefinitionNodeRef<'ast>),
    ImportStar(StarImportDefinitionNodeRef<'ast>),
    For(ForStmtDefinitionNodeRef<'ast, 'db>),
    Function(&'ast ast::StmtFunctionDef),
    Class(&'ast ast::StmtClassDef),
    TypeAlias(&'ast ast::StmtTypeAlias),
    NamedExpression(&'ast ast::ExprNamed),
    Assignment(AssignmentDefinitionNodeRef<'ast, 'db>),
    AnnotatedAssignment(AnnotatedAssignmentDefinitionNodeRef<'ast>),
    AugmentedAssignment(&'ast ast::StmtAugAssign),
    Comprehension(ComprehensionDefinitionNodeRef<'ast, 'db>),
    VariadicPositionalParameter(&'ast ast::Parameter),
    VariadicKeywordParameter(&'ast ast::Parameter),
    Parameter(&'ast ast::ParameterWithDefault),
    WithItem(WithItemDefinitionNodeRef<'ast, 'db>),
    MatchPattern(MatchPatternDefinitionNodeRef<'ast>),
    ExceptHandler(ExceptHandlerDefinitionNodeRef<'ast>),
    TypeVar(&'ast ast::TypeParamTypeVar),
    ParamSpec(&'ast ast::TypeParamParamSpec),
    TypeVarTuple(&'ast ast::TypeParamTypeVarTuple),
}

impl<'ast> From<&'ast ast::StmtFunctionDef> for DefinitionNodeRef<'ast, '_> {
    fn from(node: &'ast ast::StmtFunctionDef) -> Self {
        Self::Function(node)
    }
}

impl<'ast> From<&'ast ast::StmtClassDef> for DefinitionNodeRef<'ast, '_> {
    fn from(node: &'ast ast::StmtClassDef) -> Self {
        Self::Class(node)
    }
}

impl<'ast> From<&'ast ast::StmtTypeAlias> for DefinitionNodeRef<'ast, '_> {
    fn from(node: &'ast ast::StmtTypeAlias) -> Self {
        Self::TypeAlias(node)
    }
}

impl<'ast> From<&'ast ast::ExprNamed> for DefinitionNodeRef<'ast, '_> {
    fn from(node: &'ast ast::ExprNamed) -> Self {
        Self::NamedExpression(node)
    }
}

impl<'ast> From<&'ast ast::StmtAugAssign> for DefinitionNodeRef<'ast, '_> {
    fn from(node: &'ast ast::StmtAugAssign) -> Self {
        Self::AugmentedAssignment(node)
    }
}

impl<'ast> From<&'ast ast::TypeParamTypeVar> for DefinitionNodeRef<'ast, '_> {
    fn from(value: &'ast ast::TypeParamTypeVar) -> Self {
        Self::TypeVar(value)
    }
}

impl<'ast> From<&'ast ast::TypeParamParamSpec> for DefinitionNodeRef<'ast, '_> {
    fn from(value: &'ast ast::TypeParamParamSpec) -> Self {
        Self::ParamSpec(value)
    }
}

impl<'ast> From<&'ast ast::TypeParamTypeVarTuple> for DefinitionNodeRef<'ast, '_> {
    fn from(value: &'ast ast::TypeParamTypeVarTuple) -> Self {
        Self::TypeVarTuple(value)
    }
}

impl<'ast> From<ImportDefinitionNodeRef<'ast>> for DefinitionNodeRef<'ast, '_> {
    fn from(node_ref: ImportDefinitionNodeRef<'ast>) -> Self {
        Self::Import(node_ref)
    }
}

impl<'ast> From<ImportFromDefinitionNodeRef<'ast>> for DefinitionNodeRef<'ast, '_> {
    fn from(node_ref: ImportFromDefinitionNodeRef<'ast>) -> Self {
        Self::ImportFrom(node_ref)
    }
}

impl<'ast, 'db> From<ForStmtDefinitionNodeRef<'ast, 'db>> for DefinitionNodeRef<'ast, 'db> {
    fn from(value: ForStmtDefinitionNodeRef<'ast, 'db>) -> Self {
        Self::For(value)
    }
}

impl<'ast, 'db> From<AssignmentDefinitionNodeRef<'ast, 'db>> for DefinitionNodeRef<'ast, 'db> {
    fn from(node_ref: AssignmentDefinitionNodeRef<'ast, 'db>) -> Self {
        Self::Assignment(node_ref)
    }
}

impl<'ast> From<AnnotatedAssignmentDefinitionNodeRef<'ast>> for DefinitionNodeRef<'ast, '_> {
    fn from(node_ref: AnnotatedAssignmentDefinitionNodeRef<'ast>) -> Self {
        Self::AnnotatedAssignment(node_ref)
    }
}

impl<'ast, 'db> From<WithItemDefinitionNodeRef<'ast, 'db>> for DefinitionNodeRef<'ast, 'db> {
    fn from(node_ref: WithItemDefinitionNodeRef<'ast, 'db>) -> Self {
        Self::WithItem(node_ref)
    }
}

impl<'ast, 'db> From<ComprehensionDefinitionNodeRef<'ast, 'db>> for DefinitionNodeRef<'ast, 'db> {
    fn from(node: ComprehensionDefinitionNodeRef<'ast, 'db>) -> Self {
        Self::Comprehension(node)
    }
}

impl<'ast> From<&'ast ast::ParameterWithDefault> for DefinitionNodeRef<'ast, '_> {
    fn from(node: &'ast ast::ParameterWithDefault) -> Self {
        Self::Parameter(node)
    }
}

impl<'ast> From<MatchPatternDefinitionNodeRef<'ast>> for DefinitionNodeRef<'ast, '_> {
    fn from(node: MatchPatternDefinitionNodeRef<'ast>) -> Self {
        Self::MatchPattern(node)
    }
}

impl<'ast> From<StarImportDefinitionNodeRef<'ast>> for DefinitionNodeRef<'ast, '_> {
    fn from(node: StarImportDefinitionNodeRef<'ast>) -> Self {
        Self::ImportStar(node)
    }
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct ImportDefinitionNodeRef<'ast> {
    pub(crate) node: &'ast ast::StmtImport,
    pub(crate) alias_index: usize,
    pub(crate) is_reexported: bool,
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct StarImportDefinitionNodeRef<'ast> {
    pub(crate) node: &'ast ast::StmtImportFrom,
    pub(crate) place_id: ScopedPlaceId,
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct ImportFromDefinitionNodeRef<'ast> {
    pub(crate) node: &'ast ast::StmtImportFrom,
    pub(crate) alias_index: usize,
    pub(crate) is_reexported: bool,
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct AssignmentDefinitionNodeRef<'ast, 'db> {
    pub(crate) unpack: Option<(UnpackPosition, Unpack<'db>)>,
    pub(crate) value: &'ast ast::Expr,
    pub(crate) target: &'ast ast::Expr,
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct AnnotatedAssignmentDefinitionNodeRef<'ast> {
    pub(crate) node: &'ast ast::StmtAnnAssign,
    pub(crate) annotation: &'ast ast::Expr,
    pub(crate) value: Option<&'ast ast::Expr>,
    pub(crate) target: &'ast ast::Expr,
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct WithItemDefinitionNodeRef<'ast, 'db> {
    pub(crate) unpack: Option<(UnpackPosition, Unpack<'db>)>,
    pub(crate) context_expr: &'ast ast::Expr,
    pub(crate) target: &'ast ast::Expr,
    pub(crate) is_async: bool,
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct ForStmtDefinitionNodeRef<'ast, 'db> {
    pub(crate) unpack: Option<(UnpackPosition, Unpack<'db>)>,
    pub(crate) iterable: &'ast ast::Expr,
    pub(crate) target: &'ast ast::Expr,
    pub(crate) is_async: bool,
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct ExceptHandlerDefinitionNodeRef<'ast> {
    pub(crate) handler: &'ast ast::ExceptHandlerExceptHandler,
    pub(crate) is_star: bool,
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct ComprehensionDefinitionNodeRef<'ast, 'db> {
    pub(crate) unpack: Option<(UnpackPosition, Unpack<'db>)>,
    pub(crate) iterable: &'ast ast::Expr,
    pub(crate) target: &'ast ast::Expr,
    pub(crate) first: bool,
    pub(crate) is_async: bool,
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct MatchPatternDefinitionNodeRef<'ast> {
    /// The outermost pattern node in which the identifier being defined occurs.
    pub(crate) pattern: &'ast ast::Pattern,
    /// The identifier being defined.
    pub(crate) identifier: &'ast ast::Identifier,
    /// The index of the identifier in the pattern when visiting the `pattern` node in evaluation
    /// order.
    pub(crate) index: u32,
}

impl<'db> DefinitionNodeRef<'_, 'db> {
    #[expect(unsafe_code)]
    pub(super) unsafe fn into_owned(self, parsed: ParsedModuleRef) -> DefinitionKind<'db> {
        match self {
            DefinitionNodeRef::Import(ImportDefinitionNodeRef {
                node,
                alias_index,
                is_reexported,
            }) => DefinitionKind::Import(ImportDefinitionKind {
                node: unsafe { AstNodeRef::new(parsed, node) },
                alias_index,
                is_reexported,
            }),

            DefinitionNodeRef::ImportFrom(ImportFromDefinitionNodeRef {
                node,
                alias_index,
                is_reexported,
            }) => DefinitionKind::ImportFrom(ImportFromDefinitionKind {
                node: unsafe { AstNodeRef::new(parsed, node) },
                alias_index,
                is_reexported,
            }),
            DefinitionNodeRef::ImportStar(star_import) => {
                let StarImportDefinitionNodeRef { node, place_id } = star_import;
                DefinitionKind::StarImport(StarImportDefinitionKind {
                    node: unsafe { AstNodeRef::new(parsed, node) },
                    place_id,
                })
            }
            DefinitionNodeRef::Function(function) => {
                DefinitionKind::Function(unsafe { AstNodeRef::new(parsed, function) })
            }
            DefinitionNodeRef::Class(class) => {
                DefinitionKind::Class(unsafe { AstNodeRef::new(parsed, class) })
            }
            DefinitionNodeRef::TypeAlias(type_alias) => {
                DefinitionKind::TypeAlias(unsafe { AstNodeRef::new(parsed, type_alias) })
            }
            DefinitionNodeRef::NamedExpression(named) => {
                DefinitionKind::NamedExpression(unsafe { AstNodeRef::new(parsed, named) })
            }
            DefinitionNodeRef::Assignment(AssignmentDefinitionNodeRef {
                unpack,
                value,
                target,
            }) => DefinitionKind::Assignment(AssignmentDefinitionKind {
                target_kind: TargetKind::from(unpack),
                value: unsafe { AstNodeRef::new(parsed.clone(), value) },
                target: unsafe { AstNodeRef::new(parsed, target) },
            }),
            DefinitionNodeRef::AnnotatedAssignment(AnnotatedAssignmentDefinitionNodeRef {
                node: _,
                annotation,
                value,
                target,
            }) => DefinitionKind::AnnotatedAssignment(AnnotatedAssignmentDefinitionKind {
                target: unsafe { AstNodeRef::new(parsed.clone(), target) },
                annotation: unsafe { AstNodeRef::new(parsed.clone(), annotation) },
                value: value.map(|v| unsafe { AstNodeRef::new(parsed, v) }),
            }),
            DefinitionNodeRef::AugmentedAssignment(augmented_assignment) => {
                DefinitionKind::AugmentedAssignment(unsafe {
                    AstNodeRef::new(parsed, augmented_assignment)
                })
            }
            DefinitionNodeRef::For(ForStmtDefinitionNodeRef {
                unpack,
                iterable,
                target,
                is_async,
            }) => DefinitionKind::For(ForStmtDefinitionKind {
                target_kind: TargetKind::from(unpack),
                iterable: unsafe { AstNodeRef::new(parsed.clone(), iterable) },
                target: unsafe { AstNodeRef::new(parsed, target) },
                is_async,
            }),
            DefinitionNodeRef::Comprehension(ComprehensionDefinitionNodeRef {
                unpack,
                iterable,
                target,
                first,
                is_async,
            }) => DefinitionKind::Comprehension(ComprehensionDefinitionKind {
                target_kind: TargetKind::from(unpack),
                iterable: unsafe { AstNodeRef::new(parsed.clone(), iterable) },
                target: unsafe { AstNodeRef::new(parsed, target) },
                first,
                is_async,
            }),
            DefinitionNodeRef::VariadicPositionalParameter(parameter) => {
                DefinitionKind::VariadicPositionalParameter(unsafe {
                    AstNodeRef::new(parsed, parameter)
                })
            }
            DefinitionNodeRef::VariadicKeywordParameter(parameter) => {
                DefinitionKind::VariadicKeywordParameter(unsafe {
                    AstNodeRef::new(parsed, parameter)
                })
            }
            DefinitionNodeRef::Parameter(parameter) => {
                DefinitionKind::Parameter(unsafe { AstNodeRef::new(parsed, parameter) })
            }
            DefinitionNodeRef::WithItem(WithItemDefinitionNodeRef {
                unpack,
                context_expr,
                target,
                is_async,
            }) => DefinitionKind::WithItem(WithItemDefinitionKind {
                target_kind: TargetKind::from(unpack),
                context_expr: unsafe { AstNodeRef::new(parsed.clone(), context_expr) },
                target: unsafe { AstNodeRef::new(parsed, target) },
                is_async,
            }),
            DefinitionNodeRef::MatchPattern(MatchPatternDefinitionNodeRef {
                pattern,
                identifier,
                index,
            }) => DefinitionKind::MatchPattern(MatchPatternDefinitionKind {
                pattern: unsafe { AstNodeRef::new(parsed.clone(), pattern) },
                identifier: unsafe { AstNodeRef::new(parsed, identifier) },
                index,
            }),
            DefinitionNodeRef::ExceptHandler(ExceptHandlerDefinitionNodeRef {
                handler,
                is_star,
            }) => DefinitionKind::ExceptHandler(ExceptHandlerDefinitionKind {
                handler: unsafe { AstNodeRef::new(parsed, handler) },
                is_star,
            }),
            DefinitionNodeRef::TypeVar(node) => {
                DefinitionKind::TypeVar(unsafe { AstNodeRef::new(parsed, node) })
            }
            DefinitionNodeRef::ParamSpec(node) => {
                DefinitionKind::ParamSpec(unsafe { AstNodeRef::new(parsed, node) })
            }
            DefinitionNodeRef::TypeVarTuple(node) => {
                DefinitionKind::TypeVarTuple(unsafe { AstNodeRef::new(parsed, node) })
            }
        }
    }

    pub(super) fn key(self) -> DefinitionNodeKey {
        match self {
            Self::Import(ImportDefinitionNodeRef {
                node,
                alias_index,
                is_reexported: _,
            }) => (&node.names[alias_index]).into(),
            Self::ImportFrom(ImportFromDefinitionNodeRef {
                node,
                alias_index,
                is_reexported: _,
            }) => (&node.names[alias_index]).into(),

            // INVARIANT: for an invalid-syntax statement such as `from foo import *, bar, *`,
            // we only create a `StarImportDefinitionKind` for the *first* `*` alias in the names list.
            Self::ImportStar(StarImportDefinitionNodeRef { node, place_id: _ }) => node
                .names
                .iter()
                .find(|alias| &alias.name == "*")
                .expect(
                    "The `StmtImportFrom` node of a `StarImportDefinitionKind` instance \
                    should always have at least one `alias` with the name `*`.",
                )
                .into(),

            Self::Function(node) => node.into(),
            Self::Class(node) => node.into(),
            Self::TypeAlias(node) => node.into(),
            Self::NamedExpression(node) => node.into(),
            Self::Assignment(AssignmentDefinitionNodeRef {
                value: _,
                unpack: _,
                target,
            }) => DefinitionNodeKey(NodeKey::from_node(target)),
            Self::AnnotatedAssignment(ann_assign) => ann_assign.node.into(),
            Self::AugmentedAssignment(node) => node.into(),
            Self::For(ForStmtDefinitionNodeRef {
                target,
                iterable: _,
                unpack: _,
                is_async: _,
            }) => DefinitionNodeKey(NodeKey::from_node(target)),
            Self::Comprehension(ComprehensionDefinitionNodeRef { target, .. }) => {
                DefinitionNodeKey(NodeKey::from_node(target))
            }
            Self::VariadicPositionalParameter(node) => node.into(),
            Self::VariadicKeywordParameter(node) => node.into(),
            Self::Parameter(node) => node.into(),
            Self::WithItem(WithItemDefinitionNodeRef {
                context_expr: _,
                unpack: _,
                is_async: _,
                target,
            }) => DefinitionNodeKey(NodeKey::from_node(target)),
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
    /// True if this definition establishes a "declared type" for the place.
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

    /// True if this definition assigns a value to the place.
    ///
    /// False only for annotated assignments without a RHS.
    pub(crate) fn is_binding(self) -> bool {
        matches!(
            self,
            DefinitionCategory::Binding | DefinitionCategory::DeclarationAndBinding
        )
    }
}

/// The kind of a definition.
///
/// ## Usage in salsa tracked structs
///
/// [`DefinitionKind`] fields in salsa tracked structs should be tracked (attributed with `#[tracked]`)
/// because the kind is a thin wrapper around [`AstNodeRef`]. See the [`AstNodeRef`] documentation
/// for an in-depth explanation of why this is necessary.
#[derive(Clone, Debug)]
pub enum DefinitionKind<'db> {
    Import(ImportDefinitionKind),
    ImportFrom(ImportFromDefinitionKind),
    StarImport(StarImportDefinitionKind),
    Function(AstNodeRef<ast::StmtFunctionDef>),
    Class(AstNodeRef<ast::StmtClassDef>),
    TypeAlias(AstNodeRef<ast::StmtTypeAlias>),
    NamedExpression(AstNodeRef<ast::ExprNamed>),
    Assignment(AssignmentDefinitionKind<'db>),
    AnnotatedAssignment(AnnotatedAssignmentDefinitionKind),
    AugmentedAssignment(AstNodeRef<ast::StmtAugAssign>),
    For(ForStmtDefinitionKind<'db>),
    Comprehension(ComprehensionDefinitionKind<'db>),
    VariadicPositionalParameter(AstNodeRef<ast::Parameter>),
    VariadicKeywordParameter(AstNodeRef<ast::Parameter>),
    Parameter(AstNodeRef<ast::ParameterWithDefault>),
    WithItem(WithItemDefinitionKind<'db>),
    MatchPattern(MatchPatternDefinitionKind),
    ExceptHandler(ExceptHandlerDefinitionKind),
    TypeVar(AstNodeRef<ast::TypeParamTypeVar>),
    ParamSpec(AstNodeRef<ast::TypeParamParamSpec>),
    TypeVarTuple(AstNodeRef<ast::TypeParamTypeVarTuple>),
}

impl DefinitionKind<'_> {
    pub(crate) fn is_reexported(&self) -> bool {
        match self {
            DefinitionKind::Import(import) => import.is_reexported(),
            DefinitionKind::ImportFrom(import) => import.is_reexported(),
            _ => true,
        }
    }

    pub(crate) const fn as_star_import(&self) -> Option<&StarImportDefinitionKind> {
        match self {
            DefinitionKind::StarImport(import) => Some(import),
            _ => None,
        }
    }

    /// Returns the [`TextRange`] of the definition target.
    ///
    /// A definition target would mainly be the node representing the place being defined i.e.,
    /// [`ast::ExprName`], [`ast::Identifier`], [`ast::ExprAttribute`] or [`ast::ExprSubscript`] but could also be other nodes.
    pub(crate) fn target_range(&self, module: &ParsedModuleRef) -> TextRange {
        match self {
            DefinitionKind::Import(import) => import.alias(module).range(),
            DefinitionKind::ImportFrom(import) => import.alias(module).range(),
            DefinitionKind::StarImport(import) => import.alias(module).range(),
            DefinitionKind::Function(function) => function.node(module).name.range(),
            DefinitionKind::Class(class) => class.node(module).name.range(),
            DefinitionKind::TypeAlias(type_alias) => type_alias.node(module).name.range(),
            DefinitionKind::NamedExpression(named) => named.node(module).target.range(),
            DefinitionKind::Assignment(assignment) => assignment.target.node(module).range(),
            DefinitionKind::AnnotatedAssignment(assign) => assign.target.node(module).range(),
            DefinitionKind::AugmentedAssignment(aug_assign) => {
                aug_assign.node(module).target.range()
            }
            DefinitionKind::For(for_stmt) => for_stmt.target.node(module).range(),
            DefinitionKind::Comprehension(comp) => comp.target(module).range(),
            DefinitionKind::VariadicPositionalParameter(parameter) => {
                parameter.node(module).name.range()
            }
            DefinitionKind::VariadicKeywordParameter(parameter) => {
                parameter.node(module).name.range()
            }
            DefinitionKind::Parameter(parameter) => parameter.node(module).parameter.name.range(),
            DefinitionKind::WithItem(with_item) => with_item.target.node(module).range(),
            DefinitionKind::MatchPattern(match_pattern) => {
                match_pattern.identifier.node(module).range()
            }
            DefinitionKind::ExceptHandler(handler) => handler.node(module).range(),
            DefinitionKind::TypeVar(type_var) => type_var.node(module).name.range(),
            DefinitionKind::ParamSpec(param_spec) => param_spec.node(module).name.range(),
            DefinitionKind::TypeVarTuple(type_var_tuple) => {
                type_var_tuple.node(module).name.range()
            }
        }
    }

    /// Returns the [`TextRange`] of the entire definition.
    pub(crate) fn full_range(&self, module: &ParsedModuleRef) -> TextRange {
        match self {
            DefinitionKind::Import(import) => import.alias(module).range(),
            DefinitionKind::ImportFrom(import) => import.alias(module).range(),
            DefinitionKind::StarImport(import) => import.import(module).range(),
            DefinitionKind::Function(function) => function.node(module).range(),
            DefinitionKind::Class(class) => class.node(module).range(),
            DefinitionKind::TypeAlias(type_alias) => type_alias.node(module).range(),
            DefinitionKind::NamedExpression(named) => named.node(module).range(),
            DefinitionKind::Assignment(assignment) => assignment.target.node(module).range(),
            DefinitionKind::AnnotatedAssignment(assign) => assign.target.node(module).range(),
            DefinitionKind::AugmentedAssignment(aug_assign) => aug_assign.node(module).range(),
            DefinitionKind::For(for_stmt) => for_stmt.target.node(module).range(),
            DefinitionKind::Comprehension(comp) => comp.target(module).range(),
            DefinitionKind::VariadicPositionalParameter(parameter) => {
                parameter.node(module).range()
            }
            DefinitionKind::VariadicKeywordParameter(parameter) => parameter.node(module).range(),
            DefinitionKind::Parameter(parameter) => parameter.node(module).parameter.range(),
            DefinitionKind::WithItem(with_item) => with_item.target.node(module).range(),
            DefinitionKind::MatchPattern(match_pattern) => {
                match_pattern.identifier.node(module).range()
            }
            DefinitionKind::ExceptHandler(handler) => handler.node(module).range(),
            DefinitionKind::TypeVar(type_var) => type_var.node(module).range(),
            DefinitionKind::ParamSpec(param_spec) => param_spec.node(module).range(),
            DefinitionKind::TypeVarTuple(type_var_tuple) => type_var_tuple.node(module).range(),
        }
    }

    pub(crate) fn category(&self, in_stub: bool, module: &ParsedModuleRef) -> DefinitionCategory {
        match self {
            // functions, classes, and imports always bind, and we consider them declarations
            DefinitionKind::Function(_)
            | DefinitionKind::Class(_)
            | DefinitionKind::TypeAlias(_)
            | DefinitionKind::Import(_)
            | DefinitionKind::ImportFrom(_)
            | DefinitionKind::StarImport(_)
            | DefinitionKind::TypeVar(_)
            | DefinitionKind::ParamSpec(_)
            | DefinitionKind::TypeVarTuple(_) => DefinitionCategory::DeclarationAndBinding,
            // a parameter always binds a value, but is only a declaration if annotated
            DefinitionKind::VariadicPositionalParameter(parameter)
            | DefinitionKind::VariadicKeywordParameter(parameter) => {
                if parameter.node(module).annotation.is_some() {
                    DefinitionCategory::DeclarationAndBinding
                } else {
                    DefinitionCategory::Binding
                }
            }
            // presence of a default is irrelevant, same logic as for a no-default parameter
            DefinitionKind::Parameter(parameter_with_default) => {
                if parameter_with_default
                    .node(module)
                    .parameter
                    .annotation
                    .is_some()
                {
                    DefinitionCategory::DeclarationAndBinding
                } else {
                    DefinitionCategory::Binding
                }
            }
            // Annotated assignment is always a declaration. It is also a binding if there is a RHS
            // or if we are in a stub file. Unfortunately, it is common for stubs to omit even an `...` value placeholder.
            DefinitionKind::AnnotatedAssignment(ann_assign) => {
                if in_stub || ann_assign.value.is_some() {
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

#[derive(Copy, Clone, Debug, PartialEq, Hash)]
pub(crate) enum TargetKind<'db> {
    Sequence(UnpackPosition, Unpack<'db>),
    /// Name, attribute, or subscript.
    Single,
}

impl<'db> From<Option<(UnpackPosition, Unpack<'db>)>> for TargetKind<'db> {
    fn from(value: Option<(UnpackPosition, Unpack<'db>)>) -> Self {
        match value {
            Some((unpack_position, unpack)) => TargetKind::Sequence(unpack_position, unpack),
            None => TargetKind::Single,
        }
    }
}

#[derive(Clone, Debug)]
pub struct StarImportDefinitionKind {
    node: AstNodeRef<ast::StmtImportFrom>,
    place_id: ScopedPlaceId,
}

impl StarImportDefinitionKind {
    pub(crate) fn import<'ast>(&self, module: &'ast ParsedModuleRef) -> &'ast ast::StmtImportFrom {
        self.node.node(module)
    }

    pub(crate) fn alias<'ast>(&self, module: &'ast ParsedModuleRef) -> &'ast ast::Alias {
        // INVARIANT: for an invalid-syntax statement such as `from foo import *, bar, *`,
        // we only create a `StarImportDefinitionKind` for the *first* `*` alias in the names list.
        self.node
            .node(module)
            .names
            .iter()
            .find(|alias| &alias.name == "*")
            .expect(
                "The `StmtImportFrom` node of a `StarImportDefinitionKind` instance \
                should always have at least one `alias` with the name `*`.",
            )
    }

    pub(crate) fn place_id(&self) -> ScopedPlaceId {
        self.place_id
    }
}

#[derive(Clone, Debug)]
pub struct MatchPatternDefinitionKind {
    pattern: AstNodeRef<ast::Pattern>,
    identifier: AstNodeRef<ast::Identifier>,
    index: u32,
}

impl MatchPatternDefinitionKind {
    pub(crate) fn pattern<'ast>(&self, module: &'ast ParsedModuleRef) -> &'ast ast::Pattern {
        self.pattern.node(module)
    }

    pub(crate) fn index(&self) -> u32 {
        self.index
    }
}

/// Note that the elements of a comprehension can be in different scopes.
/// If the definition target of a comprehension is a name, it is in the comprehension's scope.
/// But if the target is an attribute or subscript, its definition is not in the comprehension's scope;
/// it is in the scope in which the root variable is bound.
/// TODO: currently we don't model this correctly and simply assume that it is in a scope outside the comprehension.
#[derive(Clone, Debug)]
pub struct ComprehensionDefinitionKind<'db> {
    target_kind: TargetKind<'db>,
    iterable: AstNodeRef<ast::Expr>,
    target: AstNodeRef<ast::Expr>,
    first: bool,
    is_async: bool,
}

impl<'db> ComprehensionDefinitionKind<'db> {
    pub(crate) fn iterable<'ast>(&self, module: &'ast ParsedModuleRef) -> &'ast ast::Expr {
        self.iterable.node(module)
    }

    pub(crate) fn target_kind(&self) -> TargetKind<'db> {
        self.target_kind
    }

    pub(crate) fn target<'ast>(&self, module: &'ast ParsedModuleRef) -> &'ast ast::Expr {
        self.target.node(module)
    }

    pub(crate) fn is_first(&self) -> bool {
        self.first
    }

    pub(crate) fn is_async(&self) -> bool {
        self.is_async
    }
}

#[derive(Clone, Debug)]
pub struct ImportDefinitionKind {
    node: AstNodeRef<ast::StmtImport>,
    alias_index: usize,
    is_reexported: bool,
}

impl ImportDefinitionKind {
    pub(crate) fn import<'ast>(&self, module: &'ast ParsedModuleRef) -> &'ast ast::StmtImport {
        self.node.node(module)
    }

    pub(crate) fn alias<'ast>(&self, module: &'ast ParsedModuleRef) -> &'ast ast::Alias {
        &self.node.node(module).names[self.alias_index]
    }

    pub(crate) fn is_reexported(&self) -> bool {
        self.is_reexported
    }
}

#[derive(Clone, Debug)]
pub struct ImportFromDefinitionKind {
    node: AstNodeRef<ast::StmtImportFrom>,
    alias_index: usize,
    is_reexported: bool,
}

impl ImportFromDefinitionKind {
    pub(crate) fn import<'ast>(&self, module: &'ast ParsedModuleRef) -> &'ast ast::StmtImportFrom {
        self.node.node(module)
    }

    pub(crate) fn alias<'ast>(&self, module: &'ast ParsedModuleRef) -> &'ast ast::Alias {
        &self.node.node(module).names[self.alias_index]
    }

    pub(crate) fn is_reexported(&self) -> bool {
        self.is_reexported
    }
}

#[derive(Clone, Debug)]
pub struct AssignmentDefinitionKind<'db> {
    target_kind: TargetKind<'db>,
    value: AstNodeRef<ast::Expr>,
    target: AstNodeRef<ast::Expr>,
}

impl<'db> AssignmentDefinitionKind<'db> {
    pub(crate) fn target_kind(&self) -> TargetKind<'db> {
        self.target_kind
    }

    pub(crate) fn value<'ast>(&self, module: &'ast ParsedModuleRef) -> &'ast ast::Expr {
        self.value.node(module)
    }

    pub(crate) fn target<'ast>(&self, module: &'ast ParsedModuleRef) -> &'ast ast::Expr {
        self.target.node(module)
    }
}

#[derive(Clone, Debug)]
pub struct AnnotatedAssignmentDefinitionKind {
    annotation: AstNodeRef<ast::Expr>,
    value: Option<AstNodeRef<ast::Expr>>,
    target: AstNodeRef<ast::Expr>,
}

impl AnnotatedAssignmentDefinitionKind {
    pub(crate) fn value<'ast>(&self, module: &'ast ParsedModuleRef) -> Option<&'ast ast::Expr> {
        self.value.as_ref().map(|value| value.node(module))
    }

    pub(crate) fn annotation<'ast>(&self, module: &'ast ParsedModuleRef) -> &'ast ast::Expr {
        self.annotation.node(module)
    }

    pub(crate) fn target<'ast>(&self, module: &'ast ParsedModuleRef) -> &'ast ast::Expr {
        self.target.node(module)
    }
}

#[derive(Clone, Debug)]
pub struct WithItemDefinitionKind<'db> {
    target_kind: TargetKind<'db>,
    context_expr: AstNodeRef<ast::Expr>,
    target: AstNodeRef<ast::Expr>,
    is_async: bool,
}

impl<'db> WithItemDefinitionKind<'db> {
    pub(crate) fn context_expr<'ast>(&self, module: &'ast ParsedModuleRef) -> &'ast ast::Expr {
        self.context_expr.node(module)
    }

    pub(crate) fn target_kind(&self) -> TargetKind<'db> {
        self.target_kind
    }

    pub(crate) fn target<'ast>(&self, module: &'ast ParsedModuleRef) -> &'ast ast::Expr {
        self.target.node(module)
    }

    pub(crate) const fn is_async(&self) -> bool {
        self.is_async
    }
}

#[derive(Clone, Debug)]
pub struct ForStmtDefinitionKind<'db> {
    target_kind: TargetKind<'db>,
    iterable: AstNodeRef<ast::Expr>,
    target: AstNodeRef<ast::Expr>,
    is_async: bool,
}

impl<'db> ForStmtDefinitionKind<'db> {
    pub(crate) fn iterable<'ast>(&self, module: &'ast ParsedModuleRef) -> &'ast ast::Expr {
        self.iterable.node(module)
    }

    pub(crate) fn target_kind(&self) -> TargetKind<'db> {
        self.target_kind
    }

    pub(crate) fn target<'ast>(&self, module: &'ast ParsedModuleRef) -> &'ast ast::Expr {
        self.target.node(module)
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
    pub(crate) fn node<'ast>(
        &self,
        module: &'ast ParsedModuleRef,
    ) -> &'ast ast::ExceptHandlerExceptHandler {
        self.handler.node(module)
    }

    pub(crate) fn handled_exceptions<'ast>(
        &self,
        module: &'ast ParsedModuleRef,
    ) -> Option<&'ast ast::Expr> {
        self.node(module).type_.as_deref()
    }

    pub(crate) fn is_star(&self) -> bool {
        self.is_star
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, salsa::Update)]
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

impl From<&ast::ExprAttribute> for DefinitionNodeKey {
    fn from(node: &ast::ExprAttribute) -> Self {
        Self(NodeKey::from_node(node))
    }
}

impl From<&ast::ExprSubscript> for DefinitionNodeKey {
    fn from(node: &ast::ExprSubscript) -> Self {
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
