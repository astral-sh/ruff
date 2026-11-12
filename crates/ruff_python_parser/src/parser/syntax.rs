use ruff_python_ast::{self as ast, AstNode, BuildId, Builder, Emit, NodeKind};
use ruff_text_size::{Ranged, TextRange};

/// The Pratt parser uses the range and kind of its current result for lookahead and errors.
/// Only its four-byte slot is written into its parent.
#[derive(Debug)]
pub(crate) struct Node<T> {
    pub(crate) id: BuildId<T>,
    pub(crate) range: TextRange,
    pub(crate) kind: NodeKind,
}
impl<T> From<ast::EmittedNode<T>> for Node<T> {
    fn from((id, range, kind): ast::EmittedNode<T>) -> Self {
        Self { id, range, kind }
    }
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
impl<T: ast::NodeType> Node<T> {
    pub(crate) fn view(self, builder: &Builder) -> T::Borrowed<'_> {
        builder.get(self.id)
    }
    pub(crate) fn from_view<'a>(view: impl AstNode<'a, Kind = T>) -> Self {
        Self {
            id: view.build_id(),
            range: view.range(),
            kind: view.node_kind(),
        }
    }
}

pub(crate) trait Construct: Emit + Sized {
    fn parsed(self, builder: &mut Builder) -> Node<Self::Output> {
        self.emit(builder).into()
    }
}
impl<T: Emit> Construct for T {}

/// Collects child IDs until the parser can put them in a contiguous arena list.
/// Keeping the first ID inline avoids a temporary allocation for single-element lists.
pub(super) struct NodeListBuilder<T> {
    first: Option<T>,
    additional: Vec<T>,
}

impl<T> NodeListBuilder<T> {
    pub(super) fn new() -> Self {
        Self {
            first: None,
            additional: Vec::new(),
        }
    }

    pub(super) fn push(&mut self, id: T) {
        if self.first.is_none() {
            self.first = Some(id);
        } else {
            self.additional.push(id);
        }
    }

    pub(super) fn into_items(self) -> impl Iterator<Item = T> {
        self.first.into_iter().chain(self.additional)
    }

    pub(super) fn finish(self, builder: &mut Builder) -> ast::ListId<T::Element>
    where
        T: ast::ListItem,
    {
        builder.list(self.into_items())
    }
}

