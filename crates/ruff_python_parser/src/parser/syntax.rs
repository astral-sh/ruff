use ruff_python_ast::{
    self as ast, NodeKind,
    flat::{self, BuildId, Builder, Emit, ListId, ListItem},
};
use ruff_text_size::{Ranged, TextRange};
use thin_vec::ThinVec;

/// The Pratt parser uses the range and kind of its current result for lookahead and errors.
/// Only its four-byte slot is written into its parent.
#[derive(Debug)]
pub(crate) struct Node<T> {
    pub(crate) id: BuildId<T>,
    pub(crate) range: TextRange,
    pub(crate) kind: NodeKind,
}
impl<T> Copy for Node<T> {}
impl<T> Clone for Node<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Ranged for Node<T> {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl<T: flat::Node> Node<T> {
    pub(crate) fn view(self, builder: &Builder) -> flat::View<'_, T> {
        builder.get(self.id)
    }
    pub(crate) fn from_view(view: flat::View<'_, T>) -> Self {
        Self {
            id: view.build_id(),
            range: view.range(),
            kind: view.node_kind(),
        }
    }
}

pub(crate) trait Construct: Emit + Sized {
    fn parsed(self, builder: &mut Builder) -> Node<Self::Output> {
        let (id, range, kind) = self.emit(builder);
        Node { id, range, kind }
    }
}
impl<T: Emit> Construct for T {}

impl<T: flat::Node + 'static> ListItem for Node<T> {
    type Element = flat::Handle<T>;
    fn word(self) -> u64 {
        self.id.word()
    }
}

pub(crate) trait IntoList<E> {
    fn into_list(self, builder: &mut Builder) -> ListId<E>;
}
impl<E> IntoList<E> for ListId<E> {
    fn into_list(self, _: &mut Builder) -> Self {
        self
    }
}
macro_rules! collection {
    ($ty:ty) => {
        impl<T: ListItem> IntoList<T::Element> for $ty {
            fn into_list(self, builder: &mut Builder) -> ListId<T::Element> {
                builder.list(self)
            }
        }
    };
}
collection!(Vec<T>);
collection!(ThinVec<T>);
collection!(Box<[T]>);

