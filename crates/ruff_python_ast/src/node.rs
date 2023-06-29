use ruff_text_size::TextRange;
use rustpython_ast::{
    Alias, Arg, ArgWithDefault, Arguments, Comprehension, Decorator, ExceptHandler, Keyword,
    MatchCase, Mod, Pattern, Stmt, TypeIgnore, WithItem,
};
use rustpython_parser::ast::{self, Expr, Ranged};
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
    ModModule(ast::ModModule),
    ModInteractive(ast::ModInteractive),
    ModExpression(ast::ModExpression),
    ModFunctionType(ast::ModFunctionType),
    StmtFunctionDef(ast::StmtFunctionDef),
    StmtAsyncFunctionDef(ast::StmtAsyncFunctionDef),
    StmtClassDef(ast::StmtClassDef),
    StmtReturn(ast::StmtReturn),
    StmtDelete(ast::StmtDelete),
    StmtAssign(ast::StmtAssign),
    StmtAugAssign(ast::StmtAugAssign),
    StmtAnnAssign(ast::StmtAnnAssign),
    StmtFor(ast::StmtFor),
    StmtAsyncFor(ast::StmtAsyncFor),
    StmtWhile(ast::StmtWhile),
    StmtIf(ast::StmtIf),
    StmtWith(ast::StmtWith),
    StmtAsyncWith(ast::StmtAsyncWith),
    StmtMatch(ast::StmtMatch),
    StmtRaise(ast::StmtRaise),
    StmtTry(ast::StmtTry),
    StmtTryStar(ast::StmtTryStar),
    StmtAssert(ast::StmtAssert),
    StmtImport(ast::StmtImport),
    StmtImportFrom(ast::StmtImportFrom),
    StmtGlobal(ast::StmtGlobal),
    StmtNonlocal(ast::StmtNonlocal),
    StmtExpr(ast::StmtExpr),
    StmtPass(ast::StmtPass),
    StmtBreak(ast::StmtBreak),
    StmtContinue(ast::StmtContinue),
    ExprBoolOp(ast::ExprBoolOp),
    ExprNamedExpr(ast::ExprNamedExpr),
    ExprBinOp(ast::ExprBinOp),
    ExprUnaryOp(ast::ExprUnaryOp),
    ExprLambda(ast::ExprLambda),
    ExprIfExp(ast::ExprIfExp),
    ExprDict(ast::ExprDict),
    ExprSet(ast::ExprSet),
    ExprListComp(ast::ExprListComp),
    ExprSetComp(ast::ExprSetComp),
    ExprDictComp(ast::ExprDictComp),
    ExprGeneratorExp(ast::ExprGeneratorExp),
    ExprAwait(ast::ExprAwait),
    ExprYield(ast::ExprYield),
    ExprYieldFrom(ast::ExprYieldFrom),
    ExprCompare(ast::ExprCompare),
    ExprCall(ast::ExprCall),
    ExprFormattedValue(ast::ExprFormattedValue),
    ExprJoinedStr(ast::ExprJoinedStr),
    ExprConstant(ast::ExprConstant),
    ExprAttribute(ast::ExprAttribute),
    ExprSubscript(ast::ExprSubscript),
    ExprStarred(ast::ExprStarred),
    ExprName(ast::ExprName),
    ExprList(ast::ExprList),
    ExprTuple(ast::ExprTuple),
    ExprSlice(ast::ExprSlice),
    ExceptHandlerExceptHandler(ast::ExceptHandlerExceptHandler),
    PatternMatchValue(ast::PatternMatchValue),
    PatternMatchSingleton(ast::PatternMatchSingleton),
    PatternMatchSequence(ast::PatternMatchSequence),
    PatternMatchMapping(ast::PatternMatchMapping),
    PatternMatchClass(ast::PatternMatchClass),
    PatternMatchStar(ast::PatternMatchStar),
    PatternMatchAs(ast::PatternMatchAs),
    PatternMatchOr(ast::PatternMatchOr),
    TypeIgnoreTypeIgnore(ast::TypeIgnoreTypeIgnore),
    Comprehension(Comprehension),
    Arguments(Arguments),
    Arg(Arg),
    ArgWithDefault(ArgWithDefault),
    Keyword(Keyword),
    Alias(Alias),
    WithItem(WithItem),
    MatchCase(MatchCase),
    Decorator(Decorator),
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

