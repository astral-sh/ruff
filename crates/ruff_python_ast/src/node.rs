use crate::prelude::*;
use ruff_text_size::TextRange;
use std::ptr::NonNull;

pub trait AstNode: Ranged {
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized;
    fn cast_ref(kind: AnyNodeRef) -> Option<&Self>;

    /// Returns the [`AnyNodeRef`] referencing this node.
    fn as_any_node_ref(&self) -> AnyNodeRef;

    /// Consumes `self` and returns its [`AnyNode`] representation.
    fn into_any_node(self) -> AnyNode;
}

#[derive(Clone, Debug, is_macro::Is, PartialEq)]
pub enum AnyNode {
    ModModule(ModModule<TextRange>),
    ModInteractive(ModInteractive<TextRange>),
    ModExpression(ModExpression<TextRange>),
    ModFunctionType(ModFunctionType<TextRange>),
    StmtFunctionDef(StmtFunctionDef<TextRange>),
    StmtAsyncFunctionDef(StmtAsyncFunctionDef<TextRange>),
    StmtClassDef(StmtClassDef<TextRange>),
    StmtReturn(StmtReturn<TextRange>),
    StmtDelete(StmtDelete<TextRange>),
    StmtAssign(StmtAssign<TextRange>),
    StmtAugAssign(StmtAugAssign<TextRange>),
    StmtAnnAssign(StmtAnnAssign<TextRange>),
    StmtFor(StmtFor<TextRange>),
    StmtAsyncFor(StmtAsyncFor<TextRange>),
    StmtWhile(StmtWhile<TextRange>),
    StmtIf(StmtIf<TextRange>),
    StmtWith(StmtWith<TextRange>),
    StmtAsyncWith(StmtAsyncWith<TextRange>),
    StmtMatch(StmtMatch<TextRange>),
    StmtRaise(StmtRaise<TextRange>),
    StmtTry(StmtTry<TextRange>),
    StmtTryStar(StmtTryStar<TextRange>),
    StmtAssert(StmtAssert<TextRange>),
    StmtImport(StmtImport<TextRange>),
    StmtImportFrom(StmtImportFrom<TextRange>),
    StmtGlobal(StmtGlobal<TextRange>),
    StmtNonlocal(StmtNonlocal<TextRange>),
    StmtExpr(StmtExpr<TextRange>),
    StmtPass(StmtPass<TextRange>),
    StmtBreak(StmtBreak<TextRange>),
    StmtContinue(StmtContinue<TextRange>),
    ExprBoolOp(ExprBoolOp<TextRange>),
    ExprNamedExpr(ExprNamedExpr<TextRange>),
    ExprBinOp(ExprBinOp<TextRange>),
    ExprUnaryOp(ExprUnaryOp<TextRange>),
    ExprLambda(ExprLambda<TextRange>),
    ExprIfExp(ExprIfExp<TextRange>),
    ExprDict(ExprDict<TextRange>),
    ExprSet(ExprSet<TextRange>),
    ExprListComp(ExprListComp<TextRange>),
    ExprSetComp(ExprSetComp<TextRange>),
    ExprDictComp(ExprDictComp<TextRange>),
    ExprGeneratorExp(ExprGeneratorExp<TextRange>),
    ExprAwait(ExprAwait<TextRange>),
    ExprYield(ExprYield<TextRange>),
    ExprYieldFrom(ExprYieldFrom<TextRange>),
    ExprCompare(ExprCompare<TextRange>),
    ExprCall(ExprCall<TextRange>),
    ExprFormattedValue(ExprFormattedValue<TextRange>),
    ExprJoinedStr(ExprJoinedStr<TextRange>),
    ExprConstant(ExprConstant<TextRange>),
    ExprAttribute(ExprAttribute<TextRange>),
    ExprSubscript(ExprSubscript<TextRange>),
    ExprStarred(ExprStarred<TextRange>),
    ExprName(ExprName<TextRange>),
    ExprList(ExprList<TextRange>),
    ExprTuple(ExprTuple<TextRange>),
    ExprSlice(ExprSlice<TextRange>),
    ExceptHandlerExceptHandler(ExceptHandlerExceptHandler<TextRange>),
    PatternMatchValue(PatternMatchValue<TextRange>),
    PatternMatchSingleton(PatternMatchSingleton<TextRange>),
    PatternMatchSequence(PatternMatchSequence<TextRange>),
    PatternMatchMapping(PatternMatchMapping<TextRange>),
    PatternMatchClass(PatternMatchClass<TextRange>),
    PatternMatchStar(PatternMatchStar<TextRange>),
    PatternMatchAs(PatternMatchAs<TextRange>),
    PatternMatchOr(PatternMatchOr<TextRange>),
    TypeIgnoreTypeIgnore(TypeIgnoreTypeIgnore<TextRange>),
    Comprehension(Comprehension<TextRange>),
    Arguments(Arguments<TextRange>),
    Arg(Arg<TextRange>),
    ArgWithDefault(ArgWithDefault<TextRange>),
    Keyword(Keyword<TextRange>),
    Alias(Alias<TextRange>),
    WithItem(WithItem<TextRange>),
    MatchCase(MatchCase<TextRange>),
    Decorator(Decorator<TextRange>),
}

impl AnyNode {
    pub fn statement(self) -> Option<Stmt> {
        match self {
            AnyNode::StmtFunctionDef(node) => Some(Stmt::FunctionDef(node)),
            AnyNode::StmtAsyncFunctionDef(node) => Some(Stmt::AsyncFunctionDef(node)),
            AnyNode::StmtClassDef(node) => Some(Stmt::ClassDef(node)),
            AnyNode::StmtReturn(node) => Some(Stmt::Return(node)),
            AnyNode::StmtDelete(node) => Some(Stmt::Delete(node)),
            AnyNode::StmtAssign(node) => Some(Stmt::Assign(node)),
            AnyNode::StmtAugAssign(node) => Some(Stmt::AugAssign(node)),
            AnyNode::StmtAnnAssign(node) => Some(Stmt::AnnAssign(node)),
            AnyNode::StmtFor(node) => Some(Stmt::For(node)),
            AnyNode::StmtAsyncFor(node) => Some(Stmt::AsyncFor(node)),
            AnyNode::StmtWhile(node) => Some(Stmt::While(node)),
            AnyNode::StmtIf(node) => Some(Stmt::If(node)),
            AnyNode::StmtWith(node) => Some(Stmt::With(node)),
            AnyNode::StmtAsyncWith(node) => Some(Stmt::AsyncWith(node)),
            AnyNode::StmtMatch(node) => Some(Stmt::Match(node)),
            AnyNode::StmtRaise(node) => Some(Stmt::Raise(node)),
            AnyNode::StmtTry(node) => Some(Stmt::Try(node)),
            AnyNode::StmtTryStar(node) => Some(Stmt::TryStar(node)),
            AnyNode::StmtAssert(node) => Some(Stmt::Assert(node)),
            AnyNode::StmtImport(node) => Some(Stmt::Import(node)),
            AnyNode::StmtImportFrom(node) => Some(Stmt::ImportFrom(node)),
            AnyNode::StmtGlobal(node) => Some(Stmt::Global(node)),
            AnyNode::StmtNonlocal(node) => Some(Stmt::Nonlocal(node)),
            AnyNode::StmtExpr(node) => Some(Stmt::Expr(node)),
            AnyNode::StmtPass(node) => Some(Stmt::Pass(node)),
            AnyNode::StmtBreak(node) => Some(Stmt::Break(node)),
            AnyNode::StmtContinue(node) => Some(Stmt::Continue(node)),

            AnyNode::ModModule(_)
            | AnyNode::ModInteractive(_)
            | AnyNode::ModExpression(_)
            | AnyNode::ModFunctionType(_)
            | AnyNode::ExprBoolOp(_)
            | AnyNode::ExprNamedExpr(_)
            | AnyNode::ExprBinOp(_)
            | AnyNode::ExprUnaryOp(_)
            | AnyNode::ExprLambda(_)
            | AnyNode::ExprIfExp(_)
            | AnyNode::ExprDict(_)
            | AnyNode::ExprSet(_)
            | AnyNode::ExprListComp(_)
            | AnyNode::ExprSetComp(_)
            | AnyNode::ExprDictComp(_)
            | AnyNode::ExprGeneratorExp(_)
            | AnyNode::ExprAwait(_)
            | AnyNode::ExprYield(_)
            | AnyNode::ExprYieldFrom(_)
            | AnyNode::ExprCompare(_)
            | AnyNode::ExprCall(_)
            | AnyNode::ExprFormattedValue(_)
            | AnyNode::ExprJoinedStr(_)
            | AnyNode::ExprConstant(_)
            | AnyNode::ExprAttribute(_)
            | AnyNode::ExprSubscript(_)
            | AnyNode::ExprStarred(_)
            | AnyNode::ExprName(_)
            | AnyNode::ExprList(_)
            | AnyNode::ExprTuple(_)
            | AnyNode::ExprSlice(_)
            | AnyNode::ExceptHandlerExceptHandler(_)
            | AnyNode::PatternMatchValue(_)
            | AnyNode::PatternMatchSingleton(_)
            | AnyNode::PatternMatchSequence(_)
            | AnyNode::PatternMatchMapping(_)
            | AnyNode::PatternMatchClass(_)
            | AnyNode::PatternMatchStar(_)
            | AnyNode::PatternMatchAs(_)
            | AnyNode::PatternMatchOr(_)
            | AnyNode::TypeIgnoreTypeIgnore(_)
            | AnyNode::Comprehension(_)
            | AnyNode::Arguments(_)
            | AnyNode::Arg(_)
            | AnyNode::ArgWithDefault(_)
            | AnyNode::Keyword(_)
            | AnyNode::Alias(_)
            | AnyNode::WithItem(_)
            | AnyNode::MatchCase(_)
            | AnyNode::Decorator(_) => None,
        }
    }

    pub fn expression(self) -> Option<Expr> {
        match self {
            AnyNode::ExprBoolOp(node) => Some(Expr::BoolOp(node)),
            AnyNode::ExprNamedExpr(node) => Some(Expr::NamedExpr(node)),
            AnyNode::ExprBinOp(node) => Some(Expr::BinOp(node)),
            AnyNode::ExprUnaryOp(node) => Some(Expr::UnaryOp(node)),
            AnyNode::ExprLambda(node) => Some(Expr::Lambda(node)),
            AnyNode::ExprIfExp(node) => Some(Expr::IfExp(node)),
            AnyNode::ExprDict(node) => Some(Expr::Dict(node)),
            AnyNode::ExprSet(node) => Some(Expr::Set(node)),
            AnyNode::ExprListComp(node) => Some(Expr::ListComp(node)),
            AnyNode::ExprSetComp(node) => Some(Expr::SetComp(node)),
            AnyNode::ExprDictComp(node) => Some(Expr::DictComp(node)),
            AnyNode::ExprGeneratorExp(node) => Some(Expr::GeneratorExp(node)),
            AnyNode::ExprAwait(node) => Some(Expr::Await(node)),
            AnyNode::ExprYield(node) => Some(Expr::Yield(node)),
            AnyNode::ExprYieldFrom(node) => Some(Expr::YieldFrom(node)),
            AnyNode::ExprCompare(node) => Some(Expr::Compare(node)),
            AnyNode::ExprCall(node) => Some(Expr::Call(node)),
            AnyNode::ExprFormattedValue(node) => Some(Expr::FormattedValue(node)),
            AnyNode::ExprJoinedStr(node) => Some(Expr::JoinedStr(node)),
            AnyNode::ExprConstant(node) => Some(Expr::Constant(node)),
            AnyNode::ExprAttribute(node) => Some(Expr::Attribute(node)),
            AnyNode::ExprSubscript(node) => Some(Expr::Subscript(node)),
            AnyNode::ExprStarred(node) => Some(Expr::Starred(node)),
            AnyNode::ExprName(node) => Some(Expr::Name(node)),
            AnyNode::ExprList(node) => Some(Expr::List(node)),
            AnyNode::ExprTuple(node) => Some(Expr::Tuple(node)),
            AnyNode::ExprSlice(node) => Some(Expr::Slice(node)),

            AnyNode::ModModule(_)
            | AnyNode::ModInteractive(_)
            | AnyNode::ModExpression(_)
            | AnyNode::ModFunctionType(_)
            | AnyNode::StmtFunctionDef(_)
            | AnyNode::StmtAsyncFunctionDef(_)
            | AnyNode::StmtClassDef(_)
            | AnyNode::StmtReturn(_)
            | AnyNode::StmtDelete(_)
            | AnyNode::StmtAssign(_)
            | AnyNode::StmtAugAssign(_)
            | AnyNode::StmtAnnAssign(_)
            | AnyNode::StmtFor(_)
            | AnyNode::StmtAsyncFor(_)
            | AnyNode::StmtWhile(_)
            | AnyNode::StmtIf(_)
            | AnyNode::StmtWith(_)
            | AnyNode::StmtAsyncWith(_)
            | AnyNode::StmtMatch(_)
            | AnyNode::StmtRaise(_)
            | AnyNode::StmtTry(_)
            | AnyNode::StmtTryStar(_)
            | AnyNode::StmtAssert(_)
            | AnyNode::StmtImport(_)
            | AnyNode::StmtImportFrom(_)
            | AnyNode::StmtGlobal(_)
            | AnyNode::StmtNonlocal(_)
            | AnyNode::StmtExpr(_)
            | AnyNode::StmtPass(_)
            | AnyNode::StmtBreak(_)
            | AnyNode::StmtContinue(_)
            | AnyNode::ExceptHandlerExceptHandler(_)
            | AnyNode::PatternMatchValue(_)
            | AnyNode::PatternMatchSingleton(_)
            | AnyNode::PatternMatchSequence(_)
            | AnyNode::PatternMatchMapping(_)
            | AnyNode::PatternMatchClass(_)
            | AnyNode::PatternMatchStar(_)
            | AnyNode::PatternMatchAs(_)
            | AnyNode::PatternMatchOr(_)
            | AnyNode::TypeIgnoreTypeIgnore(_)
            | AnyNode::Comprehension(_)
            | AnyNode::Arguments(_)
            | AnyNode::Arg(_)
            | AnyNode::ArgWithDefault(_)
            | AnyNode::Keyword(_)
            | AnyNode::Alias(_)
            | AnyNode::WithItem(_)
            | AnyNode::MatchCase(_)
            | AnyNode::Decorator(_) => None,
        }
    }

    pub fn module(self) -> Option<Mod> {
        match self {
            AnyNode::ModModule(node) => Some(Mod::Module(node)),
            AnyNode::ModInteractive(node) => Some(Mod::Interactive(node)),
            AnyNode::ModExpression(node) => Some(Mod::Expression(node)),
            AnyNode::ModFunctionType(node) => Some(Mod::FunctionType(node)),

            AnyNode::StmtFunctionDef(_)
            | AnyNode::StmtAsyncFunctionDef(_)
            | AnyNode::StmtClassDef(_)
            | AnyNode::StmtReturn(_)
            | AnyNode::StmtDelete(_)
            | AnyNode::StmtAssign(_)
            | AnyNode::StmtAugAssign(_)
            | AnyNode::StmtAnnAssign(_)
            | AnyNode::StmtFor(_)
            | AnyNode::StmtAsyncFor(_)
            | AnyNode::StmtWhile(_)
            | AnyNode::StmtIf(_)
            | AnyNode::StmtWith(_)
            | AnyNode::StmtAsyncWith(_)
            | AnyNode::StmtMatch(_)
            | AnyNode::StmtRaise(_)
            | AnyNode::StmtTry(_)
            | AnyNode::StmtTryStar(_)
            | AnyNode::StmtAssert(_)
            | AnyNode::StmtImport(_)
            | AnyNode::StmtImportFrom(_)
            | AnyNode::StmtGlobal(_)
            | AnyNode::StmtNonlocal(_)
            | AnyNode::StmtExpr(_)
            | AnyNode::StmtPass(_)
            | AnyNode::StmtBreak(_)
            | AnyNode::StmtContinue(_)
            | AnyNode::ExprBoolOp(_)
            | AnyNode::ExprNamedExpr(_)
            | AnyNode::ExprBinOp(_)
            | AnyNode::ExprUnaryOp(_)
            | AnyNode::ExprLambda(_)
            | AnyNode::ExprIfExp(_)
            | AnyNode::ExprDict(_)
            | AnyNode::ExprSet(_)
            | AnyNode::ExprListComp(_)
            | AnyNode::ExprSetComp(_)
            | AnyNode::ExprDictComp(_)
            | AnyNode::ExprGeneratorExp(_)
            | AnyNode::ExprAwait(_)
            | AnyNode::ExprYield(_)
            | AnyNode::ExprYieldFrom(_)
            | AnyNode::ExprCompare(_)
            | AnyNode::ExprCall(_)
            | AnyNode::ExprFormattedValue(_)
            | AnyNode::ExprJoinedStr(_)
            | AnyNode::ExprConstant(_)
            | AnyNode::ExprAttribute(_)
            | AnyNode::ExprSubscript(_)
            | AnyNode::ExprStarred(_)
            | AnyNode::ExprName(_)
            | AnyNode::ExprList(_)
            | AnyNode::ExprTuple(_)
            | AnyNode::ExprSlice(_)
            | AnyNode::ExceptHandlerExceptHandler(_)
            | AnyNode::PatternMatchValue(_)
            | AnyNode::PatternMatchSingleton(_)
            | AnyNode::PatternMatchSequence(_)
            | AnyNode::PatternMatchMapping(_)
            | AnyNode::PatternMatchClass(_)
            | AnyNode::PatternMatchStar(_)
            | AnyNode::PatternMatchAs(_)
            | AnyNode::PatternMatchOr(_)
            | AnyNode::TypeIgnoreTypeIgnore(_)
            | AnyNode::Comprehension(_)
            | AnyNode::Arguments(_)
            | AnyNode::Arg(_)
            | AnyNode::ArgWithDefault(_)
            | AnyNode::Keyword(_)
            | AnyNode::Alias(_)
            | AnyNode::WithItem(_)
            | AnyNode::MatchCase(_)
            | AnyNode::Decorator(_) => None,
        }
    }