macro_rules! upcast {
    ($group:ident, $($ty:ident),+) => {$(
        impl From<Node<ast::$ty>> for Node<ast::$group> {
            fn from(value: Node<ast::$ty>) -> Self {
                Self {id:value.id.into(), range:value.range, kind:value.kind}
            }
        }
    )+};
}
upcast!(Mod, ModModule, ModExpression);
upcast!(
    Stmt,
    StmtFunctionDef,
    StmtClassDef,
    StmtReturn,
    StmtDelete,
    StmtTypeAlias,
    StmtAssign,
    StmtAugAssign,
    StmtAnnAssign,
    StmtFor,
    StmtWhile,
    StmtIf,
    StmtWith,
    StmtMatch,
    StmtRaise,
    StmtTry,
    StmtAssert,
    StmtImport,
    StmtImportFrom,
    StmtGlobal,
    StmtNonlocal,
    StmtExpr,
    StmtPass,
    StmtBreak,
    StmtContinue,
    StmtIpyEscapeCommand
);
upcast!(
    Expr,
    ExprBoolOp,
    ExprNamed,
    ExprBinOp,
    ExprUnaryOp,
    ExprLambda,
    ExprIf,
    ExprDict,
    ExprSet,
    ExprListComp,
    ExprSetComp,
    ExprDictComp,
    ExprGenerator,
    ExprAwait,
    ExprYield,
    ExprYieldFrom,
    ExprCompare,
    ExprCall,
    ExprFString,
    ExprTString,
    ExprStringLiteral,
    ExprBytesLiteral,
    ExprNumberLiteral,
    ExprBooleanLiteral,
    ExprNoneLiteral,
    ExprEllipsisLiteral,
    ExprAttribute,
    ExprSubscript,
    ExprStarred,
    ExprName,
    ExprList,
    ExprTuple,
    ExprSlice,
    ExprIpyEscapeCommand
);
upcast!(ExceptHandler, ExceptHandlerExceptHandler);
upcast!(
    InterpolatedStringElement,
    InterpolatedElement,
    InterpolatedStringLiteralElement
);
upcast!(
    Pattern,
    PatternMatchValue,
    PatternMatchSingleton,
    PatternMatchSequence,
    PatternMatchMapping,
    PatternMatchClass,
    PatternMatchStar,
    PatternMatchAs,
    PatternMatchOr
);
upcast!(
    TypeParam,
    TypeParamTypeVar,
    TypeParamTypeVarTuple,
    TypeParamParamSpec
);
pub(crate) type Alias = Node<ast::Alias>;
pub(crate) type Arguments = Node<ast::Arguments>;
pub(crate) type BytesLiteral = Node<ast::BytesLiteral>;
pub(crate) type Comprehension = Node<ast::Comprehension>;
pub(crate) type ElifElseClause = Node<ast::ElifElseClause>;
pub(crate) type ExceptHandler = Node<ast::ExceptHandler>;
pub(crate) type Expr = Node<ast::Expr>;
pub(crate) type ExprAttribute = Node<ast::ExprAttribute>;
pub(crate) type ExprAwait = Node<ast::ExprAwait>;
pub(crate) type ExprBoolOp = Node<ast::ExprBoolOp>;
pub(crate) type ExprCall = Node<ast::ExprCall>;
pub(crate) type ExprCompare = Node<ast::ExprCompare>;
pub(crate) type ExprDict = Node<ast::ExprDict>;
pub(crate) type ExprDictComp = Node<ast::ExprDictComp>;
pub(crate) type ExprGenerator = Node<ast::ExprGenerator>;
pub(crate) type ExprIf = Node<ast::ExprIf>;
pub(crate) type ExprIpyEscapeCommand = Node<ast::ExprIpyEscapeCommand>;
pub(crate) type ExprLambda = Node<ast::ExprLambda>;
pub(crate) type ExprList = Node<ast::ExprList>;
pub(crate) type ExprListComp = Node<ast::ExprListComp>;
pub(crate) type ExprName = Node<ast::ExprName>;
pub(crate) type ExprNamed = Node<ast::ExprNamed>;
pub(crate) type ExprSet = Node<ast::ExprSet>;
pub(crate) type ExprSetComp = Node<ast::ExprSetComp>;
pub(crate) type ExprStarred = Node<ast::ExprStarred>;
pub(crate) type ExprSubscript = Node<ast::ExprSubscript>;
pub(crate) type ExprTuple = Node<ast::ExprTuple>;
pub(crate) type ExprUnaryOp = Node<ast::ExprUnaryOp>;
pub(crate) type FString = Node<ast::FString>;
pub(crate) type Identifier = Node<ast::Identifier>;
pub(crate) type InterpolatedElement = Node<ast::InterpolatedElement>;
pub(crate) type InterpolatedStringElement = Node<ast::InterpolatedStringElement>;
pub(crate) type InterpolatedStringLiteralElement = Node<ast::InterpolatedStringLiteralElement>;
pub(crate) type MatchCase = Node<ast::MatchCase>;
pub(crate) type Mod = Node<ast::Mod>;
pub(crate) type ModExpression = Node<ast::ModExpression>;
pub(crate) type ModModule = Node<ast::ModModule>;
pub(crate) type Parameter = Node<ast::Parameter>;
pub(crate) type ParameterWithDefault = Node<ast::ParameterWithDefault>;
pub(crate) type Parameters = Node<ast::Parameters>;
pub(crate) type Pattern = Node<ast::Pattern>;
pub(crate) type PatternMatchClass = Node<ast::PatternMatchClass>;
pub(crate) type PatternMatchMapping = Node<ast::PatternMatchMapping>;
pub(crate) type PatternMatchSequence = Node<ast::PatternMatchSequence>;
pub(crate) type PatternMatchStar = Node<ast::PatternMatchStar>;
pub(crate) type PatternMatchValue = Node<ast::PatternMatchValue>;
pub(crate) type Stmt = Node<ast::Stmt>;
pub(crate) type StmtAnnAssign = Node<ast::StmtAnnAssign>;
pub(crate) type StmtAssert = Node<ast::StmtAssert>;
pub(crate) type StmtAssign = Node<ast::StmtAssign>;
pub(crate) type StmtAugAssign = Node<ast::StmtAugAssign>;
pub(crate) type StmtBreak = Node<ast::StmtBreak>;
pub(crate) type StmtClassDef = Node<ast::StmtClassDef>;
pub(crate) type StmtContinue = Node<ast::StmtContinue>;
pub(crate) type StmtDelete = Node<ast::StmtDelete>;
pub(crate) type StmtFor = Node<ast::StmtFor>;
pub(crate) type StmtFunctionDef = Node<ast::StmtFunctionDef>;
pub(crate) type StmtGlobal = Node<ast::StmtGlobal>;
pub(crate) type StmtIf = Node<ast::StmtIf>;
pub(crate) type StmtImport = Node<ast::StmtImport>;
pub(crate) type StmtImportFrom = Node<ast::StmtImportFrom>;
pub(crate) type StmtIpyEscapeCommand = Node<ast::StmtIpyEscapeCommand>;
pub(crate) type StmtMatch = Node<ast::StmtMatch>;
pub(crate) type StmtNonlocal = Node<ast::StmtNonlocal>;
pub(crate) type StmtPass = Node<ast::StmtPass>;
pub(crate) type StmtRaise = Node<ast::StmtRaise>;
pub(crate) type StmtReturn = Node<ast::StmtReturn>;
pub(crate) type StmtTry = Node<ast::StmtTry>;
pub(crate) type StmtTypeAlias = Node<ast::StmtTypeAlias>;
pub(crate) type StmtWhile = Node<ast::StmtWhile>;
pub(crate) type StmtWith = Node<ast::StmtWith>;
pub(crate) type StringLiteral = Node<ast::StringLiteral>;
pub(crate) type TString = Node<ast::TString>;
pub(crate) type TypeParam = Node<ast::TypeParam>;
pub(crate) type TypeParams = Node<ast::TypeParams>;
pub(crate) type WithItem = Node<ast::WithItem>;

impl Expr {
    pub(super) const fn is_starred_expr(self) -> bool {
        matches!(self.kind, NodeKind::ExprStarred)
    }
    pub(super) const fn is_name_expr(self) -> bool {
        matches!(self.kind, NodeKind::ExprName)
    }
    pub(super) const fn is_named_expr(self) -> bool {
        matches!(self.kind, NodeKind::ExprNamed)
    }
    pub(super) const fn is_lambda_expr(self) -> bool {
        matches!(self.kind, NodeKind::ExprLambda)
    }
}
pub(super) struct DictItem {
    pub(super) key: Option<Expr>,
    pub(super) value: Expr,
}
impl ListItem for DictItem {
    type Element = ast::DictItem;
    fn word(self) -> u64 {
        (self.key.map(|node| node.id), self.value.id).word()
    }
}
