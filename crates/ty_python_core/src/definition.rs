use std::ops::Deref;

use ruff_db::PythonFile;
use ruff_db::files::{File, FileRange};
use ruff_db::parsed::{ParsedModuleRef, parsed_module};
use ruff_python_ast::find_node::covering_node;
use ruff_python_ast::name::Name;
use ruff_python_ast::traversal::suite;
use ruff_python_ast::{self as ast, AnyParameter, AstNode, Expr, StmtKind, Suite};
use ruff_text_size::{Ranged, TextRange, TextSize};
use smallvec::SmallVec;

use crate::LoopHeaderId;
use crate::ProgramFile;
use crate::ast_node_ref::AstNodeRef;
use crate::member::ScopedMemberId;
use crate::node_key::NodeKey;
use crate::place::ScopedPlaceId;
use crate::predicate::PatternPredicate;
use crate::scope::{FileScopeId, ScopeId};
use crate::symbol::ScopedSymbolId;
use crate::unpack::{Unpack, UnpackPosition};
use crate::use_def::BindingWithConstraintsIterator;
use crate::{Db, Program, SemanticIndex};

/// A definition of a place.
///
/// ## ID stability
/// The `Definition`'s ID is stable when the only field that change is its `kind` (AST node).
///
/// The `Definition` changes when the `file`, `scope`, or `place` change. This can be
/// because a new scope gets inserted before the `Definition` or a new place is inserted
/// before this `Definition`. However, the ID can be considered stable and it is okay to use
/// `Definition` in cross-module` salsa queries or as a field on other salsa tracked structs.
#[salsa::tracked(
    debug,
    constructor = new_internal,
    heap_size = ruff_memory_usage::heap_size
)]
#[derive(Ord, PartialOrd)]
pub struct Definition<'db> {
    /// The scope in which the definition occurs.
    ///
    /// Storing the interned scope avoids retaining the file and file-local scope separately, at
    /// the cost of database lookups when either of those values is needed.
    #[returns(copy)]
    pub scope_id: ScopeId<'db>,

    /// The place ID and re-export state of the definition.
    #[returns(copy)]
    place_info: DefinitionPlace,

    /// WARNING: Only access this field when doing type inference for the same
    /// file as where `Definition` is defined to avoid cross-file query dependencies.
    #[no_eq]
    #[returns(ref)]
    #[tracked]
    pub kind: DefinitionKind<'db>,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for Definition<'_> {}

impl<'db> Definition<'db> {
    pub(crate) fn new(
        db: &'db dyn Db,
        scope_id: ScopeId<'db>,
        place: ScopedPlaceId,
        kind: DefinitionKind<'db>,
        is_reexported: bool,
    ) -> Self {
        Self::new_internal(
            db,
            scope_id,
            DefinitionPlace::new(place, is_reexported),
            kind,
        )
    }