impl AstNode for ast::ModModule {
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
impl AstNode for ast::ModInteractive {
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
impl AstNode for ast::ModExpression {
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
impl AstNode for ast::ModFunctionType {
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
impl AstNode for ast::StmtFunctionDef {
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
impl AstNode for ast::StmtAsyncFunctionDef {
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
impl AstNode for ast::StmtClassDef {
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
impl AstNode for ast::StmtReturn {
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
impl AstNode for ast::StmtDelete {
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
impl AstNode for ast::StmtAssign {
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
impl AstNode for ast::StmtAugAssign {
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
impl AstNode for ast::StmtAnnAssign {
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
impl AstNode for ast::StmtFor {
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
impl AstNode for ast::StmtAsyncFor {
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
impl AstNode for ast::StmtWhile {
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
impl AstNode for ast::StmtIf {
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
impl AstNode for ast::StmtWith {
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
impl AstNode for ast::StmtAsyncWith {
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
impl AstNode for ast::StmtMatch {
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
impl AstNode for ast::StmtRaise {
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
impl AstNode for ast::StmtTry {
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
impl AstNode for ast::StmtTryStar {
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
impl AstNode for ast::StmtAssert {
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
impl AstNode for ast::StmtImport {
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
impl AstNode for ast::StmtImportFrom {
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
impl AstNode for ast::StmtGlobal {
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
impl AstNode for ast::StmtNonlocal {
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
impl AstNode for ast::StmtExpr {
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
impl AstNode for ast::StmtPass {
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
impl AstNode for ast::StmtBreak {
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
impl AstNode for ast::StmtContinue {
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
impl AstNode for ast::ExprBoolOp {
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
impl AstNode for ast::ExprNamedExpr {
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
impl AstNode for ast::ExprBinOp {
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
impl AstNode for ast::ExprUnaryOp {
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
impl AstNode for ast::ExprLambda {
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
impl AstNode for ast::ExprIfExp {
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
impl AstNode for ast::ExprDict {
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
impl AstNode for ast::ExprSet {
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
impl AstNode for ast::ExprListComp {
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
impl AstNode for ast::ExprSetComp {
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
impl AstNode for ast::ExprDictComp {
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
impl AstNode for ast::ExprGeneratorExp {
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
impl AstNode for ast::ExprAwait {
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
impl AstNode for ast::ExprYield {
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
impl AstNode for ast::ExprYieldFrom {
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
impl AstNode for ast::ExprCompare {
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
impl AstNode for ast::ExprCall {
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
impl AstNode for ast::ExprFormattedValue {
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
impl AstNode for ast::ExprJoinedStr {
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
impl AstNode for ast::ExprConstant {
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
impl AstNode for ast::ExprAttribute {
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
impl AstNode for ast::ExprSubscript {
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
impl AstNode for ast::ExprStarred {
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
impl AstNode for ast::ExprName {
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
impl AstNode for ast::ExprList {
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
impl AstNode for ast::ExprTuple {
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
impl AstNode for ast::ExprSlice {
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
impl AstNode for ast::ExceptHandlerExceptHandler {
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
impl AstNode for ast::PatternMatchValue {
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
impl AstNode for ast::PatternMatchSingleton {
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
impl AstNode for ast::PatternMatchSequence {
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
impl AstNode for ast::PatternMatchMapping {
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
impl AstNode for ast::PatternMatchClass {
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
impl AstNode for ast::PatternMatchStar {
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
impl AstNode for ast::PatternMatchAs {
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
impl AstNode for ast::PatternMatchOr {
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
impl AstNode for ast::TypeIgnoreTypeIgnore {
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

impl AstNode for Comprehension {
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
impl AstNode for Arguments {
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
impl AstNode for Arg {
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
impl AstNode for ArgWithDefault {
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
impl AstNode for Keyword {
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
impl AstNode for Alias {
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
impl AstNode for WithItem {
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
impl AstNode for MatchCase {
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

impl AstNode for Decorator {
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

impl From<ast::ModModule> for AnyNode {
    fn from(node: ast::ModModule) -> Self {
        AnyNode::ModModule(node)
    }
}

impl From<ast::ModInteractive> for AnyNode {
    fn from(node: ast::ModInteractive) -> Self {
        AnyNode::ModInteractive(node)
    }
}

impl From<ast::ModExpression> for AnyNode {
    fn from(node: ast::ModExpression) -> Self {
        AnyNode::ModExpression(node)
    }
}

impl From<ast::ModFunctionType> for AnyNode {
    fn from(node: ast::ModFunctionType) -> Self {
        AnyNode::ModFunctionType(node)
    }
}

impl From<ast::StmtFunctionDef> for AnyNode {
    fn from(node: ast::StmtFunctionDef) -> Self {
        AnyNode::StmtFunctionDef(node)
    }
}

impl From<ast::StmtAsyncFunctionDef> for AnyNode {
    fn from(node: ast::StmtAsyncFunctionDef) -> Self {
        AnyNode::StmtAsyncFunctionDef(node)
    }
}

impl From<ast::StmtClassDef> for AnyNode {
    fn from(node: ast::StmtClassDef) -> Self {
        AnyNode::StmtClassDef(node)
    }
}

impl From<ast::StmtReturn> for AnyNode {
    fn from(node: ast::StmtReturn) -> Self {
        AnyNode::StmtReturn(node)
    }
}

impl From<ast::StmtDelete> for AnyNode {
    fn from(node: ast::StmtDelete) -> Self {
        AnyNode::StmtDelete(node)
    }
}

impl From<ast::StmtAssign> for AnyNode {
    fn from(node: ast::StmtAssign) -> Self {
        AnyNode::StmtAssign(node)
    }
}

impl From<ast::StmtAugAssign> for AnyNode {
    fn from(node: ast::StmtAugAssign) -> Self {
        AnyNode::StmtAugAssign(node)
    }
}

impl From<ast::StmtAnnAssign> for AnyNode {
    fn from(node: ast::StmtAnnAssign) -> Self {
        AnyNode::StmtAnnAssign(node)
    }
}

impl From<ast::StmtFor> for AnyNode {
    fn from(node: ast::StmtFor) -> Self {
        AnyNode::StmtFor(node)
    }
}

impl From<ast::StmtAsyncFor> for AnyNode {
    fn from(node: ast::StmtAsyncFor) -> Self {
        AnyNode::StmtAsyncFor(node)
    }
}

impl From<ast::StmtWhile> for AnyNode {
    fn from(node: ast::StmtWhile) -> Self {
        AnyNode::StmtWhile(node)
    }
}

impl From<ast::StmtIf> for AnyNode {
    fn from(node: ast::StmtIf) -> Self {
        AnyNode::StmtIf(node)
    }
}

impl From<ast::StmtWith> for AnyNode {
    fn from(node: ast::StmtWith) -> Self {
        AnyNode::StmtWith(node)
    }
}

impl From<ast::StmtAsyncWith> for AnyNode {
    fn from(node: ast::StmtAsyncWith) -> Self {
        AnyNode::StmtAsyncWith(node)
    }
}

impl From<ast::StmtMatch> for AnyNode {
    fn from(node: ast::StmtMatch) -> Self {
        AnyNode::StmtMatch(node)
    }
}

impl From<ast::StmtRaise> for AnyNode {
    fn from(node: ast::StmtRaise) -> Self {
        AnyNode::StmtRaise(node)
    }
}

impl From<ast::StmtTry> for AnyNode {
    fn from(node: ast::StmtTry) -> Self {
        AnyNode::StmtTry(node)
    }
}

impl From<ast::StmtTryStar> for AnyNode {
    fn from(node: ast::StmtTryStar) -> Self {
        AnyNode::StmtTryStar(node)
    }
}

impl From<ast::StmtAssert> for AnyNode {
    fn from(node: ast::StmtAssert) -> Self {
        AnyNode::StmtAssert(node)
    }
}

impl From<ast::StmtImport> for AnyNode {
    fn from(node: ast::StmtImport) -> Self {
        AnyNode::StmtImport(node)
    }
}

impl From<ast::StmtImportFrom> for AnyNode {
    fn from(node: ast::StmtImportFrom) -> Self {
        AnyNode::StmtImportFrom(node)
    }
}

impl From<ast::StmtGlobal> for AnyNode {
    fn from(node: ast::StmtGlobal) -> Self {
        AnyNode::StmtGlobal(node)
    }
}

impl From<ast::StmtNonlocal> for AnyNode {
    fn from(node: ast::StmtNonlocal) -> Self {
        AnyNode::StmtNonlocal(node)
    }
}

impl From<ast::StmtExpr> for AnyNode {
    fn from(node: ast::StmtExpr) -> Self {
        AnyNode::StmtExpr(node)
    }
}

impl From<ast::StmtPass> for AnyNode {
    fn from(node: ast::StmtPass) -> Self {
        AnyNode::StmtPass(node)
    }
}

impl From<ast::StmtBreak> for AnyNode {
    fn from(node: ast::StmtBreak) -> Self {
        AnyNode::StmtBreak(node)
    }
}

impl From<ast::StmtContinue> for AnyNode {
    fn from(node: ast::StmtContinue) -> Self {
        AnyNode::StmtContinue(node)
    }
}

impl From<ast::ExprBoolOp> for AnyNode {
    fn from(node: ast::ExprBoolOp) -> Self {
        AnyNode::ExprBoolOp(node)
    }
}

impl From<ast::ExprNamedExpr> for AnyNode {
    fn from(node: ast::ExprNamedExpr) -> Self {
        AnyNode::ExprNamedExpr(node)
    }
}

impl From<ast::ExprBinOp> for AnyNode {
    fn from(node: ast::ExprBinOp) -> Self {
        AnyNode::ExprBinOp(node)
    }
}

impl From<ast::ExprUnaryOp> for AnyNode {
    fn from(node: ast::ExprUnaryOp) -> Self {
        AnyNode::ExprUnaryOp(node)
    }
}

impl From<ast::ExprLambda> for AnyNode {
    fn from(node: ast::ExprLambda) -> Self {
        AnyNode::ExprLambda(node)
    }
}

impl From<ast::ExprIfExp> for AnyNode {
    fn from(node: ast::ExprIfExp) -> Self {
        AnyNode::ExprIfExp(node)
    }
}

impl From<ast::ExprDict> for AnyNode {
    fn from(node: ast::ExprDict) -> Self {
        AnyNode::ExprDict(node)
    }
}

impl From<ast::ExprSet> for AnyNode {
    fn from(node: ast::ExprSet) -> Self {
        AnyNode::ExprSet(node)
    }
}

impl From<ast::ExprListComp> for AnyNode {
    fn from(node: ast::ExprListComp) -> Self {
        AnyNode::ExprListComp(node)
    }
}

impl From<ast::ExprSetComp> for AnyNode {
    fn from(node: ast::ExprSetComp) -> Self {
        AnyNode::ExprSetComp(node)
    }
}

impl From<ast::ExprDictComp> for AnyNode {
    fn from(node: ast::ExprDictComp) -> Self {
        AnyNode::ExprDictComp(node)
    }
}

impl From<ast::ExprGeneratorExp> for AnyNode {
    fn from(node: ast::ExprGeneratorExp) -> Self {
        AnyNode::ExprGeneratorExp(node)
    }
}

impl From<ast::ExprAwait> for AnyNode {
    fn from(node: ast::ExprAwait) -> Self {
        AnyNode::ExprAwait(node)
    }
}

impl From<ast::ExprYield> for AnyNode {
    fn from(node: ast::ExprYield) -> Self {
        AnyNode::ExprYield(node)
    }
}

impl From<ast::ExprYieldFrom> for AnyNode {
    fn from(node: ast::ExprYieldFrom) -> Self {
        AnyNode::ExprYieldFrom(node)
    }
}

impl From<ast::ExprCompare> for AnyNode {
    fn from(node: ast::ExprCompare) -> Self {
        AnyNode::ExprCompare(node)
    }
}

impl From<ast::ExprCall> for AnyNode {
    fn from(node: ast::ExprCall) -> Self {
        AnyNode::ExprCall(node)
    }
}

impl From<ast::ExprFormattedValue> for AnyNode {
    fn from(node: ast::ExprFormattedValue) -> Self {
        AnyNode::ExprFormattedValue(node)
    }
}

impl From<ast::ExprJoinedStr> for AnyNode {
    fn from(node: ast::ExprJoinedStr) -> Self {
        AnyNode::ExprJoinedStr(node)
    }
}

impl From<ast::ExprConstant> for AnyNode {
    fn from(node: ast::ExprConstant) -> Self {
        AnyNode::ExprConstant(node)
    }
}

impl From<ast::ExprAttribute> for AnyNode {
    fn from(node: ast::ExprAttribute) -> Self {
        AnyNode::ExprAttribute(node)
    }
}

impl From<ast::ExprSubscript> for AnyNode {
    fn from(node: ast::ExprSubscript) -> Self {
        AnyNode::ExprSubscript(node)
    }
}

impl From<ast::ExprStarred> for AnyNode {
    fn from(node: ast::ExprStarred) -> Self {
        AnyNode::ExprStarred(node)
    }
}

impl From<ast::ExprName> for AnyNode {
    fn from(node: ast::ExprName) -> Self {
        AnyNode::ExprName(node)
    }
}

impl From<ast::ExprList> for AnyNode {
    fn from(node: ast::ExprList) -> Self {
        AnyNode::ExprList(node)
    }
}

impl From<ast::ExprTuple> for AnyNode {
    fn from(node: ast::ExprTuple) -> Self {
        AnyNode::ExprTuple(node)
    }
}

impl From<ast::ExprSlice> for AnyNode {
    fn from(node: ast::ExprSlice) -> Self {
        AnyNode::ExprSlice(node)
    }
}

impl From<ast::ExceptHandlerExceptHandler> for AnyNode {
    fn from(node: ast::ExceptHandlerExceptHandler) -> Self {
        AnyNode::ExceptHandlerExceptHandler(node)
    }
}

impl From<ast::PatternMatchValue> for AnyNode {
    fn from(node: ast::PatternMatchValue) -> Self {
        AnyNode::PatternMatchValue(node)
    }
}

impl From<ast::PatternMatchSingleton> for AnyNode {
    fn from(node: ast::PatternMatchSingleton) -> Self {
        AnyNode::PatternMatchSingleton(node)
    }
}

impl From<ast::PatternMatchSequence> for AnyNode {
    fn from(node: ast::PatternMatchSequence) -> Self {
        AnyNode::PatternMatchSequence(node)
    }
}

impl From<ast::PatternMatchMapping> for AnyNode {
    fn from(node: ast::PatternMatchMapping) -> Self {
        AnyNode::PatternMatchMapping(node)
    }
}

impl From<ast::PatternMatchClass> for AnyNode {
    fn from(node: ast::PatternMatchClass) -> Self {
        AnyNode::PatternMatchClass(node)
    }
}

impl From<ast::PatternMatchStar> for AnyNode {
    fn from(node: ast::PatternMatchStar) -> Self {
        AnyNode::PatternMatchStar(node)
    }
}

impl From<ast::PatternMatchAs> for AnyNode {
    fn from(node: ast::PatternMatchAs) -> Self {
        AnyNode::PatternMatchAs(node)
    }
}

impl From<ast::PatternMatchOr> for AnyNode {
    fn from(node: ast::PatternMatchOr) -> Self {
        AnyNode::PatternMatchOr(node)
    }
}

impl From<ast::TypeIgnoreTypeIgnore> for AnyNode {
    fn from(node: ast::TypeIgnoreTypeIgnore) -> Self {
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
    ModModule(&'a ast::ModModule),
    ModInteractive(&'a ast::ModInteractive),
    ModExpression(&'a ast::ModExpression),
    ModFunctionType(&'a ast::ModFunctionType),
    StmtFunctionDef(&'a ast::StmtFunctionDef),
    StmtAsyncFunctionDef(&'a ast::StmtAsyncFunctionDef),
    StmtClassDef(&'a ast::StmtClassDef),
    StmtReturn(&'a ast::StmtReturn),
    StmtDelete(&'a ast::StmtDelete),
    StmtAssign(&'a ast::StmtAssign),
    StmtAugAssign(&'a ast::StmtAugAssign),
    StmtAnnAssign(&'a ast::StmtAnnAssign),
    StmtFor(&'a ast::StmtFor),
    StmtAsyncFor(&'a ast::StmtAsyncFor),
    StmtWhile(&'a ast::StmtWhile),
    StmtIf(&'a ast::StmtIf),
    StmtWith(&'a ast::StmtWith),
    StmtAsyncWith(&'a ast::StmtAsyncWith),
    StmtMatch(&'a ast::StmtMatch),
    StmtRaise(&'a ast::StmtRaise),
    StmtTry(&'a ast::StmtTry),
    StmtTryStar(&'a ast::StmtTryStar),
    StmtAssert(&'a ast::StmtAssert),
    StmtImport(&'a ast::StmtImport),
    StmtImportFrom(&'a ast::StmtImportFrom),
    StmtGlobal(&'a ast::StmtGlobal),
    StmtNonlocal(&'a ast::StmtNonlocal),
    StmtExpr(&'a ast::StmtExpr),
    StmtPass(&'a ast::StmtPass),
    StmtBreak(&'a ast::StmtBreak),
    StmtContinue(&'a ast::StmtContinue),
    ExprBoolOp(&'a ast::ExprBoolOp),
    ExprNamedExpr(&'a ast::ExprNamedExpr),
    ExprBinOp(&'a ast::ExprBinOp),
    ExprUnaryOp(&'a ast::ExprUnaryOp),
    ExprLambda(&'a ast::ExprLambda),
    ExprIfExp(&'a ast::ExprIfExp),
    ExprDict(&'a ast::ExprDict),
    ExprSet(&'a ast::ExprSet),
    ExprListComp(&'a ast::ExprListComp),
    ExprSetComp(&'a ast::ExprSetComp),
    ExprDictComp(&'a ast::ExprDictComp),
    ExprGeneratorExp(&'a ast::ExprGeneratorExp),
    ExprAwait(&'a ast::ExprAwait),
    ExprYield(&'a ast::ExprYield),
    ExprYieldFrom(&'a ast::ExprYieldFrom),
    ExprCompare(&'a ast::ExprCompare),
    ExprCall(&'a ast::ExprCall),
    ExprFormattedValue(&'a ast::ExprFormattedValue),
    ExprJoinedStr(&'a ast::ExprJoinedStr),
    ExprConstant(&'a ast::ExprConstant),
    ExprAttribute(&'a ast::ExprAttribute),
    ExprSubscript(&'a ast::ExprSubscript),
    ExprStarred(&'a ast::ExprStarred),
    ExprName(&'a ast::ExprName),
    ExprList(&'a ast::ExprList),
    ExprTuple(&'a ast::ExprTuple),
    ExprSlice(&'a ast::ExprSlice),
    ExceptHandlerExceptHandler(&'a ast::ExceptHandlerExceptHandler),
    PatternMatchValue(&'a ast::PatternMatchValue),
    PatternMatchSingleton(&'a ast::PatternMatchSingleton),
    PatternMatchSequence(&'a ast::PatternMatchSequence),
    PatternMatchMapping(&'a ast::PatternMatchMapping),
    PatternMatchClass(&'a ast::PatternMatchClass),
    PatternMatchStar(&'a ast::PatternMatchStar),
    PatternMatchAs(&'a ast::PatternMatchAs),
    PatternMatchOr(&'a ast::PatternMatchOr),
    TypeIgnoreTypeIgnore(&'a ast::TypeIgnoreTypeIgnore),
    Comprehension(&'a Comprehension),
    Arguments(&'a Arguments),
    Arg(&'a Arg),
    ArgWithDefault(&'a ArgWithDefault),
    Keyword(&'a Keyword),
    Alias(&'a Alias),
    WithItem(&'a WithItem),
    MatchCase(&'a MatchCase),
    Decorator(&'a Decorator),
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
        self.as_ptr().eq(&other.as_ptr()) && self.kind() == other.kind()
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

impl<'a> From<&'a ast::ModModule> for AnyNodeRef<'a> {
    fn from(node: &'a ast::ModModule) -> Self {
        AnyNodeRef::ModModule(node)
    }
}

impl<'a> From<&'a ast::ModInteractive> for AnyNodeRef<'a> {
    fn from(node: &'a ast::ModInteractive) -> Self {
        AnyNodeRef::ModInteractive(node)
    }
}

impl<'a> From<&'a ast::ModExpression> for AnyNodeRef<'a> {
    fn from(node: &'a ast::ModExpression) -> Self {
        AnyNodeRef::ModExpression(node)
    }
}

impl<'a> From<&'a ast::ModFunctionType> for AnyNodeRef<'a> {
    fn from(node: &'a ast::ModFunctionType) -> Self {
        AnyNodeRef::ModFunctionType(node)
    }
}

impl<'a> From<&'a ast::StmtFunctionDef> for AnyNodeRef<'a> {
    fn from(node: &'a ast::StmtFunctionDef) -> Self {
        AnyNodeRef::StmtFunctionDef(node)
    }
}

impl<'a> From<&'a ast::StmtAsyncFunctionDef> for AnyNodeRef<'a> {
    fn from(node: &'a ast::StmtAsyncFunctionDef) -> Self {
        AnyNodeRef::StmtAsyncFunctionDef(node)
    }
}

impl<'a> From<&'a ast::StmtClassDef> for AnyNodeRef<'a> {
    fn from(node: &'a ast::StmtClassDef) -> Self {
        AnyNodeRef::StmtClassDef(node)
    }
}

impl<'a> From<&'a ast::StmtReturn> for AnyNodeRef<'a> {
    fn from(node: &'a ast::StmtReturn) -> Self {
        AnyNodeRef::StmtReturn(node)
    }
}

impl<'a> From<&'a ast::StmtDelete> for AnyNodeRef<'a> {
    fn from(node: &'a ast::StmtDelete) -> Self {
        AnyNodeRef::StmtDelete(node)
    }
}

impl<'a> From<&'a ast::StmtAssign> for AnyNodeRef<'a> {
    fn from(node: &'a ast::StmtAssign) -> Self {
        AnyNodeRef::StmtAssign(node)
    }
}

impl<'a> From<&'a ast::StmtAugAssign> for AnyNodeRef<'a> {
    fn from(node: &'a ast::StmtAugAssign) -> Self {
        AnyNodeRef::StmtAugAssign(node)
    }
}

impl<'a> From<&'a ast::StmtAnnAssign> for AnyNodeRef<'a> {
    fn from(node: &'a ast::StmtAnnAssign) -> Self {
        AnyNodeRef::StmtAnnAssign(node)
    }
}

impl<'a> From<&'a ast::StmtFor> for AnyNodeRef<'a> {
    fn from(node: &'a ast::StmtFor) -> Self {
        AnyNodeRef::StmtFor(node)
    }
}

impl<'a> From<&'a ast::StmtAsyncFor> for AnyNodeRef<'a> {
    fn from(node: &'a ast::StmtAsyncFor) -> Self {
        AnyNodeRef::StmtAsyncFor(node)
    }
}

impl<'a> From<&'a ast::StmtWhile> for AnyNodeRef<'a> {
    fn from(node: &'a ast::StmtWhile) -> Self {
        AnyNodeRef::StmtWhile(node)
    }
}

impl<'a> From<&'a ast::StmtIf> for AnyNodeRef<'a> {
    fn from(node: &'a ast::StmtIf) -> Self {
        AnyNodeRef::StmtIf(node)
    }
}

impl<'a> From<&'a ast::StmtWith> for AnyNodeRef<'a> {
    fn from(node: &'a ast::StmtWith) -> Self {
        AnyNodeRef::StmtWith(node)
    }
}

impl<'a> From<&'a ast::StmtAsyncWith> for AnyNodeRef<'a> {
    fn from(node: &'a ast::StmtAsyncWith) -> Self {
        AnyNodeRef::StmtAsyncWith(node)
    }
}

impl<'a> From<&'a ast::StmtMatch> for AnyNodeRef<'a> {
    fn from(node: &'a ast::StmtMatch) -> Self {
        AnyNodeRef::StmtMatch(node)
    }
}

impl<'a> From<&'a ast::StmtRaise> for AnyNodeRef<'a> {
    fn from(node: &'a ast::StmtRaise) -> Self {
        AnyNodeRef::StmtRaise(node)
    }
}

impl<'a> From<&'a ast::StmtTry> for AnyNodeRef<'a> {
    fn from(node: &'a ast::StmtTry) -> Self {
        AnyNodeRef::StmtTry(node)
    }
}

impl<'a> From<&'a ast::StmtTryStar> for AnyNodeRef<'a> {
    fn from(node: &'a ast::StmtTryStar) -> Self {
        AnyNodeRef::StmtTryStar(node)
    }
}

impl<'a> From<&'a ast::StmtAssert> for AnyNodeRef<'a> {
    fn from(node: &'a ast::StmtAssert) -> Self {
        AnyNodeRef::StmtAssert(node)
    }
}

impl<'a> From<&'a ast::StmtImport> for AnyNodeRef<'a> {
    fn from(node: &'a ast::StmtImport) -> Self {
        AnyNodeRef::StmtImport(node)
    }
}

impl<'a> From<&'a ast::StmtImportFrom> for AnyNodeRef<'a> {
    fn from(node: &'a ast::StmtImportFrom) -> Self {
        AnyNodeRef::StmtImportFrom(node)
    }
}

impl<'a> From<&'a ast::StmtGlobal> for AnyNodeRef<'a> {
    fn from(node: &'a ast::StmtGlobal) -> Self {
        AnyNodeRef::StmtGlobal(node)
    }
}

impl<'a> From<&'a ast::StmtNonlocal> for AnyNodeRef<'a> {
    fn from(node: &'a ast::StmtNonlocal) -> Self {
        AnyNodeRef::StmtNonlocal(node)
    }
}

impl<'a> From<&'a ast::StmtExpr> for AnyNodeRef<'a> {
    fn from(node: &'a ast::StmtExpr) -> Self {
        AnyNodeRef::StmtExpr(node)
    }
}

impl<'a> From<&'a ast::StmtPass> for AnyNodeRef<'a> {
    fn from(node: &'a ast::StmtPass) -> Self {
        AnyNodeRef::StmtPass(node)
    }
}

impl<'a> From<&'a ast::StmtBreak> for AnyNodeRef<'a> {
    fn from(node: &'a ast::StmtBreak) -> Self {
        AnyNodeRef::StmtBreak(node)
    }
}

impl<'a> From<&'a ast::StmtContinue> for AnyNodeRef<'a> {
    fn from(node: &'a ast::StmtContinue) -> Self {
        AnyNodeRef::StmtContinue(node)
    }
}

impl<'a> From<&'a ast::ExprBoolOp> for AnyNodeRef<'a> {
    fn from(node: &'a ast::ExprBoolOp) -> Self {
        AnyNodeRef::ExprBoolOp(node)
    }
}

impl<'a> From<&'a ast::ExprNamedExpr> for AnyNodeRef<'a> {
    fn from(node: &'a ast::ExprNamedExpr) -> Self {
        AnyNodeRef::ExprNamedExpr(node)
    }
}

impl<'a> From<&'a ast::ExprBinOp> for AnyNodeRef<'a> {
    fn from(node: &'a ast::ExprBinOp) -> Self {
        AnyNodeRef::ExprBinOp(node)
    }
}

impl<'a> From<&'a ast::ExprUnaryOp> for AnyNodeRef<'a> {
    fn from(node: &'a ast::ExprUnaryOp) -> Self {
        AnyNodeRef::ExprUnaryOp(node)
    }
}

impl<'a> From<&'a ast::ExprLambda> for AnyNodeRef<'a> {
    fn from(node: &'a ast::ExprLambda) -> Self {
        AnyNodeRef::ExprLambda(node)
    }
}

impl<'a> From<&'a ast::ExprIfExp> for AnyNodeRef<'a> {
    fn from(node: &'a ast::ExprIfExp) -> Self {
        AnyNodeRef::ExprIfExp(node)
    }
}

impl<'a> From<&'a ast::ExprDict> for AnyNodeRef<'a> {
    fn from(node: &'a ast::ExprDict) -> Self {
        AnyNodeRef::ExprDict(node)
    }
}

impl<'a> From<&'a ast::ExprSet> for AnyNodeRef<'a> {
    fn from(node: &'a ast::ExprSet) -> Self {
        AnyNodeRef::ExprSet(node)
    }
}

impl<'a> From<&'a ast::ExprListComp> for AnyNodeRef<'a> {
    fn from(node: &'a ast::ExprListComp) -> Self {
        AnyNodeRef::ExprListComp(node)
    }
}

impl<'a> From<&'a ast::ExprSetComp> for AnyNodeRef<'a> {
    fn from(node: &'a ast::ExprSetComp) -> Self {
        AnyNodeRef::ExprSetComp(node)
    }
}

impl<'a> From<&'a ast::ExprDictComp> for AnyNodeRef<'a> {
    fn from(node: &'a ast::ExprDictComp) -> Self {
        AnyNodeRef::ExprDictComp(node)
    }
}

impl<'a> From<&'a ast::ExprGeneratorExp> for AnyNodeRef<'a> {
    fn from(node: &'a ast::ExprGeneratorExp) -> Self {
        AnyNodeRef::ExprGeneratorExp(node)
    }
}

impl<'a> From<&'a ast::ExprAwait> for AnyNodeRef<'a> {
    fn from(node: &'a ast::ExprAwait) -> Self {
        AnyNodeRef::ExprAwait(node)
    }
}

impl<'a> From<&'a ast::ExprYield> for AnyNodeRef<'a> {
    fn from(node: &'a ast::ExprYield) -> Self {
        AnyNodeRef::ExprYield(node)
    }
}

impl<'a> From<&'a ast::ExprYieldFrom> for AnyNodeRef<'a> {
    fn from(node: &'a ast::ExprYieldFrom) -> Self {
        AnyNodeRef::ExprYieldFrom(node)
    }
}

impl<'a> From<&'a ast::ExprCompare> for AnyNodeRef<'a> {
    fn from(node: &'a ast::ExprCompare) -> Self {
        AnyNodeRef::ExprCompare(node)
    }
}

impl<'a> From<&'a ast::ExprCall> for AnyNodeRef<'a> {
    fn from(node: &'a ast::ExprCall) -> Self {
        AnyNodeRef::ExprCall(node)
    }
}

impl<'a> From<&'a ast::ExprFormattedValue> for AnyNodeRef<'a> {
    fn from(node: &'a ast::ExprFormattedValue) -> Self {
        AnyNodeRef::ExprFormattedValue(node)
    }
}

impl<'a> From<&'a ast::ExprJoinedStr> for AnyNodeRef<'a> {
    fn from(node: &'a ast::ExprJoinedStr) -> Self {
        AnyNodeRef::ExprJoinedStr(node)
    }
}

impl<'a> From<&'a ast::ExprConstant> for AnyNodeRef<'a> {
    fn from(node: &'a ast::ExprConstant) -> Self {
        AnyNodeRef::ExprConstant(node)
    }
}

impl<'a> From<&'a ast::ExprAttribute> for AnyNodeRef<'a> {
    fn from(node: &'a ast::ExprAttribute) -> Self {
        AnyNodeRef::ExprAttribute(node)
    }
}

impl<'a> From<&'a ast::ExprSubscript> for AnyNodeRef<'a> {
    fn from(node: &'a ast::ExprSubscript) -> Self {
        AnyNodeRef::ExprSubscript(node)
    }
}

impl<'a> From<&'a ast::ExprStarred> for AnyNodeRef<'a> {
    fn from(node: &'a ast::ExprStarred) -> Self {
        AnyNodeRef::ExprStarred(node)
    }
}

impl<'a> From<&'a ast::ExprName> for AnyNodeRef<'a> {
    fn from(node: &'a ast::ExprName) -> Self {
        AnyNodeRef::ExprName(node)
    }
}

impl<'a> From<&'a ast::ExprList> for AnyNodeRef<'a> {
    fn from(node: &'a ast::ExprList) -> Self {
        AnyNodeRef::ExprList(node)
    }
}

impl<'a> From<&'a ast::ExprTuple> for AnyNodeRef<'a> {
    fn from(node: &'a ast::ExprTuple) -> Self {
        AnyNodeRef::ExprTuple(node)
    }
}

impl<'a> From<&'a ast::ExprSlice> for AnyNodeRef<'a> {
    fn from(node: &'a ast::ExprSlice) -> Self {
        AnyNodeRef::ExprSlice(node)
    }
}

impl<'a> From<&'a ast::ExceptHandlerExceptHandler> for AnyNodeRef<'a> {
    fn from(node: &'a ast::ExceptHandlerExceptHandler) -> Self {
        AnyNodeRef::ExceptHandlerExceptHandler(node)
    }
}

impl<'a> From<&'a ast::PatternMatchValue> for AnyNodeRef<'a> {
    fn from(node: &'a ast::PatternMatchValue) -> Self {
        AnyNodeRef::PatternMatchValue(node)
    }
}

impl<'a> From<&'a ast::PatternMatchSingleton> for AnyNodeRef<'a> {
    fn from(node: &'a ast::PatternMatchSingleton) -> Self {
        AnyNodeRef::PatternMatchSingleton(node)
    }
}

impl<'a> From<&'a ast::PatternMatchSequence> for AnyNodeRef<'a> {
    fn from(node: &'a ast::PatternMatchSequence) -> Self {
        AnyNodeRef::PatternMatchSequence(node)
    }
}

impl<'a> From<&'a ast::PatternMatchMapping> for AnyNodeRef<'a> {
    fn from(node: &'a ast::PatternMatchMapping) -> Self {
        AnyNodeRef::PatternMatchMapping(node)
    }
}

impl<'a> From<&'a ast::PatternMatchClass> for AnyNodeRef<'a> {
    fn from(node: &'a ast::PatternMatchClass) -> Self {
        AnyNodeRef::PatternMatchClass(node)
    }
}

impl<'a> From<&'a ast::PatternMatchStar> for AnyNodeRef<'a> {
    fn from(node: &'a ast::PatternMatchStar) -> Self {
        AnyNodeRef::PatternMatchStar(node)
    }
}

impl<'a> From<&'a ast::PatternMatchAs> for AnyNodeRef<'a> {
    fn from(node: &'a ast::PatternMatchAs) -> Self {
        AnyNodeRef::PatternMatchAs(node)
    }
}

impl<'a> From<&'a ast::PatternMatchOr> for AnyNodeRef<'a> {
    fn from(node: &'a ast::PatternMatchOr) -> Self {
        AnyNodeRef::PatternMatchOr(node)
    }
}

impl<'a> From<&'a ast::TypeIgnoreTypeIgnore> for AnyNodeRef<'a> {
    fn from(node: &'a ast::TypeIgnoreTypeIgnore) -> Self {
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