    pub fn pattern(self) -> Option<Pattern> {
        match self {
            AnyNode::PatternMatchValue(node) => Some(Pattern::MatchValue(node)),
            AnyNode::PatternMatchSingleton(node) => Some(Pattern::MatchSingleton(node)),
            AnyNode::PatternMatchSequence(node) => Some(Pattern::MatchSequence(node)),
            AnyNode::PatternMatchMapping(node) => Some(Pattern::MatchMapping(node)),
            AnyNode::PatternMatchClass(node) => Some(Pattern::MatchClass(node)),
            AnyNode::PatternMatchStar(node) => Some(Pattern::MatchStar(node)),
            AnyNode::PatternMatchAs(node) => Some(Pattern::MatchAs(node)),
            AnyNode::PatternMatchOr(node) => Some(Pattern::MatchOr(node)),

            AnyNode::ModModule(_)
            | AnyNode::ModInteractive(_)
            | AnyNode::ModExpression(_)
            | AnyNode::ModFunctionType(_)
            | AnyNode::StmtFunctionDef(_)
            | AnyNode::StmtAsyncFunctionDef(_)
            | AnyNode::StmtClassDef(_)
            | AnyNode::StmtReturn(_)
            | AnyNode::StmtDelete(_)
            | AnyNode::StmtAssign(_)
            | AnyNode::StmtAugAssign(_)
            | AnyNode::StmtAnnAssign(_)
            | AnyNode::StmtFor(_)
            | AnyNode::StmtAsyncFor(_)
            | AnyNode::StmtWhile(_)
            | AnyNode::StmtIf(_)
            | AnyNode::StmtWith(_)
            | AnyNode::StmtAsyncWith(_)
            | AnyNode::StmtMatch(_)
            | AnyNode::StmtRaise(_)
            | AnyNode::StmtTry(_)
            | AnyNode::StmtTryStar(_)
            | AnyNode::StmtAssert(_)
            | AnyNode::StmtImport(_)
            | AnyNode::StmtImportFrom(_)
            | AnyNode::StmtGlobal(_)
            | AnyNode::StmtNonlocal(_)
            | AnyNode::StmtExpr(_)
            | AnyNode::StmtPass(_)
            | AnyNode::StmtBreak(_)
            | AnyNode::StmtContinue(_)
            | AnyNode::ExprBoolOp(_)
            | AnyNode::ExprNamedExpr(_)
            | AnyNode::ExprBinOp(_)
            | AnyNode::ExprUnaryOp(_)
            | AnyNode::ExprLambda(_)
            | AnyNode::ExprIfExp(_)
            | AnyNode::ExprDict(_)
            | AnyNode::ExprSet(_)
            | AnyNode::ExprListComp(_)
            | AnyNode::ExprSetComp(_)
            | AnyNode::ExprDictComp(_)
            | AnyNode::ExprGeneratorExp(_)
            | AnyNode::ExprAwait(_)
            | AnyNode::ExprYield(_)
            | AnyNode::ExprYieldFrom(_)
            | AnyNode::ExprCompare(_)
            | AnyNode::ExprCall(_)
            | AnyNode::ExprFormattedValue(_)
            | AnyNode::ExprJoinedStr(_)
            | AnyNode::ExprConstant(_)
            | AnyNode::ExprAttribute(_)
            | AnyNode::ExprSubscript(_)
            | AnyNode::ExprStarred(_)
            | AnyNode::ExprName(_)
            | AnyNode::ExprList(_)
            | AnyNode::ExprTuple(_)
            | AnyNode::ExprSlice(_)
            | AnyNode::ExceptHandlerExceptHandler(_)
            | AnyNode::TypeIgnoreTypeIgnore(_)
            | AnyNode::Comprehension(_)
            | AnyNode::Arguments(_)
            | AnyNode::Arg(_)
            | AnyNode::ArgWithDefault(_)
            | AnyNode::Keyword(_)
            | AnyNode::Alias(_)
            | AnyNode::WithItem(_)
            | AnyNode::MatchCase(_)
            | AnyNode::Decorator(_) => None,
        }
    }

    pub fn except_handler(self) -> Option<ExceptHandler> {
        match self {
            AnyNode::ExceptHandlerExceptHandler(node) => Some(ExceptHandler::ExceptHandler(node)),

            AnyNode::ModModule(_)
            | AnyNode::ModInteractive(_)
            | AnyNode::ModExpression(_)
            | AnyNode::ModFunctionType(_)
            | AnyNode::StmtFunctionDef(_)
            | AnyNode::StmtAsyncFunctionDef(_)
            | AnyNode::StmtClassDef(_)
            | AnyNode::StmtReturn(_)
            | AnyNode::StmtDelete(_)
            | AnyNode::StmtAssign(_)
            | AnyNode::StmtAugAssign(_)
            | AnyNode::StmtAnnAssign(_)
            | AnyNode::StmtFor(_)
            | AnyNode::StmtAsyncFor(_)
            | AnyNode::StmtWhile(_)
            | AnyNode::StmtIf(_)
            | AnyNode::StmtWith(_)
            | AnyNode::StmtAsyncWith(_)
            | AnyNode::StmtMatch(_)
            | AnyNode::StmtRaise(_)
            | AnyNode::StmtTry(_)
            | AnyNode::StmtTryStar(_)
            | AnyNode::StmtAssert(_)
            | AnyNode::StmtImport(_)
            | AnyNode::StmtImportFrom(_)
            | AnyNode::StmtGlobal(_)
            | AnyNode::StmtNonlocal(_)
            | AnyNode::StmtExpr(_)
            | AnyNode::StmtPass(_)
            | AnyNode::StmtBreak(_)
            | AnyNode::StmtContinue(_)
            | AnyNode::ExprBoolOp(_)
            | AnyNode::ExprNamedExpr(_)
            | AnyNode::ExprBinOp(_)
            | AnyNode::ExprUnaryOp(_)
            | AnyNode::ExprLambda(_)
            | AnyNode::ExprIfExp(_)
            | AnyNode::ExprDict(_)
            | AnyNode::ExprSet(_)
            | AnyNode::ExprListComp(_)
            | AnyNode::ExprSetComp(_)
            | AnyNode::ExprDictComp(_)
            | AnyNode::ExprGeneratorExp(_)
            | AnyNode::ExprAwait(_)
            | AnyNode::ExprYield(_)
            | AnyNode::ExprYieldFrom(_)
            | AnyNode::ExprCompare(_)
            | AnyNode::ExprCall(_)
            | AnyNode::ExprFormattedValue(_)
            | AnyNode::ExprJoinedStr(_)
            | AnyNode::ExprConstant(_)
            | AnyNode::ExprAttribute(_)
            | AnyNode::ExprSubscript(_)
            | AnyNode::ExprStarred(_)
            | AnyNode::ExprName(_)
            | AnyNode::ExprList(_)
            | AnyNode::ExprTuple(_)
            | AnyNode::ExprSlice(_)
            | AnyNode::PatternMatchValue(_)
            | AnyNode::PatternMatchSingleton(_)
            | AnyNode::PatternMatchSequence(_)
            | AnyNode::PatternMatchMapping(_)
            | AnyNode::PatternMatchClass(_)
            | AnyNode::PatternMatchStar(_)
            | AnyNode::PatternMatchAs(_)
            | AnyNode::PatternMatchOr(_)
            | AnyNode::TypeIgnoreTypeIgnore(_)
            | AnyNode::Comprehension(_)
            | AnyNode::Arguments(_)
            | AnyNode::Arg(_)
            | AnyNode::ArgWithDefault(_)
            | AnyNode::Keyword(_)
            | AnyNode::Alias(_)
            | AnyNode::WithItem(_)
            | AnyNode::MatchCase(_)
            | AnyNode::Decorator(_) => None,
        }
    }

    pub fn type_ignore(self) -> Option<TypeIgnore> {
        match self {
            AnyNode::TypeIgnoreTypeIgnore(node) => Some(TypeIgnore::TypeIgnore(node)),

            AnyNode::ModModule(_)
            | AnyNode::ModInteractive(_)
            | AnyNode::ModExpression(_)
            | AnyNode::ModFunctionType(_)
            | AnyNode::StmtFunctionDef(_)
            | AnyNode::StmtAsyncFunctionDef(_)
            | AnyNode::StmtClassDef(_)
            | AnyNode::StmtReturn(_)
            | AnyNode::StmtDelete(_)
            | AnyNode::StmtAssign(_)
            | AnyNode::StmtAugAssign(_)
            | AnyNode::StmtAnnAssign(_)
            | AnyNode::StmtFor(_)
            | AnyNode::StmtAsyncFor(_)
            | AnyNode::StmtWhile(_)
            | AnyNode::StmtIf(_)
            | AnyNode::StmtWith(_)
            | AnyNode::StmtAsyncWith(_)
            | AnyNode::StmtMatch(_)
            | AnyNode::StmtRaise(_)
            | AnyNode::StmtTry(_)
            | AnyNode::StmtTryStar(_)
            | AnyNode::StmtAssert(_)
            | AnyNode::StmtImport(_)
            | AnyNode::StmtImportFrom(_)
            | AnyNode::StmtGlobal(_)
            | AnyNode::StmtNonlocal(_)
            | AnyNode::StmtExpr(_)
            | AnyNode::StmtPass(_)
            | AnyNode::StmtBreak(_)
            | AnyNode::StmtContinue(_)
            | AnyNode::ExprBoolOp(_)
            | AnyNode::ExprNamedExpr(_)
            | AnyNode::ExprBinOp(_)
            | AnyNode::ExprUnaryOp(_)
            | AnyNode::ExprLambda(_)
            | AnyNode::ExprIfExp(_)
            | AnyNode::ExprDict(_)
            | AnyNode::ExprSet(_)
            | AnyNode::ExprListComp(_)
            | AnyNode::ExprSetComp(_)
            | AnyNode::ExprDictComp(_)
            | AnyNode::ExprGeneratorExp(_)
            | AnyNode::ExprAwait(_)
            | AnyNode::ExprYield(_)
            | AnyNode::ExprYieldFrom(_)
            | AnyNode::ExprCompare(_)
            | AnyNode::ExprCall(_)
            | AnyNode::ExprFormattedValue(_)
            | AnyNode::ExprJoinedStr(_)
            | AnyNode::ExprConstant(_)
            | AnyNode::ExprAttribute(_)
            | AnyNode::ExprSubscript(_)
            | AnyNode::ExprStarred(_)
            | AnyNode::ExprName(_)
            | AnyNode::ExprList(_)
            | AnyNode::ExprTuple(_)
            | AnyNode::ExprSlice(_)
            | AnyNode::PatternMatchValue(_)
            | AnyNode::PatternMatchSingleton(_)
            | AnyNode::PatternMatchSequence(_)
            | AnyNode::PatternMatchMapping(_)
            | AnyNode::PatternMatchClass(_)
            | AnyNode::PatternMatchStar(_)
            | AnyNode::PatternMatchAs(_)
            | AnyNode::PatternMatchOr(_)
            | AnyNode::ExceptHandlerExceptHandler(_)
            | AnyNode::Comprehension(_)
            | AnyNode::Arguments(_)
            | AnyNode::Arg(_)
            | AnyNode::ArgWithDefault(_)
            | AnyNode::Keyword(_)
            | AnyNode::Alias(_)
            | AnyNode::WithItem(_)
            | AnyNode::MatchCase(_)
            | AnyNode::Decorator(_) => None,
        }
    }

    pub const fn is_statement(&self) -> bool {
        self.as_ref().is_statement()
    }

    pub const fn is_expression(&self) -> bool {
        self.as_ref().is_expression()
    }

    pub const fn is_module(&self) -> bool {
        self.as_ref().is_module()
    }

    pub const fn is_pattern(&self) -> bool {
        self.as_ref().is_pattern()
    }

    pub const fn is_except_handler(&self) -> bool {
        self.as_ref().is_except_handler()
    }

    pub const fn is_type_ignore(&self) -> bool {
        self.as_ref().is_type_ignore()
    }

    pub const fn as_ref(&self) -> AnyNodeRef {
        match self {
            Self::ModModule(node) => AnyNodeRef::ModModule(node),
            Self::ModInteractive(node) => AnyNodeRef::ModInteractive(node),
            Self::ModExpression(node) => AnyNodeRef::ModExpression(node),
            Self::ModFunctionType(node) => AnyNodeRef::ModFunctionType(node),
            Self::StmtFunctionDef(node) => AnyNodeRef::StmtFunctionDef(node),
            Self::StmtAsyncFunctionDef(node) => AnyNodeRef::StmtAsyncFunctionDef(node),
            Self::StmtClassDef(node) => AnyNodeRef::StmtClassDef(node),
            Self::StmtReturn(node) => AnyNodeRef::StmtReturn(node),
            Self::StmtDelete(node) => AnyNodeRef::StmtDelete(node),
            Self::StmtAssign(node) => AnyNodeRef::StmtAssign(node),
            Self::StmtAugAssign(node) => AnyNodeRef::StmtAugAssign(node),
            Self::StmtAnnAssign(node) => AnyNodeRef::StmtAnnAssign(node),
            Self::StmtFor(node) => AnyNodeRef::StmtFor(node),
            Self::StmtAsyncFor(node) => AnyNodeRef::StmtAsyncFor(node),
            Self::StmtWhile(node) => AnyNodeRef::StmtWhile(node),
            Self::StmtIf(node) => AnyNodeRef::StmtIf(node),
            Self::StmtWith(node) => AnyNodeRef::StmtWith(node),
            Self::StmtAsyncWith(node) => AnyNodeRef::StmtAsyncWith(node),
            Self::StmtMatch(node) => AnyNodeRef::StmtMatch(node),
            Self::StmtRaise(node) => AnyNodeRef::StmtRaise(node),
            Self::StmtTry(node) => AnyNodeRef::StmtTry(node),
            Self::StmtTryStar(node) => AnyNodeRef::StmtTryStar(node),
            Self::StmtAssert(node) => AnyNodeRef::StmtAssert(node),
            Self::StmtImport(node) => AnyNodeRef::StmtImport(node),
            Self::StmtImportFrom(node) => AnyNodeRef::StmtImportFrom(node),
            Self::StmtGlobal(node) => AnyNodeRef::StmtGlobal(node),
            Self::StmtNonlocal(node) => AnyNodeRef::StmtNonlocal(node),
            Self::StmtExpr(node) => AnyNodeRef::StmtExpr(node),
            Self::StmtPass(node) => AnyNodeRef::StmtPass(node),
            Self::StmtBreak(node) => AnyNodeRef::StmtBreak(node),
            Self::StmtContinue(node) => AnyNodeRef::StmtContinue(node),
            Self::ExprBoolOp(node) => AnyNodeRef::ExprBoolOp(node),
            Self::ExprNamedExpr(node) => AnyNodeRef::ExprNamedExpr(node),
            Self::ExprBinOp(node) => AnyNodeRef::ExprBinOp(node),
            Self::ExprUnaryOp(node) => AnyNodeRef::ExprUnaryOp(node),
            Self::ExprLambda(node) => AnyNodeRef::ExprLambda(node),
            Self::ExprIfExp(node) => AnyNodeRef::ExprIfExp(node),
            Self::ExprDict(node) => AnyNodeRef::ExprDict(node),
            Self::ExprSet(node) => AnyNodeRef::ExprSet(node),
            Self::ExprListComp(node) => AnyNodeRef::ExprListComp(node),
            Self::ExprSetComp(node) => AnyNodeRef::ExprSetComp(node),
            Self::ExprDictComp(node) => AnyNodeRef::ExprDictComp(node),
            Self::ExprGeneratorExp(node) => AnyNodeRef::ExprGeneratorExp(node),
            Self::ExprAwait(node) => AnyNodeRef::ExprAwait(node),
            Self::ExprYield(node) => AnyNodeRef::ExprYield(node),
            Self::ExprYieldFrom(node) => AnyNodeRef::ExprYieldFrom(node),
            Self::ExprCompare(node) => AnyNodeRef::ExprCompare(node),
            Self::ExprCall(node) => AnyNodeRef::ExprCall(node),
            Self::ExprFormattedValue(node) => AnyNodeRef::ExprFormattedValue(node),
            Self::ExprJoinedStr(node) => AnyNodeRef::ExprJoinedStr(node),
            Self::ExprConstant(node) => AnyNodeRef::ExprConstant(node),
            Self::ExprAttribute(node) => AnyNodeRef::ExprAttribute(node),
            Self::ExprSubscript(node) => AnyNodeRef::ExprSubscript(node),
            Self::ExprStarred(node) => AnyNodeRef::ExprStarred(node),
            Self::ExprName(node) => AnyNodeRef::ExprName(node),
            Self::ExprList(node) => AnyNodeRef::ExprList(node),
            Self::ExprTuple(node) => AnyNodeRef::ExprTuple(node),
            Self::ExprSlice(node) => AnyNodeRef::ExprSlice(node),
            Self::ExceptHandlerExceptHandler(node) => AnyNodeRef::ExceptHandlerExceptHandler(node),
            Self::PatternMatchValue(node) => AnyNodeRef::PatternMatchValue(node),
            Self::PatternMatchSingleton(node) => AnyNodeRef::PatternMatchSingleton(node),
            Self::PatternMatchSequence(node) => AnyNodeRef::PatternMatchSequence(node),
            Self::PatternMatchMapping(node) => AnyNodeRef::PatternMatchMapping(node),
            Self::PatternMatchClass(node) => AnyNodeRef::PatternMatchClass(node),
            Self::PatternMatchStar(node) => AnyNodeRef::PatternMatchStar(node),
            Self::PatternMatchAs(node) => AnyNodeRef::PatternMatchAs(node),
            Self::PatternMatchOr(node) => AnyNodeRef::PatternMatchOr(node),
            Self::TypeIgnoreTypeIgnore(node) => AnyNodeRef::TypeIgnoreTypeIgnore(node),
            Self::Comprehension(node) => AnyNodeRef::Comprehension(node),
            Self::Arguments(node) => AnyNodeRef::Arguments(node),
            Self::Arg(node) => AnyNodeRef::Arg(node),
            Self::ArgWithDefault(node) => AnyNodeRef::ArgWithDefault(node),
            Self::Keyword(node) => AnyNodeRef::Keyword(node),
            Self::Alias(node) => AnyNodeRef::Alias(node),
            Self::WithItem(node) => AnyNodeRef::WithItem(node),
            Self::MatchCase(node) => AnyNodeRef::MatchCase(node),
            Self::Decorator(node) => AnyNodeRef::Decorator(node),
        }
    }