    pub fn scope(self, db: &'db dyn Db) -> ScopeId<'db> {
        self.scope_id(db)
    }

    pub fn file(self, db: &'db dyn Db) -> File {
        self.scope_id(db).file(db)
    }

    pub fn python_file(self, db: &'db dyn Db) -> PythonFile<'db> {
        self.scope_id(db).python_file(db)
    }

    pub fn program_file(self, db: &'db dyn Db) -> ProgramFile<'db> {
        self.scope_id(db).program_file(db)
    }

    pub fn program(self, db: &'db dyn Db) -> Program<'db> {
        self.scope_id(db).program(db)
    }

    pub fn file_scope(self, db: &'db dyn Db) -> FileScopeId {
        self.scope_id(db).file_scope_id(db)
    }

    pub fn place(self, db: &'db dyn Db) -> ScopedPlaceId {
        self.place_info(db).place()
    }

    pub fn is_reexported(self, db: &'db dyn Db) -> bool {
        self.place_info(db).is_reexported()
    }

    pub fn full_range(self, db: &'db dyn Db, module: &ParsedModuleRef) -> FileRange {
        FileRange::new(self.file(db), self.kind(db).full_range(module))
    }

    pub fn focus_range(self, db: &'db dyn Db, module: &ParsedModuleRef) -> FileRange {
        FileRange::new(self.file(db), self.kind(db).target_range(module))
    }

    /// Returns the name of the item being defined, if applicable.
    pub fn name(self, db: &'db dyn Db) -> Option<String> {
        let module = parsed_module(db, self.python_file(db)).load(db);
        let kind = self.kind(db);
        match kind {
            DefinitionKind::Function(def) => {
                let node = def.node(&module);
                Some(node.name().as_str().to_string())
            }
            DefinitionKind::Class(def) => {
                let node = def.node(&module);
                Some(node.name().as_str().to_string())
            }
            DefinitionKind::TypeAlias(def) => {
                let node = def.node(&module);
                Some(
                    node.name()
                        .as_name_expr()
                        .expect("type alias name should be a NameExpr")
                        .id()
                        .as_str()
                        .to_string(),
                )
            }
            DefinitionKind::Assignment(assignment) => {
                let target_node = assignment.target.node(&module);
                target_node
                    .as_name_expr()
                    .map(|name_expr| name_expr.id().as_str().to_string())
            }
            _ => None,
        }
    }

    /// Extract a docstring from this definition, if applicable.
    /// This method returns a docstring for function, class, and attribute definitions.
    /// The docstring is extracted from the first statement in the body if it's a string literal.
    pub fn docstring(self, db: &'db dyn Db) -> Option<String> {
        let module = parsed_module(db, self.python_file(db)).load(db);
        let kind = self.kind(db);

        match kind {
            DefinitionKind::Assignment(assign_def) => {
                let assign_node = assign_def.target(&module);
                attribute_docstring(&module, assign_node)
                    .map(|docstring_expr| docstring_expr.value().to_str().to_owned())
            }
            DefinitionKind::AnnotatedAssignment(assign_def) => {
                let assign_node = assign_def.target(&module);
                attribute_docstring(&module, assign_node)
                    .map(|docstring_expr| docstring_expr.value().to_str().to_owned())
            }
            DefinitionKind::Function(function_def) => {
                let function_node = function_def.node(&module);
                docstring_from_body(function_node.body())
                    .map(|docstring_expr| docstring_expr.value().to_str().to_owned())
            }
            DefinitionKind::Class(class_def) => {
                let class_node = class_def.node(&module);
                docstring_from_body(class_node.body())
                    .map(|docstring_expr| docstring_expr.value().to_str().to_owned())
            }
            _ => None,
        }
    }
}

/// The identity of the place defined by a [`Definition`] and whether it is re-exported.
///
/// Keeping the re-export state in the enum lets it share the place ID's otherwise-unused
/// representation space. Storing it as a separate field on [`Definition`] would add padding to
/// every tracked definition.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, get_size2::GetSize)]
pub enum DefinitionPlace {
    Symbol {
        id: ScopedSymbolId,
        is_reexported: bool,
    },
    Member {
        id: ScopedMemberId,
        is_reexported: bool,
    },
}

impl DefinitionPlace {
    fn new(place: ScopedPlaceId, is_reexported: bool) -> Self {
        match place {
            ScopedPlaceId::Symbol(id) => Self::Symbol { id, is_reexported },
            ScopedPlaceId::Member(id) => Self::Member { id, is_reexported },
        }
    }

    fn place(self) -> ScopedPlaceId {
        match self {
            Self::Symbol { id, .. } => ScopedPlaceId::Symbol(id),
            Self::Member { id, .. } => ScopedPlaceId::Member(id),
        }
    }

    fn is_reexported(self) -> bool {
        match self {
            Self::Symbol { is_reexported, .. } | Self::Member { is_reexported, .. } => {
                is_reexported
            }
        }
    }
}

/// Extract a docstring from a function, module, or class body.
pub fn docstring_from_body(body: Suite<'_>) -> Option<ast::ExprStringLiteral<'_>> {
    let stmt = body.first()?;
    // Require the docstring to be a standalone expression.
    let stmt_expr = stmt.as_expr_stmt()?;
    let value = stmt_expr.value();
    // Only match string literals.
    value.as_string_literal_expr()
}

/// Extract a docstring from an attribute.
///
/// This is a non-standardized but popular-and-supported-by-sphinx kind of docstring
/// where you just place the docstring underneath an assignment to an attribute and
/// that counts as docs.
///
/// This is annoying to extract because we have a reference to (part of) an assignment statement
/// and we need to find the statement *after it*, which is easy to say but not something the
/// AST wants to encourage.
fn attribute_docstring<'a>(
    module: &'a ParsedModuleRef,
    assign_lvalue: Expr<'_>,
) -> Option<ast::ExprStringLiteral<'a>> {
    // Find all the ancestors of the assign lvalue
    let covering_node = covering_node(module.root().syntax_node(), assign_lvalue.range());
    // The assignment is the closest parent statement
    let assign = covering_node
        .find_first(|node| node.cast::<ast::Stmt>().is_some())
        .ok()?;
    let parent = assign.parent()?;
    let assign_node = assign.node();

    // The docs must be the next statement
    let parent_body = suite(assign_node, parent)?;
    let next_stmt = parent_body.next_sibling()?;

    // Require the docstring to be a standalone expression.
    let StmtKind::Expr(expr) = next_stmt.classify() else {
        return None;
    };
    let value = expr.value();

    // Only match string literals.
    value.as_string_literal_expr()
}

/// One or more [`Definition`]s.
#[derive(Debug, Default, PartialEq, Eq, get_size2::GetSize)]
pub struct Definitions<'db> {
    definitions: smallvec::SmallVec<[Definition<'db>; 1]>,
}

impl<'db> Definitions<'db> {
    pub(crate) fn push(&mut self, definition: Definition<'db>) {
        self.definitions.push(definition);
    }

    pub(crate) fn into_boxed_slice(self) -> Box<[Definition<'db>]> {
        self.definitions.into_vec().into_boxed_slice()
    }
}

impl<'db> Deref for Definitions<'db> {
    type Target = [Definition<'db>];

    fn deref(&self) -> &Self::Target {
        &self.definitions
    }
}

impl<'a, 'db> IntoIterator for &'a Definitions<'db> {
    type Item = &'a Definition<'db>;
    type IntoIter = std::slice::Iter<'a, Definition<'db>>;

    fn into_iter(self) -> Self::IntoIter {
        self.definitions.iter()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, get_size2::GetSize)]
pub enum DefinitionState<'db> {
    Defined(Definition<'db>),
    /// Represents the implicit "unbound"/"undeclared" definition of every place.
    Undefined,
    /// Represents a definition that has been deleted.
    /// This used when an attribute/subscript definition (such as `x.y = ...`, `x[0] = ...`) becomes obsolete due to a reassignment of the root place.
    Deleted,
}

impl<'db> DefinitionState<'db> {
    pub fn is_defined_and(self, f: impl Fn(Definition<'db>) -> bool) -> bool {
        matches!(self, DefinitionState::Defined(def) if f(def))
    }

    pub fn is_undefined_or(self, f: impl Fn(Definition<'db>) -> bool) -> bool {
        matches!(self, DefinitionState::Undefined)
            || matches!(self, DefinitionState::Defined(def) if f(def))
    }

    #[allow(unused)]
    pub fn definition(self) -> Option<Definition<'db>> {
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
    ImportFromSubmodule(ImportFromSubmoduleDefinitionNodeRef<'ast>),
    ImportStar(StarImportDefinitionNodeRef<'ast>),
    For(ForStmtDefinitionNodeRef<'ast, 'db>),
    Function(ast::StmtFunctionDef<'ast>),
    Class(ast::StmtClassDef<'ast>),
    TypeAlias(ast::StmtTypeAlias<'ast>),
    NamedExpression(ast::ExprNamed<'ast>),
    Assignment(AssignmentDefinitionNodeRef<'ast, 'db>),
    AnnotatedAssignment(AnnotatedAssignmentDefinitionNodeRef<'ast>),
    AugmentedAssignment(ast::StmtAugAssign<'ast>),
    DictKeyAssignment(DictKeyAssignmentNodeRef<'ast, 'db>),
    Comprehension(ComprehensionDefinitionNodeRef<'ast, 'db>),
    Parameter(ParameterDefinitionNodeRef<'ast>),
    LambdaParameter(LambdaParameterDefinitionNodeRef<'ast>),
    WithItem(WithItemDefinitionNodeRef<'ast, 'db>),
    MatchPattern(MatchPatternDefinitionNodeRef<'ast, 'db>),
    ExceptHandler(ExceptHandlerDefinitionNodeRef<'ast>),
    TypeVar(ast::TypeParamTypeVar<'ast>),
    ParamSpec(ast::TypeParamParamSpec<'ast>),
    TypeVarTuple(ast::TypeParamTypeVarTuple<'ast>),
    LoopHeader(LoopHeaderDefinitionNodeRef<'ast>),
}

impl<'ast> From<ast::StmtFunctionDef<'ast>> for DefinitionNodeRef<'ast, '_> {
    fn from(node: ast::StmtFunctionDef<'ast>) -> Self {
        Self::Function(node)
    }
}

impl<'ast> From<ast::StmtClassDef<'ast>> for DefinitionNodeRef<'ast, '_> {
    fn from(node: ast::StmtClassDef<'ast>) -> Self {
        Self::Class(node)
    }
}

impl<'ast> From<ast::StmtTypeAlias<'ast>> for DefinitionNodeRef<'ast, '_> {
    fn from(node: ast::StmtTypeAlias<'ast>) -> Self {
        Self::TypeAlias(node)
    }
}

impl<'ast> From<ast::ExprNamed<'ast>> for DefinitionNodeRef<'ast, '_> {
    fn from(node: ast::ExprNamed<'ast>) -> Self {
        Self::NamedExpression(node)
    }
}

impl<'ast> From<ast::StmtAugAssign<'ast>> for DefinitionNodeRef<'ast, '_> {
    fn from(node: ast::StmtAugAssign<'ast>) -> Self {
        Self::AugmentedAssignment(node)
    }
}

impl<'ast> From<ast::TypeParamTypeVar<'ast>> for DefinitionNodeRef<'ast, '_> {
    fn from(value: ast::TypeParamTypeVar<'ast>) -> Self {
        Self::TypeVar(value)
    }
}

impl<'ast> From<ast::TypeParamParamSpec<'ast>> for DefinitionNodeRef<'ast, '_> {
    fn from(value: ast::TypeParamParamSpec<'ast>) -> Self {
        Self::ParamSpec(value)
    }
}

impl<'ast> From<ast::TypeParamTypeVarTuple<'ast>> for DefinitionNodeRef<'ast, '_> {
    fn from(value: ast::TypeParamTypeVarTuple<'ast>) -> Self {
        Self::TypeVarTuple(value)
    }
}

impl<'ast> From<LoopHeaderDefinitionNodeRef<'ast>> for DefinitionNodeRef<'ast, '_> {
    fn from(value: LoopHeaderDefinitionNodeRef<'ast>) -> Self {
        Self::LoopHeader(value)
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

impl<'ast> From<ImportFromSubmoduleDefinitionNodeRef<'ast>> for DefinitionNodeRef<'ast, '_> {
    fn from(node_ref: ImportFromSubmoduleDefinitionNodeRef<'ast>) -> Self {
        Self::ImportFromSubmodule(node_ref)
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

impl<'ast, 'db> From<DictKeyAssignmentNodeRef<'ast, 'db>> for DefinitionNodeRef<'ast, 'db> {
    fn from(node_ref: DictKeyAssignmentNodeRef<'ast, 'db>) -> Self {
        Self::DictKeyAssignment(node_ref)
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

impl<'ast> From<ParameterDefinitionNodeRef<'ast>> for DefinitionNodeRef<'ast, '_> {
    fn from(node: ParameterDefinitionNodeRef<'ast>) -> Self {
        Self::Parameter(node)
    }
}

impl<'ast> From<LambdaParameterDefinitionNodeRef<'ast>> for DefinitionNodeRef<'ast, '_> {
    fn from(node: LambdaParameterDefinitionNodeRef<'ast>) -> Self {
        Self::LambdaParameter(node)
    }
}

impl<'ast, 'db> From<MatchPatternDefinitionNodeRef<'ast, 'db>> for DefinitionNodeRef<'ast, 'db> {
    fn from(node: MatchPatternDefinitionNodeRef<'ast, 'db>) -> Self {
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
    pub(crate) node: ast::StmtImport<'ast>,
    pub(crate) alias_index: usize,
    pub(crate) is_reexported: bool,
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct StarImportDefinitionNodeRef<'ast> {
    pub(crate) node: ast::StmtImportFrom<'ast>,
    pub(crate) symbol_id: ScopedSymbolId,
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct ImportFromDefinitionNodeRef<'ast> {
    pub(crate) node: ast::StmtImportFrom<'ast>,
    pub(crate) alias_index: usize,
    pub(crate) is_reexported: bool,
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct ImportFromSubmoduleDefinitionNodeRef<'ast> {
    pub(crate) node: ast::StmtImportFrom<'ast>,
    pub(crate) module_index: usize,
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct AssignmentDefinitionNodeRef<'ast, 'db> {
    pub(crate) unpack: Option<Unpack<'db>>,
    pub(crate) value: ast::Expr<'ast>,
    pub(crate) target: ast::Expr<'ast>,
    pub(crate) owner: BindingsOwner,
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct AnnotatedAssignmentDefinitionNodeRef<'ast> {
    pub(crate) node: ast::StmtAnnAssign<'ast>,
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct DictKeyAssignmentNodeRef<'ast, 'db> {
    pub(crate) key: ast::Expr<'ast>,
    pub(crate) value: ast::Expr<'ast>,
    pub(crate) assignment: Definition<'db>,
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct WithItemDefinitionNodeRef<'ast, 'db> {
    pub(crate) unpack: Option<(UnpackPosition, Unpack<'db>)>,
    pub(crate) item: ast::WithItem<'ast>,
    pub(crate) target: ast::Expr<'ast>,
    pub(crate) is_async: bool,
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct ForStmtDefinitionNodeRef<'ast, 'db> {
    pub(crate) unpack: Option<(UnpackPosition, Unpack<'db>)>,
    pub(crate) node: ast::StmtFor<'ast>,
    pub(crate) target: ast::Expr<'ast>,
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct ExceptHandlerDefinitionNodeRef<'ast> {
    pub(crate) handler: ast::ExceptHandlerExceptHandler<'ast>,
    pub(crate) is_star: bool,
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct LoopHeaderDefinitionNodeRef<'ast> {
    pub(crate) loop_stmt: LoopStmtRef<'ast>,
    pub(crate) place: ScopedPlaceId,
    pub(crate) loop_header_id: LoopHeaderId,
}

#[derive(Copy, Clone, Debug)]
pub(crate) enum LoopStmtRef<'ast> {
    While(ast::StmtWhile<'ast>),
    For(ast::StmtFor<'ast>),
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct ComprehensionDefinitionNodeRef<'ast, 'db> {
    pub(crate) unpack: Option<(UnpackPosition, Unpack<'db>)>,
    pub(crate) node: ast::Comprehension<'ast>,
    pub(crate) target: ast::Expr<'ast>,
    pub(crate) first: bool,
}

#[derive(Copy, Clone, Debug)]
pub(crate) enum ParameterDefinitionNodeRef<'ast> {
    VariadicPositionalParameter(ast::Parameter<'ast>),
    VariadicKeywordParameter(ast::Parameter<'ast>),
    Parameter(ast::ParameterWithDefault<'ast>),
}

impl ParameterDefinitionNodeRef<'_> {
    fn into_owned(self, parsed: &ParsedModuleRef) -> ParameterDefinitionNodeKind {
        match self {
            Self::VariadicPositionalParameter(parameter) => {
                ParameterDefinitionNodeKind::VariadicPositionalParameter(AstNodeRef::from_view(
                    parsed, parameter,
                ))
            }
            Self::VariadicKeywordParameter(parameter) => {
                ParameterDefinitionNodeKind::VariadicKeywordParameter(AstNodeRef::from_view(
                    parsed, parameter,
                ))
            }
            Self::Parameter(parameter) => {
                ParameterDefinitionNodeKind::Parameter(AstNodeRef::from_view(parsed, parameter))
            }
        }
    }

    fn key(self) -> DefinitionNodeKey {
        match self {
            Self::VariadicPositionalParameter(node) => node.into(),
            Self::VariadicKeywordParameter(node) => node.into(),
            Self::Parameter(node) => (node.parameter()).into(),
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct LambdaParameterDefinitionNodeRef<'ast> {
    pub(crate) index: usize,
    pub(crate) parameter: ParameterDefinitionNodeRef<'ast>,
    pub(crate) lambda: ast::ExprLambda<'ast>,
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct MatchPatternDefinitionNodeRef<'ast, 'db> {
    /// The outermost pattern node in which the identifier being defined occurs.
    pub(crate) pattern: ast::Pattern<'ast>,
    /// The identifier being defined.
    pub(crate) identifier: ast::Identifier<'ast>,
    /// The predicate for the complete match case containing this binding.
    pub(crate) predicate: PatternPredicate<'db>,
}

impl<'db> DefinitionNodeRef<'_, 'db> {
    pub(super) fn into_owned(self, parsed: &ParsedModuleRef) -> DefinitionKind<'db> {
        match self {
            DefinitionNodeRef::Import(ImportDefinitionNodeRef {
                node,
                alias_index,
                is_reexported,
            }) => DefinitionKind::Import(ImportDefinitionKind {
                node: AstNodeRef::from_view(parsed, node),
                alias_index: alias_index
                    .try_into()
                    .expect("import alias index should fit in u32"),
                is_reexported,
            }),
            DefinitionNodeRef::ImportFrom(ImportFromDefinitionNodeRef {
                node,
                alias_index,
                is_reexported,
            }) => DefinitionKind::ImportFrom(ImportFromDefinitionKind {
                node: AstNodeRef::from_view(parsed, node),
                alias_index: alias_index
                    .try_into()
                    .expect("import-from alias index should fit in u32"),
                is_reexported,
            }),
            DefinitionNodeRef::ImportFromSubmodule(ImportFromSubmoduleDefinitionNodeRef {
                node,
                module_index,
            }) => DefinitionKind::ImportFromSubmodule(ImportFromSubmoduleDefinitionKind {
                node: AstNodeRef::from_view(parsed, node),
                module_index: module_index
                    .try_into()
                    .expect("import-from submodule index should fit in u32"),
            }),
            DefinitionNodeRef::ImportStar(star_import) => {
                let StarImportDefinitionNodeRef { node, symbol_id } = star_import;
                DefinitionKind::StarImport(StarImportDefinitionKind {
                    node: AstNodeRef::from_view(parsed, node),
                    symbol_id,
                })
            }
            DefinitionNodeRef::Function(function) => {
                DefinitionKind::Function(FunctionDefinitionKind {
                    node: AstNodeRef::from_view(parsed, function),
                    has_decorators: !function.decorator_list().is_empty(),
                })
            }
            DefinitionNodeRef::Class(class) => {
                DefinitionKind::Class(AstNodeRef::from_view(parsed, class))
            }
            DefinitionNodeRef::TypeAlias(type_alias) => {
                DefinitionKind::TypeAlias(AstNodeRef::from_view(parsed, type_alias))
            }
            DefinitionNodeRef::NamedExpression(named) => {
                DefinitionKind::NamedExpression(AstNodeRef::from_view(parsed, named))
            }
            DefinitionNodeRef::Assignment(AssignmentDefinitionNodeRef {
                unpack,
                value,
                target,
                owner,
            }) => DefinitionKind::Assignment(AssignmentDefinitionKind {
                unpack,
                value: AstNodeRef::from_view(parsed, value),
                target: AstNodeRef::from_view(parsed, target),
                owner,
            }),
            DefinitionNodeRef::AnnotatedAssignment(AnnotatedAssignmentDefinitionNodeRef {
                node,
            }) => DefinitionKind::AnnotatedAssignment(AnnotatedAssignmentDefinitionKind {
                node: AstNodeRef::from_view(parsed, node),
                has_value: node.value().is_some(),
            }),
            DefinitionNodeRef::AugmentedAssignment(augmented_assignment) => {
                DefinitionKind::AugmentedAssignment(AstNodeRef::from_view(
                    parsed,
                    augmented_assignment,
                ))
            }
            DefinitionNodeRef::DictKeyAssignment(DictKeyAssignmentNodeRef {
                key,
                value,
                assignment,
            }) => DefinitionKind::DictKeyAssignment(DictKeyAssignmentKind {
                key: AstNodeRef::from_view(parsed, key),
                value: AstNodeRef::from_view(parsed, value),
                assignment,
            }),
            DefinitionNodeRef::For(ForStmtDefinitionNodeRef {
                unpack,
                node,
                target,
            }) => DefinitionKind::For(ForStmtDefinitionKind {
                unpack: unpack.map(|(_, unpack)| unpack),
                unpack_position: unpack.map_or(UnpackPosition::First, |(position, _)| position),
                node: AstNodeRef::from_view(parsed, node),
                target: AstNodeRef::from_view(parsed, target),
                is_async: node.is_async(),
            }),
            DefinitionNodeRef::Comprehension(ComprehensionDefinitionNodeRef {
                unpack,
                node,
                target,
                first,
            }) => DefinitionKind::Comprehension(ComprehensionDefinitionKind {
                unpack: unpack.map(|(_, unpack)| unpack),
                unpack_position: unpack.map_or(UnpackPosition::First, |(position, _)| position),
                node: AstNodeRef::from_view(parsed, node),
                target: AstNodeRef::from_view(parsed, target),
                first,
                is_async: node.is_async(),
            }),
            DefinitionNodeRef::Parameter(parameter) => {
                DefinitionKind::Parameter(parameter.into_owned(parsed))
            }
            DefinitionNodeRef::LambdaParameter(LambdaParameterDefinitionNodeRef {
                index,
                parameter,
                lambda,
            }) => DefinitionKind::LambdaParameter(LambdaParameterDefinitionNodeKind {
                index: index
                    .try_into()
                    .expect("lambda parameter index should fit in u32"),
                parameter: parameter.into_owned(parsed),
                lambda: AstNodeRef::from_view(parsed, lambda),
            }),
            DefinitionNodeRef::WithItem(WithItemDefinitionNodeRef {
                unpack,
                item,
                target,
                is_async,
            }) => DefinitionKind::WithItem(WithItemDefinitionKind {
                unpack: unpack.map(|(_, unpack)| unpack),
                unpack_position: unpack.map_or(UnpackPosition::First, |(position, _)| position),
                item: AstNodeRef::from_view(parsed, item),
                target: AstNodeRef::from_view(parsed, target),
                is_async,
            }),
            DefinitionNodeRef::MatchPattern(MatchPatternDefinitionNodeRef {
                pattern,
                identifier,
                predicate,
            }) => DefinitionKind::MatchPattern(MatchPatternDefinitionKind {
                pattern: AstNodeRef::from_view(parsed, pattern),
                identifier: AstNodeRef::from_view(parsed, identifier),
                predicate,
            }),
            DefinitionNodeRef::ExceptHandler(ExceptHandlerDefinitionNodeRef {
                handler,
                is_star,
            }) => DefinitionKind::ExceptHandler(ExceptHandlerDefinitionKind {
                handler: AstNodeRef::from_view(parsed, handler),
                is_star,
            }),
            DefinitionNodeRef::TypeVar(node) => {
                DefinitionKind::TypeVar(AstNodeRef::from_view(parsed, node))
            }
            DefinitionNodeRef::ParamSpec(node) => {
                DefinitionKind::ParamSpec(AstNodeRef::from_view(parsed, node))
            }
            DefinitionNodeRef::TypeVarTuple(node) => {
                DefinitionKind::TypeVarTuple(AstNodeRef::from_view(parsed, node))
            }
            DefinitionNodeRef::LoopHeader(LoopHeaderDefinitionNodeRef {
                loop_stmt,
                place,
                loop_header_id,
            }) => DefinitionKind::LoopHeader(LoopHeaderDefinitionKind {
                loop_header_id,
                loop_stmt: match loop_stmt {
                    LoopStmtRef::While(stmt) => {
                        LoopStmtKind::While(AstNodeRef::from_view(parsed, stmt))
                    }
                    LoopStmtRef::For(stmt) => {
                        LoopStmtKind::For(AstNodeRef::from_view(parsed, stmt))
                    }
                },
                place,
            }),
        }
    }

    pub(super) fn key(self) -> DefinitionNodeKey {
        match self {
            Self::Import(ImportDefinitionNodeRef {
                node,
                alias_index,
                is_reexported: _,
            }) => (node.names().at(alias_index)).into(),
            Self::ImportFrom(ImportFromDefinitionNodeRef {
                node,
                alias_index,
                is_reexported: _,
            }) => (node.names().at(alias_index)).into(),
            Self::ImportFromSubmodule(ImportFromSubmoduleDefinitionNodeRef { node, .. }) => {
                node.into()
            }
            // INVARIANT: for an invalid-syntax statement such as `from foo import *, bar, *`,
            // we only create a `StarImportDefinitionKind` for the *first* `*` alias in the names list.
            Self::ImportStar(StarImportDefinitionNodeRef { node, symbol_id: _ }) => node
                .names()
                .iter()
                .find(|alias| alias.name() == "*")
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
                owner: _,
            }) => DefinitionNodeKey(NodeKey::from_node(target)),
            Self::AnnotatedAssignment(ann_assign) => ann_assign.node.into(),
            Self::AugmentedAssignment(node) => node.into(),
            Self::DictKeyAssignment(node) => DefinitionNodeKey(NodeKey::from_node(node.key)),
            Self::For(ForStmtDefinitionNodeRef {
                target,
                node: _,
                unpack: _,
            }) => DefinitionNodeKey(NodeKey::from_node(target)),
            Self::Comprehension(ComprehensionDefinitionNodeRef { target, .. }) => {
                DefinitionNodeKey(NodeKey::from_node(target))
            }
            Self::LambdaParameter(LambdaParameterDefinitionNodeRef { parameter, .. }) => {
                parameter.key()
            }
            Self::Parameter(parameter) => parameter.key(),
            Self::WithItem(WithItemDefinitionNodeRef {
                item: _,
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
            Self::LoopHeader(LoopHeaderDefinitionNodeRef { loop_stmt, .. }) => match loop_stmt {
                LoopStmtRef::While(stmt) => stmt.into(),
                LoopStmtRef::For(stmt) => stmt.into(),
            },
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum DefinitionCategory {
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
    /// Annotations establish a declared type. So do function and class definitions.
    pub fn is_declaration(self) -> bool {
        matches!(
            self,
            DefinitionCategory::Declaration | DefinitionCategory::DeclarationAndBinding
        )
    }

    /// True if this definition assigns a value to the place.
    ///
    /// False only for annotated assignments without a RHS.
    pub fn is_binding(self) -> bool {
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
#[derive(Clone, Debug, get_size2::GetSize, salsa::SalsaValue)]
pub enum DefinitionKind<'db> {
    Import(ImportDefinitionKind),
    ImportFrom(ImportFromDefinitionKind),
    ImportFromSubmodule(ImportFromSubmoduleDefinitionKind),
    StarImport(StarImportDefinitionKind),
    Function(FunctionDefinitionKind),
    Class(AstNodeRef<ast::kind::StmtClassDef>),
    TypeAlias(AstNodeRef<ast::kind::StmtTypeAlias>),
    NamedExpression(AstNodeRef<ast::kind::ExprNamed>),
    Assignment(AssignmentDefinitionKind<'db>),
    AnnotatedAssignment(AnnotatedAssignmentDefinitionKind),
    AugmentedAssignment(AstNodeRef<ast::kind::StmtAugAssign>),
    DictKeyAssignment(DictKeyAssignmentKind<'db>),
    For(ForStmtDefinitionKind<'db>),
    Comprehension(ComprehensionDefinitionKind<'db>),
    Parameter(ParameterDefinitionNodeKind),
    LambdaParameter(LambdaParameterDefinitionNodeKind),
    WithItem(WithItemDefinitionKind<'db>),
    MatchPattern(MatchPatternDefinitionKind<'db>),
    ExceptHandler(ExceptHandlerDefinitionKind),
    TypeVar(AstNodeRef<ast::kind::TypeParamTypeVar>),
    ParamSpec(AstNodeRef<ast::kind::TypeParamParamSpec>),
    TypeVarTuple(AstNodeRef<ast::kind::TypeParamTypeVarTuple>),
    LoopHeader(LoopHeaderDefinitionKind),
    // Boxing here helps avoid growing the memory footprint of this enum.
    NestedBindings(Box<NestedBindingsDefinitionKind>),
}

impl<'db> DefinitionKind<'db> {
    pub(crate) fn is_reexported(&self) -> bool {
        match self {
            DefinitionKind::Import(import) => import.is_reexported(),
            DefinitionKind::ImportFrom(import) => import.is_reexported(),
            DefinitionKind::ImportFromSubmodule(_) => true,
            _ => true,
        }
    }

    pub const fn as_star_import(&self) -> Option<&StarImportDefinitionKind> {
        match self {
            DefinitionKind::StarImport(import) => Some(import),
            _ => None,
        }
    }

    pub const fn as_class(&self) -> Option<&AstNodeRef<ast::kind::StmtClassDef>> {
        match self {
            DefinitionKind::Class(class) => Some(class),
            _ => None,
        }
    }

    pub fn is_import(&self) -> bool {
        matches!(
            self,
            DefinitionKind::Import(_)
                | DefinitionKind::ImportFrom(_)
                | DefinitionKind::StarImport(_)
                | DefinitionKind::ImportFromSubmodule(_)
        )
    }

    pub const fn is_unannotated_assignment(&self) -> bool {
        matches!(self, DefinitionKind::Assignment(_))
    }

    pub(crate) fn as_unannotated_assignment(&self) -> Option<AssignmentDefinitionKind<'db>> {
        match self {
            DefinitionKind::Assignment(assignment) => Some(assignment.clone()),
            _ => None,
        }
    }

    pub const fn is_function_def(&self) -> bool {
        matches!(self, DefinitionKind::Function(_))
    }

    pub const fn is_parameter_def(&self) -> bool {
        matches!(self, DefinitionKind::Parameter(_))
    }

    pub const fn is_loop_header(&self) -> bool {
        matches!(self, DefinitionKind::LoopHeader(_))
    }

    /// Returns `true` if this definition is user-visible (i.e., not an internal
    /// synthetic definition like a loop header or nested bindings definition).
    pub const fn is_user_visible(&self) -> bool {
        !matches!(
            self,
            DefinitionKind::LoopHeader(_) | DefinitionKind::NestedBindings(_)
        )
    }

    /// Returns the [`TextRange`] of the definition target.
    ///
    /// A definition target would mainly be the node representing the place being defined i.e.,
    /// [`ast::ExprName`], [`ast::Identifier`], [`ast::ExprAttribute`] or [`ast::ExprSubscript`] but could also be other nodes.
    pub fn target_range(&self, module: &ParsedModuleRef) -> TextRange {
        match self {
            DefinitionKind::Import(import) => import.alias(module).range(),
            DefinitionKind::ImportFrom(import) => import.alias(module).range(),
            DefinitionKind::ImportFromSubmodule(import) => import.target_range(module),
            DefinitionKind::StarImport(import) => import.alias(module).range(),
            DefinitionKind::Function(function) => function.node.node(module).name().range(),
            DefinitionKind::Class(class) => class.node(module).name().range(),
            DefinitionKind::TypeAlias(type_alias) => type_alias.node(module).name().range(),
            DefinitionKind::NamedExpression(named) => named.node(module).target().range(),
            DefinitionKind::Assignment(assignment) => assignment.target(module).range(),
            DefinitionKind::AnnotatedAssignment(assign) => assign.target(module).range(),
            DefinitionKind::AugmentedAssignment(aug_assign) => {
                aug_assign.node(module).target().range()
            }
            DefinitionKind::DictKeyAssignment(dict_key_assignment) => {
                dict_key_assignment.key.node(module).range()
            }
            DefinitionKind::For(for_stmt) => for_stmt.target(module).range(),
            DefinitionKind::Comprehension(comp) => comp.target(module).range(),
            DefinitionKind::Parameter(parameter) => parameter.target_range(module),
            DefinitionKind::LambdaParameter(LambdaParameterDefinitionNodeKind {
                parameter,
                ..
            }) => parameter.target_range(module),
            DefinitionKind::WithItem(with_item) => with_item.target(module).range(),
            DefinitionKind::MatchPattern(match_pattern) => {
                match_pattern.identifier.node(module).range()
            }
            DefinitionKind::ExceptHandler(handler) => handler
                .node(module)
                .name()
                .map_or_else(|| handler.handler.node(module).range(), |node| node.range()),
            DefinitionKind::TypeVar(type_var) => type_var.node(module).name().range(),
            DefinitionKind::ParamSpec(param_spec) => param_spec.node(module).name().range(),
            DefinitionKind::TypeVarTuple(type_var_tuple) => {
                type_var_tuple.node(module).name().range()
            }
            DefinitionKind::LoopHeader(loop_header) => loop_header.range(module),
            DefinitionKind::NestedBindings(nested_bindings) => {
                // TODO: We only return the `TextRange` of one of the `nonlocal` or `global`
                // declarations that affect this variable, even if there's more than one. We could
                // find a way to return all of them, or split up the synthetic definition somehow.
                nested_bindings.nested_declarations[0].range
            }
        }
    }

    /// Returns the [`TextRange`] of the entire definition.
    pub fn full_range(&self, module: &ParsedModuleRef) -> TextRange {
        match self {
            DefinitionKind::Import(import) => import.alias(module).range(),
            DefinitionKind::ImportFrom(import) => import.alias(module).range(),
            DefinitionKind::ImportFromSubmodule(import) => import.module(module).range(),
            DefinitionKind::StarImport(import) => import.import(module).range(),
            DefinitionKind::Function(function) => function.node.node(module).range(),
            DefinitionKind::Class(class) => class.node(module).range(),
            DefinitionKind::TypeAlias(type_alias) => type_alias.node(module).range(),
            DefinitionKind::NamedExpression(named) => named.node(module).range(),
            DefinitionKind::Assignment(assign) => {
                let target_range = assign.target(module).range();
                let value_range = assign.value(module).range();
                target_range.cover(value_range)
            }
            DefinitionKind::AnnotatedAssignment(assign) => {
                let mut full_range = assign.target(module).range();
                full_range = full_range.cover(assign.annotation(module).range());

                if let Some(value) = assign.value(module) {
                    full_range = full_range.cover(value.range());
                }

                full_range
            }
            DefinitionKind::AugmentedAssignment(aug_assign) => aug_assign.node(module).range(),
            DefinitionKind::DictKeyAssignment(dict_key_assignment) => {
                dict_key_assignment.key.node(module).range()
            }
            DefinitionKind::For(for_stmt) => for_stmt.target(module).range(),
            DefinitionKind::Comprehension(comp) => comp.target(module).range(),
            DefinitionKind::Parameter(parameter) => parameter.full_range(module),
            DefinitionKind::LambdaParameter(LambdaParameterDefinitionNodeKind {
                parameter,
                ..
            }) => parameter.full_range(module),
            DefinitionKind::WithItem(with_item) => with_item.target(module).range(),
            DefinitionKind::MatchPattern(match_pattern) => {
                match_pattern.identifier.node(module).range()
            }
            DefinitionKind::ExceptHandler(handler) => handler.handler.node(module).range(),
            DefinitionKind::TypeVar(type_var) => type_var.node(module).range(),
            DefinitionKind::ParamSpec(param_spec) => param_spec.node(module).range(),
            DefinitionKind::TypeVarTuple(type_var_tuple) => type_var_tuple.node(module).range(),
            DefinitionKind::LoopHeader(loop_header) => loop_header.range(module),
            DefinitionKind::NestedBindings(nested_bindings) => {
                // TODO: We only return the `TextRange` of one of the `nonlocal` or `global`
                // declarations that affect this variable, even if there's more than one. We could
                // find a way to return all of them, or split up the synthetic definition somehow.
                nested_bindings.nested_declarations[0].range
            }
        }
    }

    pub fn category(&self, in_stub: bool, module: &ParsedModuleRef) -> DefinitionCategory {
        match self {
            // Functions and classes always bind, and we consider them declarations.
            DefinitionKind::Function(_)
            | DefinitionKind::Class(_)
            | DefinitionKind::TypeAlias(_)
            | DefinitionKind::TypeVar(_)
            | DefinitionKind::ParamSpec(_)
            | DefinitionKind::TypeVarTuple(_) => DefinitionCategory::DeclarationAndBinding,
            DefinitionKind::Parameter(parameter) => parameter.category(module),
            DefinitionKind::LambdaParameter(LambdaParameterDefinitionNodeKind {
                parameter,
                ..
            }) => parameter.category(module),
            // Annotated assignment is always a declaration. It is also a binding if there is a RHS
            // or if we are in a stub file. Unfortunately, it is common for stubs to omit even an `...` value placeholder.
            DefinitionKind::AnnotatedAssignment(ann_assign) => {
                if in_stub || ann_assign.has_value() {
                    DefinitionCategory::DeclarationAndBinding
                } else {
                    DefinitionCategory::Declaration
                }
            }
            // all of these bind values without declaring a type
            DefinitionKind::DictKeyAssignment(_)
            | DefinitionKind::NamedExpression(_)
            | DefinitionKind::Assignment(_)
            | DefinitionKind::AugmentedAssignment(_)
            | DefinitionKind::For(_)
            | DefinitionKind::Comprehension(_)
            | DefinitionKind::WithItem(_)
            | DefinitionKind::MatchPattern(_)
            | DefinitionKind::Import(_)
            | DefinitionKind::ImportFrom(_)
            | DefinitionKind::StarImport(_)
            | DefinitionKind::ImportFromSubmodule(_)
            | DefinitionKind::ExceptHandler(_)
            | DefinitionKind::LoopHeader(_)
            | DefinitionKind::NestedBindings(_) => DefinitionCategory::Binding,
        }
    }

    /// Returns the value expression for assignment-based definitions.
    ///
    /// Returns `Some` for `Assignment` and `AnnotatedAssignment` (if it has a value),
    /// `None` for all other definition kinds.
    pub fn value<'ast>(&self, module: &'ast ParsedModuleRef) -> Option<ast::Expr<'ast>> {
        match self {
            DefinitionKind::Assignment(assignment) => Some(assignment.value(module)),
            DefinitionKind::AnnotatedAssignment(assignment) => assignment.value(module),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, get_size2::GetSize)]
pub struct FunctionDefinitionKind {
    node: AstNodeRef<ast::kind::StmtFunctionDef>,
    has_decorators: bool,
}

impl FunctionDefinitionKind {
    pub fn node<'ast>(&self, module: &'ast ParsedModuleRef) -> ast::StmtFunctionDef<'ast> {
        self.node.node(module)
    }

    pub fn node_key(&self) -> NodeKey {
        NodeKey::from_node_ref(&self.node)
    }

    /// Whether the function has decorators, without loading its module's AST.
    pub fn has_decorators(&self) -> bool {
        self.has_decorators
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Hash, get_size2::GetSize)]
pub enum TargetKind<'db> {
    Sequence(UnpackPosition, Unpack<'db>),
    /// Name, attribute, or subscript.
    Single,
}

impl<'db> TargetKind<'db> {
    fn from_unpack(unpack: Option<Unpack<'db>>, unpack_position: UnpackPosition) -> Self {
        match unpack {
            Some(unpack) => TargetKind::Sequence(unpack_position, unpack),
            None => TargetKind::Single,
        }
    }
}

#[derive(Clone, Debug, get_size2::GetSize)]
pub struct StarImportDefinitionKind {
    node: AstNodeRef<ast::kind::StmtImportFrom>,
    symbol_id: ScopedSymbolId,
}

impl StarImportDefinitionKind {
    pub fn import<'ast>(&self, module: &'ast ParsedModuleRef) -> ast::StmtImportFrom<'ast> {
        self.node.node(module)
    }

    pub fn alias<'ast>(&self, module: &'ast ParsedModuleRef) -> ast::Alias<'ast> {
        // INVARIANT: for an invalid-syntax statement such as `from foo import *, bar, *`,
        // we only create a `StarImportDefinitionKind` for the *first* `*` alias in the names list.
        self.node
            .node(module)
            .names()
            .iter()
            .find(|alias| alias.name() == "*")
            .expect(
                "The `StmtImportFrom` node of a `StarImportDefinitionKind` instance \
                should always have at least one `alias` with the name `*`.",
            )
    }

    pub fn symbol_id(&self) -> ScopedSymbolId {
        self.symbol_id
    }
}

#[derive(Clone, Debug, get_size2::GetSize, salsa::SalsaValue)]
pub struct MatchPatternDefinitionKind<'db> {
    pattern: AstNodeRef<ast::kind::Pattern>,
    identifier: AstNodeRef<ast::kind::Identifier>,
    predicate: PatternPredicate<'db>,
}

impl<'db> MatchPatternDefinitionKind<'db> {
    pub fn pattern<'ast>(&self, module: &'ast ParsedModuleRef) -> ast::Pattern<'ast> {
        self.pattern.node(module)
    }

    pub fn predicate(&self) -> PatternPredicate<'db> {
        self.predicate
    }
}

/// Note that the elements of a comprehension can be in different scopes.
/// If the definition target of a comprehension is a name, it is in the comprehension's scope.
/// But if the target is an attribute or subscript, its definition is not in the comprehension's scope;
/// it is in the scope in which the root variable is bound.
/// TODO: currently we don't model this correctly and simply assume that it is in a scope outside the comprehension.
#[derive(Clone, Debug, get_size2::GetSize, salsa::SalsaValue)]
pub struct ComprehensionDefinitionKind<'db> {
    unpack: Option<Unpack<'db>>,
    node: AstNodeRef<ast::kind::Comprehension>,
    target: AstNodeRef<ast::kind::Expr>,
    first: bool,
    is_async: bool,
    unpack_position: UnpackPosition,
}

impl<'db> ComprehensionDefinitionKind<'db> {
    pub fn iterable<'ast>(&self, module: &'ast ParsedModuleRef) -> ast::Expr<'ast> {
        self.node.node(module).iter()
    }

    pub fn target_kind(&self) -> TargetKind<'db> {
        TargetKind::from_unpack(self.unpack, self.unpack_position)
    }

    pub fn target<'ast>(&self, module: &'ast ParsedModuleRef) -> ast::Expr<'ast> {
        self.target.node(module)
    }

    pub fn is_first(&self) -> bool {
        self.first
    }

    pub fn is_async(&self) -> bool {
        self.is_async
    }
}

#[derive(Clone, Debug, get_size2::GetSize)]
pub enum ParameterDefinitionNodeKind {
    VariadicPositionalParameter(AstNodeRef<ast::kind::Parameter>),
    VariadicKeywordParameter(AstNodeRef<ast::kind::Parameter>),
    Parameter(AstNodeRef<ast::kind::ParameterWithDefault>),
}

impl ParameterDefinitionNodeKind {
    fn target_range(&self, module: &ParsedModuleRef) -> TextRange {
        match self {
            Self::VariadicPositionalParameter(parameter) => parameter.node(module).name().range(),
            Self::VariadicKeywordParameter(parameter) => parameter.node(module).name().range(),
            Self::Parameter(parameter) => parameter.node(module).parameter().name().range(),
        }
    }

    fn full_range(&self, module: &ParsedModuleRef) -> TextRange {
        match self {
            Self::VariadicPositionalParameter(parameter) => parameter.node(module).range(),
            Self::VariadicKeywordParameter(parameter) => parameter.node(module).range(),
            Self::Parameter(parameter) => parameter.node(module).parameter().range(),
        }
    }

    fn category(&self, module: &ParsedModuleRef) -> DefinitionCategory {
        match self {
            // a parameter always binds a value, but is only a declaration if annotated
            Self::VariadicPositionalParameter(parameter)
            | Self::VariadicKeywordParameter(parameter) => {
                if parameter.node(module).annotation().is_some() {
                    DefinitionCategory::DeclarationAndBinding
                } else {
                    DefinitionCategory::Binding
                }
            }
            // presence of a default is irrelevant, same logic as for a no-default parameter
            Self::Parameter(parameter_with_default) => {
                if parameter_with_default
                    .node(module)
                    .parameter()
                    .annotation()
                    .is_some()
                {
                    DefinitionCategory::DeclarationAndBinding
                } else {
                    DefinitionCategory::Binding
                }
            }
        }
    }

    pub fn annotation<'ast>(&self, module: &'ast ParsedModuleRef) -> Option<ast::Expr<'ast>> {
        match self {
            Self::VariadicPositionalParameter(parameter)
            | Self::VariadicKeywordParameter(parameter) => parameter.node(module).annotation(),
            Self::Parameter(parameter_with_default) => {
                parameter_with_default.node(module).parameter().annotation()
            }
        }
    }
}

#[derive(Clone, Debug, get_size2::GetSize)]
pub struct LambdaParameterDefinitionNodeKind {
    pub index: u32,
    pub lambda: AstNodeRef<ast::kind::ExprLambda>,
    pub parameter: ParameterDefinitionNodeKind,
}

#[derive(Clone, Debug, get_size2::GetSize)]
pub struct ImportDefinitionKind {
    node: AstNodeRef<ast::kind::StmtImport>,
    alias_index: u32,
    is_reexported: bool,
}

impl ImportDefinitionKind {
    pub fn import<'ast>(&self, module: &'ast ParsedModuleRef) -> ast::StmtImport<'ast> {
        self.node.node(module)
    }

    pub fn alias<'ast>(&self, module: &'ast ParsedModuleRef) -> ast::Alias<'ast> {
        self.node.node(module).names().at(self.alias_index as usize)
    }

    fn is_reexported(&self) -> bool {
        self.is_reexported
    }
}

#[derive(Clone, Debug, get_size2::GetSize)]
pub struct ImportFromDefinitionKind {
    node: AstNodeRef<ast::kind::StmtImportFrom>,
    alias_index: u32,
    is_reexported: bool,
}

impl ImportFromDefinitionKind {
    pub fn import<'ast>(&self, module: &'ast ParsedModuleRef) -> ast::StmtImportFrom<'ast> {
        self.node.node(module)
    }

    pub fn alias<'ast>(&self, module: &'ast ParsedModuleRef) -> ast::Alias<'ast> {
        self.node.node(module).names().at(self.alias_index as usize)
    }

    fn is_reexported(&self) -> bool {
        self.is_reexported
    }
}
#[derive(Clone, Debug, get_size2::GetSize)]
pub struct ImportFromSubmoduleDefinitionKind {
    node: AstNodeRef<ast::kind::StmtImportFrom>,
    module_index: u32,
}

impl ImportFromSubmoduleDefinitionKind {
    pub fn import<'ast>(&self, module: &'ast ParsedModuleRef) -> ast::StmtImportFrom<'ast> {
        self.node.node(module)
    }

    fn module<'ast>(&self, module: &'ast ParsedModuleRef) -> ast::Identifier<'ast> {
        self.import(module)
            .module()
            .expect("import-from submodule definitions should always have a module identifier")
    }

    fn target_range(&self, module: &ParsedModuleRef) -> TextRange {
        let module_ident = self.module(module);
        let module_str = module_ident.as_str();

        // Find the dot that terminates the target component.
        let Some((end_offset, _)) = module_str
            .match_indices('.')
            .nth(self.module_index as usize)
        else {
            // This shouldn't happen but just in case, provide a safe default
            return module_ident.range();
        };

        // Find the start of the target component (after the previous dot, or string start).
        let start_offset = module_str[..end_offset].rfind('.').map_or(0, |pos| pos + 1);

        let Ok(start) = TextSize::try_from(start_offset) else {
            return module_ident.range();
        };
        let Ok(end) = TextSize::try_from(end_offset) else {
            return module_ident.range();
        };
        TextRange::new(start, end) + module_ident.start()
    }
}

/// The inference region that owns bindings created while evaluating an assignment's value.
#[derive(Clone, Copy, Debug, Eq, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub enum BindingsOwner {
    /// A simple-name assignment is represented by its definition.
    Definition,
    /// An assignment with multiple, unpacking, or non-name targets is represented by its statement.
    Statement,
}

#[derive(Clone, Debug, get_size2::GetSize, salsa::SalsaValue)]
pub struct AssignmentDefinitionKind<'db> {
    unpack: Option<Unpack<'db>>,
    value: AstNodeRef<ast::kind::Expr>,
    target: AstNodeRef<ast::kind::Expr>,
    owner: BindingsOwner,
}

impl<'db> AssignmentDefinitionKind<'db> {
    pub fn unpack(&self) -> Option<Unpack<'db>> {
        self.unpack
    }

    pub fn value<'ast>(&self, module: &'ast ParsedModuleRef) -> ast::Expr<'ast> {
        self.value.node(module)
    }

    pub fn target<'ast>(&self, module: &'ast ParsedModuleRef) -> ast::Expr<'ast> {
        self.target.node(module)
    }

    pub fn owner(&self) -> BindingsOwner {
        self.owner
    }
}

#[derive(Clone, Debug, get_size2::GetSize)]
pub struct AnnotatedAssignmentDefinitionKind {
    node: AstNodeRef<ast::kind::StmtAnnAssign>,
    has_value: bool,
}

impl AnnotatedAssignmentDefinitionKind {
    fn node<'ast>(&self, module: &'ast ParsedModuleRef) -> ast::StmtAnnAssign<'ast> {
        self.node.node(module)
    }

    pub fn value<'ast>(&self, module: &'ast ParsedModuleRef) -> Option<ast::Expr<'ast>> {
        self.node(module).value()
    }

    /// Returns whether this annotated assignment has a right-hand-side value.
    pub const fn has_value(&self) -> bool {
        self.has_value
    }

    pub fn annotation<'ast>(&self, module: &'ast ParsedModuleRef) -> ast::Expr<'ast> {
        self.node(module).annotation()
    }

    pub fn target<'ast>(&self, module: &'ast ParsedModuleRef) -> ast::Expr<'ast> {
        self.node(module).target()
    }
}

#[derive(Clone, Debug, get_size2::GetSize, salsa::SalsaValue)]
pub struct DictKeyAssignmentKind<'db> {
    key: AstNodeRef<ast::kind::Expr>,
    value: AstNodeRef<ast::kind::Expr>,
    assignment: Definition<'db>,
}

impl<'db> DictKeyAssignmentKind<'db> {
    pub fn key<'ast>(&self, module: &'ast ParsedModuleRef) -> ast::Expr<'ast> {
        self.key.node(module)
    }

    pub fn value<'ast>(&self, module: &'ast ParsedModuleRef) -> ast::Expr<'ast> {
        self.value.node(module)
    }

    pub fn assignment(&self) -> Definition<'db> {
        self.assignment
    }
}