macro_rules! upcast {
    ($group:ident, $($ty:ident),+) => {$(
        impl From<Node<ast::kind::$ty>> for Node<ast::kind::$group> {
            fn from(value: Node<ast::kind::$ty>) -> Self {
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
pub(crate) type Alias = Node<ast::kind::Alias>;
pub(crate) type Arguments = Node<ast::kind::Arguments>;
pub(crate) type BytesLiteral = Node<ast::kind::BytesLiteral>;
pub(crate) type Comprehension = Node<ast::kind::Comprehension>;
pub(crate) type ElifElseClause = Node<ast::kind::ElifElseClause>;
pub(crate) type ExceptHandler = Node<ast::kind::ExceptHandler>;
pub(crate) type Expr = Node<ast::kind::Expr>;
pub(crate) type ExprAttribute = Node<ast::kind::ExprAttribute>;
pub(crate) type ExprAwait = Node<ast::kind::ExprAwait>;
pub(crate) type ExprBoolOp = Node<ast::kind::ExprBoolOp>;
pub(crate) type ExprCall = Node<ast::kind::ExprCall>;
pub(crate) type ExprCompare = Node<ast::kind::ExprCompare>;
pub(crate) type ExprDict = Node<ast::kind::ExprDict>;
pub(crate) type ExprDictComp = Node<ast::kind::ExprDictComp>;
pub(crate) type ExprGenerator = Node<ast::kind::ExprGenerator>;
pub(crate) type ExprIf = Node<ast::kind::ExprIf>;
pub(crate) type ExprIpyEscapeCommand = Node<ast::kind::ExprIpyEscapeCommand>;
pub(crate) type ExprLambda = Node<ast::kind::ExprLambda>;
pub(crate) type ExprList = Node<ast::kind::ExprList>;
pub(crate) type ExprListComp = Node<ast::kind::ExprListComp>;
pub(crate) type ExprName = Node<ast::kind::ExprName>;
pub(crate) type ExprNamed = Node<ast::kind::ExprNamed>;
pub(crate) type ExprSet = Node<ast::kind::ExprSet>;
pub(crate) type ExprSetComp = Node<ast::kind::ExprSetComp>;
pub(crate) type ExprStarred = Node<ast::kind::ExprStarred>;
pub(crate) type ExprSubscript = Node<ast::kind::ExprSubscript>;
pub(crate) type ExprTuple = Node<ast::kind::ExprTuple>;
pub(crate) type ExprUnaryOp = Node<ast::kind::ExprUnaryOp>;
pub(crate) type FString = Node<ast::kind::FString>;
pub(crate) type Identifier = Node<ast::kind::Identifier>;
pub(crate) type InterpolatedElement = Node<ast::kind::InterpolatedElement>;
pub(crate) type InterpolatedStringElement = Node<ast::kind::InterpolatedStringElement>;
pub(crate) type InterpolatedStringLiteralElement =
    Node<ast::kind::InterpolatedStringLiteralElement>;
pub(crate) type MatchCase = Node<ast::kind::MatchCase>;
pub(crate) type Mod = Node<ast::kind::Mod>;
pub(crate) type ModExpression = Node<ast::kind::ModExpression>;
pub(crate) type ModModule = Node<ast::kind::ModModule>;
pub(crate) type Parameter = Node<ast::kind::Parameter>;
pub(crate) type ParameterWithDefault = Node<ast::kind::ParameterWithDefault>;
pub(crate) type Parameters = Node<ast::kind::Parameters>;
pub(crate) type Pattern = Node<ast::kind::Pattern>;
pub(crate) type PatternMatchClass = Node<ast::kind::PatternMatchClass>;
pub(crate) type PatternMatchMapping = Node<ast::kind::PatternMatchMapping>;
pub(crate) type PatternMatchSequence = Node<ast::kind::PatternMatchSequence>;
pub(crate) type PatternMatchStar = Node<ast::kind::PatternMatchStar>;
pub(crate) type PatternMatchValue = Node<ast::kind::PatternMatchValue>;
pub(crate) type Stmt = Node<ast::kind::Stmt>;
pub(crate) type StmtAnnAssign = Node<ast::kind::StmtAnnAssign>;
pub(crate) type StmtAssert = Node<ast::kind::StmtAssert>;
pub(crate) type StmtAssign = Node<ast::kind::StmtAssign>;
pub(crate) type StmtAugAssign = Node<ast::kind::StmtAugAssign>;
pub(crate) type StmtBreak = Node<ast::kind::StmtBreak>;
pub(crate) type StmtClassDef = Node<ast::kind::StmtClassDef>;
pub(crate) type StmtContinue = Node<ast::kind::StmtContinue>;
pub(crate) type StmtDelete = Node<ast::kind::StmtDelete>;
pub(crate) type StmtFor = Node<ast::kind::StmtFor>;
pub(crate) type StmtFunctionDef = Node<ast::kind::StmtFunctionDef>;
pub(crate) type StmtGlobal = Node<ast::kind::StmtGlobal>;
pub(crate) type StmtIf = Node<ast::kind::StmtIf>;
pub(crate) type StmtImport = Node<ast::kind::StmtImport>;
pub(crate) type StmtImportFrom = Node<ast::kind::StmtImportFrom>;
pub(crate) type StmtIpyEscapeCommand = Node<ast::kind::StmtIpyEscapeCommand>;
pub(crate) type StmtMatch = Node<ast::kind::StmtMatch>;
pub(crate) type StmtNonlocal = Node<ast::kind::StmtNonlocal>;
pub(crate) type StmtPass = Node<ast::kind::StmtPass>;
pub(crate) type StmtRaise = Node<ast::kind::StmtRaise>;
pub(crate) type StmtReturn = Node<ast::kind::StmtReturn>;
pub(crate) type StmtTry = Node<ast::kind::StmtTry>;
pub(crate) type StmtTypeAlias = Node<ast::kind::StmtTypeAlias>;
pub(crate) type StmtWhile = Node<ast::kind::StmtWhile>;
pub(crate) type StmtWith = Node<ast::kind::StmtWith>;
pub(crate) type StringLiteral = Node<ast::kind::StringLiteral>;
pub(crate) type TString = Node<ast::kind::TString>;
pub(crate) type TypeParam = Node<ast::kind::TypeParam>;
pub(crate) type TypeParams = Node<ast::kind::TypeParams>;

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