    /// Returns the node's [`kind`](NodeKind) that has no data associated and is [`Copy`].
    pub const fn kind(&self) -> NodeKind {
        self.as_ref().kind()
    }
}

impl AstNode for ModModule<TextRange> {
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::ModModule(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::ModModule(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}
impl AstNode for ModInteractive<TextRange> {
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::ModInteractive(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::ModInteractive(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}
impl AstNode for ModExpression<TextRange> {
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::ModExpression(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::ModExpression(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}
impl AstNode for ModFunctionType<TextRange> {
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::ModFunctionType(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::ModFunctionType(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}
impl AstNode for StmtFunctionDef<TextRange> {
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::StmtFunctionDef(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::StmtFunctionDef(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}
impl AstNode for StmtAsyncFunctionDef<TextRange> {
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::StmtAsyncFunctionDef(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::StmtAsyncFunctionDef(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}
impl AstNode for StmtClassDef<TextRange> {
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::StmtClassDef(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::StmtClassDef(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}
impl AstNode for StmtReturn<TextRange> {
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::StmtReturn(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::StmtReturn(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}
impl AstNode for StmtDelete<TextRange> {
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::StmtDelete(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::StmtDelete(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}
impl AstNode for StmtAssign<TextRange> {
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::StmtAssign(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::StmtAssign(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}
impl AstNode for StmtAugAssign<TextRange> {
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::StmtAugAssign(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::StmtAugAssign(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}
impl AstNode for StmtAnnAssign<TextRange> {
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::StmtAnnAssign(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::StmtAnnAssign(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}
impl AstNode for StmtFor<TextRange> {
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::StmtFor(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::StmtFor(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}
impl AstNode for StmtAsyncFor<TextRange> {
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::StmtAsyncFor(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::StmtAsyncFor(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}
impl AstNode for StmtWhile<TextRange> {
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::StmtWhile(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::StmtWhile(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}
impl AstNode for StmtIf<TextRange> {
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::StmtIf(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::StmtIf(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}
impl AstNode for StmtWith<TextRange> {
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::StmtWith(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::StmtWith(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}
impl AstNode for StmtAsyncWith<TextRange> {
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::StmtAsyncWith(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::StmtAsyncWith(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}
impl AstNode for StmtMatch<TextRange> {
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::StmtMatch(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::StmtMatch(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}
impl AstNode for StmtRaise<TextRange> {
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::StmtRaise(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::StmtRaise(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}
impl AstNode for StmtTry<TextRange> {
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::StmtTry(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::StmtTry(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}
impl AstNode for StmtTryStar<TextRange> {
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::StmtTryStar(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::StmtTryStar(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}
impl AstNode for StmtAssert<TextRange> {
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::StmtAssert(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::StmtAssert(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}
impl AstNode for StmtImport<TextRange> {
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::StmtImport(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::StmtImport(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}
impl AstNode for StmtImportFrom<TextRange> {
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::StmtImportFrom(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::StmtImportFrom(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}
impl AstNode for StmtGlobal<TextRange> {
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::StmtGlobal(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::StmtGlobal(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}
impl AstNode for StmtNonlocal<TextRange> {
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::StmtNonlocal(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::StmtNonlocal(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}
impl AstNode for StmtExpr<TextRange> {
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::StmtExpr(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::StmtExpr(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}
impl AstNode for StmtPass<TextRange> {
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::StmtPass(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::StmtPass(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}
impl AstNode for StmtBreak<TextRange> {
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::StmtBreak(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::StmtBreak(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}
impl AstNode for StmtContinue<TextRange> {
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::StmtContinue(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::StmtContinue(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}
impl AstNode for ExprBoolOp<TextRange> {
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::ExprBoolOp(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::ExprBoolOp(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}
impl AstNode for ExprNamedExpr<TextRange> {
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::ExprNamedExpr(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::ExprNamedExpr(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}
impl AstNode for ExprBinOp<TextRange> {
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::ExprBinOp(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::ExprBinOp(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}
impl AstNode for ExprUnaryOp<TextRange> {
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::ExprUnaryOp(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::ExprUnaryOp(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}
impl AstNode for ExprLambda<TextRange> {
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::ExprLambda(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::ExprLambda(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}
impl AstNode for ExprIfExp<TextRange> {
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::ExprIfExp(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::ExprIfExp(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}
impl AstNode for ExprDict<TextRange> {
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::ExprDict(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::ExprDict(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}
impl AstNode for ExprSet<TextRange> {
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::ExprSet(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::ExprSet(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}
impl AstNode for ExprListComp<TextRange> {
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::ExprListComp(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::ExprListComp(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}
impl AstNode for ExprSetComp<TextRange> {
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::ExprSetComp(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::ExprSetComp(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}
impl AstNode for ExprDictComp<TextRange> {
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::ExprDictComp(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::ExprDictComp(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}
impl AstNode for ExprGeneratorExp<TextRange> {
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::ExprGeneratorExp(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::ExprGeneratorExp(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}
impl AstNode for ExprAwait<TextRange> {
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::ExprAwait(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::ExprAwait(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}
impl AstNode for ExprYield<TextRange> {
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::ExprYield(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::ExprYield(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}
impl AstNode for ExprYieldFrom<TextRange> {
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::ExprYieldFrom(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::ExprYieldFrom(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}
impl AstNode for ExprCompare<TextRange> {
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::ExprCompare(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::ExprCompare(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}
impl AstNode for ExprCall<TextRange> {
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::ExprCall(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::ExprCall(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}
impl AstNode for ExprFormattedValue<TextRange> {
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::ExprFormattedValue(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::ExprFormattedValue(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}
impl AstNode for ExprJoinedStr<TextRange> {
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::ExprJoinedStr(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::ExprJoinedStr(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}
impl AstNode for ExprConstant<TextRange> {
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::ExprConstant(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::ExprConstant(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}
impl AstNode for ExprAttribute<TextRange> {
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::ExprAttribute(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::ExprAttribute(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}
impl AstNode for ExprSubscript<TextRange> {
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::ExprSubscript(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::ExprSubscript(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}
impl AstNode for ExprStarred<TextRange> {
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::ExprStarred(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::ExprStarred(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}
impl AstNode for ExprName<TextRange> {
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::ExprName(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::ExprName(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}
impl AstNode for ExprList<TextRange> {
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::ExprList(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::ExprList(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}
impl AstNode for ExprTuple<TextRange> {
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::ExprTuple(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::ExprTuple(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}
impl AstNode for ExprSlice<TextRange> {
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::ExprSlice(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::ExprSlice(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}
impl AstNode for ExceptHandlerExceptHandler<TextRange> {
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::ExceptHandlerExceptHandler(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::ExceptHandlerExceptHandler(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}
impl AstNode for PatternMatchValue<TextRange> {
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::PatternMatchValue(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::PatternMatchValue(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}
impl AstNode for PatternMatchSingleton<TextRange> {
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::PatternMatchSingleton(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::PatternMatchSingleton(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}
impl AstNode for PatternMatchSequence<TextRange> {
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::PatternMatchSequence(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::PatternMatchSequence(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}
impl AstNode for PatternMatchMapping<TextRange> {
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::PatternMatchMapping(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::PatternMatchMapping(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}
impl AstNode for PatternMatchClass<TextRange> {
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::PatternMatchClass(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::PatternMatchClass(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}
impl AstNode for PatternMatchStar<TextRange> {
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::PatternMatchStar(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::PatternMatchStar(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}
impl AstNode for PatternMatchAs<TextRange> {
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::PatternMatchAs(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::PatternMatchAs(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}
impl AstNode for PatternMatchOr<TextRange> {
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::PatternMatchOr(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::PatternMatchOr(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}
impl AstNode for TypeIgnoreTypeIgnore<TextRange> {
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::TypeIgnoreTypeIgnore(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::TypeIgnoreTypeIgnore(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl AstNode for Comprehension<TextRange> {
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::Comprehension(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::Comprehension(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}
impl AstNode for Arguments<TextRange> {
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::Arguments(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::Arguments(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}
impl AstNode for Arg<TextRange> {
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::Arg(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::Arg(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}
impl AstNode for ArgWithDefault<TextRange> {
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::ArgWithDefault(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::ArgWithDefault(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}
impl AstNode for Keyword<TextRange> {
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::Keyword(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::Keyword(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}
impl AstNode for Alias<TextRange> {
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::Alias(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::Alias(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}
impl AstNode for WithItem<TextRange> {
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::WithItem(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::WithItem(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}
impl AstNode for MatchCase<TextRange> {
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::MatchCase(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::MatchCase(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl AstNode for Decorator<TextRange> {
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::Decorator(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::Decorator(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl From<Stmt> for AnyNode {
    fn from(stmt: Stmt) -> Self {
        match stmt {
            Stmt::FunctionDef(node) => AnyNode::StmtFunctionDef(node),
            Stmt::AsyncFunctionDef(node) => AnyNode::StmtAsyncFunctionDef(node),
            Stmt::ClassDef(node) => AnyNode::StmtClassDef(node),
            Stmt::Return(node) => AnyNode::StmtReturn(node),
            Stmt::Delete(node) => AnyNode::StmtDelete(node),
            Stmt::Assign(node) => AnyNode::StmtAssign(node),
            Stmt::AugAssign(node) => AnyNode::StmtAugAssign(node),
            Stmt::AnnAssign(node) => AnyNode::StmtAnnAssign(node),
            Stmt::For(node) => AnyNode::StmtFor(node),
            Stmt::AsyncFor(node) => AnyNode::StmtAsyncFor(node),
            Stmt::While(node) => AnyNode::StmtWhile(node),
            Stmt::If(node) => AnyNode::StmtIf(node),
            Stmt::With(node) => AnyNode::StmtWith(node),
            Stmt::AsyncWith(node) => AnyNode::StmtAsyncWith(node),
            Stmt::Match(node) => AnyNode::StmtMatch(node),
            Stmt::Raise(node) => AnyNode::StmtRaise(node),
            Stmt::Try(node) => AnyNode::StmtTry(node),
            Stmt::TryStar(node) => AnyNode::StmtTryStar(node),
            Stmt::Assert(node) => AnyNode::StmtAssert(node),
            Stmt::Import(node) => AnyNode::StmtImport(node),
            Stmt::ImportFrom(node) => AnyNode::StmtImportFrom(node),
            Stmt::Global(node) => AnyNode::StmtGlobal(node),
            Stmt::Nonlocal(node) => AnyNode::StmtNonlocal(node),
            Stmt::Expr(node) => AnyNode::StmtExpr(node),
            Stmt::Pass(node) => AnyNode::StmtPass(node),
            Stmt::Break(node) => AnyNode::StmtBreak(node),
            Stmt::Continue(node) => AnyNode::StmtContinue(node),
        }
    }
}

impl From<Expr> for AnyNode {
    fn from(expr: Expr) -> Self {
        match expr {
            Expr::BoolOp(node) => AnyNode::ExprBoolOp(node),
            Expr::NamedExpr(node) => AnyNode::ExprNamedExpr(node),
            Expr::BinOp(node) => AnyNode::ExprBinOp(node),
            Expr::UnaryOp(node) => AnyNode::ExprUnaryOp(node),
            Expr::Lambda(node) => AnyNode::ExprLambda(node),
            Expr::IfExp(node) => AnyNode::ExprIfExp(node),
            Expr::Dict(node) => AnyNode::ExprDict(node),
            Expr::Set(node) => AnyNode::ExprSet(node),
            Expr::ListComp(node) => AnyNode::ExprListComp(node),
            Expr::SetComp(node) => AnyNode::ExprSetComp(node),
            Expr::DictComp(node) => AnyNode::ExprDictComp(node),
            Expr::GeneratorExp(node) => AnyNode::ExprGeneratorExp(node),
            Expr::Await(node) => AnyNode::ExprAwait(node),
            Expr::Yield(node) => AnyNode::ExprYield(node),
            Expr::YieldFrom(node) => AnyNode::ExprYieldFrom(node),
            Expr::Compare(node) => AnyNode::ExprCompare(node),
            Expr::Call(node) => AnyNode::ExprCall(node),
            Expr::FormattedValue(node) => AnyNode::ExprFormattedValue(node),
            Expr::JoinedStr(node) => AnyNode::ExprJoinedStr(node),
            Expr::Constant(node) => AnyNode::ExprConstant(node),
            Expr::Attribute(node) => AnyNode::ExprAttribute(node),
            Expr::Subscript(node) => AnyNode::ExprSubscript(node),
            Expr::Starred(node) => AnyNode::ExprStarred(node),
            Expr::Name(node) => AnyNode::ExprName(node),
            Expr::List(node) => AnyNode::ExprList(node),
            Expr::Tuple(node) => AnyNode::ExprTuple(node),
            Expr::Slice(node) => AnyNode::ExprSlice(node),
        }
    }
}

impl From<Mod> for AnyNode {
    fn from(module: Mod) -> Self {
        match module {
            Mod::Module(node) => AnyNode::ModModule(node),
            Mod::Interactive(node) => AnyNode::ModInteractive(node),
            Mod::Expression(node) => AnyNode::ModExpression(node),
            Mod::FunctionType(node) => AnyNode::ModFunctionType(node),
        }
    }
}

impl From<Pattern> for AnyNode {
    fn from(pattern: Pattern) -> Self {
        match pattern {
            Pattern::MatchValue(node) => AnyNode::PatternMatchValue(node),
            Pattern::MatchSingleton(node) => AnyNode::PatternMatchSingleton(node),
            Pattern::MatchSequence(node) => AnyNode::PatternMatchSequence(node),
            Pattern::MatchMapping(node) => AnyNode::PatternMatchMapping(node),
            Pattern::MatchClass(node) => AnyNode::PatternMatchClass(node),
            Pattern::MatchStar(node) => AnyNode::PatternMatchStar(node),
            Pattern::MatchAs(node) => AnyNode::PatternMatchAs(node),
            Pattern::MatchOr(node) => AnyNode::PatternMatchOr(node),
        }
    }
}

impl From<ExceptHandler> for AnyNode {
    fn from(handler: ExceptHandler) -> Self {
        match handler {
            ExceptHandler::ExceptHandler(handler) => AnyNode::ExceptHandlerExceptHandler(handler),
        }
    }
}

impl From<TypeIgnore> for AnyNode {
    fn from(ignore: TypeIgnore) -> Self {
        match ignore {
            TypeIgnore::TypeIgnore(ignore) => AnyNode::TypeIgnoreTypeIgnore(ignore),
        }
    }
}

impl From<ModModule> for AnyNode {
    fn from(node: ModModule) -> Self {
        AnyNode::ModModule(node)
    }
}

impl From<ModInteractive> for AnyNode {
    fn from(node: ModInteractive) -> Self {
        AnyNode::ModInteractive(node)
    }
}

impl From<ModExpression> for AnyNode {
    fn from(node: ModExpression) -> Self {
        AnyNode::ModExpression(node)
    }
}

impl From<ModFunctionType> for AnyNode {
    fn from(node: ModFunctionType) -> Self {
        AnyNode::ModFunctionType(node)
    }
}

impl From<StmtFunctionDef> for AnyNode {
    fn from(node: StmtFunctionDef) -> Self {
        AnyNode::StmtFunctionDef(node)
    }
}

impl From<StmtAsyncFunctionDef> for AnyNode {
    fn from(node: StmtAsyncFunctionDef) -> Self {
        AnyNode::StmtAsyncFunctionDef(node)
    }
}

impl From<StmtClassDef> for AnyNode {
    fn from(node: StmtClassDef) -> Self {
        AnyNode::StmtClassDef(node)
    }
}

impl From<StmtReturn> for AnyNode {
    fn from(node: StmtReturn) -> Self {
        AnyNode::StmtReturn(node)
    }
}

impl From<StmtDelete> for AnyNode {
    fn from(node: StmtDelete) -> Self {
        AnyNode::StmtDelete(node)
    }
}

impl From<StmtAssign> for AnyNode {
    fn from(node: StmtAssign) -> Self {
        AnyNode::StmtAssign(node)
    }
}

impl From<StmtAugAssign> for AnyNode {
    fn from(node: StmtAugAssign) -> Self {
        AnyNode::StmtAugAssign(node)
    }
}

impl From<StmtAnnAssign> for AnyNode {
    fn from(node: StmtAnnAssign) -> Self {
        AnyNode::StmtAnnAssign(node)
    }
}

impl From<StmtFor> for AnyNode {
    fn from(node: StmtFor) -> Self {
        AnyNode::StmtFor(node)
    }
}

impl From<StmtAsyncFor> for AnyNode {
    fn from(node: StmtAsyncFor) -> Self {
        AnyNode::StmtAsyncFor(node)
    }
}

impl From<StmtWhile> for AnyNode {
    fn from(node: StmtWhile) -> Self {
        AnyNode::StmtWhile(node)
    }
}

impl From<StmtIf> for AnyNode {
    fn from(node: StmtIf) -> Self {
        AnyNode::StmtIf(node)
    }
}

impl From<StmtWith> for AnyNode {
    fn from(node: StmtWith) -> Self {
        AnyNode::StmtWith(node)
    }
}

impl From<StmtAsyncWith> for AnyNode {
    fn from(node: StmtAsyncWith) -> Self {
        AnyNode::StmtAsyncWith(node)
    }
}

impl From<StmtMatch> for AnyNode {
    fn from(node: StmtMatch) -> Self {
        AnyNode::StmtMatch(node)
    }
}

impl From<StmtRaise> for AnyNode {
    fn from(node: StmtRaise) -> Self {
        AnyNode::StmtRaise(node)
    }
}

impl From<StmtTry> for AnyNode {
    fn from(node: StmtTry) -> Self {
        AnyNode::StmtTry(node)
    }
}

impl From<StmtTryStar> for AnyNode {
    fn from(node: StmtTryStar) -> Self {
        AnyNode::StmtTryStar(node)
    }
}

impl From<StmtAssert> for AnyNode {
    fn from(node: StmtAssert) -> Self {
        AnyNode::StmtAssert(node)
    }
}

impl From<StmtImport> for AnyNode {
    fn from(node: StmtImport) -> Self {
        AnyNode::StmtImport(node)
    }
}

impl From<StmtImportFrom> for AnyNode {
    fn from(node: StmtImportFrom) -> Self {
        AnyNode::StmtImportFrom(node)
    }
}

impl From<StmtGlobal> for AnyNode {
    fn from(node: StmtGlobal) -> Self {
        AnyNode::StmtGlobal(node)
    }
}

impl From<StmtNonlocal> for AnyNode {
    fn from(node: StmtNonlocal) -> Self {
        AnyNode::StmtNonlocal(node)
    }
}

impl From<StmtExpr> for AnyNode {
    fn from(node: StmtExpr) -> Self {
        AnyNode::StmtExpr(node)
    }
}

impl From<StmtPass> for AnyNode {
    fn from(node: StmtPass) -> Self {
        AnyNode::StmtPass(node)
    }
}

impl From<StmtBreak> for AnyNode {
    fn from(node: StmtBreak) -> Self {
        AnyNode::StmtBreak(node)
    }
}

impl From<StmtContinue> for AnyNode {
    fn from(node: StmtContinue) -> Self {
        AnyNode::StmtContinue(node)
    }
}

impl From<ExprBoolOp> for AnyNode {
    fn from(node: ExprBoolOp) -> Self {
        AnyNode::ExprBoolOp(node)
    }
}

impl From<ExprNamedExpr> for AnyNode {
    fn from(node: ExprNamedExpr) -> Self {
        AnyNode::ExprNamedExpr(node)
    }
}

impl From<ExprBinOp> for AnyNode {
    fn from(node: ExprBinOp) -> Self {
        AnyNode::ExprBinOp(node)
    }
}

impl From<ExprUnaryOp> for AnyNode {
    fn from(node: ExprUnaryOp) -> Self {
        AnyNode::ExprUnaryOp(node)
    }
}

impl From<ExprLambda> for AnyNode {
    fn from(node: ExprLambda) -> Self {
        AnyNode::ExprLambda(node)
    }
}

impl From<ExprIfExp> for AnyNode {
    fn from(node: ExprIfExp) -> Self {
        AnyNode::ExprIfExp(node)
    }
}

impl From<ExprDict> for AnyNode {
    fn from(node: ExprDict) -> Self {
        AnyNode::ExprDict(node)
    }
}

impl From<ExprSet> for AnyNode {
    fn from(node: ExprSet) -> Self {
        AnyNode::ExprSet(node)
    }
}

impl From<ExprListComp> for AnyNode {
    fn from(node: ExprListComp) -> Self {
        AnyNode::ExprListComp(node)
    }
}

impl From<ExprSetComp> for AnyNode {
    fn from(node: ExprSetComp) -> Self {
        AnyNode::ExprSetComp(node)
    }
}

impl From<ExprDictComp> for AnyNode {
    fn from(node: ExprDictComp) -> Self {
        AnyNode::ExprDictComp(node)
    }
}

impl From<ExprGeneratorExp> for AnyNode {
    fn from(node: ExprGeneratorExp) -> Self {
        AnyNode::ExprGeneratorExp(node)
    }
}

impl From<ExprAwait> for AnyNode {
    fn from(node: ExprAwait) -> Self {
        AnyNode::ExprAwait(node)
    }
}

impl From<ExprYield> for AnyNode {
    fn from(node: ExprYield) -> Self {
        AnyNode::ExprYield(node)
    }
}

impl From<ExprYieldFrom> for AnyNode {
    fn from(node: ExprYieldFrom) -> Self {
        AnyNode::ExprYieldFrom(node)
    }
}

impl From<ExprCompare> for AnyNode {
    fn from(node: ExprCompare) -> Self {
        AnyNode::ExprCompare(node)
    }
}

impl From<ExprCall> for AnyNode {
    fn from(node: ExprCall) -> Self {
        AnyNode::ExprCall(node)
    }
}

impl From<ExprFormattedValue> for AnyNode {
    fn from(node: ExprFormattedValue) -> Self {
        AnyNode::ExprFormattedValue(node)
    }
}

impl From<ExprJoinedStr> for AnyNode {
    fn from(node: ExprJoinedStr) -> Self {
        AnyNode::ExprJoinedStr(node)
    }
}

impl From<ExprConstant> for AnyNode {
    fn from(node: ExprConstant) -> Self {
        AnyNode::ExprConstant(node)
    }
}

impl From<ExprAttribute> for AnyNode {
    fn from(node: ExprAttribute) -> Self {
        AnyNode::ExprAttribute(node)
    }
}

impl From<ExprSubscript> for AnyNode {
    fn from(node: ExprSubscript) -> Self {
        AnyNode::ExprSubscript(node)
    }
}

impl From<ExprStarred> for AnyNode {
    fn from(node: ExprStarred) -> Self {
        AnyNode::ExprStarred(node)
    }
}

impl From<ExprName> for AnyNode {
    fn from(node: ExprName) -> Self {
        AnyNode::ExprName(node)
    }
}

impl From<ExprList> for AnyNode {
    fn from(node: ExprList) -> Self {
        AnyNode::ExprList(node)
    }
}

impl From<ExprTuple> for AnyNode {
    fn from(node: ExprTuple) -> Self {
        AnyNode::ExprTuple(node)
    }
}

impl From<ExprSlice> for AnyNode {
    fn from(node: ExprSlice) -> Self {
        AnyNode::ExprSlice(node)
    }
}

impl From<ExceptHandlerExceptHandler> for AnyNode {
    fn from(node: ExceptHandlerExceptHandler) -> Self {
        AnyNode::ExceptHandlerExceptHandler(node)
    }
}

impl From<PatternMatchValue> for AnyNode {
    fn from(node: PatternMatchValue) -> Self {
        AnyNode::PatternMatchValue(node)
    }
}

impl From<PatternMatchSingleton> for AnyNode {
    fn from(node: PatternMatchSingleton) -> Self {
        AnyNode::PatternMatchSingleton(node)
    }
}

impl From<PatternMatchSequence> for AnyNode {
    fn from(node: PatternMatchSequence) -> Self {
        AnyNode::PatternMatchSequence(node)
    }
}

impl From<PatternMatchMapping> for AnyNode {
    fn from(node: PatternMatchMapping) -> Self {
        AnyNode::PatternMatchMapping(node)
    }
}

impl From<PatternMatchClass> for AnyNode {
    fn from(node: PatternMatchClass) -> Self {
        AnyNode::PatternMatchClass(node)
    }
}

impl From<PatternMatchStar> for AnyNode {
    fn from(node: PatternMatchStar) -> Self {
        AnyNode::PatternMatchStar(node)
    }
}

impl From<PatternMatchAs> for AnyNode {
    fn from(node: PatternMatchAs) -> Self {
        AnyNode::PatternMatchAs(node)
    }
}

impl From<PatternMatchOr> for AnyNode {
    fn from(node: PatternMatchOr) -> Self {
        AnyNode::PatternMatchOr(node)
    }
}

impl From<TypeIgnoreTypeIgnore> for AnyNode {
    fn from(node: TypeIgnoreTypeIgnore) -> Self {
        AnyNode::TypeIgnoreTypeIgnore(node)
    }
}

impl From<Comprehension> for AnyNode {
    fn from(node: Comprehension) -> Self {
        AnyNode::Comprehension(node)
    }
}
impl From<Arguments> for AnyNode {
    fn from(node: Arguments) -> Self {
        AnyNode::Arguments(node)
    }
}
impl From<Arg> for AnyNode {
    fn from(node: Arg) -> Self {
        AnyNode::Arg(node)
    }
}
impl From<ArgWithDefault> for AnyNode {
    fn from(node: ArgWithDefault) -> Self {
        AnyNode::ArgWithDefault(node)
    }
}
impl From<Keyword> for AnyNode {
    fn from(node: Keyword) -> Self {
        AnyNode::Keyword(node)
    }
}
impl From<Alias> for AnyNode {
    fn from(node: Alias) -> Self {
        AnyNode::Alias(node)
    }
}
impl From<WithItem> for AnyNode {
    fn from(node: WithItem) -> Self {
        AnyNode::WithItem(node)
    }
}
impl From<MatchCase> for AnyNode {
    fn from(node: MatchCase) -> Self {
        AnyNode::MatchCase(node)
    }
}
impl From<Decorator> for AnyNode {
    fn from(node: Decorator) -> Self {
        AnyNode::Decorator(node)
    }
}

impl Ranged for AnyNode {
    fn range(&self) -> TextRange {
        match self {
            AnyNode::ModModule(node) => node.range(),
            AnyNode::ModInteractive(node) => node.range(),
            AnyNode::ModExpression(node) => node.range(),
            AnyNode::ModFunctionType(node) => node.range(),
            AnyNode::StmtFunctionDef(node) => node.range(),
            AnyNode::StmtAsyncFunctionDef(node) => node.range(),
            AnyNode::StmtClassDef(node) => node.range(),
            AnyNode::StmtReturn(node) => node.range(),
            AnyNode::StmtDelete(node) => node.range(),
            AnyNode::StmtAssign(node) => node.range(),
            AnyNode::StmtAugAssign(node) => node.range(),
            AnyNode::StmtAnnAssign(node) => node.range(),
            AnyNode::StmtFor(node) => node.range(),
            AnyNode::StmtAsyncFor(node) => node.range(),
            AnyNode::StmtWhile(node) => node.range(),
            AnyNode::StmtIf(node) => node.range(),
            AnyNode::StmtWith(node) => node.range(),
            AnyNode::StmtAsyncWith(node) => node.range(),
            AnyNode::StmtMatch(node) => node.range(),
            AnyNode::StmtRaise(node) => node.range(),
            AnyNode::StmtTry(node) => node.range(),
            AnyNode::StmtTryStar(node) => node.range(),
            AnyNode::StmtAssert(node) => node.range(),
            AnyNode::StmtImport(node) => node.range(),
            AnyNode::StmtImportFrom(node) => node.range(),
            AnyNode::StmtGlobal(node) => node.range(),
            AnyNode::StmtNonlocal(node) => node.range(),
            AnyNode::StmtExpr(node) => node.range(),
            AnyNode::StmtPass(node) => node.range(),
            AnyNode::StmtBreak(node) => node.range(),
            AnyNode::StmtContinue(node) => node.range(),
            AnyNode::ExprBoolOp(node) => node.range(),
            AnyNode::ExprNamedExpr(node) => node.range(),
            AnyNode::ExprBinOp(node) => node.range(),
            AnyNode::ExprUnaryOp(node) => node.range(),
            AnyNode::ExprLambda(node) => node.range(),
            AnyNode::ExprIfExp(node) => node.range(),
            AnyNode::ExprDict(node) => node.range(),
            AnyNode::ExprSet(node) => node.range(),
            AnyNode::ExprListComp(node) => node.range(),
            AnyNode::ExprSetComp(node) => node.range(),
            AnyNode::ExprDictComp(node) => node.range(),
            AnyNode::ExprGeneratorExp(node) => node.range(),
            AnyNode::ExprAwait(node) => node.range(),
            AnyNode::ExprYield(node) => node.range(),
            AnyNode::ExprYieldFrom(node) => node.range(),
            AnyNode::ExprCompare(node) => node.range(),
            AnyNode::ExprCall(node) => node.range(),
            AnyNode::ExprFormattedValue(node) => node.range(),
            AnyNode::ExprJoinedStr(node) => node.range(),
            AnyNode::ExprConstant(node) => node.range(),
            AnyNode::ExprAttribute(node) => node.range(),
            AnyNode::ExprSubscript(node) => node.range(),
            AnyNode::ExprStarred(node) => node.range(),
            AnyNode::ExprName(node) => node.range(),
            AnyNode::ExprList(node) => node.range(),
            AnyNode::ExprTuple(node) => node.range(),
            AnyNode::ExprSlice(node) => node.range(),
            AnyNode::ExceptHandlerExceptHandler(node) => node.range(),
            AnyNode::PatternMatchValue(node) => node.range(),
            AnyNode::PatternMatchSingleton(node) => node.range(),
            AnyNode::PatternMatchSequence(node) => node.range(),
            AnyNode::PatternMatchMapping(node) => node.range(),
            AnyNode::PatternMatchClass(node) => node.range(),
            AnyNode::PatternMatchStar(node) => node.range(),
            AnyNode::PatternMatchAs(node) => node.range(),
            AnyNode::PatternMatchOr(node) => node.range(),
            AnyNode::TypeIgnoreTypeIgnore(node) => node.range(),
            AnyNode::Comprehension(node) => node.range(),
            AnyNode::Arguments(node) => node.range(),
            AnyNode::Arg(node) => node.range(),
            AnyNode::ArgWithDefault(node) => node.range(),
            AnyNode::Keyword(node) => node.range(),
            AnyNode::Alias(node) => node.range(),
            AnyNode::WithItem(node) => node.range(),
            AnyNode::MatchCase(node) => node.range(),
            AnyNode::Decorator(node) => node.range(),
        }
    }
}

#[derive(Copy, Clone, Debug, is_macro::Is, PartialEq)]
pub enum AnyNodeRef<'a> {
    ModModule(&'a ModModule<TextRange>),
    ModInteractive(&'a ModInteractive<TextRange>),
    ModExpression(&'a ModExpression<TextRange>),
    ModFunctionType(&'a ModFunctionType<TextRange>),
    StmtFunctionDef(&'a StmtFunctionDef<TextRange>),
    StmtAsyncFunctionDef(&'a StmtAsyncFunctionDef<TextRange>),
    StmtClassDef(&'a StmtClassDef<TextRange>),
    StmtReturn(&'a StmtReturn<TextRange>),
    StmtDelete(&'a StmtDelete<TextRange>),
    StmtAssign(&'a StmtAssign<TextRange>),
    StmtAugAssign(&'a StmtAugAssign<TextRange>),
    StmtAnnAssign(&'a StmtAnnAssign<TextRange>),
    StmtFor(&'a StmtFor<TextRange>),
    StmtAsyncFor(&'a StmtAsyncFor<TextRange>),
    StmtWhile(&'a StmtWhile<TextRange>),
    StmtIf(&'a StmtIf<TextRange>),
    StmtWith(&'a StmtWith<TextRange>),
    StmtAsyncWith(&'a StmtAsyncWith<TextRange>),
    StmtMatch(&'a StmtMatch<TextRange>),
    StmtRaise(&'a StmtRaise<TextRange>),
    StmtTry(&'a StmtTry<TextRange>),
    StmtTryStar(&'a StmtTryStar<TextRange>),
    StmtAssert(&'a StmtAssert<TextRange>),
    StmtImport(&'a StmtImport<TextRange>),
    StmtImportFrom(&'a StmtImportFrom<TextRange>),
    StmtGlobal(&'a StmtGlobal<TextRange>),
    StmtNonlocal(&'a StmtNonlocal<TextRange>),
    StmtExpr(&'a StmtExpr<TextRange>),
    StmtPass(&'a StmtPass<TextRange>),
    StmtBreak(&'a StmtBreak<TextRange>),
    StmtContinue(&'a StmtContinue<TextRange>),
    ExprBoolOp(&'a ExprBoolOp<TextRange>),
    ExprNamedExpr(&'a ExprNamedExpr<TextRange>),
    ExprBinOp(&'a ExprBinOp<TextRange>),
    ExprUnaryOp(&'a ExprUnaryOp<TextRange>),
    ExprLambda(&'a ExprLambda<TextRange>),
    ExprIfExp(&'a ExprIfExp<TextRange>),
    ExprDict(&'a ExprDict<TextRange>),
    ExprSet(&'a ExprSet<TextRange>),
    ExprListComp(&'a ExprListComp<TextRange>),
    ExprSetComp(&'a ExprSetComp<TextRange>),
    ExprDictComp(&'a ExprDictComp<TextRange>),
    ExprGeneratorExp(&'a ExprGeneratorExp<TextRange>),
    ExprAwait(&'a ExprAwait<TextRange>),
    ExprYield(&'a ExprYield<TextRange>),
    ExprYieldFrom(&'a ExprYieldFrom<TextRange>),
    ExprCompare(&'a ExprCompare<TextRange>),
    ExprCall(&'a ExprCall<TextRange>),
    ExprFormattedValue(&'a ExprFormattedValue<TextRange>),
    ExprJoinedStr(&'a ExprJoinedStr<TextRange>),
    ExprConstant(&'a ExprConstant<TextRange>),
    ExprAttribute(&'a ExprAttribute<TextRange>),
    ExprSubscript(&'a ExprSubscript<TextRange>),
    ExprStarred(&'a ExprStarred<TextRange>),
    ExprName(&'a ExprName<TextRange>),
    ExprList(&'a ExprList<TextRange>),
    ExprTuple(&'a ExprTuple<TextRange>),
    ExprSlice(&'a ExprSlice<TextRange>),
    ExceptHandlerExceptHandler(&'a ExceptHandlerExceptHandler<TextRange>),
    PatternMatchValue(&'a PatternMatchValue<TextRange>),
    PatternMatchSingleton(&'a PatternMatchSingleton<TextRange>),
    PatternMatchSequence(&'a PatternMatchSequence<TextRange>),
    PatternMatchMapping(&'a PatternMatchMapping<TextRange>),
    PatternMatchClass(&'a PatternMatchClass<TextRange>),
    PatternMatchStar(&'a PatternMatchStar<TextRange>),
    PatternMatchAs(&'a PatternMatchAs<TextRange>),
    PatternMatchOr(&'a PatternMatchOr<TextRange>),
    TypeIgnoreTypeIgnore(&'a TypeIgnoreTypeIgnore<TextRange>),
    Comprehension(&'a Comprehension<TextRange>),
    Arguments(&'a Arguments<TextRange>),
    Arg(&'a Arg<TextRange>),
    ArgWithDefault(&'a ArgWithDefault<TextRange>),
    Keyword(&'a Keyword<TextRange>),
    Alias(&'a Alias<TextRange>),
    WithItem(&'a WithItem<TextRange>),
    MatchCase(&'a MatchCase<TextRange>),
    Decorator(&'a Decorator<TextRange>),
}

impl AnyNodeRef<'_> {
    pub fn as_ptr(&self) -> NonNull<()> {
        match self {
            AnyNodeRef::ModModule(node) => NonNull::from(*node).cast(),
            AnyNodeRef::ModInteractive(node) => NonNull::from(*node).cast(),
            AnyNodeRef::ModExpression(node) => NonNull::from(*node).cast(),
            AnyNodeRef::ModFunctionType(node) => NonNull::from(*node).cast(),
            AnyNodeRef::StmtFunctionDef(node) => NonNull::from(*node).cast(),
            AnyNodeRef::StmtAsyncFunctionDef(node) => NonNull::from(*node).cast(),
            AnyNodeRef::StmtClassDef(node) => NonNull::from(*node).cast(),
            AnyNodeRef::StmtReturn(node) => NonNull::from(*node).cast(),
            AnyNodeRef::StmtDelete(node) => NonNull::from(*node).cast(),
            AnyNodeRef::StmtAssign(node) => NonNull::from(*node).cast(),
            AnyNodeRef::StmtAugAssign(node) => NonNull::from(*node).cast(),
            AnyNodeRef::StmtAnnAssign(node) => NonNull::from(*node).cast(),
            AnyNodeRef::StmtFor(node) => NonNull::from(*node).cast(),
            AnyNodeRef::StmtAsyncFor(node) => NonNull::from(*node).cast(),
            AnyNodeRef::StmtWhile(node) => NonNull::from(*node).cast(),
            AnyNodeRef::StmtIf(node) => NonNull::from(*node).cast(),
            AnyNodeRef::StmtWith(node) => NonNull::from(*node).cast(),
            AnyNodeRef::StmtAsyncWith(node) => NonNull::from(*node).cast(),
            AnyNodeRef::StmtMatch(node) => NonNull::from(*node).cast(),
            AnyNodeRef::StmtRaise(node) => NonNull::from(*node).cast(),
            AnyNodeRef::StmtTry(node) => NonNull::from(*node).cast(),
            AnyNodeRef::StmtTryStar(node) => NonNull::from(*node).cast(),
            AnyNodeRef::StmtAssert(node) => NonNull::from(*node).cast(),
            AnyNodeRef::StmtImport(node) => NonNull::from(*node).cast(),
            AnyNodeRef::StmtImportFrom(node) => NonNull::from(*node).cast(),
            AnyNodeRef::StmtGlobal(node) => NonNull::from(*node).cast(),
            AnyNodeRef::StmtNonlocal(node) => NonNull::from(*node).cast(),
            AnyNodeRef::StmtExpr(node) => NonNull::from(*node).cast(),
            AnyNodeRef::StmtPass(node) => NonNull::from(*node).cast(),
            AnyNodeRef::StmtBreak(node) => NonNull::from(*node).cast(),
            AnyNodeRef::StmtContinue(node) => NonNull::from(*node).cast(),
            AnyNodeRef::ExprBoolOp(node) => NonNull::from(*node).cast(),
            AnyNodeRef::ExprNamedExpr(node) => NonNull::from(*node).cast(),
            AnyNodeRef::ExprBinOp(node) => NonNull::from(*node).cast(),
            AnyNodeRef::ExprUnaryOp(node) => NonNull::from(*node).cast(),
            AnyNodeRef::ExprLambda(node) => NonNull::from(*node).cast(),
            AnyNodeRef::ExprIfExp(node) => NonNull::from(*node).cast(),
            AnyNodeRef::ExprDict(node) => NonNull::from(*node).cast(),
            AnyNodeRef::ExprSet(node) => NonNull::from(*node).cast(),
            AnyNodeRef::ExprListComp(node) => NonNull::from(*node).cast(),
            AnyNodeRef::ExprSetComp(node) => NonNull::from(*node).cast(),
            AnyNodeRef::ExprDictComp(node) => NonNull::from(*node).cast(),
            AnyNodeRef::ExprGeneratorExp(node) => NonNull::from(*node).cast(),
            AnyNodeRef::ExprAwait(node) => NonNull::from(*node).cast(),
            AnyNodeRef::ExprYield(node) => NonNull::from(*node).cast(),
            AnyNodeRef::ExprYieldFrom(node) => NonNull::from(*node).cast(),
            AnyNodeRef::ExprCompare(node) => NonNull::from(*node).cast(),
            AnyNodeRef::ExprCall(node) => NonNull::from(*node).cast(),
            AnyNodeRef::ExprFormattedValue(node) => NonNull::from(*node).cast(),
            AnyNodeRef::ExprJoinedStr(node) => NonNull::from(*node).cast(),
            AnyNodeRef::ExprConstant(node) => NonNull::from(*node).cast(),
            AnyNodeRef::ExprAttribute(node) => NonNull::from(*node).cast(),
            AnyNodeRef::ExprSubscript(node) => NonNull::from(*node).cast(),
            AnyNodeRef::ExprStarred(node) => NonNull::from(*node).cast(),
            AnyNodeRef::ExprName(node) => NonNull::from(*node).cast(),
            AnyNodeRef::ExprList(node) => NonNull::from(*node).cast(),
            AnyNodeRef::ExprTuple(node) => NonNull::from(*node).cast(),
            AnyNodeRef::ExprSlice(node) => NonNull::from(*node).cast(),
            AnyNodeRef::ExceptHandlerExceptHandler(node) => NonNull::from(*node).cast(),
            AnyNodeRef::PatternMatchValue(node) => NonNull::from(*node).cast(),
            AnyNodeRef::PatternMatchSingleton(node) => NonNull::from(*node).cast(),
            AnyNodeRef::PatternMatchSequence(node) => NonNull::from(*node).cast(),
            AnyNodeRef::PatternMatchMapping(node) => NonNull::from(*node).cast(),
            AnyNodeRef::PatternMatchClass(node) => NonNull::from(*node).cast(),
            AnyNodeRef::PatternMatchStar(node) => NonNull::from(*node).cast(),
            AnyNodeRef::PatternMatchAs(node) => NonNull::from(*node).cast(),
            AnyNodeRef::PatternMatchOr(node) => NonNull::from(*node).cast(),
            AnyNodeRef::TypeIgnoreTypeIgnore(node) => NonNull::from(*node).cast(),
            AnyNodeRef::Comprehension(node) => NonNull::from(*node).cast(),
            AnyNodeRef::Arguments(node) => NonNull::from(*node).cast(),
            AnyNodeRef::Arg(node) => NonNull::from(*node).cast(),
            AnyNodeRef::ArgWithDefault(node) => NonNull::from(*node).cast(),
            AnyNodeRef::Keyword(node) => NonNull::from(*node).cast(),
            AnyNodeRef::Alias(node) => NonNull::from(*node).cast(),
            AnyNodeRef::WithItem(node) => NonNull::from(*node).cast(),
            AnyNodeRef::MatchCase(node) => NonNull::from(*node).cast(),
            AnyNodeRef::Decorator(node) => NonNull::from(*node).cast(),
        }
    }

    /// Compares two any node refs by their pointers (referential equality).
    pub fn ptr_eq(self, other: AnyNodeRef) -> bool {
        self.as_ptr().eq(&other.as_ptr())
    }

    /// Returns the node's [`kind`](NodeKind) that has no data associated and is [`Copy`].
    pub const fn kind(self) -> NodeKind {
        match self {
            AnyNodeRef::ModModule(_) => NodeKind::ModModule,
            AnyNodeRef::ModInteractive(_) => NodeKind::ModInteractive,
            AnyNodeRef::ModExpression(_) => NodeKind::ModExpression,
            AnyNodeRef::ModFunctionType(_) => NodeKind::ModFunctionType,
            AnyNodeRef::StmtFunctionDef(_) => NodeKind::StmtFunctionDef,
            AnyNodeRef::StmtAsyncFunctionDef(_) => NodeKind::StmtAsyncFunctionDef,
            AnyNodeRef::StmtClassDef(_) => NodeKind::StmtClassDef,
            AnyNodeRef::StmtReturn(_) => NodeKind::StmtReturn,
            AnyNodeRef::StmtDelete(_) => NodeKind::StmtDelete,
            AnyNodeRef::StmtAssign(_) => NodeKind::StmtAssign,
            AnyNodeRef::StmtAugAssign(_) => NodeKind::StmtAugAssign,
            AnyNodeRef::StmtAnnAssign(_) => NodeKind::StmtAnnAssign,
            AnyNodeRef::StmtFor(_) => NodeKind::StmtFor,
            AnyNodeRef::StmtAsyncFor(_) => NodeKind::StmtAsyncFor,
            AnyNodeRef::StmtWhile(_) => NodeKind::StmtWhile,
            AnyNodeRef::StmtIf(_) => NodeKind::StmtIf,
            AnyNodeRef::StmtWith(_) => NodeKind::StmtWith,
            AnyNodeRef::StmtAsyncWith(_) => NodeKind::StmtAsyncWith,
            AnyNodeRef::StmtMatch(_) => NodeKind::StmtMatch,
            AnyNodeRef::StmtRaise(_) => NodeKind::StmtRaise,
            AnyNodeRef::StmtTry(_) => NodeKind::StmtTry,
            AnyNodeRef::StmtTryStar(_) => NodeKind::StmtTryStar,
            AnyNodeRef::StmtAssert(_) => NodeKind::StmtAssert,
            AnyNodeRef::StmtImport(_) => NodeKind::StmtImport,
            AnyNodeRef::StmtImportFrom(_) => NodeKind::StmtImportFrom,
            AnyNodeRef::StmtGlobal(_) => NodeKind::StmtGlobal,
            AnyNodeRef::StmtNonlocal(_) => NodeKind::StmtNonlocal,
            AnyNodeRef::StmtExpr(_) => NodeKind::StmtExpr,
            AnyNodeRef::StmtPass(_) => NodeKind::StmtPass,
            AnyNodeRef::StmtBreak(_) => NodeKind::StmtBreak,
            AnyNodeRef::StmtContinue(_) => NodeKind::StmtContinue,
            AnyNodeRef::ExprBoolOp(_) => NodeKind::ExprBoolOp,
            AnyNodeRef::ExprNamedExpr(_) => NodeKind::ExprNamedExpr,
            AnyNodeRef::ExprBinOp(_) => NodeKind::ExprBinOp,
            AnyNodeRef::ExprUnaryOp(_) => NodeKind::ExprUnaryOp,
            AnyNodeRef::ExprLambda(_) => NodeKind::ExprLambda,
            AnyNodeRef::ExprIfExp(_) => NodeKind::ExprIfExp,
            AnyNodeRef::ExprDict(_) => NodeKind::ExprDict,
            AnyNodeRef::ExprSet(_) => NodeKind::ExprSet,
            AnyNodeRef::ExprListComp(_) => NodeKind::ExprListComp,
            AnyNodeRef::ExprSetComp(_) => NodeKind::ExprSetComp,
            AnyNodeRef::ExprDictComp(_) => NodeKind::ExprDictComp,
            AnyNodeRef::ExprGeneratorExp(_) => NodeKind::ExprGeneratorExp,
            AnyNodeRef::ExprAwait(_) => NodeKind::ExprAwait,
            AnyNodeRef::ExprYield(_) => NodeKind::ExprYield,
            AnyNodeRef::ExprYieldFrom(_) => NodeKind::ExprYieldFrom,
            AnyNodeRef::ExprCompare(_) => NodeKind::ExprCompare,
            AnyNodeRef::ExprCall(_) => NodeKind::ExprCall,
            AnyNodeRef::ExprFormattedValue(_) => NodeKind::ExprFormattedValue,
            AnyNodeRef::ExprJoinedStr(_) => NodeKind::ExprJoinedStr,
            AnyNodeRef::ExprConstant(_) => NodeKind::ExprConstant,
            AnyNodeRef::ExprAttribute(_) => NodeKind::ExprAttribute,
            AnyNodeRef::ExprSubscript(_) => NodeKind::ExprSubscript,
            AnyNodeRef::ExprStarred(_) => NodeKind::ExprStarred,
            AnyNodeRef::ExprName(_) => NodeKind::ExprName,
            AnyNodeRef::ExprList(_) => NodeKind::ExprList,
            AnyNodeRef::ExprTuple(_) => NodeKind::ExprTuple,
            AnyNodeRef::ExprSlice(_) => NodeKind::ExprSlice,
            AnyNodeRef::ExceptHandlerExceptHandler(_) => NodeKind::ExceptHandlerExceptHandler,
            AnyNodeRef::PatternMatchValue(_) => NodeKind::PatternMatchValue,
            AnyNodeRef::PatternMatchSingleton(_) => NodeKind::PatternMatchSingleton,
            AnyNodeRef::PatternMatchSequence(_) => NodeKind::PatternMatchSequence,
            AnyNodeRef::PatternMatchMapping(_) => NodeKind::PatternMatchMapping,
            AnyNodeRef::PatternMatchClass(_) => NodeKind::PatternMatchClass,
            AnyNodeRef::PatternMatchStar(_) => NodeKind::PatternMatchStar,
            AnyNodeRef::PatternMatchAs(_) => NodeKind::PatternMatchAs,
            AnyNodeRef::PatternMatchOr(_) => NodeKind::PatternMatchOr,
            AnyNodeRef::TypeIgnoreTypeIgnore(_) => NodeKind::TypeIgnoreTypeIgnore,
            AnyNodeRef::Comprehension(_) => NodeKind::Comprehension,
            AnyNodeRef::Arguments(_) => NodeKind::Arguments,
            AnyNodeRef::Arg(_) => NodeKind::Arg,
            AnyNodeRef::ArgWithDefault(_) => NodeKind::ArgWithDefault,
            AnyNodeRef::Keyword(_) => NodeKind::Keyword,
            AnyNodeRef::Alias(_) => NodeKind::Alias,
            AnyNodeRef::WithItem(_) => NodeKind::WithItem,
            AnyNodeRef::MatchCase(_) => NodeKind::MatchCase,
            AnyNodeRef::Decorator(_) => NodeKind::Decorator,
        }
    }

    pub const fn is_statement(self) -> bool {
        match self {
            AnyNodeRef::StmtFunctionDef(_)
            | AnyNodeRef::StmtAsyncFunctionDef(_)
            | AnyNodeRef::StmtClassDef(_)
            | AnyNodeRef::StmtReturn(_)
            | AnyNodeRef::StmtDelete(_)
            | AnyNodeRef::StmtAssign(_)
            | AnyNodeRef::StmtAugAssign(_)
            | AnyNodeRef::StmtAnnAssign(_)
            | AnyNodeRef::StmtFor(_)
            | AnyNodeRef::StmtAsyncFor(_)
            | AnyNodeRef::StmtWhile(_)
            | AnyNodeRef::StmtIf(_)
            | AnyNodeRef::StmtWith(_)
            | AnyNodeRef::StmtAsyncWith(_)
            | AnyNodeRef::StmtMatch(_)
            | AnyNodeRef::StmtRaise(_)
            | AnyNodeRef::StmtTry(_)
            | AnyNodeRef::StmtTryStar(_)
            | AnyNodeRef::StmtAssert(_)
            | AnyNodeRef::StmtImport(_)
            | AnyNodeRef::StmtImportFrom(_)
            | AnyNodeRef::StmtGlobal(_)
            | AnyNodeRef::StmtNonlocal(_)
            | AnyNodeRef::StmtExpr(_)
            | AnyNodeRef::StmtPass(_)
            | AnyNodeRef::StmtBreak(_)
            | AnyNodeRef::StmtContinue(_) => true,

            AnyNodeRef::ModModule(_)
            | AnyNodeRef::ModInteractive(_)
            | AnyNodeRef::ModExpression(_)
            | AnyNodeRef::ModFunctionType(_)
            | AnyNodeRef::ExprBoolOp(_)
            | AnyNodeRef::ExprNamedExpr(_)
            | AnyNodeRef::ExprBinOp(_)
            | AnyNodeRef::ExprUnaryOp(_)
            | AnyNodeRef::ExprLambda(_)
            | AnyNodeRef::ExprIfExp(_)
            | AnyNodeRef::ExprDict(_)
            | AnyNodeRef::ExprSet(_)
            | AnyNodeRef::ExprListComp(_)
            | AnyNodeRef::ExprSetComp(_)
            | AnyNodeRef::ExprDictComp(_)
            | AnyNodeRef::ExprGeneratorExp(_)
            | AnyNodeRef::ExprAwait(_)
            | AnyNodeRef::ExprYield(_)
            | AnyNodeRef::ExprYieldFrom(_)
            | AnyNodeRef::ExprCompare(_)
            | AnyNodeRef::ExprCall(_)
            | AnyNodeRef::ExprFormattedValue(_)
            | AnyNodeRef::ExprJoinedStr(_)
            | AnyNodeRef::ExprConstant(_)
            | AnyNodeRef::ExprAttribute(_)
            | AnyNodeRef::ExprSubscript(_)
            | AnyNodeRef::ExprStarred(_)
            | AnyNodeRef::ExprName(_)
            | AnyNodeRef::ExprList(_)
            | AnyNodeRef::ExprTuple(_)
            | AnyNodeRef::ExprSlice(_)
            | AnyNodeRef::ExceptHandlerExceptHandler(_)
            | AnyNodeRef::PatternMatchValue(_)
            | AnyNodeRef::PatternMatchSingleton(_)
            | AnyNodeRef::PatternMatchSequence(_)
            | AnyNodeRef::PatternMatchMapping(_)
            | AnyNodeRef::PatternMatchClass(_)
            | AnyNodeRef::PatternMatchStar(_)
            | AnyNodeRef::PatternMatchAs(_)
            | AnyNodeRef::PatternMatchOr(_)
            | AnyNodeRef::TypeIgnoreTypeIgnore(_)
            | AnyNodeRef::Comprehension(_)
            | AnyNodeRef::Arguments(_)
            | AnyNodeRef::Arg(_)
            | AnyNodeRef::ArgWithDefault(_)
            | AnyNodeRef::Keyword(_)
            | AnyNodeRef::Alias(_)
            | AnyNodeRef::WithItem(_)
            | AnyNodeRef::MatchCase(_)
            | AnyNodeRef::Decorator(_) => false,
        }
    }

    pub const fn is_expression(self) -> bool {
        match self {
            AnyNodeRef::ExprBoolOp(_)
            | AnyNodeRef::ExprNamedExpr(_)
            | AnyNodeRef::ExprBinOp(_)
            | AnyNodeRef::ExprUnaryOp(_)
            | AnyNodeRef::ExprLambda(_)
            | AnyNodeRef::ExprIfExp(_)
            | AnyNodeRef::ExprDict(_)
            | AnyNodeRef::ExprSet(_)
            | AnyNodeRef::ExprListComp(_)
            | AnyNodeRef::ExprSetComp(_)
            | AnyNodeRef::ExprDictComp(_)
            | AnyNodeRef::ExprGeneratorExp(_)
            | AnyNodeRef::ExprAwait(_)
            | AnyNodeRef::ExprYield(_)
            | AnyNodeRef::ExprYieldFrom(_)
            | AnyNodeRef::ExprCompare(_)
            | AnyNodeRef::ExprCall(_)
            | AnyNodeRef::ExprFormattedValue(_)
            | AnyNodeRef::ExprJoinedStr(_)
            | AnyNodeRef::ExprConstant(_)
            | AnyNodeRef::ExprAttribute(_)
            | AnyNodeRef::ExprSubscript(_)
            | AnyNodeRef::ExprStarred(_)
            | AnyNodeRef::ExprName(_)
            | AnyNodeRef::ExprList(_)
            | AnyNodeRef::ExprTuple(_)
            | AnyNodeRef::ExprSlice(_) => true,

            AnyNodeRef::ModModule(_)
            | AnyNodeRef::ModInteractive(_)
            | AnyNodeRef::ModExpression(_)
            | AnyNodeRef::ModFunctionType(_)
            | AnyNodeRef::StmtFunctionDef(_)
            | AnyNodeRef::StmtAsyncFunctionDef(_)
            | AnyNodeRef::StmtClassDef(_)
            | AnyNodeRef::StmtReturn(_)
            | AnyNodeRef::StmtDelete(_)
            | AnyNodeRef::StmtAssign(_)
            | AnyNodeRef::StmtAugAssign(_)
            | AnyNodeRef::StmtAnnAssign(_)
            | AnyNodeRef::StmtFor(_)
            | AnyNodeRef::StmtAsyncFor(_)
            | AnyNodeRef::StmtWhile(_)
            | AnyNodeRef::StmtIf(_)
            | AnyNodeRef::StmtWith(_)
            | AnyNodeRef::StmtAsyncWith(_)
            | AnyNodeRef::StmtMatch(_)
            | AnyNodeRef::StmtRaise(_)
            | AnyNodeRef::StmtTry(_)
            | AnyNodeRef::StmtTryStar(_)
            | AnyNodeRef::StmtAssert(_)
            | AnyNodeRef::StmtImport(_)
            | AnyNodeRef::StmtImportFrom(_)
            | AnyNodeRef::StmtGlobal(_)
            | AnyNodeRef::StmtNonlocal(_)
            | AnyNodeRef::StmtExpr(_)
            | AnyNodeRef::StmtPass(_)
            | AnyNodeRef::StmtBreak(_)
            | AnyNodeRef::StmtContinue(_)
            | AnyNodeRef::ExceptHandlerExceptHandler(_)
            | AnyNodeRef::PatternMatchValue(_)
            | AnyNodeRef::PatternMatchSingleton(_)
            | AnyNodeRef::PatternMatchSequence(_)
            | AnyNodeRef::PatternMatchMapping(_)
            | AnyNodeRef::PatternMatchClass(_)
            | AnyNodeRef::PatternMatchStar(_)
            | AnyNodeRef::PatternMatchAs(_)
            | AnyNodeRef::PatternMatchOr(_)
            | AnyNodeRef::TypeIgnoreTypeIgnore(_)
            | AnyNodeRef::Comprehension(_)
            | AnyNodeRef::Arguments(_)
            | AnyNodeRef::Arg(_)
            | AnyNodeRef::ArgWithDefault(_)
            | AnyNodeRef::Keyword(_)
            | AnyNodeRef::Alias(_)
            | AnyNodeRef::WithItem(_)
            | AnyNodeRef::MatchCase(_)
            | AnyNodeRef::Decorator(_) => false,
        }
    }

    pub const fn is_module(self) -> bool {
        match self {
            AnyNodeRef::ModModule(_)
            | AnyNodeRef::ModInteractive(_)
            | AnyNodeRef::ModExpression(_)
            | AnyNodeRef::ModFunctionType(_) => true,

            AnyNodeRef::StmtFunctionDef(_)
            | AnyNodeRef::StmtAsyncFunctionDef(_)
            | AnyNodeRef::StmtClassDef(_)
            | AnyNodeRef::StmtReturn(_)
            | AnyNodeRef::StmtDelete(_)
            | AnyNodeRef::StmtAssign(_)
            | AnyNodeRef::StmtAugAssign(_)
            | AnyNodeRef::StmtAnnAssign(_)
            | AnyNodeRef::StmtFor(_)
            | AnyNodeRef::StmtAsyncFor(_)
            | AnyNodeRef::StmtWhile(_)
            | AnyNodeRef::StmtIf(_)
            | AnyNodeRef::StmtWith(_)
            | AnyNodeRef::StmtAsyncWith(_)
            | AnyNodeRef::StmtMatch(_)
            | AnyNodeRef::StmtRaise(_)
            | AnyNodeRef::StmtTry(_)
            | AnyNodeRef::StmtTryStar(_)
            | AnyNodeRef::StmtAssert(_)
            | AnyNodeRef::StmtImport(_)
            | AnyNodeRef::StmtImportFrom(_)
            | AnyNodeRef::StmtGlobal(_)
            | AnyNodeRef::StmtNonlocal(_)
            | AnyNodeRef::StmtExpr(_)
            | AnyNodeRef::StmtPass(_)
            | AnyNodeRef::StmtBreak(_)
            | AnyNodeRef::StmtContinue(_)
            | AnyNodeRef::ExprBoolOp(_)
            | AnyNodeRef::ExprNamedExpr(_)
            | AnyNodeRef::ExprBinOp(_)
            | AnyNodeRef::ExprUnaryOp(_)
            | AnyNodeRef::ExprLambda(_)
            | AnyNodeRef::ExprIfExp(_)
            | AnyNodeRef::ExprDict(_)
            | AnyNodeRef::ExprSet(_)
            | AnyNodeRef::ExprListComp(_)
            | AnyNodeRef::ExprSetComp(_)
            | AnyNodeRef::ExprDictComp(_)
            | AnyNodeRef::ExprGeneratorExp(_)
            | AnyNodeRef::ExprAwait(_)
            | AnyNodeRef::ExprYield(_)
            | AnyNodeRef::ExprYieldFrom(_)
            | AnyNodeRef::ExprCompare(_)
            | AnyNodeRef::ExprCall(_)
            | AnyNodeRef::ExprFormattedValue(_)
            | AnyNodeRef::ExprJoinedStr(_)
            | AnyNodeRef::ExprConstant(_)
            | AnyNodeRef::ExprAttribute(_)
            | AnyNodeRef::ExprSubscript(_)
            | AnyNodeRef::ExprStarred(_)
            | AnyNodeRef::ExprName(_)
            | AnyNodeRef::ExprList(_)
            | AnyNodeRef::ExprTuple(_)
            | AnyNodeRef::ExprSlice(_)
            | AnyNodeRef::ExceptHandlerExceptHandler(_)
            | AnyNodeRef::PatternMatchValue(_)
            | AnyNodeRef::PatternMatchSingleton(_)
            | AnyNodeRef::PatternMatchSequence(_)
            | AnyNodeRef::PatternMatchMapping(_)
            | AnyNodeRef::PatternMatchClass(_)
            | AnyNodeRef::PatternMatchStar(_)
            | AnyNodeRef::PatternMatchAs(_)
            | AnyNodeRef::PatternMatchOr(_)
            | AnyNodeRef::TypeIgnoreTypeIgnore(_)
            | AnyNodeRef::Comprehension(_)
            | AnyNodeRef::Arguments(_)
            | AnyNodeRef::Arg(_)
            | AnyNodeRef::ArgWithDefault(_)
            | AnyNodeRef::Keyword(_)
            | AnyNodeRef::Alias(_)
            | AnyNodeRef::WithItem(_)
            | AnyNodeRef::MatchCase(_)
            | AnyNodeRef::Decorator(_) => false,
        }
    }

    pub const fn is_pattern(self) -> bool {
        match self {
            AnyNodeRef::PatternMatchValue(_)
            | AnyNodeRef::PatternMatchSingleton(_)
            | AnyNodeRef::PatternMatchSequence(_)
            | AnyNodeRef::PatternMatchMapping(_)
            | AnyNodeRef::PatternMatchClass(_)
            | AnyNodeRef::PatternMatchStar(_)
            | AnyNodeRef::PatternMatchAs(_)
            | AnyNodeRef::PatternMatchOr(_) => true,

            AnyNodeRef::ModModule(_)
            | AnyNodeRef::ModInteractive(_)
            | AnyNodeRef::ModExpression(_)
            | AnyNodeRef::ModFunctionType(_)
            | AnyNodeRef::StmtFunctionDef(_)
            | AnyNodeRef::StmtAsyncFunctionDef(_)
            | AnyNodeRef::StmtClassDef(_)
            | AnyNodeRef::StmtReturn(_)
            | AnyNodeRef::StmtDelete(_)
            | AnyNodeRef::StmtAssign(_)
            | AnyNodeRef::StmtAugAssign(_)
            | AnyNodeRef::StmtAnnAssign(_)
            | AnyNodeRef::StmtFor(_)
            | AnyNodeRef::StmtAsyncFor(_)
            | AnyNodeRef::StmtWhile(_)
            | AnyNodeRef::StmtIf(_)
            | AnyNodeRef::StmtWith(_)
            | AnyNodeRef::StmtAsyncWith(_)
            | AnyNodeRef::StmtMatch(_)
            | AnyNodeRef::StmtRaise(_)
            | AnyNodeRef::StmtTry(_)
            | AnyNodeRef::StmtTryStar(_)
            | AnyNodeRef::StmtAssert(_)
            | AnyNodeRef::StmtImport(_)
            | AnyNodeRef::StmtImportFrom(_)
            | AnyNodeRef::StmtGlobal(_)
            | AnyNodeRef::StmtNonlocal(_)
            | AnyNodeRef::StmtExpr(_)
            | AnyNodeRef::StmtPass(_)
            | AnyNodeRef::StmtBreak(_)
            | AnyNodeRef::StmtContinue(_)
            | AnyNodeRef::ExprBoolOp(_)
            | AnyNodeRef::ExprNamedExpr(_)
            | AnyNodeRef::ExprBinOp(_)
            | AnyNodeRef::ExprUnaryOp(_)
            | AnyNodeRef::ExprLambda(_)
            | AnyNodeRef::ExprIfExp(_)
            | AnyNodeRef::ExprDict(_)
            | AnyNodeRef::ExprSet(_)
            | AnyNodeRef::ExprListComp(_)
            | AnyNodeRef::ExprSetComp(_)
            | AnyNodeRef::ExprDictComp(_)
            | AnyNodeRef::ExprGeneratorExp(_)
            | AnyNodeRef::ExprAwait(_)
            | AnyNodeRef::ExprYield(_)
            | AnyNodeRef::ExprYieldFrom(_)
            | AnyNodeRef::ExprCompare(_)
            | AnyNodeRef::ExprCall(_)
            | AnyNodeRef::ExprFormattedValue(_)
            | AnyNodeRef::ExprJoinedStr(_)
            | AnyNodeRef::ExprConstant(_)
            | AnyNodeRef::ExprAttribute(_)
            | AnyNodeRef::ExprSubscript(_)
            | AnyNodeRef::ExprStarred(_)
            | AnyNodeRef::ExprName(_)
            | AnyNodeRef::ExprList(_)
            | AnyNodeRef::ExprTuple(_)
            | AnyNodeRef::ExprSlice(_)
            | AnyNodeRef::ExceptHandlerExceptHandler(_)
            | AnyNodeRef::TypeIgnoreTypeIgnore(_)
            | AnyNodeRef::Comprehension(_)
            | AnyNodeRef::Arguments(_)
            | AnyNodeRef::Arg(_)
            | AnyNodeRef::ArgWithDefault(_)
            | AnyNodeRef::Keyword(_)
            | AnyNodeRef::Alias(_)
            | AnyNodeRef::WithItem(_)
            | AnyNodeRef::MatchCase(_)
            | AnyNodeRef::Decorator(_) => false,
        }
    }

    pub const fn is_except_handler(self) -> bool {
        match self {
            AnyNodeRef::ExceptHandlerExceptHandler(_) => true,

            AnyNodeRef::ModModule(_)
            | AnyNodeRef::ModInteractive(_)
            | AnyNodeRef::ModExpression(_)
            | AnyNodeRef::ModFunctionType(_)
            | AnyNodeRef::StmtFunctionDef(_)
            | AnyNodeRef::StmtAsyncFunctionDef(_)
            | AnyNodeRef::StmtClassDef(_)
            | AnyNodeRef::StmtReturn(_)
            | AnyNodeRef::StmtDelete(_)
            | AnyNodeRef::StmtAssign(_)
            | AnyNodeRef::StmtAugAssign(_)
            | AnyNodeRef::StmtAnnAssign(_)
            | AnyNodeRef::StmtFor(_)
            | AnyNodeRef::StmtAsyncFor(_)
            | AnyNodeRef::StmtWhile(_)
            | AnyNodeRef::StmtIf(_)
            | AnyNodeRef::StmtWith(_)
            | AnyNodeRef::StmtAsyncWith(_)
            | AnyNodeRef::StmtMatch(_)
            | AnyNodeRef::StmtRaise(_)
            | AnyNodeRef::StmtTry(_)
            | AnyNodeRef::StmtTryStar(_)
            | AnyNodeRef::StmtAssert(_)
            | AnyNodeRef::StmtImport(_)
            | AnyNodeRef::StmtImportFrom(_)
            | AnyNodeRef::StmtGlobal(_)
            | AnyNodeRef::StmtNonlocal(_)
            | AnyNodeRef::StmtExpr(_)
            | AnyNodeRef::StmtPass(_)
            | AnyNodeRef::StmtBreak(_)
            | AnyNodeRef::StmtContinue(_)
            | AnyNodeRef::ExprBoolOp(_)
            | AnyNodeRef::ExprNamedExpr(_)
            | AnyNodeRef::ExprBinOp(_)
            | AnyNodeRef::ExprUnaryOp(_)
            | AnyNodeRef::ExprLambda(_)
            | AnyNodeRef::ExprIfExp(_)
            | AnyNodeRef::ExprDict(_)
            | AnyNodeRef::ExprSet(_)
            | AnyNodeRef::ExprListComp(_)
            | AnyNodeRef::ExprSetComp(_)
            | AnyNodeRef::ExprDictComp(_)
            | AnyNodeRef::ExprGeneratorExp(_)
            | AnyNodeRef::ExprAwait(_)
            | AnyNodeRef::ExprYield(_)
            | AnyNodeRef::ExprYieldFrom(_)
            | AnyNodeRef::ExprCompare(_)
            | AnyNodeRef::ExprCall(_)
            | AnyNodeRef::ExprFormattedValue(_)
            | AnyNodeRef::ExprJoinedStr(_)
            | AnyNodeRef::ExprConstant(_)
            | AnyNodeRef::ExprAttribute(_)
            | AnyNodeRef::ExprSubscript(_)
            | AnyNodeRef::ExprStarred(_)
            | AnyNodeRef::ExprName(_)
            | AnyNodeRef::ExprList(_)
            | AnyNodeRef::ExprTuple(_)
            | AnyNodeRef::ExprSlice(_)
            | AnyNodeRef::PatternMatchValue(_)
            | AnyNodeRef::PatternMatchSingleton(_)
            | AnyNodeRef::PatternMatchSequence(_)
            | AnyNodeRef::PatternMatchMapping(_)
            | AnyNodeRef::PatternMatchClass(_)
            | AnyNodeRef::PatternMatchStar(_)
            | AnyNodeRef::PatternMatchAs(_)
            | AnyNodeRef::PatternMatchOr(_)
            | AnyNodeRef::TypeIgnoreTypeIgnore(_)
            | AnyNodeRef::Comprehension(_)
            | AnyNodeRef::Arguments(_)
            | AnyNodeRef::Arg(_)
            | AnyNodeRef::ArgWithDefault(_)
            | AnyNodeRef::Keyword(_)
            | AnyNodeRef::Alias(_)
            | AnyNodeRef::WithItem(_)
            | AnyNodeRef::MatchCase(_)
            | AnyNodeRef::Decorator(_) => false,
        }
    }

    pub const fn is_type_ignore(self) -> bool {
        match self {
            AnyNodeRef::TypeIgnoreTypeIgnore(_) => true,

            AnyNodeRef::ModModule(_)
            | AnyNodeRef::ModInteractive(_)
            | AnyNodeRef::ModExpression(_)
            | AnyNodeRef::ModFunctionType(_)
            | AnyNodeRef::StmtFunctionDef(_)
            | AnyNodeRef::StmtAsyncFunctionDef(_)
            | AnyNodeRef::StmtClassDef(_)
            | AnyNodeRef::StmtReturn(_)
            | AnyNodeRef::StmtDelete(_)
            | AnyNodeRef::StmtAssign(_)
            | AnyNodeRef::StmtAugAssign(_)
            | AnyNodeRef::StmtAnnAssign(_)
            | AnyNodeRef::StmtFor(_)
            | AnyNodeRef::StmtAsyncFor(_)
            | AnyNodeRef::StmtWhile(_)
            | AnyNodeRef::StmtIf(_)
            | AnyNodeRef::StmtWith(_)
            | AnyNodeRef::StmtAsyncWith(_)
            | AnyNodeRef::StmtMatch(_)
            | AnyNodeRef::StmtRaise(_)
            | AnyNodeRef::StmtTry(_)
            | AnyNodeRef::StmtTryStar(_)
            | AnyNodeRef::StmtAssert(_)
            | AnyNodeRef::StmtImport(_)
            | AnyNodeRef::StmtImportFrom(_)
            | AnyNodeRef::StmtGlobal(_)
            | AnyNodeRef::StmtNonlocal(_)
            | AnyNodeRef::StmtExpr(_)
            | AnyNodeRef::StmtPass(_)
            | AnyNodeRef::StmtBreak(_)
            | AnyNodeRef::StmtContinue(_)
            | AnyNodeRef::ExprBoolOp(_)
            | AnyNodeRef::ExprNamedExpr(_)
            | AnyNodeRef::ExprBinOp(_)
            | AnyNodeRef::ExprUnaryOp(_)
            | AnyNodeRef::ExprLambda(_)
            | AnyNodeRef::ExprIfExp(_)
            | AnyNodeRef::ExprDict(_)
            | AnyNodeRef::ExprSet(_)
            | AnyNodeRef::ExprListComp(_)
            | AnyNodeRef::ExprSetComp(_)
            | AnyNodeRef::ExprDictComp(_)
            | AnyNodeRef::ExprGeneratorExp(_)
            | AnyNodeRef::ExprAwait(_)
            | AnyNodeRef::ExprYield(_)
            | AnyNodeRef::ExprYieldFrom(_)
            | AnyNodeRef::ExprCompare(_)
            | AnyNodeRef::ExprCall(_)
            | AnyNodeRef::ExprFormattedValue(_)
            | AnyNodeRef::ExprJoinedStr(_)
            | AnyNodeRef::ExprConstant(_)
            | AnyNodeRef::ExprAttribute(_)
            | AnyNodeRef::ExprSubscript(_)
            | AnyNodeRef::ExprStarred(_)
            | AnyNodeRef::ExprName(_)
            | AnyNodeRef::ExprList(_)
            | AnyNodeRef::ExprTuple(_)
            | AnyNodeRef::ExprSlice(_)
            | AnyNodeRef::PatternMatchValue(_)
            | AnyNodeRef::PatternMatchSingleton(_)
            | AnyNodeRef::PatternMatchSequence(_)
            | AnyNodeRef::PatternMatchMapping(_)
            | AnyNodeRef::PatternMatchClass(_)
            | AnyNodeRef::PatternMatchStar(_)
            | AnyNodeRef::PatternMatchAs(_)
            | AnyNodeRef::PatternMatchOr(_)
            | AnyNodeRef::ExceptHandlerExceptHandler(_)
            | AnyNodeRef::Comprehension(_)
            | AnyNodeRef::Arguments(_)
            | AnyNodeRef::Arg(_)
            | AnyNodeRef::ArgWithDefault(_)
            | AnyNodeRef::Keyword(_)
            | AnyNodeRef::Alias(_)
            | AnyNodeRef::WithItem(_)
            | AnyNodeRef::MatchCase(_)
            | AnyNodeRef::Decorator(_) => false,
        }
    }

    pub const fn is_node_with_body(self) -> bool {
        matches!(
            self,
            AnyNodeRef::StmtIf(_)
                | AnyNodeRef::StmtFor(_)
                | AnyNodeRef::StmtAsyncFor(_)
                | AnyNodeRef::StmtWhile(_)
                | AnyNodeRef::StmtWith(_)
                | AnyNodeRef::StmtAsyncWith(_)
                | AnyNodeRef::StmtMatch(_)
                | AnyNodeRef::StmtFunctionDef(_)
                | AnyNodeRef::StmtAsyncFunctionDef(_)
                | AnyNodeRef::StmtClassDef(_)
        )
    }
}

impl<'a> From<&'a ModModule> for AnyNodeRef<'a> {
    fn from(node: &'a ModModule) -> Self {
        AnyNodeRef::ModModule(node)
    }
}

impl<'a> From<&'a ModInteractive> for AnyNodeRef<'a> {
    fn from(node: &'a ModInteractive) -> Self {
        AnyNodeRef::ModInteractive(node)
    }
}

impl<'a> From<&'a ModExpression> for AnyNodeRef<'a> {
    fn from(node: &'a ModExpression) -> Self {
        AnyNodeRef::ModExpression(node)
    }
}

impl<'a> From<&'a ModFunctionType> for AnyNodeRef<'a> {
    fn from(node: &'a ModFunctionType) -> Self {
        AnyNodeRef::ModFunctionType(node)
    }
}

impl<'a> From<&'a StmtFunctionDef> for AnyNodeRef<'a> {
    fn from(node: &'a StmtFunctionDef) -> Self {
        AnyNodeRef::StmtFunctionDef(node)
    }
}

impl<'a> From<&'a StmtAsyncFunctionDef> for AnyNodeRef<'a> {
    fn from(node: &'a StmtAsyncFunctionDef) -> Self {
        AnyNodeRef::StmtAsyncFunctionDef(node)
    }
}

impl<'a> From<&'a StmtClassDef> for AnyNodeRef<'a> {
    fn from(node: &'a StmtClassDef) -> Self {
        AnyNodeRef::StmtClassDef(node)
    }
}

impl<'a> From<&'a StmtReturn> for AnyNodeRef<'a> {
    fn from(node: &'a StmtReturn) -> Self {
        AnyNodeRef::StmtReturn(node)
    }
}

impl<'a> From<&'a StmtDelete> for AnyNodeRef<'a> {
    fn from(node: &'a StmtDelete) -> Self {
        AnyNodeRef::StmtDelete(node)
    }
}

impl<'a> From<&'a StmtAssign> for AnyNodeRef<'a> {
    fn from(node: &'a StmtAssign) -> Self {
        AnyNodeRef::StmtAssign(node)
    }
}

impl<'a> From<&'a StmtAugAssign> for AnyNodeRef<'a> {
    fn from(node: &'a StmtAugAssign) -> Self {
        AnyNodeRef::StmtAugAssign(node)
    }
}

impl<'a> From<&'a StmtAnnAssign> for AnyNodeRef<'a> {
    fn from(node: &'a StmtAnnAssign) -> Self {
        AnyNodeRef::StmtAnnAssign(node)
    }
}

impl<'a> From<&'a StmtFor> for AnyNodeRef<'a> {
    fn from(node: &'a StmtFor) -> Self {
        AnyNodeRef::StmtFor(node)
    }
}

impl<'a> From<&'a StmtAsyncFor> for AnyNodeRef<'a> {
    fn from(node: &'a StmtAsyncFor) -> Self {
        AnyNodeRef::StmtAsyncFor(node)
    }
}

impl<'a> From<&'a StmtWhile> for AnyNodeRef<'a> {
    fn from(node: &'a StmtWhile) -> Self {
        AnyNodeRef::StmtWhile(node)
    }
}

impl<'a> From<&'a StmtIf> for AnyNodeRef<'a> {
    fn from(node: &'a StmtIf) -> Self {
        AnyNodeRef::StmtIf(node)
    }
}

impl<'a> From<&'a StmtWith> for AnyNodeRef<'a> {
    fn from(node: &'a StmtWith) -> Self {
        AnyNodeRef::StmtWith(node)
    }
}

impl<'a> From<&'a StmtAsyncWith> for AnyNodeRef<'a> {
    fn from(node: &'a StmtAsyncWith) -> Self {
        AnyNodeRef::StmtAsyncWith(node)
    }
}

impl<'a> From<&'a StmtMatch> for AnyNodeRef<'a> {
    fn from(node: &'a StmtMatch) -> Self {
        AnyNodeRef::StmtMatch(node)
    }
}

impl<'a> From<&'a StmtRaise> for AnyNodeRef<'a> {
    fn from(node: &'a StmtRaise) -> Self {
        AnyNodeRef::StmtRaise(node)
    }
}

impl<'a> From<&'a StmtTry> for AnyNodeRef<'a> {
    fn from(node: &'a StmtTry) -> Self {
        AnyNodeRef::StmtTry(node)
    }
}

impl<'a> From<&'a StmtTryStar> for AnyNodeRef<'a> {
    fn from(node: &'a StmtTryStar) -> Self {
        AnyNodeRef::StmtTryStar(node)
    }
}

impl<'a> From<&'a StmtAssert> for AnyNodeRef<'a> {
    fn from(node: &'a StmtAssert) -> Self {
        AnyNodeRef::StmtAssert(node)
    }
}

impl<'a> From<&'a StmtImport> for AnyNodeRef<'a> {
    fn from(node: &'a StmtImport) -> Self {
        AnyNodeRef::StmtImport(node)
    }
}

impl<'a> From<&'a StmtImportFrom> for AnyNodeRef<'a> {
    fn from(node: &'a StmtImportFrom) -> Self {
        AnyNodeRef::StmtImportFrom(node)
    }
}

impl<'a> From<&'a StmtGlobal> for AnyNodeRef<'a> {
    fn from(node: &'a StmtGlobal) -> Self {
        AnyNodeRef::StmtGlobal(node)
    }
}

impl<'a> From<&'a StmtNonlocal> for AnyNodeRef<'a> {
    fn from(node: &'a StmtNonlocal) -> Self {
        AnyNodeRef::StmtNonlocal(node)
    }
}

impl<'a> From<&'a StmtExpr> for AnyNodeRef<'a> {
    fn from(node: &'a StmtExpr) -> Self {
        AnyNodeRef::StmtExpr(node)
    }
}

impl<'a> From<&'a StmtPass> for AnyNodeRef<'a> {
    fn from(node: &'a StmtPass) -> Self {
        AnyNodeRef::StmtPass(node)
    }
}

impl<'a> From<&'a StmtBreak> for AnyNodeRef<'a> {
    fn from(node: &'a StmtBreak) -> Self {
        AnyNodeRef::StmtBreak(node)
    }
}

impl<'a> From<&'a StmtContinue> for AnyNodeRef<'a> {
    fn from(node: &'a StmtContinue) -> Self {
        AnyNodeRef::StmtContinue(node)
    }
}

impl<'a> From<&'a ExprBoolOp> for AnyNodeRef<'a> {
    fn from(node: &'a ExprBoolOp) -> Self {
        AnyNodeRef::ExprBoolOp(node)
    }
}

impl<'a> From<&'a ExprNamedExpr> for AnyNodeRef<'a> {
    fn from(node: &'a ExprNamedExpr) -> Self {
        AnyNodeRef::ExprNamedExpr(node)
    }
}

impl<'a> From<&'a ExprBinOp> for AnyNodeRef<'a> {
    fn from(node: &'a ExprBinOp) -> Self {
        AnyNodeRef::ExprBinOp(node)
    }
}

impl<'a> From<&'a ExprUnaryOp> for AnyNodeRef<'a> {
    fn from(node: &'a ExprUnaryOp) -> Self {
        AnyNodeRef::ExprUnaryOp(node)
    }
}

impl<'a> From<&'a ExprLambda> for AnyNodeRef<'a> {
    fn from(node: &'a ExprLambda) -> Self {
        AnyNodeRef::ExprLambda(node)
    }
}

impl<'a> From<&'a ExprIfExp> for AnyNodeRef<'a> {
    fn from(node: &'a ExprIfExp) -> Self {
        AnyNodeRef::ExprIfExp(node)
    }
}

impl<'a> From<&'a ExprDict> for AnyNodeRef<'a> {
    fn from(node: &'a ExprDict) -> Self {
        AnyNodeRef::ExprDict(node)
    }
}

impl<'a> From<&'a ExprSet> for AnyNodeRef<'a> {
    fn from(node: &'a ExprSet) -> Self {
        AnyNodeRef::ExprSet(node)
    }
}

impl<'a> From<&'a ExprListComp> for AnyNodeRef<'a> {
    fn from(node: &'a ExprListComp) -> Self {
        AnyNodeRef::ExprListComp(node)
    }
}

impl<'a> From<&'a ExprSetComp> for AnyNodeRef<'a> {
    fn from(node: &'a ExprSetComp) -> Self {
        AnyNodeRef::ExprSetComp(node)
    }
}

impl<'a> From<&'a ExprDictComp> for AnyNodeRef<'a> {
    fn from(node: &'a ExprDictComp) -> Self {
        AnyNodeRef::ExprDictComp(node)
    }
}

impl<'a> From<&'a ExprGeneratorExp> for AnyNodeRef<'a> {
    fn from(node: &'a ExprGeneratorExp) -> Self {
        AnyNodeRef::ExprGeneratorExp(node)
    }
}

impl<'a> From<&'a ExprAwait> for AnyNodeRef<'a> {
    fn from(node: &'a ExprAwait) -> Self {
        AnyNodeRef::ExprAwait(node)
    }
}

impl<'a> From<&'a ExprYield> for AnyNodeRef<'a> {
    fn from(node: &'a ExprYield) -> Self {
        AnyNodeRef::ExprYield(node)
    }
}

impl<'a> From<&'a ExprYieldFrom> for AnyNodeRef<'a> {
    fn from(node: &'a ExprYieldFrom) -> Self {
        AnyNodeRef::ExprYieldFrom(node)
    }
}

impl<'a> From<&'a ExprCompare> for AnyNodeRef<'a> {
    fn from(node: &'a ExprCompare) -> Self {
        AnyNodeRef::ExprCompare(node)
    }
}

impl<'a> From<&'a ExprCall> for AnyNodeRef<'a> {
    fn from(node: &'a ExprCall) -> Self {
        AnyNodeRef::ExprCall(node)
    }
}

impl<'a> From<&'a ExprFormattedValue> for AnyNodeRef<'a> {
    fn from(node: &'a ExprFormattedValue) -> Self {
        AnyNodeRef::ExprFormattedValue(node)
    }
}

impl<'a> From<&'a ExprJoinedStr> for AnyNodeRef<'a> {
    fn from(node: &'a ExprJoinedStr) -> Self {
        AnyNodeRef::ExprJoinedStr(node)
    }
}

impl<'a> From<&'a ExprConstant> for AnyNodeRef<'a> {
    fn from(node: &'a ExprConstant) -> Self {
        AnyNodeRef::ExprConstant(node)
    }
}

impl<'a> From<&'a ExprAttribute> for AnyNodeRef<'a> {
    fn from(node: &'a ExprAttribute) -> Self {
        AnyNodeRef::ExprAttribute(node)
    }
}

impl<'a> From<&'a ExprSubscript> for AnyNodeRef<'a> {
    fn from(node: &'a ExprSubscript) -> Self {
        AnyNodeRef::ExprSubscript(node)
    }
}

impl<'a> From<&'a ExprStarred> for AnyNodeRef<'a> {
    fn from(node: &'a ExprStarred) -> Self {
        AnyNodeRef::ExprStarred(node)
    }
}

impl<'a> From<&'a ExprName> for AnyNodeRef<'a> {
    fn from(node: &'a ExprName) -> Self {
        AnyNodeRef::ExprName(node)
    }
}

impl<'a> From<&'a ExprList> for AnyNodeRef<'a> {
    fn from(node: &'a ExprList) -> Self {
        AnyNodeRef::ExprList(node)
    }
}

impl<'a> From<&'a ExprTuple> for AnyNodeRef<'a> {
    fn from(node: &'a ExprTuple) -> Self {
        AnyNodeRef::ExprTuple(node)
    }
}

impl<'a> From<&'a ExprSlice> for AnyNodeRef<'a> {
    fn from(node: &'a ExprSlice) -> Self {
        AnyNodeRef::ExprSlice(node)
    }
}

impl<'a> From<&'a ExceptHandlerExceptHandler> for AnyNodeRef<'a> {
    fn from(node: &'a ExceptHandlerExceptHandler) -> Self {
        AnyNodeRef::ExceptHandlerExceptHandler(node)
    }
}

impl<'a> From<&'a PatternMatchValue> for AnyNodeRef<'a> {
    fn from(node: &'a PatternMatchValue) -> Self {
        AnyNodeRef::PatternMatchValue(node)
    }
}

impl<'a> From<&'a PatternMatchSingleton> for AnyNodeRef<'a> {
    fn from(node: &'a PatternMatchSingleton) -> Self {
        AnyNodeRef::PatternMatchSingleton(node)
    }
}

impl<'a> From<&'a PatternMatchSequence> for AnyNodeRef<'a> {
    fn from(node: &'a PatternMatchSequence) -> Self {
        AnyNodeRef::PatternMatchSequence(node)
    }
}

impl<'a> From<&'a PatternMatchMapping> for AnyNodeRef<'a> {
    fn from(node: &'a PatternMatchMapping) -> Self {
        AnyNodeRef::PatternMatchMapping(node)
    }
}

impl<'a> From<&'a PatternMatchClass> for AnyNodeRef<'a> {
    fn from(node: &'a PatternMatchClass) -> Self {
        AnyNodeRef::PatternMatchClass(node)
    }
}

impl<'a> From<&'a PatternMatchStar> for AnyNodeRef<'a> {
    fn from(node: &'a PatternMatchStar) -> Self {
        AnyNodeRef::PatternMatchStar(node)
    }
}

impl<'a> From<&'a PatternMatchAs> for AnyNodeRef<'a> {
    fn from(node: &'a PatternMatchAs) -> Self {
        AnyNodeRef::PatternMatchAs(node)
    }
}

impl<'a> From<&'a PatternMatchOr> for AnyNodeRef<'a> {
    fn from(node: &'a PatternMatchOr) -> Self {
        AnyNodeRef::PatternMatchOr(node)
    }
}

impl<'a> From<&'a TypeIgnoreTypeIgnore> for AnyNodeRef<'a> {
    fn from(node: &'a TypeIgnoreTypeIgnore) -> Self {
        AnyNodeRef::TypeIgnoreTypeIgnore(node)
    }
}

impl<'a> From<&'a Decorator> for AnyNodeRef<'a> {
    fn from(node: &'a Decorator) -> Self {
        AnyNodeRef::Decorator(node)
    }
}

impl<'a> From<&'a Stmt> for AnyNodeRef<'a> {
    fn from(stmt: &'a Stmt) -> Self {
        match stmt {
            Stmt::FunctionDef(node) => AnyNodeRef::StmtFunctionDef(node),
            Stmt::AsyncFunctionDef(node) => AnyNodeRef::StmtAsyncFunctionDef(node),
            Stmt::ClassDef(node) => AnyNodeRef::StmtClassDef(node),
            Stmt::Return(node) => AnyNodeRef::StmtReturn(node),
            Stmt::Delete(node) => AnyNodeRef::StmtDelete(node),
            Stmt::Assign(node) => AnyNodeRef::StmtAssign(node),
            Stmt::AugAssign(node) => AnyNodeRef::StmtAugAssign(node),
            Stmt::AnnAssign(node) => AnyNodeRef::StmtAnnAssign(node),
            Stmt::For(node) => AnyNodeRef::StmtFor(node),
            Stmt::AsyncFor(node) => AnyNodeRef::StmtAsyncFor(node),
            Stmt::While(node) => AnyNodeRef::StmtWhile(node),
            Stmt::If(node) => AnyNodeRef::StmtIf(node),
            Stmt::With(node) => AnyNodeRef::StmtWith(node),
            Stmt::AsyncWith(node) => AnyNodeRef::StmtAsyncWith(node),
            Stmt::Match(node) => AnyNodeRef::StmtMatch(node),
            Stmt::Raise(node) => AnyNodeRef::StmtRaise(node),
            Stmt::Try(node) => AnyNodeRef::StmtTry(node),
            Stmt::TryStar(node) => AnyNodeRef::StmtTryStar(node),
            Stmt::Assert(node) => AnyNodeRef::StmtAssert(node),
            Stmt::Import(node) => AnyNodeRef::StmtImport(node),
            Stmt::ImportFrom(node) => AnyNodeRef::StmtImportFrom(node),
            Stmt::Global(node) => AnyNodeRef::StmtGlobal(node),
            Stmt::Nonlocal(node) => AnyNodeRef::StmtNonlocal(node),
            Stmt::Expr(node) => AnyNodeRef::StmtExpr(node),
            Stmt::Pass(node) => AnyNodeRef::StmtPass(node),
            Stmt::Break(node) => AnyNodeRef::StmtBreak(node),
            Stmt::Continue(node) => AnyNodeRef::StmtContinue(node),
        }
    }
}

impl<'a> From<&'a Expr> for AnyNodeRef<'a> {
    fn from(expr: &'a Expr) -> Self {
        match expr {
            Expr::BoolOp(node) => AnyNodeRef::ExprBoolOp(node),
            Expr::NamedExpr(node) => AnyNodeRef::ExprNamedExpr(node),
            Expr::BinOp(node) => AnyNodeRef::ExprBinOp(node),
            Expr::UnaryOp(node) => AnyNodeRef::ExprUnaryOp(node),
            Expr::Lambda(node) => AnyNodeRef::ExprLambda(node),
            Expr::IfExp(node) => AnyNodeRef::ExprIfExp(node),
            Expr::Dict(node) => AnyNodeRef::ExprDict(node),
            Expr::Set(node) => AnyNodeRef::ExprSet(node),
            Expr::ListComp(node) => AnyNodeRef::ExprListComp(node),
            Expr::SetComp(node) => AnyNodeRef::ExprSetComp(node),
            Expr::DictComp(node) => AnyNodeRef::ExprDictComp(node),
            Expr::GeneratorExp(node) => AnyNodeRef::ExprGeneratorExp(node),
            Expr::Await(node) => AnyNodeRef::ExprAwait(node),
            Expr::Yield(node) => AnyNodeRef::ExprYield(node),
            Expr::YieldFrom(node) => AnyNodeRef::ExprYieldFrom(node),
            Expr::Compare(node) => AnyNodeRef::ExprCompare(node),
            Expr::Call(node) => AnyNodeRef::ExprCall(node),
            Expr::FormattedValue(node) => AnyNodeRef::ExprFormattedValue(node),
            Expr::JoinedStr(node) => AnyNodeRef::ExprJoinedStr(node),
            Expr::Constant(node) => AnyNodeRef::ExprConstant(node),
            Expr::Attribute(node) => AnyNodeRef::ExprAttribute(node),
            Expr::Subscript(node) => AnyNodeRef::ExprSubscript(node),
            Expr::Starred(node) => AnyNodeRef::ExprStarred(node),
            Expr::Name(node) => AnyNodeRef::ExprName(node),
            Expr::List(node) => AnyNodeRef::ExprList(node),
            Expr::Tuple(node) => AnyNodeRef::ExprTuple(node),
            Expr::Slice(node) => AnyNodeRef::ExprSlice(node),
        }
    }
}

impl<'a> From<&'a Mod> for AnyNodeRef<'a> {
    fn from(module: &'a Mod) -> Self {
        match module {
            Mod::Module(node) => AnyNodeRef::ModModule(node),
            Mod::Interactive(node) => AnyNodeRef::ModInteractive(node),
            Mod::Expression(node) => AnyNodeRef::ModExpression(node),
            Mod::FunctionType(node) => AnyNodeRef::ModFunctionType(node),
        }
    }
}

impl<'a> From<&'a Pattern> for AnyNodeRef<'a> {
    fn from(pattern: &'a Pattern) -> Self {
        match pattern {
            Pattern::MatchValue(node) => AnyNodeRef::PatternMatchValue(node),
            Pattern::MatchSingleton(node) => AnyNodeRef::PatternMatchSingleton(node),
            Pattern::MatchSequence(node) => AnyNodeRef::PatternMatchSequence(node),
            Pattern::MatchMapping(node) => AnyNodeRef::PatternMatchMapping(node),
            Pattern::MatchClass(node) => AnyNodeRef::PatternMatchClass(node),
            Pattern::MatchStar(node) => AnyNodeRef::PatternMatchStar(node),
            Pattern::MatchAs(node) => AnyNodeRef::PatternMatchAs(node),
            Pattern::MatchOr(node) => AnyNodeRef::PatternMatchOr(node),
        }
    }
}

impl<'a> From<&'a ExceptHandler> for AnyNodeRef<'a> {
    fn from(handler: &'a ExceptHandler) -> Self {
        match handler {
            ExceptHandler::ExceptHandler(handler) => {
                AnyNodeRef::ExceptHandlerExceptHandler(handler)
            }
        }
    }
}

impl<'a> From<&'a TypeIgnore> for AnyNodeRef<'a> {
    fn from(ignore: &'a TypeIgnore) -> Self {
        match ignore {
            TypeIgnore::TypeIgnore(ignore) => AnyNodeRef::TypeIgnoreTypeIgnore(ignore),
        }
    }
}

impl<'a> From<&'a Comprehension> for AnyNodeRef<'a> {
    fn from(node: &'a Comprehension) -> Self {
        AnyNodeRef::Comprehension(node)
    }
}
impl<'a> From<&'a Arguments> for AnyNodeRef<'a> {
    fn from(node: &'a Arguments) -> Self {
        AnyNodeRef::Arguments(node)
    }
}
impl<'a> From<&'a Arg> for AnyNodeRef<'a> {
    fn from(node: &'a Arg) -> Self {
        AnyNodeRef::Arg(node)
    }
}
impl<'a> From<&'a ArgWithDefault> for AnyNodeRef<'a> {
    fn from(node: &'a ArgWithDefault) -> Self {
        AnyNodeRef::ArgWithDefault(node)
    }
}
impl<'a> From<&'a Keyword> for AnyNodeRef<'a> {
    fn from(node: &'a Keyword) -> Self {
        AnyNodeRef::Keyword(node)
    }
}
impl<'a> From<&'a Alias> for AnyNodeRef<'a> {
    fn from(node: &'a Alias) -> Self {
        AnyNodeRef::Alias(node)
    }
}
impl<'a> From<&'a WithItem> for AnyNodeRef<'a> {
    fn from(node: &'a WithItem) -> Self {
        AnyNodeRef::WithItem(node)
    }
}
impl<'a> From<&'a MatchCase> for AnyNodeRef<'a> {
    fn from(node: &'a MatchCase) -> Self {
        AnyNodeRef::MatchCase(node)
    }
}

impl Ranged for AnyNodeRef<'_> {
    fn range(&self) -> TextRange {
        match self {
            AnyNodeRef::ModModule(node) => node.range(),
            AnyNodeRef::ModInteractive(node) => node.range(),
            AnyNodeRef::ModExpression(node) => node.range(),
            AnyNodeRef::ModFunctionType(node) => node.range(),
            AnyNodeRef::StmtFunctionDef(node) => node.range(),
            AnyNodeRef::StmtAsyncFunctionDef(node) => node.range(),
            AnyNodeRef::StmtClassDef(node) => node.range(),
            AnyNodeRef::StmtReturn(node) => node.range(),
            AnyNodeRef::StmtDelete(node) => node.range(),
            AnyNodeRef::StmtAssign(node) => node.range(),
            AnyNodeRef::StmtAugAssign(node) => node.range(),
            AnyNodeRef::StmtAnnAssign(node) => node.range(),
            AnyNodeRef::StmtFor(node) => node.range(),
            AnyNodeRef::StmtAsyncFor(node) => node.range(),
            AnyNodeRef::StmtWhile(node) => node.range(),
            AnyNodeRef::StmtIf(node) => node.range(),
            AnyNodeRef::StmtWith(node) => node.range(),
            AnyNodeRef::StmtAsyncWith(node) => node.range(),
            AnyNodeRef::StmtMatch(node) => node.range(),
            AnyNodeRef::StmtRaise(node) => node.range(),
            AnyNodeRef::StmtTry(node) => node.range(),
            AnyNodeRef::StmtTryStar(node) => node.range(),
            AnyNodeRef::StmtAssert(node) => node.range(),
            AnyNodeRef::StmtImport(node) => node.range(),
            AnyNodeRef::StmtImportFrom(node) => node.range(),
            AnyNodeRef::StmtGlobal(node) => node.range(),
            AnyNodeRef::StmtNonlocal(node) => node.range(),
            AnyNodeRef::StmtExpr(node) => node.range(),
            AnyNodeRef::StmtPass(node) => node.range(),
            AnyNodeRef::StmtBreak(node) => node.range(),
            AnyNodeRef::StmtContinue(node) => node.range(),
            AnyNodeRef::ExprBoolOp(node) => node.range(),
            AnyNodeRef::ExprNamedExpr(node) => node.range(),
            AnyNodeRef::ExprBinOp(node) => node.range(),
            AnyNodeRef::ExprUnaryOp(node) => node.range(),
            AnyNodeRef::ExprLambda(node) => node.range(),
            AnyNodeRef::ExprIfExp(node) => node.range(),
            AnyNodeRef::ExprDict(node) => node.range(),
            AnyNodeRef::ExprSet(node) => node.range(),
            AnyNodeRef::ExprListComp(node) => node.range(),
            AnyNodeRef::ExprSetComp(node) => node.range(),
            AnyNodeRef::ExprDictComp(node) => node.range(),
            AnyNodeRef::ExprGeneratorExp(node) => node.range(),
            AnyNodeRef::ExprAwait(node) => node.range(),
            AnyNodeRef::ExprYield(node) => node.range(),
            AnyNodeRef::ExprYieldFrom(node) => node.range(),
            AnyNodeRef::ExprCompare(node) => node.range(),
            AnyNodeRef::ExprCall(node) => node.range(),
            AnyNodeRef::ExprFormattedValue(node) => node.range(),
            AnyNodeRef::ExprJoinedStr(node) => node.range(),
            AnyNodeRef::ExprConstant(node) => node.range(),
            AnyNodeRef::ExprAttribute(node) => node.range(),
            AnyNodeRef::ExprSubscript(node) => node.range(),
            AnyNodeRef::ExprStarred(node) => node.range(),
            AnyNodeRef::ExprName(node) => node.range(),
            AnyNodeRef::ExprList(node) => node.range(),
            AnyNodeRef::ExprTuple(node) => node.range(),
            AnyNodeRef::ExprSlice(node) => node.range(),
            AnyNodeRef::ExceptHandlerExceptHandler(node) => node.range(),
            AnyNodeRef::PatternMatchValue(node) => node.range(),
            AnyNodeRef::PatternMatchSingleton(node) => node.range(),
            AnyNodeRef::PatternMatchSequence(node) => node.range(),
            AnyNodeRef::PatternMatchMapping(node) => node.range(),
            AnyNodeRef::PatternMatchClass(node) => node.range(),
            AnyNodeRef::PatternMatchStar(node) => node.range(),
            AnyNodeRef::PatternMatchAs(node) => node.range(),
            AnyNodeRef::PatternMatchOr(node) => node.range(),
            AnyNodeRef::TypeIgnoreTypeIgnore(node) => node.range(),
            AnyNodeRef::Comprehension(node) => node.range(),
            AnyNodeRef::Arguments(node) => node.range(),
            AnyNodeRef::Arg(node) => node.range(),
            AnyNodeRef::ArgWithDefault(node) => node.range(),
            AnyNodeRef::Keyword(node) => node.range(),
            AnyNodeRef::Alias(node) => node.range(),
            AnyNodeRef::WithItem(node) => node.range(),
            AnyNodeRef::MatchCase(node) => node.range(),
            AnyNodeRef::Decorator(node) => node.range(),
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum NodeKind {
    ModModule,
    ModInteractive,
    ModExpression,
    ModFunctionType,
    StmtFunctionDef,
    StmtAsyncFunctionDef,
    StmtClassDef,
    StmtReturn,
    StmtDelete,
    StmtAssign,
    StmtAugAssign,
    StmtAnnAssign,
    StmtFor,
    StmtAsyncFor,
    StmtWhile,
    StmtIf,
    StmtWith,
    StmtAsyncWith,
    StmtMatch,
    StmtRaise,
    StmtTry,
    StmtTryStar,
    StmtAssert,
    StmtImport,
    StmtImportFrom,
    StmtGlobal,
    StmtNonlocal,
    StmtExpr,
    StmtPass,
    StmtBreak,
    StmtContinue,
    ExprBoolOp,
    ExprNamedExpr,
    ExprBinOp,
    ExprUnaryOp,
    ExprLambda,
    ExprIfExp,
    ExprDict,
    ExprSet,
    ExprListComp,
    ExprSetComp,
    ExprDictComp,
    ExprGeneratorExp,
    ExprAwait,
    ExprYield,
    ExprYieldFrom,
    ExprCompare,
    ExprCall,
    ExprFormattedValue,
    ExprJoinedStr,
    ExprConstant,
    ExprAttribute,
    ExprSubscript,
    ExprStarred,
    ExprName,
    ExprList,
    ExprTuple,
    ExprSlice,
    ExceptHandlerExceptHandler,
    PatternMatchValue,
    PatternMatchSingleton,
    PatternMatchSequence,
    PatternMatchMapping,
    PatternMatchClass,
    PatternMatchStar,
    PatternMatchAs,
    PatternMatchOr,
    TypeIgnoreTypeIgnore,
    Comprehension,
    Arguments,
    Arg,
    ArgWithDefault,
    Keyword,
    Alias,
    WithItem,
    MatchCase,
    Decorator,
}