#[derive(Clone, Debug, get_size2::GetSize, salsa::SalsaValue)]
pub struct WithItemDefinitionKind<'db> {
    unpack: Option<Unpack<'db>>,
    item: AstNodeRef<ast::kind::WithItem>,
    target: AstNodeRef<ast::kind::Expr>,
    is_async: bool,
    unpack_position: UnpackPosition,
}

impl<'db> WithItemDefinitionKind<'db> {
    pub fn context_expr<'ast>(&self, module: &'ast ParsedModuleRef) -> ast::Expr<'ast> {
        self.item.node(module).context_expr()
    }

    pub fn target_kind(&self) -> TargetKind<'db> {
        TargetKind::from_unpack(self.unpack, self.unpack_position)
    }

    pub fn target<'ast>(&self, module: &'ast ParsedModuleRef) -> ast::Expr<'ast> {
        self.target.node(module)
    }

    pub const fn is_async(&self) -> bool {
        self.is_async
    }
}

#[derive(Clone, Debug, get_size2::GetSize, salsa::SalsaValue)]
pub struct ForStmtDefinitionKind<'db> {
    unpack: Option<Unpack<'db>>,
    node: AstNodeRef<ast::kind::StmtFor>,
    target: AstNodeRef<ast::kind::Expr>,
    is_async: bool,
    unpack_position: UnpackPosition,
}

impl<'db> ForStmtDefinitionKind<'db> {
    pub fn iterable<'ast>(&self, module: &'ast ParsedModuleRef) -> ast::Expr<'ast> {
        self.node.node(module).iter()
    }

    pub fn target_kind(&self) -> TargetKind<'db> {
        TargetKind::from_unpack(self.unpack, self.unpack_position)
    }

    pub fn target<'ast>(&self, module: &'ast ParsedModuleRef) -> ast::Expr<'ast> {
        self.target.node(module)
    }

    pub const fn is_async(&self) -> bool {
        self.is_async
    }
}

#[derive(Clone, Debug, get_size2::GetSize)]
pub struct ExceptHandlerDefinitionKind {
    handler: AstNodeRef<ast::kind::ExceptHandlerExceptHandler>,
    is_star: bool,
}

impl ExceptHandlerDefinitionKind {
    pub fn node<'ast>(
        &self,
        module: &'ast ParsedModuleRef,
    ) -> ast::ExceptHandlerExceptHandler<'ast> {
        self.handler.node(module)
    }

    pub fn handled_exceptions<'ast>(
        &self,
        module: &'ast ParsedModuleRef,
    ) -> Option<ast::Expr<'ast>> {
        self.node(module).type_()
    }

    pub fn is_star(&self) -> bool {
        self.is_star
    }
}

/// Definition kind for a loop header entry.
#[derive(Clone, Debug, get_size2::GetSize)]
pub struct LoopHeaderDefinitionKind {
    /// The `LoopHeader` is reserved before walking the loop and populated afterward.
    loop_header_id: LoopHeaderId,
    loop_stmt: LoopStmtKind,
    place: ScopedPlaceId,
}

#[derive(Clone, Debug, get_size2::GetSize)]
pub(crate) enum LoopStmtKind {
    While(AstNodeRef<ast::kind::StmtWhile>),
    For(AstNodeRef<ast::kind::StmtFor>),
}

impl LoopHeaderDefinitionKind {
    pub fn loop_header_id(&self) -> LoopHeaderId {
        self.loop_header_id
    }

    pub fn place(&self) -> ScopedPlaceId {
        self.place
    }

    fn range(&self, module: &ParsedModuleRef) -> TextRange {
        match &self.loop_stmt {
            LoopStmtKind::While(stmt) => stmt.node(module).range(),
            LoopStmtKind::For(stmt) => stmt.node(module).range(),
        }
    }
}

#[derive(Clone, Debug, get_size2::GetSize)]
pub struct NestedBindingsDefinitionKind {
    pub name: Name,
    pub execution: NestedBindingExecution,
    // Note that in general this can include both `global` and `nonlocal` declarations from
    // different nested scopes, because we don't necessarily know at synthesis time which of those
    // kind will be visible in the current scope.
    pub nested_declarations: SmallVec<[crate::builder::NestedDeclaration; 1]>,
}

impl NestedBindingsDefinitionKind {
    /// Returns every nested binding source and whether it was declared `global`.
    ///
    /// Use [`Self::visible_binding_sources`] when resolving the binding in a particular scope.
    fn binding_sources<'index, 'db>(
        &'index self,
        index: &'index SemanticIndex<'db>,
    ) -> impl Iterator<Item = (bool, BindingWithConstraintsIterator<'index, 'db>)> + 'index {
        self.nested_declarations.iter().filter_map(|declaration| {
            debug_assert!(declaration.is_bound);
            let symbol = index
                .place_table(declaration.file_scope_id)
                .symbol_id(&self.name)?;
            let use_def = index.use_def_map(declaration.file_scope_id);
            let bindings = match self.execution {
                NestedBindingExecution::Lazy => use_def.reachable_bindings(symbol.into()),
                NestedBindingExecution::Eager => use_def.end_of_scope_bindings(symbol.into()),
            };
            Some((declaration.is_global(), bindings))
        })
    }

    /// Returns nested binding sources that can update the same variable as `scope`.
    ///
    /// A synthetic binding can collect both `global` and `nonlocal` writes to one name:
    ///
    /// ```python
    /// x = 0
    ///
    /// def outer():
    ///     x = 1
    ///
    ///     def change_global():
    ///         global x
    ///         x = 2
    ///
    ///     def change_nonlocal():
    ///         nonlocal x
    ///         x = 3
    /// ```
    ///
    /// Only `change_nonlocal` can update `outer`'s local `x`. Nested functions also cannot
    /// capture a class-local variable, so class scopes do not see nonlocal writes to their
    /// own bindings.
    pub fn visible_binding_sources<'index, 'db>(
        &'index self,
        index: &'index SemanticIndex<'db>,
        scope: FileScopeId,
    ) -> impl Iterator<Item = BindingWithConstraintsIterator<'index, 'db>> + 'index {
        let symbol_id = index.place_table(scope).symbol_id(&self.name);
        let sees_global = symbol_id
            .is_some_and(|symbol_id| index.symbol_resolves_to_global_scope(symbol_id, scope));
        let sees_nonlocal = !sees_global
            && symbol_id.is_some_and(|symbol_id| {
                !(index.scope(scope).kind().is_class()
                    && index.place_table(scope).symbol(symbol_id).is_local())
            });

        self.binding_sources(index)
            .filter_map(move |(is_global, bindings)| {
                (if is_global {
                    sees_global
                } else {
                    sees_nonlocal
                })
                .then_some(bindings)
            })
    }
}

/// Describes when writes from a nested scope can affect its containing scope.
#[derive(Copy, Clone, Debug, Eq, PartialEq, get_size2::GetSize)]
pub enum NestedBindingExecution {
    /// The nested scope can run later or repeatedly, as with a function body.
    Lazy,
    /// The nested scope is modeled as running while evaluating the containing expression.
    Eager,
}

#[derive(
    Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, get_size2::GetSize, salsa::SalsaValue,
)]
pub struct DefinitionNodeKey(NodeKey);

impl DefinitionNodeKey {
    pub(crate) fn module_index(self) -> Option<usize> {
        self.0.module_index()
    }

    pub(crate) fn from_node_ref(node: ruff_python_ast::SyntaxNode<'_>) -> Self {
        if let Some(parameter) = node.cast::<ast::ParameterWithDefault>() {
            parameter.into()
        } else {
            Self(NodeKey::from_node(node))
        }
    }

    pub fn from_assignment(node: ast::StmtAssign<'_>) -> impl Iterator<Item = DefinitionNodeKey> {
        node.targets()
            .iter()
            .map(|target| Self(NodeKey::from_node(target)))
    }
}

impl From<ast::Alias<'_>> for DefinitionNodeKey {
    fn from(node: ast::Alias<'_>) -> Self {
        Self(NodeKey::from_node(node))
    }
}

impl From<ast::StmtImportFrom<'_>> for DefinitionNodeKey {
    fn from(node: ast::StmtImportFrom<'_>) -> Self {
        Self(NodeKey::from_node(node))
    }
}

impl From<ast::StmtFunctionDef<'_>> for DefinitionNodeKey {
    fn from(node: ast::StmtFunctionDef<'_>) -> Self {
        Self(NodeKey::from_node(node))
    }
}

impl From<ast::StmtClassDef<'_>> for DefinitionNodeKey {
    fn from(node: ast::StmtClassDef<'_>) -> Self {
        Self(NodeKey::from_node(node))
    }
}

impl From<ast::StmtTypeAlias<'_>> for DefinitionNodeKey {
    fn from(node: ast::StmtTypeAlias<'_>) -> Self {
        Self(NodeKey::from_node(node))
    }
}

impl From<ast::ExprName<'_>> for DefinitionNodeKey {
    fn from(node: ast::ExprName<'_>) -> Self {
        Self(NodeKey::from_node(node))
    }
}

impl From<ast::ExprAttribute<'_>> for DefinitionNodeKey {
    fn from(node: ast::ExprAttribute<'_>) -> Self {
        Self(NodeKey::from_node(node))
    }
}

impl From<ast::ExprSubscript<'_>> for DefinitionNodeKey {
    fn from(node: ast::ExprSubscript<'_>) -> Self {
        Self(NodeKey::from_node(node))
    }
}

impl From<ast::ExprNamed<'_>> for DefinitionNodeKey {
    fn from(node: ast::ExprNamed<'_>) -> Self {
        Self(NodeKey::from_node(node))
    }
}

impl From<ast::StmtAnnAssign<'_>> for DefinitionNodeKey {
    fn from(node: ast::StmtAnnAssign<'_>) -> Self {
        Self(NodeKey::from_node(node))
    }
}

impl From<ast::StmtAugAssign<'_>> for DefinitionNodeKey {
    fn from(node: ast::StmtAugAssign<'_>) -> Self {
        Self(NodeKey::from_node(node))
    }
}

impl From<ast::StmtWhile<'_>> for DefinitionNodeKey {
    fn from(node: ast::StmtWhile<'_>) -> Self {
        Self(NodeKey::from_node(node))
    }
}

impl From<ast::StmtFor<'_>> for DefinitionNodeKey {
    fn from(node: ast::StmtFor<'_>) -> Self {
        Self(NodeKey::from_node(node))
    }
}

impl From<ast::Parameter<'_>> for DefinitionNodeKey {
    fn from(node: ast::Parameter<'_>) -> Self {
        Self(NodeKey::from_node(node))
    }
}

impl From<ast::ParameterWithDefault<'_>> for DefinitionNodeKey {
    fn from(node: ast::ParameterWithDefault<'_>) -> Self {
        Self(NodeKey::from_node(node.parameter()))
    }
}

impl From<AnyParameter<'_>> for DefinitionNodeKey {
    fn from(value: AnyParameter) -> Self {
        Self(match value {
            AnyParameter::Variadic(node) => NodeKey::from_node(node),
            AnyParameter::NonVariadic(node) => NodeKey::from_node(node.parameter()),
        })
    }
}

impl From<ast::Identifier<'_>> for DefinitionNodeKey {
    fn from(identifier: ast::Identifier<'_>) -> Self {
        Self(NodeKey::from_node(identifier))
    }
}

impl From<ast::ExceptHandlerExceptHandler<'_>> for DefinitionNodeKey {
    fn from(handler: ast::ExceptHandlerExceptHandler<'_>) -> Self {
        Self(NodeKey::from_node(handler))
    }
}

impl From<ast::TypeParamTypeVar<'_>> for DefinitionNodeKey {
    fn from(value: ast::TypeParamTypeVar<'_>) -> Self {
        Self(NodeKey::from_node(value))
    }
}

impl From<ast::TypeParamParamSpec<'_>> for DefinitionNodeKey {
    fn from(value: ast::TypeParamParamSpec<'_>) -> Self {
        Self(NodeKey::from_node(value))
    }
}

impl From<ast::TypeParamTypeVarTuple<'_>> for DefinitionNodeKey {
    fn from(value: ast::TypeParamTypeVarTuple<'_>) -> Self {
        Self(NodeKey::from_node(value))
    }
}

impl<T: ast::NodeType> From<&AstNodeRef<T>> for DefinitionNodeKey
where
    for<'a> T::Borrowed<'a>: Into<DefinitionNodeKey>,
{
    fn from(value: &AstNodeRef<T>) -> Self {
        Self(NodeKey::from_node_ref(value))
    }
}
