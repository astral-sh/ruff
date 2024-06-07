use crate::visitor::source_order::SourceOrderVisitor;
use crate::{
    self as ast, Alias, AnyParameterRef, ArgOrKeyword, Arguments, Comprehension, Decorator,
    ExceptHandler, Expr, FStringElement, Keyword, MatchCase, Mod, Parameter, ParameterWithDefault,
    Parameters, Pattern, PatternArguments, PatternKeyword, Stmt, StmtAnnAssign, StmtAssert,
    StmtAssign, StmtAugAssign, StmtBreak, StmtClassDef, StmtContinue, StmtDelete, StmtExpr,
    StmtFor, StmtFunctionDef, StmtGlobal, StmtIf, StmtImport, StmtImportFrom, StmtIpyEscapeCommand,
    StmtMatch, StmtNonlocal, StmtPass, StmtRaise, StmtReturn, StmtTry, StmtTypeAlias, StmtWhile,
    StmtWith, TypeParam, TypeParamParamSpec, TypeParamTypeVar, TypeParamTypeVarTuple, TypeParams,
    WithItem,
};
use ruff_text_size::{Ranged, TextRange};
use std::ptr::NonNull;

pub trait AstNode: Ranged {
    type Ref<'a>;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized;
    fn cast_ref(kind: AnyNodeRef<'_>) -> Option<Self::Ref<'_>>;

    fn can_cast(kind: NodeKind) -> bool;

    /// Returns the [`AnyNodeRef`] referencing this node.
    fn as_any_node_ref(&self) -> AnyNodeRef;

    /// Consumes `self` and returns its [`AnyNode`] representation.
    fn into_any_node(self) -> AnyNode;

    fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized;
}

#[derive(Clone, Debug, is_macro::Is, PartialEq)]
pub enum AnyNode {
    ModModule(ast::ModModule),
    ModExpression(ast::ModExpression),
    StmtFunctionDef(ast::StmtFunctionDef),
    StmtClassDef(ast::StmtClassDef),
    StmtReturn(ast::StmtReturn),
    StmtDelete(ast::StmtDelete),
    StmtTypeAlias(ast::StmtTypeAlias),
    StmtAssign(ast::StmtAssign),
    StmtAugAssign(ast::StmtAugAssign),
    StmtAnnAssign(ast::StmtAnnAssign),
    StmtFor(ast::StmtFor),
    StmtWhile(ast::StmtWhile),
    StmtIf(ast::StmtIf),
    StmtWith(ast::StmtWith),
    StmtMatch(ast::StmtMatch),
    StmtRaise(ast::StmtRaise),
    StmtTry(ast::StmtTry),
    StmtAssert(ast::StmtAssert),
    StmtImport(ast::StmtImport),
    StmtImportFrom(ast::StmtImportFrom),
    StmtGlobal(ast::StmtGlobal),
    StmtNonlocal(ast::StmtNonlocal),
    StmtExpr(ast::StmtExpr),
    StmtPass(ast::StmtPass),
    StmtBreak(ast::StmtBreak),
    StmtContinue(ast::StmtContinue),
    StmtIpyEscapeCommand(ast::StmtIpyEscapeCommand),
    ExprBoolOp(ast::ExprBoolOp),
    ExprNamed(ast::ExprNamed),
    ExprBinOp(ast::ExprBinOp),
    ExprUnaryOp(ast::ExprUnaryOp),
    ExprLambda(ast::ExprLambda),
    ExprIf(ast::ExprIf),
    ExprDict(ast::ExprDict),
    ExprSet(ast::ExprSet),
    ExprListComp(ast::ExprListComp),
    ExprSetComp(ast::ExprSetComp),
    ExprDictComp(ast::ExprDictComp),
    ExprGenerator(ast::ExprGenerator),
    ExprAwait(ast::ExprAwait),
    ExprYield(ast::ExprYield),
    ExprYieldFrom(ast::ExprYieldFrom),
    ExprCompare(ast::ExprCompare),
    ExprCall(ast::ExprCall),
    ExprFString(ast::ExprFString),
    ExprStringLiteral(ast::ExprStringLiteral),
    ExprBytesLiteral(ast::ExprBytesLiteral),
    ExprNumberLiteral(ast::ExprNumberLiteral),
    ExprBooleanLiteral(ast::ExprBooleanLiteral),
    ExprNoneLiteral(ast::ExprNoneLiteral),
    ExprEllipsisLiteral(ast::ExprEllipsisLiteral),
    ExprAttribute(ast::ExprAttribute),
    ExprSubscript(ast::ExprSubscript),
    ExprStarred(ast::ExprStarred),
    ExprName(ast::ExprName),
    ExprList(ast::ExprList),
    ExprTuple(ast::ExprTuple),
    ExprSlice(ast::ExprSlice),
    ExprIpyEscapeCommand(ast::ExprIpyEscapeCommand),
    ExceptHandlerExceptHandler(ast::ExceptHandlerExceptHandler),
    FStringExpressionElement(ast::FStringExpressionElement),
    FStringLiteralElement(ast::FStringLiteralElement),
    FStringFormatSpec(ast::FStringFormatSpec),
    PatternMatchValue(ast::PatternMatchValue),
    PatternMatchSingleton(ast::PatternMatchSingleton),
    PatternMatchSequence(ast::PatternMatchSequence),
    PatternMatchMapping(ast::PatternMatchMapping),
    PatternMatchClass(ast::PatternMatchClass),
    PatternMatchStar(ast::PatternMatchStar),
    PatternMatchAs(ast::PatternMatchAs),
    PatternMatchOr(ast::PatternMatchOr),
    PatternArguments(PatternArguments),
    PatternKeyword(PatternKeyword),
    Comprehension(Comprehension),
    Arguments(Arguments),
    Parameters(Parameters),
    Parameter(Parameter),
    ParameterWithDefault(ParameterWithDefault),
    Keyword(Keyword),
    Alias(Alias),
    WithItem(WithItem),
    MatchCase(MatchCase),
    Decorator(Decorator),
    ElifElseClause(ast::ElifElseClause),
    TypeParams(TypeParams),
    TypeParamTypeVar(TypeParamTypeVar),
    TypeParamTypeVarTuple(TypeParamTypeVarTuple),
    TypeParamParamSpec(TypeParamParamSpec),
    FString(ast::FString),
    StringLiteral(ast::StringLiteral),
    BytesLiteral(ast::BytesLiteral),
}

impl AnyNode {
    pub fn statement(self) -> Option<Stmt> {
        Stmt::cast(self)
    }

    pub fn expression(self) -> Option<Expr> {
        match self {
            AnyNode::ExprBoolOp(node) => Some(Expr::BoolOp(node)),
            AnyNode::ExprNamed(node) => Some(Expr::Named(node)),
            AnyNode::ExprBinOp(node) => Some(Expr::BinOp(node)),
            AnyNode::ExprUnaryOp(node) => Some(Expr::UnaryOp(node)),
            AnyNode::ExprLambda(node) => Some(Expr::Lambda(node)),
            AnyNode::ExprIf(node) => Some(Expr::If(node)),
            AnyNode::ExprDict(node) => Some(Expr::Dict(node)),
            AnyNode::ExprSet(node) => Some(Expr::Set(node)),
            AnyNode::ExprListComp(node) => Some(Expr::ListComp(node)),
            AnyNode::ExprSetComp(node) => Some(Expr::SetComp(node)),
            AnyNode::ExprDictComp(node) => Some(Expr::DictComp(node)),
            AnyNode::ExprGenerator(node) => Some(Expr::Generator(node)),
            AnyNode::ExprAwait(node) => Some(Expr::Await(node)),
            AnyNode::ExprYield(node) => Some(Expr::Yield(node)),
            AnyNode::ExprYieldFrom(node) => Some(Expr::YieldFrom(node)),
            AnyNode::ExprCompare(node) => Some(Expr::Compare(node)),
            AnyNode::ExprCall(node) => Some(Expr::Call(node)),
            AnyNode::ExprFString(node) => Some(Expr::FString(node)),
            AnyNode::ExprStringLiteral(node) => Some(Expr::StringLiteral(node)),
            AnyNode::ExprBytesLiteral(node) => Some(Expr::BytesLiteral(node)),
            AnyNode::ExprNumberLiteral(node) => Some(Expr::NumberLiteral(node)),
            AnyNode::ExprBooleanLiteral(node) => Some(Expr::BooleanLiteral(node)),
            AnyNode::ExprNoneLiteral(node) => Some(Expr::NoneLiteral(node)),
            AnyNode::ExprEllipsisLiteral(node) => Some(Expr::EllipsisLiteral(node)),
            AnyNode::ExprAttribute(node) => Some(Expr::Attribute(node)),
            AnyNode::ExprSubscript(node) => Some(Expr::Subscript(node)),
            AnyNode::ExprStarred(node) => Some(Expr::Starred(node)),
            AnyNode::ExprName(node) => Some(Expr::Name(node)),
            AnyNode::ExprList(node) => Some(Expr::List(node)),
            AnyNode::ExprTuple(node) => Some(Expr::Tuple(node)),
            AnyNode::ExprSlice(node) => Some(Expr::Slice(node)),
            AnyNode::ExprIpyEscapeCommand(node) => Some(Expr::IpyEscapeCommand(node)),

            AnyNode::ModModule(_)
            | AnyNode::ModExpression(_)
            | AnyNode::StmtFunctionDef(_)
            | AnyNode::StmtClassDef(_)
            | AnyNode::StmtReturn(_)
            | AnyNode::StmtDelete(_)
            | AnyNode::StmtTypeAlias(_)
            | AnyNode::StmtAssign(_)
            | AnyNode::StmtAugAssign(_)
            | AnyNode::StmtAnnAssign(_)
            | AnyNode::StmtFor(_)
            | AnyNode::StmtWhile(_)
            | AnyNode::StmtIf(_)
            | AnyNode::StmtWith(_)
            | AnyNode::StmtMatch(_)
            | AnyNode::StmtRaise(_)
            | AnyNode::StmtTry(_)
            | AnyNode::StmtAssert(_)
            | AnyNode::StmtImport(_)
            | AnyNode::StmtImportFrom(_)
            | AnyNode::StmtGlobal(_)
            | AnyNode::StmtNonlocal(_)
            | AnyNode::StmtExpr(_)
            | AnyNode::StmtPass(_)
            | AnyNode::StmtBreak(_)
            | AnyNode::StmtContinue(_)
            | AnyNode::StmtIpyEscapeCommand(_)
            | AnyNode::ExceptHandlerExceptHandler(_)
            | AnyNode::FStringExpressionElement(_)
            | AnyNode::FStringLiteralElement(_)
            | AnyNode::FStringFormatSpec(_)
            | AnyNode::PatternMatchValue(_)
            | AnyNode::PatternMatchSingleton(_)
            | AnyNode::PatternMatchSequence(_)
            | AnyNode::PatternMatchMapping(_)
            | AnyNode::PatternMatchClass(_)
            | AnyNode::PatternMatchStar(_)
            | AnyNode::PatternMatchAs(_)
            | AnyNode::PatternMatchOr(_)
            | AnyNode::PatternArguments(_)
            | AnyNode::PatternKeyword(_)
            | AnyNode::Comprehension(_)
            | AnyNode::Arguments(_)
            | AnyNode::Parameters(_)
            | AnyNode::Parameter(_)
            | AnyNode::ParameterWithDefault(_)
            | AnyNode::Keyword(_)
            | AnyNode::Alias(_)
            | AnyNode::WithItem(_)
            | AnyNode::MatchCase(_)
            | AnyNode::Decorator(_)
            | AnyNode::TypeParams(_)
            | AnyNode::TypeParamTypeVar(_)
            | AnyNode::TypeParamTypeVarTuple(_)
            | AnyNode::TypeParamParamSpec(_)
            | AnyNode::FString(_)
            | AnyNode::StringLiteral(_)
            | AnyNode::BytesLiteral(_)
            | AnyNode::ElifElseClause(_) => None,
        }
    }

    pub fn module(self) -> Option<Mod> {
        match self {
            AnyNode::ModModule(node) => Some(Mod::Module(node)),
            AnyNode::ModExpression(node) => Some(Mod::Expression(node)),

            AnyNode::StmtFunctionDef(_)
            | AnyNode::StmtClassDef(_)
            | AnyNode::StmtReturn(_)
            | AnyNode::StmtDelete(_)
            | AnyNode::StmtTypeAlias(_)
            | AnyNode::StmtAssign(_)
            | AnyNode::StmtAugAssign(_)
            | AnyNode::StmtAnnAssign(_)
            | AnyNode::StmtFor(_)
            | AnyNode::StmtWhile(_)
            | AnyNode::StmtIf(_)
            | AnyNode::StmtWith(_)
            | AnyNode::StmtMatch(_)
            | AnyNode::StmtRaise(_)
            | AnyNode::StmtTry(_)
            | AnyNode::StmtAssert(_)
            | AnyNode::StmtImport(_)
            | AnyNode::StmtImportFrom(_)
            | AnyNode::StmtGlobal(_)
            | AnyNode::StmtNonlocal(_)
            | AnyNode::StmtExpr(_)
            | AnyNode::StmtPass(_)
            | AnyNode::StmtBreak(_)
            | AnyNode::StmtContinue(_)
            | AnyNode::StmtIpyEscapeCommand(_)
            | AnyNode::ExprBoolOp(_)
            | AnyNode::ExprNamed(_)
            | AnyNode::ExprBinOp(_)
            | AnyNode::ExprUnaryOp(_)
            | AnyNode::ExprLambda(_)
            | AnyNode::ExprIf(_)
            | AnyNode::ExprDict(_)
            | AnyNode::ExprSet(_)
            | AnyNode::ExprListComp(_)
            | AnyNode::ExprSetComp(_)
            | AnyNode::ExprDictComp(_)
            | AnyNode::ExprGenerator(_)
            | AnyNode::ExprAwait(_)
            | AnyNode::ExprYield(_)
            | AnyNode::ExprYieldFrom(_)
            | AnyNode::ExprCompare(_)
            | AnyNode::ExprCall(_)
            | AnyNode::FStringExpressionElement(_)
            | AnyNode::FStringLiteralElement(_)
            | AnyNode::FStringFormatSpec(_)
            | AnyNode::ExprFString(_)
            | AnyNode::ExprStringLiteral(_)
            | AnyNode::ExprBytesLiteral(_)
            | AnyNode::ExprNumberLiteral(_)
            | AnyNode::ExprBooleanLiteral(_)
            | AnyNode::ExprNoneLiteral(_)
            | AnyNode::ExprEllipsisLiteral(_)
            | AnyNode::ExprAttribute(_)
            | AnyNode::ExprSubscript(_)
            | AnyNode::ExprStarred(_)
            | AnyNode::ExprName(_)
            | AnyNode::ExprList(_)
            | AnyNode::ExprTuple(_)
            | AnyNode::ExprSlice(_)
            | AnyNode::ExprIpyEscapeCommand(_)
            | AnyNode::ExceptHandlerExceptHandler(_)
            | AnyNode::PatternMatchValue(_)
            | AnyNode::PatternMatchSingleton(_)
            | AnyNode::PatternMatchSequence(_)
            | AnyNode::PatternMatchMapping(_)
            | AnyNode::PatternMatchClass(_)
            | AnyNode::PatternMatchStar(_)
            | AnyNode::PatternMatchAs(_)
            | AnyNode::PatternMatchOr(_)
            | AnyNode::PatternArguments(_)
            | AnyNode::PatternKeyword(_)
            | AnyNode::Comprehension(_)
            | AnyNode::Arguments(_)
            | AnyNode::Parameters(_)
            | AnyNode::Parameter(_)
            | AnyNode::ParameterWithDefault(_)
            | AnyNode::Keyword(_)
            | AnyNode::Alias(_)
            | AnyNode::WithItem(_)
            | AnyNode::MatchCase(_)
            | AnyNode::Decorator(_)
            | AnyNode::TypeParams(_)
            | AnyNode::TypeParamTypeVar(_)
            | AnyNode::TypeParamTypeVarTuple(_)
            | AnyNode::TypeParamParamSpec(_)
            | AnyNode::FString(_)
            | AnyNode::StringLiteral(_)
            | AnyNode::BytesLiteral(_)
            | AnyNode::ElifElseClause(_) => None,
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
            | AnyNode::ModExpression(_)
            | AnyNode::StmtFunctionDef(_)
            | AnyNode::StmtClassDef(_)
            | AnyNode::StmtReturn(_)
            | AnyNode::StmtDelete(_)
            | AnyNode::StmtTypeAlias(_)
            | AnyNode::StmtAssign(_)
            | AnyNode::StmtAugAssign(_)
            | AnyNode::StmtAnnAssign(_)
            | AnyNode::StmtFor(_)
            | AnyNode::StmtWhile(_)
            | AnyNode::StmtIf(_)
            | AnyNode::StmtWith(_)
            | AnyNode::StmtMatch(_)
            | AnyNode::StmtRaise(_)
            | AnyNode::StmtTry(_)
            | AnyNode::StmtAssert(_)
            | AnyNode::StmtImport(_)
            | AnyNode::StmtImportFrom(_)
            | AnyNode::StmtGlobal(_)
            | AnyNode::StmtNonlocal(_)
            | AnyNode::StmtExpr(_)
            | AnyNode::StmtPass(_)
            | AnyNode::StmtBreak(_)
            | AnyNode::StmtContinue(_)
            | AnyNode::StmtIpyEscapeCommand(_)
            | AnyNode::ExprBoolOp(_)
            | AnyNode::ExprNamed(_)
            | AnyNode::ExprBinOp(_)
            | AnyNode::ExprUnaryOp(_)
            | AnyNode::ExprLambda(_)
            | AnyNode::ExprIf(_)
            | AnyNode::ExprDict(_)
            | AnyNode::ExprSet(_)
            | AnyNode::ExprListComp(_)
            | AnyNode::ExprSetComp(_)
            | AnyNode::ExprDictComp(_)
            | AnyNode::ExprGenerator(_)
            | AnyNode::ExprAwait(_)
            | AnyNode::ExprYield(_)
            | AnyNode::ExprYieldFrom(_)
            | AnyNode::ExprCompare(_)
            | AnyNode::ExprCall(_)
            | AnyNode::FStringExpressionElement(_)
            | AnyNode::FStringLiteralElement(_)
            | AnyNode::FStringFormatSpec(_)
            | AnyNode::ExprFString(_)
            | AnyNode::ExprStringLiteral(_)
            | AnyNode::ExprBytesLiteral(_)
            | AnyNode::ExprNumberLiteral(_)
            | AnyNode::ExprBooleanLiteral(_)
            | AnyNode::ExprNoneLiteral(_)
            | AnyNode::ExprEllipsisLiteral(_)
            | AnyNode::ExprAttribute(_)
            | AnyNode::ExprSubscript(_)
            | AnyNode::ExprStarred(_)
            | AnyNode::ExprName(_)
            | AnyNode::ExprList(_)
            | AnyNode::ExprTuple(_)
            | AnyNode::ExprSlice(_)
            | AnyNode::ExprIpyEscapeCommand(_)
            | AnyNode::ExceptHandlerExceptHandler(_)
            | AnyNode::PatternArguments(_)
            | AnyNode::PatternKeyword(_)
            | AnyNode::Comprehension(_)
            | AnyNode::Arguments(_)
            | AnyNode::Parameters(_)
            | AnyNode::Parameter(_)
            | AnyNode::ParameterWithDefault(_)
            | AnyNode::Keyword(_)
            | AnyNode::Alias(_)
            | AnyNode::WithItem(_)
            | AnyNode::MatchCase(_)
            | AnyNode::Decorator(_)
            | AnyNode::TypeParams(_)
            | AnyNode::TypeParamTypeVar(_)
            | AnyNode::TypeParamTypeVarTuple(_)
            | AnyNode::TypeParamParamSpec(_)
            | AnyNode::FString(_)
            | AnyNode::StringLiteral(_)
            | AnyNode::BytesLiteral(_)
            | AnyNode::ElifElseClause(_) => None,
        }
    }

    pub fn except_handler(self) -> Option<ExceptHandler> {
        match self {
            AnyNode::ExceptHandlerExceptHandler(node) => Some(ExceptHandler::ExceptHandler(node)),

            AnyNode::ModModule(_)
            | AnyNode::ModExpression(_)
            | AnyNode::StmtFunctionDef(_)
            | AnyNode::StmtClassDef(_)
            | AnyNode::StmtReturn(_)
            | AnyNode::StmtDelete(_)
            | AnyNode::StmtTypeAlias(_)
            | AnyNode::StmtAssign(_)
            | AnyNode::StmtAugAssign(_)
            | AnyNode::StmtAnnAssign(_)
            | AnyNode::StmtFor(_)
            | AnyNode::StmtWhile(_)
            | AnyNode::StmtIf(_)
            | AnyNode::StmtWith(_)
            | AnyNode::StmtMatch(_)
            | AnyNode::StmtRaise(_)
            | AnyNode::StmtTry(_)
            | AnyNode::StmtAssert(_)
            | AnyNode::StmtImport(_)
            | AnyNode::StmtImportFrom(_)
            | AnyNode::StmtGlobal(_)
            | AnyNode::StmtNonlocal(_)
            | AnyNode::StmtExpr(_)
            | AnyNode::StmtPass(_)
            | AnyNode::StmtBreak(_)
            | AnyNode::StmtContinue(_)
            | AnyNode::StmtIpyEscapeCommand(_)
            | AnyNode::ExprBoolOp(_)
            | AnyNode::ExprNamed(_)
            | AnyNode::ExprBinOp(_)
            | AnyNode::ExprUnaryOp(_)
            | AnyNode::ExprLambda(_)
            | AnyNode::ExprIf(_)
            | AnyNode::ExprDict(_)
            | AnyNode::ExprSet(_)
            | AnyNode::ExprListComp(_)
            | AnyNode::ExprSetComp(_)
            | AnyNode::ExprDictComp(_)
            | AnyNode::ExprGenerator(_)
            | AnyNode::ExprAwait(_)
            | AnyNode::ExprYield(_)
            | AnyNode::ExprYieldFrom(_)
            | AnyNode::ExprCompare(_)
            | AnyNode::ExprCall(_)
            | AnyNode::FStringExpressionElement(_)
            | AnyNode::FStringLiteralElement(_)
            | AnyNode::FStringFormatSpec(_)
            | AnyNode::ExprFString(_)
            | AnyNode::ExprStringLiteral(_)
            | AnyNode::ExprBytesLiteral(_)
            | AnyNode::ExprNumberLiteral(_)
            | AnyNode::ExprBooleanLiteral(_)
            | AnyNode::ExprNoneLiteral(_)
            | AnyNode::ExprEllipsisLiteral(_)
            | AnyNode::ExprAttribute(_)
            | AnyNode::ExprSubscript(_)
            | AnyNode::ExprStarred(_)
            | AnyNode::ExprName(_)
            | AnyNode::ExprList(_)
            | AnyNode::ExprTuple(_)
            | AnyNode::ExprSlice(_)
            | AnyNode::ExprIpyEscapeCommand(_)
            | AnyNode::PatternMatchValue(_)
            | AnyNode::PatternMatchSingleton(_)
            | AnyNode::PatternMatchSequence(_)
            | AnyNode::PatternMatchMapping(_)
            | AnyNode::PatternMatchClass(_)
            | AnyNode::PatternMatchStar(_)
            | AnyNode::PatternMatchAs(_)
            | AnyNode::PatternMatchOr(_)
            | AnyNode::PatternArguments(_)
            | AnyNode::PatternKeyword(_)
            | AnyNode::Comprehension(_)
            | AnyNode::Arguments(_)
            | AnyNode::Parameters(_)
            | AnyNode::Parameter(_)
            | AnyNode::ParameterWithDefault(_)
            | AnyNode::Keyword(_)
            | AnyNode::Alias(_)
            | AnyNode::WithItem(_)
            | AnyNode::MatchCase(_)
            | AnyNode::Decorator(_)
            | AnyNode::TypeParams(_)
            | AnyNode::TypeParamTypeVar(_)
            | AnyNode::TypeParamTypeVarTuple(_)
            | AnyNode::TypeParamParamSpec(_)
            | AnyNode::FString(_)
            | AnyNode::StringLiteral(_)
            | AnyNode::BytesLiteral(_)
            | AnyNode::ElifElseClause(_) => None,
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

    pub const fn as_ref(&self) -> AnyNodeRef {
        match self {
            Self::ModModule(node) => AnyNodeRef::ModModule(node),
            Self::ModExpression(node) => AnyNodeRef::ModExpression(node),
            Self::StmtFunctionDef(node) => AnyNodeRef::StmtFunctionDef(node),
            Self::StmtClassDef(node) => AnyNodeRef::StmtClassDef(node),
            Self::StmtReturn(node) => AnyNodeRef::StmtReturn(node),
            Self::StmtDelete(node) => AnyNodeRef::StmtDelete(node),
            Self::StmtTypeAlias(node) => AnyNodeRef::StmtTypeAlias(node),
            Self::StmtAssign(node) => AnyNodeRef::StmtAssign(node),
            Self::StmtAugAssign(node) => AnyNodeRef::StmtAugAssign(node),
            Self::StmtAnnAssign(node) => AnyNodeRef::StmtAnnAssign(node),
            Self::StmtFor(node) => AnyNodeRef::StmtFor(node),
            Self::StmtWhile(node) => AnyNodeRef::StmtWhile(node),
            Self::StmtIf(node) => AnyNodeRef::StmtIf(node),
            Self::StmtWith(node) => AnyNodeRef::StmtWith(node),
            Self::StmtMatch(node) => AnyNodeRef::StmtMatch(node),
            Self::StmtRaise(node) => AnyNodeRef::StmtRaise(node),
            Self::StmtTry(node) => AnyNodeRef::StmtTry(node),
            Self::StmtAssert(node) => AnyNodeRef::StmtAssert(node),
            Self::StmtImport(node) => AnyNodeRef::StmtImport(node),
            Self::StmtImportFrom(node) => AnyNodeRef::StmtImportFrom(node),
            Self::StmtGlobal(node) => AnyNodeRef::StmtGlobal(node),
            Self::StmtNonlocal(node) => AnyNodeRef::StmtNonlocal(node),
            Self::StmtExpr(node) => AnyNodeRef::StmtExpr(node),
            Self::StmtPass(node) => AnyNodeRef::StmtPass(node),
            Self::StmtBreak(node) => AnyNodeRef::StmtBreak(node),
            Self::StmtContinue(node) => AnyNodeRef::StmtContinue(node),
            Self::StmtIpyEscapeCommand(node) => AnyNodeRef::StmtIpyEscapeCommand(node),
            Self::ExprBoolOp(node) => AnyNodeRef::ExprBoolOp(node),
            Self::ExprNamed(node) => AnyNodeRef::ExprNamed(node),
            Self::ExprBinOp(node) => AnyNodeRef::ExprBinOp(node),
            Self::ExprUnaryOp(node) => AnyNodeRef::ExprUnaryOp(node),
            Self::ExprLambda(node) => AnyNodeRef::ExprLambda(node),
            Self::ExprIf(node) => AnyNodeRef::ExprIf(node),
            Self::ExprDict(node) => AnyNodeRef::ExprDict(node),
            Self::ExprSet(node) => AnyNodeRef::ExprSet(node),
            Self::ExprListComp(node) => AnyNodeRef::ExprListComp(node),
            Self::ExprSetComp(node) => AnyNodeRef::ExprSetComp(node),
            Self::ExprDictComp(node) => AnyNodeRef::ExprDictComp(node),
            Self::ExprGenerator(node) => AnyNodeRef::ExprGenerator(node),
            Self::ExprAwait(node) => AnyNodeRef::ExprAwait(node),
            Self::ExprYield(node) => AnyNodeRef::ExprYield(node),
            Self::ExprYieldFrom(node) => AnyNodeRef::ExprYieldFrom(node),
            Self::ExprCompare(node) => AnyNodeRef::ExprCompare(node),
            Self::ExprCall(node) => AnyNodeRef::ExprCall(node),
            Self::FStringExpressionElement(node) => AnyNodeRef::FStringExpressionElement(node),
            Self::FStringLiteralElement(node) => AnyNodeRef::FStringLiteralElement(node),
            Self::FStringFormatSpec(node) => AnyNodeRef::FStringFormatSpec(node),
            Self::ExprFString(node) => AnyNodeRef::ExprFString(node),
            Self::ExprStringLiteral(node) => AnyNodeRef::ExprStringLiteral(node),
            Self::ExprBytesLiteral(node) => AnyNodeRef::ExprBytesLiteral(node),
            Self::ExprNumberLiteral(node) => AnyNodeRef::ExprNumberLiteral(node),
            Self::ExprBooleanLiteral(node) => AnyNodeRef::ExprBooleanLiteral(node),
            Self::ExprNoneLiteral(node) => AnyNodeRef::ExprNoneLiteral(node),
            Self::ExprEllipsisLiteral(node) => AnyNodeRef::ExprEllipsisLiteral(node),
            Self::ExprAttribute(node) => AnyNodeRef::ExprAttribute(node),
            Self::ExprSubscript(node) => AnyNodeRef::ExprSubscript(node),
            Self::ExprStarred(node) => AnyNodeRef::ExprStarred(node),
            Self::ExprName(node) => AnyNodeRef::ExprName(node),
            Self::ExprList(node) => AnyNodeRef::ExprList(node),
            Self::ExprTuple(node) => AnyNodeRef::ExprTuple(node),
            Self::ExprSlice(node) => AnyNodeRef::ExprSlice(node),
            Self::ExprIpyEscapeCommand(node) => AnyNodeRef::ExprIpyEscapeCommand(node),
            Self::ExceptHandlerExceptHandler(node) => AnyNodeRef::ExceptHandlerExceptHandler(node),
            Self::PatternMatchValue(node) => AnyNodeRef::PatternMatchValue(node),
            Self::PatternMatchSingleton(node) => AnyNodeRef::PatternMatchSingleton(node),
            Self::PatternMatchSequence(node) => AnyNodeRef::PatternMatchSequence(node),
            Self::PatternMatchMapping(node) => AnyNodeRef::PatternMatchMapping(node),
            Self::PatternMatchClass(node) => AnyNodeRef::PatternMatchClass(node),
            Self::PatternMatchStar(node) => AnyNodeRef::PatternMatchStar(node),
            Self::PatternMatchAs(node) => AnyNodeRef::PatternMatchAs(node),
            Self::PatternMatchOr(node) => AnyNodeRef::PatternMatchOr(node),
            Self::PatternArguments(node) => AnyNodeRef::PatternArguments(node),
            Self::PatternKeyword(node) => AnyNodeRef::PatternKeyword(node),
            Self::Comprehension(node) => AnyNodeRef::Comprehension(node),
            Self::Arguments(node) => AnyNodeRef::Arguments(node),
            Self::Parameters(node) => AnyNodeRef::Parameters(node),
            Self::Parameter(node) => AnyNodeRef::Parameter(node),
            Self::ParameterWithDefault(node) => AnyNodeRef::ParameterWithDefault(node),
            Self::Keyword(node) => AnyNodeRef::Keyword(node),
            Self::Alias(node) => AnyNodeRef::Alias(node),
            Self::WithItem(node) => AnyNodeRef::WithItem(node),
            Self::MatchCase(node) => AnyNodeRef::MatchCase(node),
            Self::Decorator(node) => AnyNodeRef::Decorator(node),
            Self::TypeParams(node) => AnyNodeRef::TypeParams(node),
            Self::TypeParamTypeVar(node) => AnyNodeRef::TypeParamTypeVar(node),
            Self::TypeParamTypeVarTuple(node) => AnyNodeRef::TypeParamTypeVarTuple(node),
            Self::TypeParamParamSpec(node) => AnyNodeRef::TypeParamParamSpec(node),
            Self::FString(node) => AnyNodeRef::FString(node),
            Self::StringLiteral(node) => AnyNodeRef::StringLiteral(node),
            Self::BytesLiteral(node) => AnyNodeRef::BytesLiteral(node),
            Self::ElifElseClause(node) => AnyNodeRef::ElifElseClause(node),
        }
    }

    /// Returns the node's [`kind`](NodeKind) that has no data associated and is [`Copy`].
    pub const fn kind(&self) -> NodeKind {
        self.as_ref().kind()
    }
}

impl AstNode for ast::ModModule {
    type Ref<'a> = &'a Self;

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

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::ModModule)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ModModule { body, range: _ } = self;
        visitor.visit_body(body);
    }
}

impl AstNode for ast::ModExpression {
    type Ref<'a> = &'a Self;

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

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::ModExpression)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ModExpression { body, range: _ } = self;
        visitor.visit_expr(body);
    }
}
impl AstNode for ast::StmtFunctionDef {
    type Ref<'a> = &'a Self;

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

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::StmtFunctionDef)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::StmtFunctionDef {
            parameters,
            body,
            decorator_list,
            returns,
            type_params,
            ..
        } = self;

        for decorator in decorator_list {
            visitor.visit_decorator(decorator);
        }

        if let Some(type_params) = type_params {
            visitor.visit_type_params(type_params);
        }

        visitor.visit_parameters(parameters);

        for expr in returns {
            visitor.visit_annotation(expr);
        }

        visitor.visit_body(body);
    }
}
impl AstNode for ast::StmtClassDef {
    type Ref<'a> = &'a Self;

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

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::StmtClassDef)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::StmtClassDef {
            arguments,
            body,
            decorator_list,
            type_params,
            ..
        } = self;

        for decorator in decorator_list {
            visitor.visit_decorator(decorator);
        }

        if let Some(type_params) = type_params {
            visitor.visit_type_params(type_params);
        }

        if let Some(arguments) = arguments {
            visitor.visit_arguments(arguments);
        }

        visitor.visit_body(body);
    }
}
impl AstNode for ast::StmtReturn {
    type Ref<'a> = &'a Self;
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

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::StmtReturn)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::StmtReturn { value, range: _ } = self;
        if let Some(expr) = value {
            visitor.visit_expr(expr);
        }
    }
}
impl AstNode for ast::StmtDelete {
    type Ref<'a> = &'a Self;
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

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::StmtDelete)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::StmtDelete { targets, range: _ } = self;
        for expr in targets {
            visitor.visit_expr(expr);
        }
    }
}
impl AstNode for ast::StmtTypeAlias {
    type Ref<'a> = &'a Self;
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::StmtTypeAlias(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::StmtTypeAlias(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::StmtTypeAlias)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::StmtTypeAlias {
            range: _,
            name,
            type_params,
            value,
        } = self;

        visitor.visit_expr(name);
        if let Some(type_params) = type_params {
            visitor.visit_type_params(type_params);
        }
        visitor.visit_expr(value);
    }
}
impl AstNode for ast::StmtAssign {
    type Ref<'a> = &'a Self;
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

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::StmtAssign)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::StmtAssign {
            targets,
            value,
            range: _,
        } = self;

        for expr in targets {
            visitor.visit_expr(expr);
        }

        visitor.visit_expr(value);
    }
}
impl AstNode for ast::StmtAugAssign {
    type Ref<'a> = &'a Self;
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

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::StmtAugAssign)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::StmtAugAssign {
            target,
            op,
            value,
            range: _,
        } = self;

        visitor.visit_expr(target);
        visitor.visit_operator(op);
        visitor.visit_expr(value);
    }
}
impl AstNode for ast::StmtAnnAssign {
    type Ref<'a> = &'a Self;
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

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::StmtAnnAssign)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::StmtAnnAssign {
            target,
            annotation,
            value,
            range: _,
            simple: _,
        } = self;

        visitor.visit_expr(target);
        visitor.visit_annotation(annotation);
        if let Some(expr) = value {
            visitor.visit_expr(expr);
        }
    }
}
impl AstNode for ast::StmtFor {
    type Ref<'a> = &'a Self;
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

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::StmtFor)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::StmtFor {
            target,
            iter,
            body,
            orelse,
            ..
        } = self;

        visitor.visit_expr(target);
        visitor.visit_expr(iter);
        visitor.visit_body(body);
        visitor.visit_body(orelse);
    }
}
impl AstNode for ast::StmtWhile {
    type Ref<'a> = &'a Self;
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

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::StmtWhile)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::StmtWhile {
            test,
            body,
            orelse,
            range: _,
        } = self;

        visitor.visit_expr(test);
        visitor.visit_body(body);
        visitor.visit_body(orelse);
    }
}
impl AstNode for ast::StmtIf {
    type Ref<'a> = &'a Self;
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

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::StmtIf)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::StmtIf {
            test,
            body,
            elif_else_clauses,
            range: _,
        } = self;

        visitor.visit_expr(test);
        visitor.visit_body(body);
        for clause in elif_else_clauses {
            visitor.visit_elif_else_clause(clause);
        }
    }
}
impl AstNode for ast::ElifElseClause {
    type Ref<'a> = &'a Self;
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::ElifElseClause(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::ElifElseClause(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::ElifElseClause)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ElifElseClause {
            range: _,
            test,
            body,
        } = self;
        if let Some(test) = test {
            visitor.visit_expr(test);
        }
        visitor.visit_body(body);
    }
}
impl AstNode for ast::StmtWith {
    type Ref<'a> = &'a Self;
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

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::StmtWith)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::StmtWith {
            items,
            body,
            is_async: _,
            range: _,
        } = self;

        for with_item in items {
            visitor.visit_with_item(with_item);
        }
        visitor.visit_body(body);
    }
}
impl AstNode for ast::StmtMatch {
    type Ref<'a> = &'a Self;
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

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::StmtMatch)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::StmtMatch {
            subject,
            cases,
            range: _,
        } = self;

        visitor.visit_expr(subject);
        for match_case in cases {
            visitor.visit_match_case(match_case);
        }
    }
}
impl AstNode for ast::StmtRaise {
    type Ref<'a> = &'a Self;
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

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::StmtRaise)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::StmtRaise {
            exc,
            cause,
            range: _,
        } = self;

        if let Some(expr) = exc {
            visitor.visit_expr(expr);
        };
        if let Some(expr) = cause {
            visitor.visit_expr(expr);
        };
    }
}
impl AstNode for ast::StmtTry {
    type Ref<'a> = &'a Self;
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

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::StmtTry)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::StmtTry {
            body,
            handlers,
            orelse,
            finalbody,
            is_star: _,
            range: _,
        } = self;

        visitor.visit_body(body);
        for except_handler in handlers {
            visitor.visit_except_handler(except_handler);
        }
        visitor.visit_body(orelse);
        visitor.visit_body(finalbody);
    }
}
impl AstNode for ast::StmtAssert {
    type Ref<'a> = &'a Self;
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

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::StmtAssert)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::StmtAssert {
            test,
            msg,
            range: _,
        } = self;
        visitor.visit_expr(test);
        if let Some(expr) = msg {
            visitor.visit_expr(expr);
        }
    }
}
impl AstNode for ast::StmtImport {
    type Ref<'a> = &'a Self;
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

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::StmtImport)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::StmtImport { names, range: _ } = self;

        for alias in names {
            visitor.visit_alias(alias);
        }
    }
}
impl AstNode for ast::StmtImportFrom {
    type Ref<'a> = &'a Self;
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

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::StmtImportFrom)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::StmtImportFrom {
            range: _,
            module: _,
            names,
            level: _,
        } = self;

        for alias in names {
            visitor.visit_alias(alias);
        }
    }
}
impl AstNode for ast::StmtGlobal {
    type Ref<'a> = &'a Self;
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

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::StmtGlobal)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    #[inline]
    fn visit_source_order<'a, V>(&'a self, _visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
    }
}
impl AstNode for ast::StmtNonlocal {
    type Ref<'a> = &'a Self;
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

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::StmtNonlocal)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    #[inline]
    fn visit_source_order<'a, V>(&'a self, _visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
    }
}
impl AstNode for ast::StmtExpr {
    type Ref<'a> = &'a Self;
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

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::StmtExpr)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::StmtExpr { value, range: _ } = self;

        visitor.visit_expr(value);
    }
}
impl AstNode for ast::StmtPass {
    type Ref<'a> = &'a Self;
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

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::StmtPass)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    #[inline]
    fn visit_source_order<'a, V>(&'a self, _visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
    }
}
impl AstNode for ast::StmtBreak {
    type Ref<'a> = &'a Self;
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

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::StmtBreak)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    #[inline]
    fn visit_source_order<'a, V>(&'a self, _visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
    }
}
impl AstNode for ast::StmtContinue {
    type Ref<'a> = &'a Self;
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

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::StmtContinue)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    #[inline]
    fn visit_source_order<'a, V>(&'a self, _visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
    }
}
impl AstNode for ast::StmtIpyEscapeCommand {
    type Ref<'a> = &'a Self;
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::StmtIpyEscapeCommand(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::StmtIpyEscapeCommand(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::StmtIpyEscapeCommand)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    #[inline]
    fn visit_source_order<'a, V>(&'a self, _visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
    }
}
impl AstNode for ast::ExprBoolOp {
    type Ref<'a> = &'a Self;
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

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::ExprBoolOp)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprBoolOp {
            op,
            values,
            range: _,
        } = self;
        match values.as_slice() {
            [left, rest @ ..] => {
                visitor.visit_expr(left);
                visitor.visit_bool_op(op);
                for expr in rest {
                    visitor.visit_expr(expr);
                }
            }
            [] => {
                visitor.visit_bool_op(op);
            }
        }
    }
}
impl AstNode for ast::ExprNamed {
    type Ref<'a> = &'a Self;
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::ExprNamed(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::ExprNamed(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::ExprNamed)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprNamed {
            target,
            value,
            range: _,
        } = self;
        visitor.visit_expr(target);
        visitor.visit_expr(value);
    }
}
impl AstNode for ast::ExprBinOp {
    type Ref<'a> = &'a Self;
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

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::ExprBinOp)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprBinOp {
            left,
            op,
            right,
            range: _,
        } = self;
        visitor.visit_expr(left);
        visitor.visit_operator(op);
        visitor.visit_expr(right);
    }
}
impl AstNode for ast::ExprUnaryOp {
    type Ref<'a> = &'a Self;
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

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::ExprUnaryOp)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprUnaryOp {
            op,
            operand,
            range: _,
        } = self;

        visitor.visit_unary_op(op);
        visitor.visit_expr(operand);
    }
}
impl AstNode for ast::ExprLambda {
    type Ref<'a> = &'a Self;

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

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::ExprLambda)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprLambda {
            parameters,
            body,
            range: _,
        } = self;

        if let Some(parameters) = parameters {
            visitor.visit_parameters(parameters);
        }
        visitor.visit_expr(body);
    }
}
impl AstNode for ast::ExprIf {
    type Ref<'a> = &'a Self;
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::ExprIf(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::ExprIf(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::ExprIf)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprIf {
            test,
            body,
            orelse,
            range: _,
        } = self;

        // `body if test else orelse`
        visitor.visit_expr(body);
        visitor.visit_expr(test);
        visitor.visit_expr(orelse);
    }
}
impl AstNode for ast::ExprDict {
    type Ref<'a> = &'a Self;
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

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::ExprDict)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprDict { items, range: _ } = self;

        for ast::DictItem { key, value } in items {
            if let Some(key) = key {
                visitor.visit_expr(key);
            }
            visitor.visit_expr(value);
        }
    }
}
impl AstNode for ast::ExprSet {
    type Ref<'a> = &'a Self;
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

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::ExprSet)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprSet { elts, range: _ } = self;

        for expr in elts {
            visitor.visit_expr(expr);
        }
    }
}
impl AstNode for ast::ExprListComp {
    type Ref<'a> = &'a Self;
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

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::ExprListComp)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprListComp {
            elt,
            generators,
            range: _,
        } = self;

        visitor.visit_expr(elt);
        for comprehension in generators {
            visitor.visit_comprehension(comprehension);
        }
    }
}
impl AstNode for ast::ExprSetComp {
    type Ref<'a> = &'a Self;
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

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::ExprSetComp)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprSetComp {
            elt,
            generators,
            range: _,
        } = self;

        visitor.visit_expr(elt);
        for comprehension in generators {
            visitor.visit_comprehension(comprehension);
        }
    }
}
impl AstNode for ast::ExprDictComp {
    type Ref<'a> = &'a Self;
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

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::ExprDictComp)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprDictComp {
            key,
            value,
            generators,
            range: _,
        } = self;

        visitor.visit_expr(key);
        visitor.visit_expr(value);

        for comprehension in generators {
            visitor.visit_comprehension(comprehension);
        }
    }
}
impl AstNode for ast::ExprGenerator {
    type Ref<'a> = &'a Self;
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::ExprGenerator(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::ExprGenerator(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::ExprGenerator)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprGenerator {
            elt,
            generators,
            range: _,
            parenthesized: _,
        } = self;
        visitor.visit_expr(elt);
        for comprehension in generators {
            visitor.visit_comprehension(comprehension);
        }
    }
}
impl AstNode for ast::ExprAwait {
    type Ref<'a> = &'a Self;
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

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::ExprAwait)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprAwait { value, range: _ } = self;
        visitor.visit_expr(value);
    }
}
impl AstNode for ast::ExprYield {
    type Ref<'a> = &'a Self;
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

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::ExprYield)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprYield { value, range: _ } = self;
        if let Some(expr) = value {
            visitor.visit_expr(expr);
        }
    }
}
impl AstNode for ast::ExprYieldFrom {
    type Ref<'a> = &'a Self;
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

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::ExprYieldFrom)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprYieldFrom { value, range: _ } = self;
        visitor.visit_expr(value);
    }
}
impl AstNode for ast::ExprCompare {
    type Ref<'a> = &'a Self;
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

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::ExprCompare)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprCompare {
            left,
            ops,
            comparators,
            range: _,
        } = self;

        visitor.visit_expr(left);

        for (op, comparator) in ops.iter().zip(&**comparators) {
            visitor.visit_cmp_op(op);
            visitor.visit_expr(comparator);
        }
    }
}
impl AstNode for ast::ExprCall {
    type Ref<'a> = &'a Self;
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

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::ExprCall)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprCall {
            func,
            arguments,
            range: _,
        } = self;
        visitor.visit_expr(func);
        visitor.visit_arguments(arguments);
    }
}
impl AstNode for ast::FStringFormatSpec {
    type Ref<'a> = &'a Self;
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::FStringFormatSpec(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::FStringFormatSpec(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::FStringFormatSpec)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        for element in &self.elements {
            visitor.visit_f_string_element(element);
        }
    }
}
impl AstNode for ast::FStringExpressionElement {
    type Ref<'a> = &'a Self;
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::FStringExpressionElement(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::FStringExpressionElement(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::FStringExpressionElement)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::FStringExpressionElement {
            expression,
            format_spec,
            ..
        } = self;
        visitor.visit_expr(expression);

        if let Some(format_spec) = format_spec {
            for spec_part in &format_spec.elements {
                visitor.visit_f_string_element(spec_part);
            }
        }
    }
}
impl AstNode for ast::FStringLiteralElement {
    type Ref<'a> = &'a Self;
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::FStringLiteralElement(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::FStringLiteralElement(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::FStringLiteralElement)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    fn visit_source_order<'a, V>(&'a self, _visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
    }
}
impl AstNode for ast::ExprFString {
    type Ref<'a> = &'a Self;
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::ExprFString(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::ExprFString(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::ExprFString)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprFString { value, range: _ } = self;

        for f_string_part in value {
            match f_string_part {
                ast::FStringPart::Literal(string_literal) => {
                    visitor.visit_string_literal(string_literal);
                }
                ast::FStringPart::FString(f_string) => {
                    visitor.visit_f_string(f_string);
                }
            }
        }
    }
}
impl AstNode for ast::ExprStringLiteral {
    type Ref<'a> = &'a Self;
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::ExprStringLiteral(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::ExprStringLiteral(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::ExprStringLiteral)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprStringLiteral { value, range: _ } = self;

        for string_literal in value {
            visitor.visit_string_literal(string_literal);
        }
    }
}
impl AstNode for ast::ExprBytesLiteral {
    type Ref<'a> = &'a Self;
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::ExprBytesLiteral(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::ExprBytesLiteral(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::ExprBytesLiteral)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprBytesLiteral { value, range: _ } = self;

        for bytes_literal in value {
            visitor.visit_bytes_literal(bytes_literal);
        }
    }
}
impl AstNode for ast::ExprNumberLiteral {
    type Ref<'a> = &'a Self;
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::ExprNumberLiteral(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::ExprNumberLiteral(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::ExprNumberLiteral)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    fn visit_source_order<'a, V>(&'a self, _visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
    }
}
impl AstNode for ast::ExprBooleanLiteral {
    type Ref<'a> = &'a Self;
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::ExprBooleanLiteral(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::ExprBooleanLiteral(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::ExprBooleanLiteral)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    fn visit_source_order<'a, V>(&'a self, _visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
    }
}
impl AstNode for ast::ExprNoneLiteral {
    type Ref<'a> = &'a Self;
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::ExprNoneLiteral(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::ExprNoneLiteral(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::ExprNoneLiteral)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    fn visit_source_order<'a, V>(&'a self, _visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
    }
}
impl AstNode for ast::ExprEllipsisLiteral {
    type Ref<'a> = &'a Self;
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::ExprEllipsisLiteral(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::ExprEllipsisLiteral(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::ExprEllipsisLiteral)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    fn visit_source_order<'a, V>(&'a self, _visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
    }
}
impl AstNode for ast::ExprAttribute {
    type Ref<'a> = &'a Self;
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

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::ExprAttribute)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprAttribute {
            value,
            attr: _,
            ctx: _,
            range: _,
        } = self;

        visitor.visit_expr(value);
    }
}
impl AstNode for ast::ExprSubscript {
    type Ref<'a> = &'a Self;
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

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::ExprSubscript)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprSubscript {
            value,
            slice,
            ctx: _,
            range: _,
        } = self;
        visitor.visit_expr(value);
        visitor.visit_expr(slice);
    }
}
impl AstNode for ast::ExprStarred {
    type Ref<'a> = &'a Self;
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

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::ExprStarred)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprStarred {
            value,
            ctx: _,
            range: _,
        } = self;

        visitor.visit_expr(value);
    }
}
impl AstNode for ast::ExprName {
    type Ref<'a> = &'a Self;
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

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::ExprName)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    #[inline]
    fn visit_source_order<'a, V>(&'a self, _visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprName {
            id: _,
            ctx: _,
            range: _,
        } = self;
    }
}
impl AstNode for ast::ExprList {
    type Ref<'a> = &'a Self;
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

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::ExprList)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprList {
            elts,
            ctx: _,
            range: _,
        } = self;

        for expr in elts {
            visitor.visit_expr(expr);
        }
    }
}
impl AstNode for ast::ExprTuple {
    type Ref<'a> = &'a Self;
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

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::ExprTuple)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprTuple {
            elts,
            ctx: _,
            range: _,
            parenthesized: _,
        } = self;

        for expr in elts {
            visitor.visit_expr(expr);
        }
    }
}
impl AstNode for ast::ExprSlice {
    type Ref<'a> = &'a Self;
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

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::ExprSlice)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
    fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprSlice {
            lower,
            upper,
            step,
            range: _,
        } = self;

        if let Some(expr) = lower {
            visitor.visit_expr(expr);
        }
        if let Some(expr) = upper {
            visitor.visit_expr(expr);
        }
        if let Some(expr) = step {
            visitor.visit_expr(expr);
        }
    }
}
impl AstNode for ast::ExprIpyEscapeCommand {
    type Ref<'a> = &'a Self;
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::ExprIpyEscapeCommand(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::ExprIpyEscapeCommand(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::ExprIpyEscapeCommand)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    #[inline]
    fn visit_source_order<'a, V>(&'a self, _visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprIpyEscapeCommand {
            range: _,
            kind: _,
            value: _,
        } = self;
    }
}
impl AstNode for ast::ExceptHandlerExceptHandler {
    type Ref<'a> = &'a Self;
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

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::ExceptHandlerExceptHandler)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExceptHandlerExceptHandler {
            range: _,
            type_,
            name: _,
            body,
        } = self;
        if let Some(expr) = type_ {
            visitor.visit_expr(expr);
        }
        visitor.visit_body(body);
    }
}
impl AstNode for ast::PatternMatchValue {
    type Ref<'a> = &'a Self;
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

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::PatternMatchValue)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::PatternMatchValue { value, range: _ } = self;
        visitor.visit_expr(value);
    }
}
impl AstNode for ast::PatternMatchSingleton {
    type Ref<'a> = &'a Self;
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

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::PatternMatchSingleton)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::PatternMatchSingleton { value, range: _ } = self;
        visitor.visit_singleton(value);
    }
}
impl AstNode for ast::PatternMatchSequence {
    type Ref<'a> = &'a Self;
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

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::PatternMatchSequence)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::PatternMatchSequence { patterns, range: _ } = self;
        for pattern in patterns {
            visitor.visit_pattern(pattern);
        }
    }
}
impl AstNode for ast::PatternMatchMapping {
    type Ref<'a> = &'a Self;
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

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::PatternMatchMapping)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::PatternMatchMapping {
            keys,
            patterns,
            range: _,
            rest: _,
        } = self;
        for (key, pattern) in keys.iter().zip(patterns) {
            visitor.visit_expr(key);
            visitor.visit_pattern(pattern);
        }
    }
}
impl AstNode for ast::PatternMatchClass {
    type Ref<'a> = &'a Self;
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

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::PatternMatchClass)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::PatternMatchClass {
            cls,
            arguments: parameters,
            range: _,
        } = self;
        visitor.visit_expr(cls);
        visitor.visit_pattern_arguments(parameters);
    }
}
impl AstNode for ast::PatternMatchStar {
    type Ref<'a> = &'a Self;
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

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::PatternMatchStar)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    #[inline]
    fn visit_source_order<'a, V>(&'a self, _visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::PatternMatchStar { range: _, name: _ } = self;
    }
}
impl AstNode for ast::PatternMatchAs {
    type Ref<'a> = &'a Self;
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

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::PatternMatchAs)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::PatternMatchAs {
            pattern,
            range: _,
            name: _,
        } = self;
        if let Some(pattern) = pattern {
            visitor.visit_pattern(pattern);
        }
    }
}
impl AstNode for ast::PatternMatchOr {
    type Ref<'a> = &'a Self;
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

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::PatternMatchOr)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::PatternMatchOr { patterns, range: _ } = self;
        for pattern in patterns {
            visitor.visit_pattern(pattern);
        }
    }
}
impl AstNode for PatternArguments {
    type Ref<'a> = &'a Self;
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::PatternArguments(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::PatternArguments(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::PatternArguments)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let PatternArguments {
            range: _,
            patterns,
            keywords,
        } = self;

        for pattern in patterns {
            visitor.visit_pattern(pattern);
        }

        for keyword in keywords {
            visitor.visit_pattern_keyword(keyword);
        }
    }
}
impl AstNode for PatternKeyword {
    type Ref<'a> = &'a Self;
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::PatternKeyword(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::PatternKeyword(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::PatternKeyword)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let PatternKeyword {
            range: _,
            attr: _,
            pattern,
        } = self;

        visitor.visit_pattern(pattern);
    }
}

impl AstNode for Comprehension {
    type Ref<'a> = &'a Self;
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

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::Comprehension)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::Comprehension {
            range: _,
            target,
            iter,
            ifs,
            is_async: _,
        } = self;
        visitor.visit_expr(target);
        visitor.visit_expr(iter);

        for expr in ifs {
            visitor.visit_expr(expr);
        }
    }
}
impl AstNode for Arguments {
    type Ref<'a> = &'a Self;
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

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::Arguments)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        for arg_or_keyword in self.arguments_source_order() {
            match arg_or_keyword {
                ArgOrKeyword::Arg(arg) => visitor.visit_expr(arg),
                ArgOrKeyword::Keyword(keyword) => visitor.visit_keyword(keyword),
            }
        }
    }
}
impl AstNode for Parameters {
    type Ref<'a> = &'a Self;
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::Parameters(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::Parameters(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::Parameters)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        for parameter in self {
            match parameter {
                AnyParameterRef::NonVariadic(parameter_with_default) => {
                    visitor.visit_parameter_with_default(parameter_with_default);
                }
                AnyParameterRef::Variadic(parameter) => visitor.visit_parameter(parameter),
            }
        }
    }
}
impl AstNode for Parameter {
    type Ref<'a> = &'a Self;
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::Parameter(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::Parameter(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::Parameter)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::Parameter {
            range: _,
            name: _,
            annotation,
        } = self;

        if let Some(expr) = annotation {
            visitor.visit_annotation(expr);
        }
    }
}
impl AstNode for ParameterWithDefault {
    type Ref<'a> = &'a Self;
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::ParameterWithDefault(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::ParameterWithDefault(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::ParameterWithDefault)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ParameterWithDefault {
            range: _,
            parameter,
            default,
        } = self;
        visitor.visit_parameter(parameter);
        if let Some(expr) = default {
            visitor.visit_expr(expr);
        }
    }
}
impl AstNode for Keyword {
    type Ref<'a> = &'a Self;
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

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::Keyword)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::Keyword {
            range: _,
            arg: _,
            value,
        } = self;

        visitor.visit_expr(value);
    }
}
impl AstNode for Alias {
    type Ref<'a> = &'a Self;
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

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::Alias)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    #[inline]
    fn visit_source_order<'a, V>(&'a self, _visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::Alias {
            range: _,
            name: _,
            asname: _,
        } = self;
    }
}
impl AstNode for WithItem {
    type Ref<'a> = &'a Self;
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

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::WithItem)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::WithItem {
            range: _,
            context_expr,
            optional_vars,
        } = self;

        visitor.visit_expr(context_expr);

        if let Some(expr) = optional_vars {
            visitor.visit_expr(expr);
        }
    }
}
impl AstNode for MatchCase {
    type Ref<'a> = &'a Self;
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

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::MatchCase)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::MatchCase {
            range: _,
            pattern,
            guard,
            body,
        } = self;

        visitor.visit_pattern(pattern);
        if let Some(expr) = guard {
            visitor.visit_expr(expr);
        }
        visitor.visit_body(body);
    }
}

impl AstNode for Decorator {
    type Ref<'a> = &'a Self;
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

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::Decorator)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::Decorator {
            range: _,
            expression,
        } = self;

        visitor.visit_expr(expression);
    }
}
impl AstNode for ast::TypeParams {
    type Ref<'a> = &'a Self;
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::TypeParams(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::TypeParams(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::TypeParams)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::TypeParams {
            range: _,
            type_params,
        } = self;

        for type_param in type_params {
            visitor.visit_type_param(type_param);
        }
    }
}
impl AstNode for ast::TypeParamTypeVar {
    type Ref<'a> = &'a Self;
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::TypeParamTypeVar(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::TypeParamTypeVar(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::TypeParamTypeVar)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::TypeParamTypeVar {
            bound,
            default,
            name: _,
            range: _,
        } = self;

        if let Some(expr) = bound {
            visitor.visit_expr(expr);
        }
        if let Some(expr) = default {
            visitor.visit_expr(expr);
        }
    }
}
impl AstNode for ast::TypeParamTypeVarTuple {
    type Ref<'a> = &'a Self;
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::TypeParamTypeVarTuple(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::TypeParamTypeVarTuple(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::TypeParamTypeVarTuple)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    #[inline]
    fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::TypeParamTypeVarTuple {
            range: _,
            name: _,
            default,
        } = self;
        if let Some(expr) = default {
            visitor.visit_expr(expr);
        }
    }
}
impl AstNode for ast::TypeParamParamSpec {
    type Ref<'a> = &'a Self;
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::TypeParamParamSpec(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::TypeParamParamSpec(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::TypeParamParamSpec)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    #[inline]
    fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::TypeParamParamSpec {
            range: _,
            name: _,
            default,
        } = self;
        if let Some(expr) = default {
            visitor.visit_expr(expr);
        }
    }
}
impl AstNode for ast::FString {
    type Ref<'a> = &'a Self;
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::FString(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::FString(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::FString)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::FString {
            elements,
            range: _,
            flags: _,
        } = self;

        for fstring_element in elements {
            visitor.visit_f_string_element(fstring_element);
        }
    }
}
impl AstNode for ast::StringLiteral {
    type Ref<'a> = &'a Self;
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::StringLiteral(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::StringLiteral(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::StringLiteral)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    fn visit_source_order<'a, V>(&'a self, _visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
    }
}
impl AstNode for ast::BytesLiteral {
    type Ref<'a> = &'a Self;
    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::BytesLiteral(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::BytesLiteral(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: NodeKind) -> bool {
        matches!(kind, NodeKind::BytesLiteral)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }

    fn visit_source_order<'a, V>(&'a self, _visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
    }
}

impl AstNode for Stmt {
    type Ref<'a> = StatementRef<'a>;

    fn cast(kind: AnyNode) -> Option<Self> {
        match kind {
            AnyNode::StmtFunctionDef(node) => Some(Stmt::FunctionDef(node)),
            AnyNode::StmtClassDef(node) => Some(Stmt::ClassDef(node)),
            AnyNode::StmtReturn(node) => Some(Stmt::Return(node)),
            AnyNode::StmtDelete(node) => Some(Stmt::Delete(node)),
            AnyNode::StmtTypeAlias(node) => Some(Stmt::TypeAlias(node)),
            AnyNode::StmtAssign(node) => Some(Stmt::Assign(node)),
            AnyNode::StmtAugAssign(node) => Some(Stmt::AugAssign(node)),
            AnyNode::StmtAnnAssign(node) => Some(Stmt::AnnAssign(node)),
            AnyNode::StmtFor(node) => Some(Stmt::For(node)),
            AnyNode::StmtWhile(node) => Some(Stmt::While(node)),
            AnyNode::StmtIf(node) => Some(Stmt::If(node)),
            AnyNode::StmtWith(node) => Some(Stmt::With(node)),
            AnyNode::StmtMatch(node) => Some(Stmt::Match(node)),
            AnyNode::StmtRaise(node) => Some(Stmt::Raise(node)),
            AnyNode::StmtTry(node) => Some(Stmt::Try(node)),
            AnyNode::StmtAssert(node) => Some(Stmt::Assert(node)),
            AnyNode::StmtImport(node) => Some(Stmt::Import(node)),
            AnyNode::StmtImportFrom(node) => Some(Stmt::ImportFrom(node)),
            AnyNode::StmtGlobal(node) => Some(Stmt::Global(node)),
            AnyNode::StmtNonlocal(node) => Some(Stmt::Nonlocal(node)),
            AnyNode::StmtExpr(node) => Some(Stmt::Expr(node)),
            AnyNode::StmtPass(node) => Some(Stmt::Pass(node)),
            AnyNode::StmtBreak(node) => Some(Stmt::Break(node)),
            AnyNode::StmtContinue(node) => Some(Stmt::Continue(node)),
            AnyNode::StmtIpyEscapeCommand(node) => Some(Stmt::IpyEscapeCommand(node)),

            AnyNode::ModModule(_)
            | AnyNode::ModExpression(_)
            | AnyNode::ExprBoolOp(_)
            | AnyNode::ExprNamed(_)
            | AnyNode::ExprBinOp(_)
            | AnyNode::ExprUnaryOp(_)
            | AnyNode::ExprLambda(_)
            | AnyNode::ExprIf(_)
            | AnyNode::ExprDict(_)
            | AnyNode::ExprSet(_)
            | AnyNode::ExprListComp(_)
            | AnyNode::ExprSetComp(_)
            | AnyNode::ExprDictComp(_)
            | AnyNode::ExprGenerator(_)
            | AnyNode::ExprAwait(_)
            | AnyNode::ExprYield(_)
            | AnyNode::ExprYieldFrom(_)
            | AnyNode::ExprCompare(_)
            | AnyNode::ExprCall(_)
            | AnyNode::FStringExpressionElement(_)
            | AnyNode::FStringLiteralElement(_)
            | AnyNode::FStringFormatSpec(_)
            | AnyNode::ExprFString(_)
            | AnyNode::ExprStringLiteral(_)
            | AnyNode::ExprBytesLiteral(_)
            | AnyNode::ExprNumberLiteral(_)
            | AnyNode::ExprBooleanLiteral(_)
            | AnyNode::ExprNoneLiteral(_)
            | AnyNode::ExprEllipsisLiteral(_)
            | AnyNode::ExprAttribute(_)
            | AnyNode::ExprSubscript(_)
            | AnyNode::ExprStarred(_)
            | AnyNode::ExprName(_)
            | AnyNode::ExprList(_)
            | AnyNode::ExprTuple(_)
            | AnyNode::ExprSlice(_)
            | AnyNode::ExprIpyEscapeCommand(_)
            | AnyNode::ExceptHandlerExceptHandler(_)
            | AnyNode::PatternMatchValue(_)
            | AnyNode::PatternMatchSingleton(_)
            | AnyNode::PatternMatchSequence(_)
            | AnyNode::PatternMatchMapping(_)
            | AnyNode::PatternMatchClass(_)
            | AnyNode::PatternMatchStar(_)
            | AnyNode::PatternMatchAs(_)
            | AnyNode::PatternMatchOr(_)
            | AnyNode::PatternArguments(_)
            | AnyNode::PatternKeyword(_)
            | AnyNode::Comprehension(_)
            | AnyNode::Arguments(_)
            | AnyNode::Parameters(_)
            | AnyNode::Parameter(_)
            | AnyNode::ParameterWithDefault(_)
            | AnyNode::Keyword(_)
            | AnyNode::Alias(_)
            | AnyNode::WithItem(_)
            | AnyNode::MatchCase(_)
            | AnyNode::Decorator(_)
            | AnyNode::TypeParams(_)
            | AnyNode::TypeParamTypeVar(_)
            | AnyNode::TypeParamTypeVarTuple(_)
            | AnyNode::TypeParamParamSpec(_)
            | AnyNode::FString(_)
            | AnyNode::StringLiteral(_)
            | AnyNode::BytesLiteral(_)
            | AnyNode::ElifElseClause(_) => None,
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<Self::Ref<'_>> {
        match kind {
            AnyNodeRef::StmtFunctionDef(statement) => Some(StatementRef::FunctionDef(statement)),
            AnyNodeRef::StmtClassDef(statement) => Some(StatementRef::ClassDef(statement)),
            AnyNodeRef::StmtReturn(statement) => Some(StatementRef::Return(statement)),
            AnyNodeRef::StmtDelete(statement) => Some(StatementRef::Delete(statement)),
            AnyNodeRef::StmtTypeAlias(statement) => Some(StatementRef::TypeAlias(statement)),
            AnyNodeRef::StmtAssign(statement) => Some(StatementRef::Assign(statement)),
            AnyNodeRef::StmtAugAssign(statement) => Some(StatementRef::AugAssign(statement)),
            AnyNodeRef::StmtAnnAssign(statement) => Some(StatementRef::AnnAssign(statement)),
            AnyNodeRef::StmtFor(statement) => Some(StatementRef::For(statement)),
            AnyNodeRef::StmtWhile(statement) => Some(StatementRef::While(statement)),
            AnyNodeRef::StmtIf(statement) => Some(StatementRef::If(statement)),
            AnyNodeRef::StmtWith(statement) => Some(StatementRef::With(statement)),
            AnyNodeRef::StmtMatch(statement) => Some(StatementRef::Match(statement)),
            AnyNodeRef::StmtRaise(statement) => Some(StatementRef::Raise(statement)),
            AnyNodeRef::StmtTry(statement) => Some(StatementRef::Try(statement)),
            AnyNodeRef::StmtAssert(statement) => Some(StatementRef::Assert(statement)),
            AnyNodeRef::StmtImport(statement) => Some(StatementRef::Import(statement)),
            AnyNodeRef::StmtImportFrom(statement) => Some(StatementRef::ImportFrom(statement)),
            AnyNodeRef::StmtGlobal(statement) => Some(StatementRef::Global(statement)),
            AnyNodeRef::StmtNonlocal(statement) => Some(StatementRef::Nonlocal(statement)),
            AnyNodeRef::StmtExpr(statement) => Some(StatementRef::Expr(statement)),
            AnyNodeRef::StmtPass(statement) => Some(StatementRef::Pass(statement)),
            AnyNodeRef::StmtBreak(statement) => Some(StatementRef::Break(statement)),
            AnyNodeRef::StmtContinue(statement) => Some(StatementRef::Continue(statement)),
            AnyNodeRef::StmtIpyEscapeCommand(statement) => {
                Some(StatementRef::IpyEscapeCommand(statement))
            }
            AnyNodeRef::ModModule(_)
            | AnyNodeRef::ModExpression(_)
            | AnyNodeRef::ExprBoolOp(_)
            | AnyNodeRef::ExprNamed(_)
            | AnyNodeRef::ExprBinOp(_)
            | AnyNodeRef::ExprUnaryOp(_)
            | AnyNodeRef::ExprLambda(_)
            | AnyNodeRef::ExprIf(_)
            | AnyNodeRef::ExprDict(_)
            | AnyNodeRef::ExprSet(_)
            | AnyNodeRef::ExprListComp(_)
            | AnyNodeRef::ExprSetComp(_)
            | AnyNodeRef::ExprDictComp(_)
            | AnyNodeRef::ExprGenerator(_)
            | AnyNodeRef::ExprAwait(_)
            | AnyNodeRef::ExprYield(_)
            | AnyNodeRef::ExprYieldFrom(_)
            | AnyNodeRef::ExprCompare(_)
            | AnyNodeRef::ExprCall(_)
            | AnyNodeRef::FStringExpressionElement(_)
            | AnyNodeRef::FStringLiteralElement(_)
            | AnyNodeRef::FStringFormatSpec(_)
            | AnyNodeRef::ExprFString(_)
            | AnyNodeRef::ExprStringLiteral(_)
            | AnyNodeRef::ExprBytesLiteral(_)
            | AnyNodeRef::ExprNumberLiteral(_)
            | AnyNodeRef::ExprBooleanLiteral(_)
            | AnyNodeRef::ExprNoneLiteral(_)
            | AnyNodeRef::ExprEllipsisLiteral(_)
            | AnyNodeRef::ExprAttribute(_)
            | AnyNodeRef::ExprSubscript(_)
            | AnyNodeRef::ExprStarred(_)
            | AnyNodeRef::ExprName(_)
            | AnyNodeRef::ExprList(_)
            | AnyNodeRef::ExprTuple(_)
            | AnyNodeRef::ExprSlice(_)
            | AnyNodeRef::ExprIpyEscapeCommand(_)
            | AnyNodeRef::ExceptHandlerExceptHandler(_)
            | AnyNodeRef::PatternMatchValue(_)
            | AnyNodeRef::PatternMatchSingleton(_)
            | AnyNodeRef::PatternMatchSequence(_)
            | AnyNodeRef::PatternMatchMapping(_)
            | AnyNodeRef::PatternMatchClass(_)
            | AnyNodeRef::PatternMatchStar(_)
            | AnyNodeRef::PatternMatchAs(_)
            | AnyNodeRef::PatternMatchOr(_)
            | AnyNodeRef::PatternArguments(_)
            | AnyNodeRef::PatternKeyword(_)
            | AnyNodeRef::Comprehension(_)
            | AnyNodeRef::Arguments(_)
            | AnyNodeRef::Parameters(_)
            | AnyNodeRef::Parameter(_)
            | AnyNodeRef::ParameterWithDefault(_)
            | AnyNodeRef::Keyword(_)
            | AnyNodeRef::Alias(_)
            | AnyNodeRef::WithItem(_)
            | AnyNodeRef::MatchCase(_)
            | AnyNodeRef::Decorator(_)
            | AnyNodeRef::TypeParams(_)
            | AnyNodeRef::TypeParamTypeVar(_)
            | AnyNodeRef::TypeParamTypeVarTuple(_)
            | AnyNodeRef::TypeParamParamSpec(_)
            | AnyNodeRef::FString(_)
            | AnyNodeRef::StringLiteral(_)
            | AnyNodeRef::BytesLiteral(_)
            | AnyNodeRef::ElifElseClause(_) => None,
        }
    }

    fn can_cast(kind: NodeKind) -> bool {
        match kind {
            NodeKind::StmtClassDef
            | NodeKind::StmtReturn
            | NodeKind::StmtDelete
            | NodeKind::StmtTypeAlias
            | NodeKind::StmtAssign
            | NodeKind::StmtAugAssign
            | NodeKind::StmtAnnAssign
            | NodeKind::StmtFor
            | NodeKind::StmtWhile
            | NodeKind::StmtIf
            | NodeKind::StmtWith
            | NodeKind::StmtMatch
            | NodeKind::StmtRaise
            | NodeKind::StmtTry
            | NodeKind::StmtAssert
            | NodeKind::StmtImport
            | NodeKind::StmtImportFrom
            | NodeKind::StmtGlobal
            | NodeKind::StmtNonlocal
            | NodeKind::StmtIpyEscapeCommand
            | NodeKind::StmtExpr
            | NodeKind::StmtPass
            | NodeKind::StmtBreak
            | NodeKind::StmtContinue => true,
            NodeKind::ExprBoolOp
            | NodeKind::ModModule
            | NodeKind::ModInteractive
            | NodeKind::ModExpression
            | NodeKind::ModFunctionType
            | NodeKind::StmtFunctionDef
            | NodeKind::ExprNamed
            | NodeKind::ExprBinOp
            | NodeKind::ExprUnaryOp
            | NodeKind::ExprLambda
            | NodeKind::ExprIf
            | NodeKind::ExprDict
            | NodeKind::ExprSet
            | NodeKind::ExprListComp
            | NodeKind::ExprSetComp
            | NodeKind::ExprDictComp
            | NodeKind::ExprGenerator
            | NodeKind::ExprAwait
            | NodeKind::ExprYield
            | NodeKind::ExprYieldFrom
            | NodeKind::ExprCompare
            | NodeKind::ExprCall
            | NodeKind::FStringExpressionElement
            | NodeKind::FStringLiteralElement
            | NodeKind::FStringFormatSpec
            | NodeKind::ExprFString
            | NodeKind::ExprStringLiteral
            | NodeKind::ExprBytesLiteral
            | NodeKind::ExprNumberLiteral
            | NodeKind::ExprBooleanLiteral
            | NodeKind::ExprNoneLiteral
            | NodeKind::ExprEllipsisLiteral
            | NodeKind::ExprAttribute
            | NodeKind::ExprSubscript
            | NodeKind::ExprStarred
            | NodeKind::ExprName
            | NodeKind::ExprList
            | NodeKind::ExprTuple
            | NodeKind::ExprSlice
            | NodeKind::ExprIpyEscapeCommand
            | NodeKind::ExceptHandlerExceptHandler
            | NodeKind::PatternMatchValue
            | NodeKind::PatternMatchSingleton
            | NodeKind::PatternMatchSequence
            | NodeKind::PatternMatchMapping
            | NodeKind::PatternMatchClass
            | NodeKind::PatternMatchStar
            | NodeKind::PatternMatchAs
            | NodeKind::PatternMatchOr
            | NodeKind::PatternArguments
            | NodeKind::PatternKeyword
            | NodeKind::TypeIgnoreTypeIgnore
            | NodeKind::Comprehension
            | NodeKind::Arguments
            | NodeKind::Parameters
            | NodeKind::Parameter
            | NodeKind::ParameterWithDefault
            | NodeKind::Keyword
            | NodeKind::Alias
            | NodeKind::WithItem
            | NodeKind::MatchCase
            | NodeKind::Decorator
            | NodeKind::ElifElseClause
            | NodeKind::TypeParams
            | NodeKind::TypeParamTypeVar
            | NodeKind::TypeParamTypeVarTuple
            | NodeKind::TypeParamParamSpec
            | NodeKind::FString
            | NodeKind::StringLiteral
            | NodeKind::BytesLiteral => false,
        }
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        match self {
            Stmt::FunctionDef(stmt) => stmt.as_any_node_ref(),
            Stmt::ClassDef(stmt) => stmt.as_any_node_ref(),
            Stmt::Return(stmt) => stmt.as_any_node_ref(),
            Stmt::Delete(stmt) => stmt.as_any_node_ref(),
            Stmt::Assign(stmt) => stmt.as_any_node_ref(),
            Stmt::AugAssign(stmt) => stmt.as_any_node_ref(),
            Stmt::AnnAssign(stmt) => stmt.as_any_node_ref(),
            Stmt::TypeAlias(stmt) => stmt.as_any_node_ref(),
            Stmt::For(stmt) => stmt.as_any_node_ref(),
            Stmt::While(stmt) => stmt.as_any_node_ref(),
            Stmt::If(stmt) => stmt.as_any_node_ref(),
            Stmt::With(stmt) => stmt.as_any_node_ref(),
            Stmt::Match(stmt) => stmt.as_any_node_ref(),
            Stmt::Raise(stmt) => stmt.as_any_node_ref(),
            Stmt::Try(stmt) => stmt.as_any_node_ref(),
            Stmt::Assert(stmt) => stmt.as_any_node_ref(),
            Stmt::Import(stmt) => stmt.as_any_node_ref(),
            Stmt::ImportFrom(stmt) => stmt.as_any_node_ref(),
            Stmt::Global(stmt) => stmt.as_any_node_ref(),
            Stmt::Nonlocal(stmt) => stmt.as_any_node_ref(),
            Stmt::Expr(stmt) => stmt.as_any_node_ref(),
            Stmt::Pass(stmt) => stmt.as_any_node_ref(),
            Stmt::Break(stmt) => stmt.as_any_node_ref(),
            Stmt::Continue(stmt) => stmt.as_any_node_ref(),
            Stmt::IpyEscapeCommand(stmt) => stmt.as_any_node_ref(),
        }
    }

    fn into_any_node(self) -> AnyNode {
        match self {
            Stmt::FunctionDef(stmt) => stmt.into_any_node(),
            Stmt::ClassDef(stmt) => stmt.into_any_node(),
            Stmt::Return(stmt) => stmt.into_any_node(),
            Stmt::Delete(stmt) => stmt.into_any_node(),
            Stmt::Assign(stmt) => stmt.into_any_node(),
            Stmt::AugAssign(stmt) => stmt.into_any_node(),
            Stmt::AnnAssign(stmt) => stmt.into_any_node(),
            Stmt::TypeAlias(stmt) => stmt.into_any_node(),
            Stmt::For(stmt) => stmt.into_any_node(),
            Stmt::While(stmt) => stmt.into_any_node(),
            Stmt::If(stmt) => stmt.into_any_node(),
            Stmt::With(stmt) => stmt.into_any_node(),
            Stmt::Match(stmt) => stmt.into_any_node(),
            Stmt::Raise(stmt) => stmt.into_any_node(),
            Stmt::Try(stmt) => stmt.into_any_node(),
            Stmt::Assert(stmt) => stmt.into_any_node(),
            Stmt::Import(stmt) => stmt.into_any_node(),
            Stmt::ImportFrom(stmt) => stmt.into_any_node(),
            Stmt::Global(stmt) => stmt.into_any_node(),
            Stmt::Nonlocal(stmt) => stmt.into_any_node(),
            Stmt::Expr(stmt) => stmt.into_any_node(),
            Stmt::Pass(stmt) => stmt.into_any_node(),
            Stmt::Break(stmt) => stmt.into_any_node(),
            Stmt::Continue(stmt) => stmt.into_any_node(),
            Stmt::IpyEscapeCommand(stmt) => stmt.into_any_node(),
        }
    }

    fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        match self {
            Stmt::FunctionDef(stmt) => stmt.visit_source_order(visitor),
            Stmt::ClassDef(stmt) => stmt.visit_source_order(visitor),
            Stmt::Return(stmt) => stmt.visit_source_order(visitor),
            Stmt::Delete(stmt) => stmt.visit_source_order(visitor),
            Stmt::Assign(stmt) => stmt.visit_source_order(visitor),
            Stmt::AugAssign(stmt) => stmt.visit_source_order(visitor),
            Stmt::AnnAssign(stmt) => stmt.visit_source_order(visitor),
            Stmt::TypeAlias(stmt) => stmt.visit_source_order(visitor),
            Stmt::For(stmt) => stmt.visit_source_order(visitor),
            Stmt::While(stmt) => stmt.visit_source_order(visitor),
            Stmt::If(stmt) => stmt.visit_source_order(visitor),
            Stmt::With(stmt) => stmt.visit_source_order(visitor),
            Stmt::Match(stmt) => stmt.visit_source_order(visitor),
            Stmt::Raise(stmt) => stmt.visit_source_order(visitor),
            Stmt::Try(stmt) => stmt.visit_source_order(visitor),
            Stmt::Assert(stmt) => stmt.visit_source_order(visitor),
            Stmt::Import(stmt) => stmt.visit_source_order(visitor),
            Stmt::ImportFrom(stmt) => stmt.visit_source_order(visitor),
            Stmt::Global(stmt) => stmt.visit_source_order(visitor),
            Stmt::Nonlocal(stmt) => stmt.visit_source_order(visitor),
            Stmt::Expr(stmt) => stmt.visit_source_order(visitor),
            Stmt::Pass(stmt) => stmt.visit_source_order(visitor),
            Stmt::Break(stmt) => stmt.visit_source_order(visitor),
            Stmt::Continue(stmt) => stmt.visit_source_order(visitor),
            Stmt::IpyEscapeCommand(stmt) => stmt.visit_source_order(visitor),
        }
    }
}

impl AstNode for TypeParam {
    type Ref<'a> = TypeParamRef<'a>;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        match kind {
            AnyNode::TypeParamTypeVar(node) => Some(TypeParam::TypeVar(node)),
            AnyNode::TypeParamTypeVarTuple(node) => Some(TypeParam::TypeVarTuple(node)),
            AnyNode::TypeParamParamSpec(node) => Some(TypeParam::ParamSpec(node)),
            _ => None,
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<Self::Ref<'_>> {
        match kind {
            AnyNodeRef::TypeParamTypeVar(node) => Some(TypeParamRef::TypeVar(node)),
            AnyNodeRef::TypeParamTypeVarTuple(node) => Some(TypeParamRef::TypeVarTuple(node)),
            AnyNodeRef::TypeParamParamSpec(node) => Some(TypeParamRef::ParamSpec(node)),
            _ => None,
        }
    }

    fn can_cast(kind: NodeKind) -> bool {
        matches!(
            kind,
            NodeKind::TypeParamTypeVar
                | NodeKind::TypeParamTypeVarTuple
                | NodeKind::TypeParamParamSpec
        )
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        match self {
            TypeParam::TypeVar(node) => node.as_any_node_ref(),
            TypeParam::TypeVarTuple(node) => node.as_any_node_ref(),
            TypeParam::ParamSpec(node) => node.as_any_node_ref(),
        }
    }

    fn into_any_node(self) -> AnyNode {
        match self {
            TypeParam::TypeVar(node) => node.into_any_node(),
            TypeParam::TypeVarTuple(node) => node.into_any_node(),
            TypeParam::ParamSpec(node) => node.into_any_node(),
        }
    }

    fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        match self {
            TypeParam::TypeVar(node) => node.visit_source_order(visitor),
            TypeParam::TypeVarTuple(node) => node.visit_source_order(visitor),
            TypeParam::ParamSpec(node) => node.visit_source_order(visitor),
        }
    }
}

impl From<Stmt> for AnyNode {
    fn from(stmt: Stmt) -> Self {
        match stmt {
            Stmt::FunctionDef(node) => AnyNode::StmtFunctionDef(node),
            Stmt::ClassDef(node) => AnyNode::StmtClassDef(node),
            Stmt::Return(node) => AnyNode::StmtReturn(node),
            Stmt::Delete(node) => AnyNode::StmtDelete(node),
            Stmt::TypeAlias(node) => AnyNode::StmtTypeAlias(node),
            Stmt::Assign(node) => AnyNode::StmtAssign(node),
            Stmt::AugAssign(node) => AnyNode::StmtAugAssign(node),
            Stmt::AnnAssign(node) => AnyNode::StmtAnnAssign(node),
            Stmt::For(node) => AnyNode::StmtFor(node),
            Stmt::While(node) => AnyNode::StmtWhile(node),
            Stmt::If(node) => AnyNode::StmtIf(node),
            Stmt::With(node) => AnyNode::StmtWith(node),
            Stmt::Match(node) => AnyNode::StmtMatch(node),
            Stmt::Raise(node) => AnyNode::StmtRaise(node),
            Stmt::Try(node) => AnyNode::StmtTry(node),
            Stmt::Assert(node) => AnyNode::StmtAssert(node),
            Stmt::Import(node) => AnyNode::StmtImport(node),
            Stmt::ImportFrom(node) => AnyNode::StmtImportFrom(node),
            Stmt::Global(node) => AnyNode::StmtGlobal(node),
            Stmt::Nonlocal(node) => AnyNode::StmtNonlocal(node),
            Stmt::Expr(node) => AnyNode::StmtExpr(node),
            Stmt::Pass(node) => AnyNode::StmtPass(node),
            Stmt::Break(node) => AnyNode::StmtBreak(node),
            Stmt::Continue(node) => AnyNode::StmtContinue(node),
            Stmt::IpyEscapeCommand(node) => AnyNode::StmtIpyEscapeCommand(node),
        }
    }
}

impl From<Expr> for AnyNode {
    fn from(expr: Expr) -> Self {
        match expr {
            Expr::BoolOp(node) => AnyNode::ExprBoolOp(node),
            Expr::Named(node) => AnyNode::ExprNamed(node),
            Expr::BinOp(node) => AnyNode::ExprBinOp(node),
            Expr::UnaryOp(node) => AnyNode::ExprUnaryOp(node),
            Expr::Lambda(node) => AnyNode::ExprLambda(node),
            Expr::If(node) => AnyNode::ExprIf(node),
            Expr::Dict(node) => AnyNode::ExprDict(node),
            Expr::Set(node) => AnyNode::ExprSet(node),
            Expr::ListComp(node) => AnyNode::ExprListComp(node),
            Expr::SetComp(node) => AnyNode::ExprSetComp(node),
            Expr::DictComp(node) => AnyNode::ExprDictComp(node),
            Expr::Generator(node) => AnyNode::ExprGenerator(node),
            Expr::Await(node) => AnyNode::ExprAwait(node),
            Expr::Yield(node) => AnyNode::ExprYield(node),
            Expr::YieldFrom(node) => AnyNode::ExprYieldFrom(node),
            Expr::Compare(node) => AnyNode::ExprCompare(node),
            Expr::Call(node) => AnyNode::ExprCall(node),
            Expr::FString(node) => AnyNode::ExprFString(node),
            Expr::StringLiteral(node) => AnyNode::ExprStringLiteral(node),
            Expr::BytesLiteral(node) => AnyNode::ExprBytesLiteral(node),
            Expr::NumberLiteral(node) => AnyNode::ExprNumberLiteral(node),
            Expr::BooleanLiteral(node) => AnyNode::ExprBooleanLiteral(node),
            Expr::NoneLiteral(node) => AnyNode::ExprNoneLiteral(node),
            Expr::EllipsisLiteral(node) => AnyNode::ExprEllipsisLiteral(node),
            Expr::Attribute(node) => AnyNode::ExprAttribute(node),
            Expr::Subscript(node) => AnyNode::ExprSubscript(node),
            Expr::Starred(node) => AnyNode::ExprStarred(node),
            Expr::Name(node) => AnyNode::ExprName(node),
            Expr::List(node) => AnyNode::ExprList(node),
            Expr::Tuple(node) => AnyNode::ExprTuple(node),
            Expr::Slice(node) => AnyNode::ExprSlice(node),
            Expr::IpyEscapeCommand(node) => AnyNode::ExprIpyEscapeCommand(node),
        }
    }
}

impl From<Mod> for AnyNode {
    fn from(module: Mod) -> Self {
        match module {
            Mod::Module(node) => AnyNode::ModModule(node),
            Mod::Expression(node) => AnyNode::ModExpression(node),
        }
    }
}

impl From<FStringElement> for AnyNode {
    fn from(element: FStringElement) -> Self {
        match element {
            FStringElement::Literal(node) => AnyNode::FStringLiteralElement(node),
            FStringElement::Expression(node) => AnyNode::FStringExpressionElement(node),
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

impl From<ast::ModModule> for AnyNode {
    fn from(node: ast::ModModule) -> Self {
        AnyNode::ModModule(node)
    }
}

impl From<ast::ModExpression> for AnyNode {
    fn from(node: ast::ModExpression) -> Self {
        AnyNode::ModExpression(node)
    }
}

impl From<ast::StmtFunctionDef> for AnyNode {
    fn from(node: ast::StmtFunctionDef) -> Self {
        AnyNode::StmtFunctionDef(node)
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

impl From<ast::StmtTypeAlias> for AnyNode {
    fn from(node: ast::StmtTypeAlias) -> Self {
        AnyNode::StmtTypeAlias(node)
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

impl From<ast::ElifElseClause> for AnyNode {
    fn from(node: ast::ElifElseClause) -> Self {
        AnyNode::ElifElseClause(node)
    }
}

impl From<ast::StmtWith> for AnyNode {
    fn from(node: ast::StmtWith) -> Self {
        AnyNode::StmtWith(node)
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

impl From<ast::StmtIpyEscapeCommand> for AnyNode {
    fn from(node: ast::StmtIpyEscapeCommand) -> Self {
        AnyNode::StmtIpyEscapeCommand(node)
    }
}

impl From<ast::ExprBoolOp> for AnyNode {
    fn from(node: ast::ExprBoolOp) -> Self {
        AnyNode::ExprBoolOp(node)
    }
}

impl From<ast::ExprNamed> for AnyNode {
    fn from(node: ast::ExprNamed) -> Self {
        AnyNode::ExprNamed(node)
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

impl From<ast::ExprIf> for AnyNode {
    fn from(node: ast::ExprIf) -> Self {
        AnyNode::ExprIf(node)
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

impl From<ast::ExprGenerator> for AnyNode {
    fn from(node: ast::ExprGenerator) -> Self {
        AnyNode::ExprGenerator(node)
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

impl From<ast::FStringExpressionElement> for AnyNode {
    fn from(node: ast::FStringExpressionElement) -> Self {
        AnyNode::FStringExpressionElement(node)
    }
}

impl From<ast::FStringLiteralElement> for AnyNode {
    fn from(node: ast::FStringLiteralElement) -> Self {
        AnyNode::FStringLiteralElement(node)
    }
}

impl From<ast::FStringFormatSpec> for AnyNode {
    fn from(node: ast::FStringFormatSpec) -> Self {
        AnyNode::FStringFormatSpec(node)
    }
}

impl From<ast::ExprFString> for AnyNode {
    fn from(node: ast::ExprFString) -> Self {
        AnyNode::ExprFString(node)
    }
}

impl From<ast::ExprStringLiteral> for AnyNode {
    fn from(node: ast::ExprStringLiteral) -> Self {
        AnyNode::ExprStringLiteral(node)
    }
}

impl From<ast::ExprBytesLiteral> for AnyNode {
    fn from(node: ast::ExprBytesLiteral) -> Self {
        AnyNode::ExprBytesLiteral(node)
    }
}

impl From<ast::ExprNumberLiteral> for AnyNode {
    fn from(node: ast::ExprNumberLiteral) -> Self {
        AnyNode::ExprNumberLiteral(node)
    }
}

impl From<ast::ExprBooleanLiteral> for AnyNode {
    fn from(node: ast::ExprBooleanLiteral) -> Self {
        AnyNode::ExprBooleanLiteral(node)
    }
}

impl From<ast::ExprNoneLiteral> for AnyNode {
    fn from(node: ast::ExprNoneLiteral) -> Self {
        AnyNode::ExprNoneLiteral(node)
    }
}

impl From<ast::ExprEllipsisLiteral> for AnyNode {
    fn from(node: ast::ExprEllipsisLiteral) -> Self {
        AnyNode::ExprEllipsisLiteral(node)
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

impl From<ast::ExprIpyEscapeCommand> for AnyNode {
    fn from(node: ast::ExprIpyEscapeCommand) -> Self {
        AnyNode::ExprIpyEscapeCommand(node)
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

impl From<PatternArguments> for AnyNode {
    fn from(node: PatternArguments) -> Self {
        AnyNode::PatternArguments(node)
    }
}

impl From<PatternKeyword> for AnyNode {
    fn from(node: PatternKeyword) -> Self {
        AnyNode::PatternKeyword(node)
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
impl From<Parameters> for AnyNode {
    fn from(node: Parameters) -> Self {
        AnyNode::Parameters(node)
    }
}
impl From<Parameter> for AnyNode {
    fn from(node: Parameter) -> Self {
        AnyNode::Parameter(node)
    }
}
impl From<ParameterWithDefault> for AnyNode {
    fn from(node: ParameterWithDefault) -> Self {
        AnyNode::ParameterWithDefault(node)
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
impl From<TypeParams> for AnyNode {
    fn from(node: TypeParams) -> Self {
        AnyNode::TypeParams(node)
    }
}
impl From<TypeParamTypeVar> for AnyNode {
    fn from(node: TypeParamTypeVar) -> Self {
        AnyNode::TypeParamTypeVar(node)
    }
}

impl From<TypeParamTypeVarTuple> for AnyNode {
    fn from(node: TypeParamTypeVarTuple) -> Self {
        AnyNode::TypeParamTypeVarTuple(node)
    }
}

impl From<TypeParamParamSpec> for AnyNode {
    fn from(node: TypeParamParamSpec) -> Self {
        AnyNode::TypeParamParamSpec(node)
    }
}

impl From<ast::FString> for AnyNode {
    fn from(node: ast::FString) -> Self {
        AnyNode::FString(node)
    }
}

impl From<ast::StringLiteral> for AnyNode {
    fn from(node: ast::StringLiteral) -> Self {
        AnyNode::StringLiteral(node)
    }
}

impl From<ast::BytesLiteral> for AnyNode {
    fn from(node: ast::BytesLiteral) -> Self {
        AnyNode::BytesLiteral(node)
    }
}

impl Ranged for AnyNode {
    fn range(&self) -> TextRange {
        match self {
            AnyNode::ModModule(node) => node.range(),
            AnyNode::ModExpression(node) => node.range(),
            AnyNode::StmtFunctionDef(node) => node.range(),
            AnyNode::StmtClassDef(node) => node.range(),
            AnyNode::StmtReturn(node) => node.range(),
            AnyNode::StmtDelete(node) => node.range(),
            AnyNode::StmtTypeAlias(node) => node.range(),
            AnyNode::StmtAssign(node) => node.range(),
            AnyNode::StmtAugAssign(node) => node.range(),
            AnyNode::StmtAnnAssign(node) => node.range(),
            AnyNode::StmtFor(node) => node.range(),
            AnyNode::StmtWhile(node) => node.range(),
            AnyNode::StmtIf(node) => node.range(),
            AnyNode::StmtWith(node) => node.range(),
            AnyNode::StmtMatch(node) => node.range(),
            AnyNode::StmtRaise(node) => node.range(),
            AnyNode::StmtTry(node) => node.range(),
            AnyNode::StmtAssert(node) => node.range(),
            AnyNode::StmtImport(node) => node.range(),
            AnyNode::StmtImportFrom(node) => node.range(),
            AnyNode::StmtGlobal(node) => node.range(),
            AnyNode::StmtNonlocal(node) => node.range(),
            AnyNode::StmtExpr(node) => node.range(),
            AnyNode::StmtPass(node) => node.range(),
            AnyNode::StmtBreak(node) => node.range(),
            AnyNode::StmtContinue(node) => node.range(),
            AnyNode::StmtIpyEscapeCommand(node) => node.range(),
            AnyNode::ExprBoolOp(node) => node.range(),
            AnyNode::ExprNamed(node) => node.range(),
            AnyNode::ExprBinOp(node) => node.range(),
            AnyNode::ExprUnaryOp(node) => node.range(),
            AnyNode::ExprLambda(node) => node.range(),
            AnyNode::ExprIf(node) => node.range(),
            AnyNode::ExprDict(node) => node.range(),
            AnyNode::ExprSet(node) => node.range(),
            AnyNode::ExprListComp(node) => node.range(),
            AnyNode::ExprSetComp(node) => node.range(),
            AnyNode::ExprDictComp(node) => node.range(),
            AnyNode::ExprGenerator(node) => node.range(),
            AnyNode::ExprAwait(node) => node.range(),
            AnyNode::ExprYield(node) => node.range(),
            AnyNode::ExprYieldFrom(node) => node.range(),
            AnyNode::ExprCompare(node) => node.range(),
            AnyNode::ExprCall(node) => node.range(),
            AnyNode::FStringExpressionElement(node) => node.range(),
            AnyNode::FStringLiteralElement(node) => node.range(),
            AnyNode::FStringFormatSpec(node) => node.range(),
            AnyNode::ExprFString(node) => node.range(),
            AnyNode::ExprStringLiteral(node) => node.range(),
            AnyNode::ExprBytesLiteral(node) => node.range(),
            AnyNode::ExprNumberLiteral(node) => node.range(),
            AnyNode::ExprBooleanLiteral(node) => node.range(),
            AnyNode::ExprNoneLiteral(node) => node.range(),
            AnyNode::ExprEllipsisLiteral(node) => node.range(),
            AnyNode::ExprAttribute(node) => node.range(),
            AnyNode::ExprSubscript(node) => node.range(),
            AnyNode::ExprStarred(node) => node.range(),
            AnyNode::ExprName(node) => node.range(),
            AnyNode::ExprList(node) => node.range(),
            AnyNode::ExprTuple(node) => node.range(),
            AnyNode::ExprSlice(node) => node.range(),
            AnyNode::ExprIpyEscapeCommand(node) => node.range(),
            AnyNode::ExceptHandlerExceptHandler(node) => node.range(),
            AnyNode::PatternMatchValue(node) => node.range(),
            AnyNode::PatternMatchSingleton(node) => node.range(),
            AnyNode::PatternMatchSequence(node) => node.range(),
            AnyNode::PatternMatchMapping(node) => node.range(),
            AnyNode::PatternMatchClass(node) => node.range(),
            AnyNode::PatternMatchStar(node) => node.range(),
            AnyNode::PatternMatchAs(node) => node.range(),
            AnyNode::PatternMatchOr(node) => node.range(),
            AnyNode::PatternArguments(node) => node.range(),
            AnyNode::PatternKeyword(node) => node.range(),
            AnyNode::Comprehension(node) => node.range(),
            AnyNode::Arguments(node) => node.range(),
            AnyNode::Parameters(node) => node.range(),
            AnyNode::Parameter(node) => node.range(),
            AnyNode::ParameterWithDefault(node) => node.range(),
            AnyNode::Keyword(node) => node.range(),
            AnyNode::Alias(node) => node.range(),
            AnyNode::WithItem(node) => node.range(),
            AnyNode::MatchCase(node) => node.range(),
            AnyNode::Decorator(node) => node.range(),
            AnyNode::TypeParams(node) => node.range(),
            AnyNode::TypeParamTypeVar(node) => node.range(),
            AnyNode::TypeParamTypeVarTuple(node) => node.range(),
            AnyNode::TypeParamParamSpec(node) => node.range(),
            AnyNode::FString(node) => node.range(),
            AnyNode::StringLiteral(node) => node.range(),
            AnyNode::BytesLiteral(node) => node.range(),
            AnyNode::ElifElseClause(node) => node.range(),
        }
    }
}

#[derive(Copy, Clone, Debug, is_macro::Is, PartialEq)]
pub enum AnyNodeRef<'a> {
    ModModule(&'a ast::ModModule),
    ModExpression(&'a ast::ModExpression),
    StmtFunctionDef(&'a ast::StmtFunctionDef),
    StmtClassDef(&'a ast::StmtClassDef),
    StmtReturn(&'a ast::StmtReturn),
    StmtDelete(&'a ast::StmtDelete),
    StmtTypeAlias(&'a ast::StmtTypeAlias),
    StmtAssign(&'a ast::StmtAssign),
    StmtAugAssign(&'a ast::StmtAugAssign),
    StmtAnnAssign(&'a ast::StmtAnnAssign),
    StmtFor(&'a ast::StmtFor),
    StmtWhile(&'a ast::StmtWhile),
    StmtIf(&'a ast::StmtIf),
    StmtWith(&'a ast::StmtWith),
    StmtMatch(&'a ast::StmtMatch),
    StmtRaise(&'a ast::StmtRaise),
    StmtTry(&'a ast::StmtTry),
    StmtAssert(&'a ast::StmtAssert),
    StmtImport(&'a ast::StmtImport),
    StmtImportFrom(&'a ast::StmtImportFrom),
    StmtGlobal(&'a ast::StmtGlobal),
    StmtNonlocal(&'a ast::StmtNonlocal),
    StmtExpr(&'a ast::StmtExpr),
    StmtPass(&'a ast::StmtPass),
    StmtBreak(&'a ast::StmtBreak),
    StmtContinue(&'a ast::StmtContinue),
    StmtIpyEscapeCommand(&'a ast::StmtIpyEscapeCommand),
    ExprBoolOp(&'a ast::ExprBoolOp),
    ExprNamed(&'a ast::ExprNamed),
    ExprBinOp(&'a ast::ExprBinOp),
    ExprUnaryOp(&'a ast::ExprUnaryOp),
    ExprLambda(&'a ast::ExprLambda),
    ExprIf(&'a ast::ExprIf),
    ExprDict(&'a ast::ExprDict),
    ExprSet(&'a ast::ExprSet),
    ExprListComp(&'a ast::ExprListComp),
    ExprSetComp(&'a ast::ExprSetComp),
    ExprDictComp(&'a ast::ExprDictComp),
    ExprGenerator(&'a ast::ExprGenerator),
    ExprAwait(&'a ast::ExprAwait),
    ExprYield(&'a ast::ExprYield),
    ExprYieldFrom(&'a ast::ExprYieldFrom),
    ExprCompare(&'a ast::ExprCompare),
    ExprCall(&'a ast::ExprCall),
    FStringExpressionElement(&'a ast::FStringExpressionElement),
    FStringLiteralElement(&'a ast::FStringLiteralElement),
    FStringFormatSpec(&'a ast::FStringFormatSpec),
    ExprFString(&'a ast::ExprFString),
    ExprStringLiteral(&'a ast::ExprStringLiteral),
    ExprBytesLiteral(&'a ast::ExprBytesLiteral),
    ExprNumberLiteral(&'a ast::ExprNumberLiteral),
    ExprBooleanLiteral(&'a ast::ExprBooleanLiteral),
    ExprNoneLiteral(&'a ast::ExprNoneLiteral),
    ExprEllipsisLiteral(&'a ast::ExprEllipsisLiteral),
    ExprAttribute(&'a ast::ExprAttribute),
    ExprSubscript(&'a ast::ExprSubscript),
    ExprStarred(&'a ast::ExprStarred),
    ExprName(&'a ast::ExprName),
    ExprList(&'a ast::ExprList),
    ExprTuple(&'a ast::ExprTuple),
    ExprSlice(&'a ast::ExprSlice),
    ExprIpyEscapeCommand(&'a ast::ExprIpyEscapeCommand),
    ExceptHandlerExceptHandler(&'a ast::ExceptHandlerExceptHandler),
    PatternMatchValue(&'a ast::PatternMatchValue),
    PatternMatchSingleton(&'a ast::PatternMatchSingleton),
    PatternMatchSequence(&'a ast::PatternMatchSequence),
    PatternMatchMapping(&'a ast::PatternMatchMapping),
    PatternMatchClass(&'a ast::PatternMatchClass),
    PatternMatchStar(&'a ast::PatternMatchStar),
    PatternMatchAs(&'a ast::PatternMatchAs),
    PatternMatchOr(&'a ast::PatternMatchOr),
    PatternArguments(&'a ast::PatternArguments),
    PatternKeyword(&'a ast::PatternKeyword),
    Comprehension(&'a Comprehension),
    Arguments(&'a Arguments),
    Parameters(&'a Parameters),
    Parameter(&'a Parameter),
    ParameterWithDefault(&'a ParameterWithDefault),
    Keyword(&'a Keyword),
    Alias(&'a Alias),
    WithItem(&'a WithItem),
    MatchCase(&'a MatchCase),
    Decorator(&'a Decorator),
    TypeParams(&'a TypeParams),
    TypeParamTypeVar(&'a TypeParamTypeVar),
    TypeParamTypeVarTuple(&'a TypeParamTypeVarTuple),
    TypeParamParamSpec(&'a TypeParamParamSpec),
    FString(&'a ast::FString),
    StringLiteral(&'a ast::StringLiteral),
    BytesLiteral(&'a ast::BytesLiteral),
    ElifElseClause(&'a ast::ElifElseClause),
}

impl<'a> AnyNodeRef<'a> {
    pub fn as_ptr(&self) -> NonNull<()> {
        match self {
            AnyNodeRef::ModModule(node) => NonNull::from(*node).cast(),
            AnyNodeRef::ModExpression(node) => NonNull::from(*node).cast(),
            AnyNodeRef::StmtFunctionDef(node) => NonNull::from(*node).cast(),
            AnyNodeRef::StmtClassDef(node) => NonNull::from(*node).cast(),
            AnyNodeRef::StmtReturn(node) => NonNull::from(*node).cast(),
            AnyNodeRef::StmtDelete(node) => NonNull::from(*node).cast(),
            AnyNodeRef::StmtTypeAlias(node) => NonNull::from(*node).cast(),
            AnyNodeRef::StmtAssign(node) => NonNull::from(*node).cast(),
            AnyNodeRef::StmtAugAssign(node) => NonNull::from(*node).cast(),
            AnyNodeRef::StmtAnnAssign(node) => NonNull::from(*node).cast(),
            AnyNodeRef::StmtFor(node) => NonNull::from(*node).cast(),
            AnyNodeRef::StmtWhile(node) => NonNull::from(*node).cast(),
            AnyNodeRef::StmtIf(node) => NonNull::from(*node).cast(),
            AnyNodeRef::StmtWith(node) => NonNull::from(*node).cast(),
            AnyNodeRef::StmtMatch(node) => NonNull::from(*node).cast(),
            AnyNodeRef::StmtRaise(node) => NonNull::from(*node).cast(),
            AnyNodeRef::StmtTry(node) => NonNull::from(*node).cast(),
            AnyNodeRef::StmtAssert(node) => NonNull::from(*node).cast(),
            AnyNodeRef::StmtImport(node) => NonNull::from(*node).cast(),
            AnyNodeRef::StmtImportFrom(node) => NonNull::from(*node).cast(),
            AnyNodeRef::StmtGlobal(node) => NonNull::from(*node).cast(),
            AnyNodeRef::StmtNonlocal(node) => NonNull::from(*node).cast(),
            AnyNodeRef::StmtExpr(node) => NonNull::from(*node).cast(),
            AnyNodeRef::StmtPass(node) => NonNull::from(*node).cast(),
            AnyNodeRef::StmtBreak(node) => NonNull::from(*node).cast(),
            AnyNodeRef::StmtContinue(node) => NonNull::from(*node).cast(),
            AnyNodeRef::StmtIpyEscapeCommand(node) => NonNull::from(*node).cast(),
            AnyNodeRef::ExprBoolOp(node) => NonNull::from(*node).cast(),
            AnyNodeRef::ExprNamed(node) => NonNull::from(*node).cast(),
            AnyNodeRef::ExprBinOp(node) => NonNull::from(*node).cast(),
            AnyNodeRef::ExprUnaryOp(node) => NonNull::from(*node).cast(),
            AnyNodeRef::ExprLambda(node) => NonNull::from(*node).cast(),
            AnyNodeRef::ExprIf(node) => NonNull::from(*node).cast(),
            AnyNodeRef::ExprDict(node) => NonNull::from(*node).cast(),
            AnyNodeRef::ExprSet(node) => NonNull::from(*node).cast(),
            AnyNodeRef::ExprListComp(node) => NonNull::from(*node).cast(),
            AnyNodeRef::ExprSetComp(node) => NonNull::from(*node).cast(),
            AnyNodeRef::ExprDictComp(node) => NonNull::from(*node).cast(),
            AnyNodeRef::ExprGenerator(node) => NonNull::from(*node).cast(),
            AnyNodeRef::ExprAwait(node) => NonNull::from(*node).cast(),
            AnyNodeRef::ExprYield(node) => NonNull::from(*node).cast(),
            AnyNodeRef::ExprYieldFrom(node) => NonNull::from(*node).cast(),
            AnyNodeRef::ExprCompare(node) => NonNull::from(*node).cast(),
            AnyNodeRef::ExprCall(node) => NonNull::from(*node).cast(),
            AnyNodeRef::FStringExpressionElement(node) => NonNull::from(*node).cast(),
            AnyNodeRef::FStringLiteralElement(node) => NonNull::from(*node).cast(),
            AnyNodeRef::FStringFormatSpec(node) => NonNull::from(*node).cast(),
            AnyNodeRef::ExprFString(node) => NonNull::from(*node).cast(),
            AnyNodeRef::ExprStringLiteral(node) => NonNull::from(*node).cast(),
            AnyNodeRef::ExprBytesLiteral(node) => NonNull::from(*node).cast(),
            AnyNodeRef::ExprNumberLiteral(node) => NonNull::from(*node).cast(),
            AnyNodeRef::ExprBooleanLiteral(node) => NonNull::from(*node).cast(),
            AnyNodeRef::ExprNoneLiteral(node) => NonNull::from(*node).cast(),
            AnyNodeRef::ExprEllipsisLiteral(node) => NonNull::from(*node).cast(),
            AnyNodeRef::ExprAttribute(node) => NonNull::from(*node).cast(),
            AnyNodeRef::ExprSubscript(node) => NonNull::from(*node).cast(),
            AnyNodeRef::ExprStarred(node) => NonNull::from(*node).cast(),
            AnyNodeRef::ExprName(node) => NonNull::from(*node).cast(),
            AnyNodeRef::ExprList(node) => NonNull::from(*node).cast(),
            AnyNodeRef::ExprTuple(node) => NonNull::from(*node).cast(),
            AnyNodeRef::ExprSlice(node) => NonNull::from(*node).cast(),
            AnyNodeRef::ExprIpyEscapeCommand(node) => NonNull::from(*node).cast(),
            AnyNodeRef::ExceptHandlerExceptHandler(node) => NonNull::from(*node).cast(),
            AnyNodeRef::PatternMatchValue(node) => NonNull::from(*node).cast(),
            AnyNodeRef::PatternMatchSingleton(node) => NonNull::from(*node).cast(),
            AnyNodeRef::PatternMatchSequence(node) => NonNull::from(*node).cast(),
            AnyNodeRef::PatternMatchMapping(node) => NonNull::from(*node).cast(),
            AnyNodeRef::PatternMatchClass(node) => NonNull::from(*node).cast(),
            AnyNodeRef::PatternMatchStar(node) => NonNull::from(*node).cast(),
            AnyNodeRef::PatternMatchAs(node) => NonNull::from(*node).cast(),
            AnyNodeRef::PatternMatchOr(node) => NonNull::from(*node).cast(),
            AnyNodeRef::PatternArguments(node) => NonNull::from(*node).cast(),
            AnyNodeRef::PatternKeyword(node) => NonNull::from(*node).cast(),
            AnyNodeRef::Comprehension(node) => NonNull::from(*node).cast(),
            AnyNodeRef::Arguments(node) => NonNull::from(*node).cast(),
            AnyNodeRef::Parameters(node) => NonNull::from(*node).cast(),
            AnyNodeRef::Parameter(node) => NonNull::from(*node).cast(),
            AnyNodeRef::ParameterWithDefault(node) => NonNull::from(*node).cast(),
            AnyNodeRef::Keyword(node) => NonNull::from(*node).cast(),
            AnyNodeRef::Alias(node) => NonNull::from(*node).cast(),
            AnyNodeRef::WithItem(node) => NonNull::from(*node).cast(),
            AnyNodeRef::MatchCase(node) => NonNull::from(*node).cast(),
            AnyNodeRef::Decorator(node) => NonNull::from(*node).cast(),
            AnyNodeRef::TypeParams(node) => NonNull::from(*node).cast(),
            AnyNodeRef::TypeParamTypeVar(node) => NonNull::from(*node).cast(),
            AnyNodeRef::TypeParamTypeVarTuple(node) => NonNull::from(*node).cast(),
            AnyNodeRef::TypeParamParamSpec(node) => NonNull::from(*node).cast(),
            AnyNodeRef::FString(node) => NonNull::from(*node).cast(),
            AnyNodeRef::StringLiteral(node) => NonNull::from(*node).cast(),
            AnyNodeRef::BytesLiteral(node) => NonNull::from(*node).cast(),
            AnyNodeRef::ElifElseClause(node) => NonNull::from(*node).cast(),
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
            AnyNodeRef::ModExpression(_) => NodeKind::ModExpression,
            AnyNodeRef::StmtFunctionDef(_) => NodeKind::StmtFunctionDef,
            AnyNodeRef::StmtClassDef(_) => NodeKind::StmtClassDef,
            AnyNodeRef::StmtReturn(_) => NodeKind::StmtReturn,
            AnyNodeRef::StmtDelete(_) => NodeKind::StmtDelete,
            AnyNodeRef::StmtTypeAlias(_) => NodeKind::StmtTypeAlias,
            AnyNodeRef::StmtAssign(_) => NodeKind::StmtAssign,
            AnyNodeRef::StmtAugAssign(_) => NodeKind::StmtAugAssign,
            AnyNodeRef::StmtAnnAssign(_) => NodeKind::StmtAnnAssign,
            AnyNodeRef::StmtFor(_) => NodeKind::StmtFor,
            AnyNodeRef::StmtWhile(_) => NodeKind::StmtWhile,
            AnyNodeRef::StmtIf(_) => NodeKind::StmtIf,
            AnyNodeRef::StmtWith(_) => NodeKind::StmtWith,
            AnyNodeRef::StmtMatch(_) => NodeKind::StmtMatch,
            AnyNodeRef::StmtRaise(_) => NodeKind::StmtRaise,
            AnyNodeRef::StmtTry(_) => NodeKind::StmtTry,
            AnyNodeRef::StmtAssert(_) => NodeKind::StmtAssert,
            AnyNodeRef::StmtImport(_) => NodeKind::StmtImport,
            AnyNodeRef::StmtImportFrom(_) => NodeKind::StmtImportFrom,
            AnyNodeRef::StmtGlobal(_) => NodeKind::StmtGlobal,
            AnyNodeRef::StmtNonlocal(_) => NodeKind::StmtNonlocal,
            AnyNodeRef::StmtExpr(_) => NodeKind::StmtExpr,
            AnyNodeRef::StmtPass(_) => NodeKind::StmtPass,
            AnyNodeRef::StmtBreak(_) => NodeKind::StmtBreak,
            AnyNodeRef::StmtContinue(_) => NodeKind::StmtContinue,
            AnyNodeRef::StmtIpyEscapeCommand(_) => NodeKind::StmtIpyEscapeCommand,
            AnyNodeRef::ExprBoolOp(_) => NodeKind::ExprBoolOp,
            AnyNodeRef::ExprNamed(_) => NodeKind::ExprNamed,
            AnyNodeRef::ExprBinOp(_) => NodeKind::ExprBinOp,
            AnyNodeRef::ExprUnaryOp(_) => NodeKind::ExprUnaryOp,
            AnyNodeRef::ExprLambda(_) => NodeKind::ExprLambda,
            AnyNodeRef::ExprIf(_) => NodeKind::ExprIf,
            AnyNodeRef::ExprDict(_) => NodeKind::ExprDict,
            AnyNodeRef::ExprSet(_) => NodeKind::ExprSet,
            AnyNodeRef::ExprListComp(_) => NodeKind::ExprListComp,
            AnyNodeRef::ExprSetComp(_) => NodeKind::ExprSetComp,
            AnyNodeRef::ExprDictComp(_) => NodeKind::ExprDictComp,
            AnyNodeRef::ExprGenerator(_) => NodeKind::ExprGenerator,
            AnyNodeRef::ExprAwait(_) => NodeKind::ExprAwait,
            AnyNodeRef::ExprYield(_) => NodeKind::ExprYield,
            AnyNodeRef::ExprYieldFrom(_) => NodeKind::ExprYieldFrom,
            AnyNodeRef::ExprCompare(_) => NodeKind::ExprCompare,
            AnyNodeRef::ExprCall(_) => NodeKind::ExprCall,
            AnyNodeRef::FStringExpressionElement(_) => NodeKind::FStringExpressionElement,
            AnyNodeRef::FStringLiteralElement(_) => NodeKind::FStringLiteralElement,
            AnyNodeRef::FStringFormatSpec(_) => NodeKind::FStringFormatSpec,
            AnyNodeRef::ExprFString(_) => NodeKind::ExprFString,
            AnyNodeRef::ExprStringLiteral(_) => NodeKind::ExprStringLiteral,
            AnyNodeRef::ExprBytesLiteral(_) => NodeKind::ExprBytesLiteral,
            AnyNodeRef::ExprNumberLiteral(_) => NodeKind::ExprNumberLiteral,
            AnyNodeRef::ExprBooleanLiteral(_) => NodeKind::ExprBooleanLiteral,
            AnyNodeRef::ExprNoneLiteral(_) => NodeKind::ExprNoneLiteral,
            AnyNodeRef::ExprEllipsisLiteral(_) => NodeKind::ExprEllipsisLiteral,
            AnyNodeRef::ExprAttribute(_) => NodeKind::ExprAttribute,
            AnyNodeRef::ExprSubscript(_) => NodeKind::ExprSubscript,
            AnyNodeRef::ExprStarred(_) => NodeKind::ExprStarred,
            AnyNodeRef::ExprName(_) => NodeKind::ExprName,
            AnyNodeRef::ExprList(_) => NodeKind::ExprList,
            AnyNodeRef::ExprTuple(_) => NodeKind::ExprTuple,
            AnyNodeRef::ExprSlice(_) => NodeKind::ExprSlice,
            AnyNodeRef::ExprIpyEscapeCommand(_) => NodeKind::ExprIpyEscapeCommand,
            AnyNodeRef::ExceptHandlerExceptHandler(_) => NodeKind::ExceptHandlerExceptHandler,
            AnyNodeRef::PatternMatchValue(_) => NodeKind::PatternMatchValue,
            AnyNodeRef::PatternMatchSingleton(_) => NodeKind::PatternMatchSingleton,
            AnyNodeRef::PatternMatchSequence(_) => NodeKind::PatternMatchSequence,
            AnyNodeRef::PatternMatchMapping(_) => NodeKind::PatternMatchMapping,
            AnyNodeRef::PatternMatchClass(_) => NodeKind::PatternMatchClass,
            AnyNodeRef::PatternMatchStar(_) => NodeKind::PatternMatchStar,
            AnyNodeRef::PatternMatchAs(_) => NodeKind::PatternMatchAs,
            AnyNodeRef::PatternMatchOr(_) => NodeKind::PatternMatchOr,
            AnyNodeRef::PatternArguments(_) => NodeKind::PatternArguments,
            AnyNodeRef::PatternKeyword(_) => NodeKind::PatternKeyword,
            AnyNodeRef::Comprehension(_) => NodeKind::Comprehension,
            AnyNodeRef::Arguments(_) => NodeKind::Arguments,
            AnyNodeRef::Parameters(_) => NodeKind::Parameters,
            AnyNodeRef::Parameter(_) => NodeKind::Parameter,
            AnyNodeRef::ParameterWithDefault(_) => NodeKind::ParameterWithDefault,
            AnyNodeRef::Keyword(_) => NodeKind::Keyword,
            AnyNodeRef::Alias(_) => NodeKind::Alias,
            AnyNodeRef::WithItem(_) => NodeKind::WithItem,
            AnyNodeRef::MatchCase(_) => NodeKind::MatchCase,
            AnyNodeRef::Decorator(_) => NodeKind::Decorator,
            AnyNodeRef::TypeParams(_) => NodeKind::TypeParams,
            AnyNodeRef::TypeParamTypeVar(_) => NodeKind::TypeParamTypeVar,
            AnyNodeRef::TypeParamTypeVarTuple(_) => NodeKind::TypeParamTypeVarTuple,
            AnyNodeRef::TypeParamParamSpec(_) => NodeKind::TypeParamParamSpec,
            AnyNodeRef::FString(_) => NodeKind::FString,
            AnyNodeRef::StringLiteral(_) => NodeKind::StringLiteral,
            AnyNodeRef::BytesLiteral(_) => NodeKind::BytesLiteral,
            AnyNodeRef::ElifElseClause(_) => NodeKind::ElifElseClause,
        }
    }

    pub const fn is_statement(self) -> bool {
        match self {
            AnyNodeRef::StmtFunctionDef(_)
            | AnyNodeRef::StmtClassDef(_)
            | AnyNodeRef::StmtReturn(_)
            | AnyNodeRef::StmtDelete(_)
            | AnyNodeRef::StmtTypeAlias(_)
            | AnyNodeRef::StmtAssign(_)
            | AnyNodeRef::StmtAugAssign(_)
            | AnyNodeRef::StmtAnnAssign(_)
            | AnyNodeRef::StmtFor(_)
            | AnyNodeRef::StmtWhile(_)
            | AnyNodeRef::StmtIf(_)
            | AnyNodeRef::StmtWith(_)
            | AnyNodeRef::StmtMatch(_)
            | AnyNodeRef::StmtRaise(_)
            | AnyNodeRef::StmtTry(_)
            | AnyNodeRef::StmtAssert(_)
            | AnyNodeRef::StmtImport(_)
            | AnyNodeRef::StmtImportFrom(_)
            | AnyNodeRef::StmtGlobal(_)
            | AnyNodeRef::StmtNonlocal(_)
            | AnyNodeRef::StmtExpr(_)
            | AnyNodeRef::StmtPass(_)
            | AnyNodeRef::StmtBreak(_)
            | AnyNodeRef::StmtContinue(_)
            | AnyNodeRef::StmtIpyEscapeCommand(_) => true,

            AnyNodeRef::ModModule(_)
            | AnyNodeRef::ModExpression(_)
            | AnyNodeRef::ExprBoolOp(_)
            | AnyNodeRef::ExprNamed(_)
            | AnyNodeRef::ExprBinOp(_)
            | AnyNodeRef::ExprUnaryOp(_)
            | AnyNodeRef::ExprLambda(_)
            | AnyNodeRef::ExprIf(_)
            | AnyNodeRef::ExprDict(_)
            | AnyNodeRef::ExprSet(_)
            | AnyNodeRef::ExprListComp(_)
            | AnyNodeRef::ExprSetComp(_)
            | AnyNodeRef::ExprDictComp(_)
            | AnyNodeRef::ExprGenerator(_)
            | AnyNodeRef::ExprAwait(_)
            | AnyNodeRef::ExprYield(_)
            | AnyNodeRef::ExprYieldFrom(_)
            | AnyNodeRef::ExprCompare(_)
            | AnyNodeRef::ExprCall(_)
            | AnyNodeRef::FStringExpressionElement(_)
            | AnyNodeRef::FStringLiteralElement(_)
            | AnyNodeRef::FStringFormatSpec(_)
            | AnyNodeRef::ExprFString(_)
            | AnyNodeRef::ExprStringLiteral(_)
            | AnyNodeRef::ExprBytesLiteral(_)
            | AnyNodeRef::ExprNumberLiteral(_)
            | AnyNodeRef::ExprBooleanLiteral(_)
            | AnyNodeRef::ExprNoneLiteral(_)
            | AnyNodeRef::ExprEllipsisLiteral(_)
            | AnyNodeRef::ExprAttribute(_)
            | AnyNodeRef::ExprSubscript(_)
            | AnyNodeRef::ExprStarred(_)
            | AnyNodeRef::ExprName(_)
            | AnyNodeRef::ExprList(_)
            | AnyNodeRef::ExprTuple(_)
            | AnyNodeRef::ExprSlice(_)
            | AnyNodeRef::ExprIpyEscapeCommand(_)
            | AnyNodeRef::ExceptHandlerExceptHandler(_)
            | AnyNodeRef::PatternMatchValue(_)
            | AnyNodeRef::PatternMatchSingleton(_)
            | AnyNodeRef::PatternMatchSequence(_)
            | AnyNodeRef::PatternMatchMapping(_)
            | AnyNodeRef::PatternMatchClass(_)
            | AnyNodeRef::PatternMatchStar(_)
            | AnyNodeRef::PatternMatchAs(_)
            | AnyNodeRef::PatternMatchOr(_)
            | AnyNodeRef::PatternArguments(_)
            | AnyNodeRef::PatternKeyword(_)
            | AnyNodeRef::Comprehension(_)
            | AnyNodeRef::Arguments(_)
            | AnyNodeRef::Parameters(_)
            | AnyNodeRef::Parameter(_)
            | AnyNodeRef::ParameterWithDefault(_)
            | AnyNodeRef::Keyword(_)
            | AnyNodeRef::Alias(_)
            | AnyNodeRef::WithItem(_)
            | AnyNodeRef::MatchCase(_)
            | AnyNodeRef::Decorator(_)
            | AnyNodeRef::TypeParams(_)
            | AnyNodeRef::TypeParamTypeVar(_)
            | AnyNodeRef::TypeParamTypeVarTuple(_)
            | AnyNodeRef::TypeParamParamSpec(_)
            | AnyNodeRef::FString(_)
            | AnyNodeRef::StringLiteral(_)
            | AnyNodeRef::BytesLiteral(_)
            | AnyNodeRef::ElifElseClause(_) => false,
        }
    }

    pub const fn is_expression(self) -> bool {
        match self {
            AnyNodeRef::ExprBoolOp(_)
            | AnyNodeRef::ExprNamed(_)
            | AnyNodeRef::ExprBinOp(_)
            | AnyNodeRef::ExprUnaryOp(_)
            | AnyNodeRef::ExprLambda(_)
            | AnyNodeRef::ExprIf(_)
            | AnyNodeRef::ExprDict(_)
            | AnyNodeRef::ExprSet(_)
            | AnyNodeRef::ExprListComp(_)
            | AnyNodeRef::ExprSetComp(_)
            | AnyNodeRef::ExprDictComp(_)
            | AnyNodeRef::ExprGenerator(_)
            | AnyNodeRef::ExprAwait(_)
            | AnyNodeRef::ExprYield(_)
            | AnyNodeRef::ExprYieldFrom(_)
            | AnyNodeRef::ExprCompare(_)
            | AnyNodeRef::ExprCall(_)
            | AnyNodeRef::ExprFString(_)
            | AnyNodeRef::ExprStringLiteral(_)
            | AnyNodeRef::ExprBytesLiteral(_)
            | AnyNodeRef::ExprNumberLiteral(_)
            | AnyNodeRef::ExprBooleanLiteral(_)
            | AnyNodeRef::ExprNoneLiteral(_)
            | AnyNodeRef::ExprEllipsisLiteral(_)
            | AnyNodeRef::ExprAttribute(_)
            | AnyNodeRef::ExprSubscript(_)
            | AnyNodeRef::ExprStarred(_)
            | AnyNodeRef::ExprName(_)
            | AnyNodeRef::ExprList(_)
            | AnyNodeRef::ExprTuple(_)
            | AnyNodeRef::ExprSlice(_)
            | AnyNodeRef::ExprIpyEscapeCommand(_) => true,

            AnyNodeRef::ModModule(_)
            | AnyNodeRef::ModExpression(_)
            | AnyNodeRef::StmtFunctionDef(_)
            | AnyNodeRef::StmtClassDef(_)
            | AnyNodeRef::StmtReturn(_)
            | AnyNodeRef::StmtDelete(_)
            | AnyNodeRef::StmtTypeAlias(_)
            | AnyNodeRef::StmtAssign(_)
            | AnyNodeRef::StmtAugAssign(_)
            | AnyNodeRef::StmtAnnAssign(_)
            | AnyNodeRef::StmtFor(_)
            | AnyNodeRef::StmtWhile(_)
            | AnyNodeRef::StmtIf(_)
            | AnyNodeRef::StmtWith(_)
            | AnyNodeRef::StmtMatch(_)
            | AnyNodeRef::StmtRaise(_)
            | AnyNodeRef::StmtTry(_)
            | AnyNodeRef::StmtAssert(_)
            | AnyNodeRef::StmtImport(_)
            | AnyNodeRef::StmtImportFrom(_)
            | AnyNodeRef::StmtGlobal(_)
            | AnyNodeRef::StmtNonlocal(_)
            | AnyNodeRef::StmtExpr(_)
            | AnyNodeRef::StmtPass(_)
            | AnyNodeRef::StmtBreak(_)
            | AnyNodeRef::StmtContinue(_)
            | AnyNodeRef::StmtIpyEscapeCommand(_)
            | AnyNodeRef::ExceptHandlerExceptHandler(_)
            | AnyNodeRef::FStringExpressionElement(_)
            | AnyNodeRef::FStringLiteralElement(_)
            | AnyNodeRef::FStringFormatSpec(_)
            | AnyNodeRef::PatternMatchValue(_)
            | AnyNodeRef::PatternMatchSingleton(_)
            | AnyNodeRef::PatternMatchSequence(_)
            | AnyNodeRef::PatternMatchMapping(_)
            | AnyNodeRef::PatternMatchClass(_)
            | AnyNodeRef::PatternMatchStar(_)
            | AnyNodeRef::PatternMatchAs(_)
            | AnyNodeRef::PatternMatchOr(_)
            | AnyNodeRef::PatternArguments(_)
            | AnyNodeRef::PatternKeyword(_)
            | AnyNodeRef::Comprehension(_)
            | AnyNodeRef::Arguments(_)
            | AnyNodeRef::Parameters(_)
            | AnyNodeRef::Parameter(_)
            | AnyNodeRef::ParameterWithDefault(_)
            | AnyNodeRef::Keyword(_)
            | AnyNodeRef::Alias(_)
            | AnyNodeRef::WithItem(_)
            | AnyNodeRef::MatchCase(_)
            | AnyNodeRef::Decorator(_)
            | AnyNodeRef::TypeParams(_)
            | AnyNodeRef::TypeParamTypeVar(_)
            | AnyNodeRef::TypeParamTypeVarTuple(_)
            | AnyNodeRef::TypeParamParamSpec(_)
            | AnyNodeRef::FString(_)
            | AnyNodeRef::StringLiteral(_)
            | AnyNodeRef::BytesLiteral(_)
            | AnyNodeRef::ElifElseClause(_) => false,
        }
    }

    pub const fn is_module(self) -> bool {
        match self {
            AnyNodeRef::ModModule(_) | AnyNodeRef::ModExpression(_) => true,

            AnyNodeRef::StmtFunctionDef(_)
            | AnyNodeRef::StmtClassDef(_)
            | AnyNodeRef::StmtReturn(_)
            | AnyNodeRef::StmtDelete(_)
            | AnyNodeRef::StmtTypeAlias(_)
            | AnyNodeRef::StmtAssign(_)
            | AnyNodeRef::StmtAugAssign(_)
            | AnyNodeRef::StmtAnnAssign(_)
            | AnyNodeRef::StmtFor(_)
            | AnyNodeRef::StmtWhile(_)
            | AnyNodeRef::StmtIf(_)
            | AnyNodeRef::StmtWith(_)
            | AnyNodeRef::StmtMatch(_)
            | AnyNodeRef::StmtRaise(_)
            | AnyNodeRef::StmtTry(_)
            | AnyNodeRef::StmtAssert(_)
            | AnyNodeRef::StmtImport(_)
            | AnyNodeRef::StmtImportFrom(_)
            | AnyNodeRef::StmtGlobal(_)
            | AnyNodeRef::StmtNonlocal(_)
            | AnyNodeRef::StmtExpr(_)
            | AnyNodeRef::StmtPass(_)
            | AnyNodeRef::StmtBreak(_)
            | AnyNodeRef::StmtContinue(_)
            | AnyNodeRef::StmtIpyEscapeCommand(_)
            | AnyNodeRef::ExprBoolOp(_)
            | AnyNodeRef::ExprNamed(_)
            | AnyNodeRef::ExprBinOp(_)
            | AnyNodeRef::ExprUnaryOp(_)
            | AnyNodeRef::ExprLambda(_)
            | AnyNodeRef::ExprIf(_)
            | AnyNodeRef::ExprDict(_)
            | AnyNodeRef::ExprSet(_)
            | AnyNodeRef::ExprListComp(_)
            | AnyNodeRef::ExprSetComp(_)
            | AnyNodeRef::ExprDictComp(_)
            | AnyNodeRef::ExprGenerator(_)
            | AnyNodeRef::ExprAwait(_)
            | AnyNodeRef::ExprYield(_)
            | AnyNodeRef::ExprYieldFrom(_)
            | AnyNodeRef::ExprCompare(_)
            | AnyNodeRef::ExprCall(_)
            | AnyNodeRef::FStringExpressionElement(_)
            | AnyNodeRef::FStringLiteralElement(_)
            | AnyNodeRef::FStringFormatSpec(_)
            | AnyNodeRef::ExprFString(_)
            | AnyNodeRef::ExprStringLiteral(_)
            | AnyNodeRef::ExprBytesLiteral(_)
            | AnyNodeRef::ExprNumberLiteral(_)
            | AnyNodeRef::ExprBooleanLiteral(_)
            | AnyNodeRef::ExprNoneLiteral(_)
            | AnyNodeRef::ExprEllipsisLiteral(_)
            | AnyNodeRef::ExprAttribute(_)
            | AnyNodeRef::ExprSubscript(_)
            | AnyNodeRef::ExprStarred(_)
            | AnyNodeRef::ExprName(_)
            | AnyNodeRef::ExprList(_)
            | AnyNodeRef::ExprTuple(_)
            | AnyNodeRef::ExprSlice(_)
            | AnyNodeRef::ExprIpyEscapeCommand(_)
            | AnyNodeRef::ExceptHandlerExceptHandler(_)
            | AnyNodeRef::PatternMatchValue(_)
            | AnyNodeRef::PatternMatchSingleton(_)
            | AnyNodeRef::PatternMatchSequence(_)
            | AnyNodeRef::PatternMatchMapping(_)
            | AnyNodeRef::PatternMatchClass(_)
            | AnyNodeRef::PatternMatchStar(_)
            | AnyNodeRef::PatternMatchAs(_)
            | AnyNodeRef::PatternMatchOr(_)
            | AnyNodeRef::PatternArguments(_)
            | AnyNodeRef::PatternKeyword(_)
            | AnyNodeRef::Comprehension(_)
            | AnyNodeRef::Arguments(_)
            | AnyNodeRef::Parameters(_)
            | AnyNodeRef::Parameter(_)
            | AnyNodeRef::ParameterWithDefault(_)
            | AnyNodeRef::Keyword(_)
            | AnyNodeRef::Alias(_)
            | AnyNodeRef::WithItem(_)
            | AnyNodeRef::MatchCase(_)
            | AnyNodeRef::Decorator(_)
            | AnyNodeRef::TypeParams(_)
            | AnyNodeRef::TypeParamTypeVar(_)
            | AnyNodeRef::TypeParamTypeVarTuple(_)
            | AnyNodeRef::TypeParamParamSpec(_)
            | AnyNodeRef::FString(_)
            | AnyNodeRef::StringLiteral(_)
            | AnyNodeRef::BytesLiteral(_)
            | AnyNodeRef::ElifElseClause(_) => false,
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
            | AnyNodeRef::ModExpression(_)
            | AnyNodeRef::StmtFunctionDef(_)
            | AnyNodeRef::StmtClassDef(_)
            | AnyNodeRef::StmtReturn(_)
            | AnyNodeRef::StmtDelete(_)
            | AnyNodeRef::StmtTypeAlias(_)
            | AnyNodeRef::StmtAssign(_)
            | AnyNodeRef::StmtAugAssign(_)
            | AnyNodeRef::StmtAnnAssign(_)
            | AnyNodeRef::StmtFor(_)
            | AnyNodeRef::StmtWhile(_)
            | AnyNodeRef::StmtIf(_)
            | AnyNodeRef::StmtWith(_)
            | AnyNodeRef::StmtMatch(_)
            | AnyNodeRef::StmtRaise(_)
            | AnyNodeRef::StmtTry(_)
            | AnyNodeRef::StmtAssert(_)
            | AnyNodeRef::StmtImport(_)
            | AnyNodeRef::StmtImportFrom(_)
            | AnyNodeRef::StmtGlobal(_)
            | AnyNodeRef::StmtNonlocal(_)
            | AnyNodeRef::StmtExpr(_)
            | AnyNodeRef::StmtPass(_)
            | AnyNodeRef::StmtBreak(_)
            | AnyNodeRef::StmtContinue(_)
            | AnyNodeRef::StmtIpyEscapeCommand(_)
            | AnyNodeRef::ExprBoolOp(_)
            | AnyNodeRef::ExprNamed(_)
            | AnyNodeRef::ExprBinOp(_)
            | AnyNodeRef::ExprUnaryOp(_)
            | AnyNodeRef::ExprLambda(_)
            | AnyNodeRef::ExprIf(_)
            | AnyNodeRef::ExprDict(_)
            | AnyNodeRef::ExprSet(_)
            | AnyNodeRef::ExprListComp(_)
            | AnyNodeRef::ExprSetComp(_)
            | AnyNodeRef::ExprDictComp(_)
            | AnyNodeRef::ExprGenerator(_)
            | AnyNodeRef::ExprAwait(_)
            | AnyNodeRef::ExprYield(_)
            | AnyNodeRef::ExprYieldFrom(_)
            | AnyNodeRef::ExprCompare(_)
            | AnyNodeRef::ExprCall(_)
            | AnyNodeRef::FStringExpressionElement(_)
            | AnyNodeRef::FStringLiteralElement(_)
            | AnyNodeRef::FStringFormatSpec(_)
            | AnyNodeRef::ExprFString(_)
            | AnyNodeRef::ExprStringLiteral(_)
            | AnyNodeRef::ExprBytesLiteral(_)
            | AnyNodeRef::ExprNumberLiteral(_)
            | AnyNodeRef::ExprBooleanLiteral(_)
            | AnyNodeRef::ExprNoneLiteral(_)
            | AnyNodeRef::ExprEllipsisLiteral(_)
            | AnyNodeRef::ExprAttribute(_)
            | AnyNodeRef::ExprSubscript(_)
            | AnyNodeRef::ExprStarred(_)
            | AnyNodeRef::ExprName(_)
            | AnyNodeRef::ExprList(_)
            | AnyNodeRef::ExprTuple(_)
            | AnyNodeRef::ExprSlice(_)
            | AnyNodeRef::ExprIpyEscapeCommand(_)
            | AnyNodeRef::PatternArguments(_)
            | AnyNodeRef::PatternKeyword(_)
            | AnyNodeRef::ExceptHandlerExceptHandler(_)
            | AnyNodeRef::Comprehension(_)
            | AnyNodeRef::Arguments(_)
            | AnyNodeRef::Parameters(_)
            | AnyNodeRef::Parameter(_)
            | AnyNodeRef::ParameterWithDefault(_)
            | AnyNodeRef::Keyword(_)
            | AnyNodeRef::Alias(_)
            | AnyNodeRef::WithItem(_)
            | AnyNodeRef::MatchCase(_)
            | AnyNodeRef::Decorator(_)
            | AnyNodeRef::TypeParams(_)
            | AnyNodeRef::TypeParamTypeVar(_)
            | AnyNodeRef::TypeParamTypeVarTuple(_)
            | AnyNodeRef::TypeParamParamSpec(_)
            | AnyNodeRef::FString(_)
            | AnyNodeRef::StringLiteral(_)
            | AnyNodeRef::BytesLiteral(_)
            | AnyNodeRef::ElifElseClause(_) => false,
        }
    }

    pub const fn is_except_handler(self) -> bool {
        match self {
            AnyNodeRef::ExceptHandlerExceptHandler(_) => true,

            AnyNodeRef::ModModule(_)
            | AnyNodeRef::ModExpression(_)
            | AnyNodeRef::StmtFunctionDef(_)
            | AnyNodeRef::StmtClassDef(_)
            | AnyNodeRef::StmtReturn(_)
            | AnyNodeRef::StmtDelete(_)
            | AnyNodeRef::StmtTypeAlias(_)
            | AnyNodeRef::StmtAssign(_)
            | AnyNodeRef::StmtAugAssign(_)
            | AnyNodeRef::StmtAnnAssign(_)
            | AnyNodeRef::StmtFor(_)
            | AnyNodeRef::StmtWhile(_)
            | AnyNodeRef::StmtIf(_)
            | AnyNodeRef::StmtWith(_)
            | AnyNodeRef::StmtMatch(_)
            | AnyNodeRef::StmtRaise(_)
            | AnyNodeRef::StmtTry(_)
            | AnyNodeRef::StmtAssert(_)
            | AnyNodeRef::StmtImport(_)
            | AnyNodeRef::StmtImportFrom(_)
            | AnyNodeRef::StmtGlobal(_)
            | AnyNodeRef::StmtNonlocal(_)
            | AnyNodeRef::StmtExpr(_)
            | AnyNodeRef::StmtPass(_)
            | AnyNodeRef::StmtBreak(_)
            | AnyNodeRef::StmtContinue(_)
            | AnyNodeRef::StmtIpyEscapeCommand(_)
            | AnyNodeRef::ExprBoolOp(_)
            | AnyNodeRef::ExprNamed(_)
            | AnyNodeRef::ExprBinOp(_)
            | AnyNodeRef::ExprUnaryOp(_)
            | AnyNodeRef::ExprLambda(_)
            | AnyNodeRef::ExprIf(_)
            | AnyNodeRef::ExprDict(_)
            | AnyNodeRef::ExprSet(_)
            | AnyNodeRef::ExprListComp(_)
            | AnyNodeRef::ExprSetComp(_)
            | AnyNodeRef::ExprDictComp(_)
            | AnyNodeRef::ExprGenerator(_)
            | AnyNodeRef::ExprAwait(_)
            | AnyNodeRef::ExprYield(_)
            | AnyNodeRef::ExprYieldFrom(_)
            | AnyNodeRef::ExprCompare(_)
            | AnyNodeRef::ExprCall(_)
            | AnyNodeRef::FStringExpressionElement(_)
            | AnyNodeRef::FStringLiteralElement(_)
            | AnyNodeRef::FStringFormatSpec(_)
            | AnyNodeRef::ExprFString(_)
            | AnyNodeRef::ExprStringLiteral(_)
            | AnyNodeRef::ExprBytesLiteral(_)
            | AnyNodeRef::ExprNumberLiteral(_)
            | AnyNodeRef::ExprBooleanLiteral(_)
            | AnyNodeRef::ExprNoneLiteral(_)
            | AnyNodeRef::ExprEllipsisLiteral(_)
            | AnyNodeRef::ExprAttribute(_)
            | AnyNodeRef::ExprSubscript(_)
            | AnyNodeRef::ExprStarred(_)
            | AnyNodeRef::ExprName(_)
            | AnyNodeRef::ExprList(_)
            | AnyNodeRef::ExprTuple(_)
            | AnyNodeRef::ExprSlice(_)
            | AnyNodeRef::ExprIpyEscapeCommand(_)
            | AnyNodeRef::PatternMatchValue(_)
            | AnyNodeRef::PatternMatchSingleton(_)
            | AnyNodeRef::PatternMatchSequence(_)
            | AnyNodeRef::PatternMatchMapping(_)
            | AnyNodeRef::PatternMatchClass(_)
            | AnyNodeRef::PatternMatchStar(_)
            | AnyNodeRef::PatternMatchAs(_)
            | AnyNodeRef::PatternMatchOr(_)
            | AnyNodeRef::PatternArguments(_)
            | AnyNodeRef::PatternKeyword(_)
            | AnyNodeRef::Comprehension(_)
            | AnyNodeRef::Arguments(_)
            | AnyNodeRef::Parameters(_)
            | AnyNodeRef::Parameter(_)
            | AnyNodeRef::ParameterWithDefault(_)
            | AnyNodeRef::Keyword(_)
            | AnyNodeRef::Alias(_)
            | AnyNodeRef::WithItem(_)
            | AnyNodeRef::MatchCase(_)
            | AnyNodeRef::Decorator(_)
            | AnyNodeRef::TypeParams(_)
            | AnyNodeRef::TypeParamTypeVar(_)
            | AnyNodeRef::TypeParamTypeVarTuple(_)
            | AnyNodeRef::TypeParamParamSpec(_)
            | AnyNodeRef::FString(_)
            | AnyNodeRef::StringLiteral(_)
            | AnyNodeRef::BytesLiteral(_)
            | AnyNodeRef::ElifElseClause(_) => false,
        }
    }

    /// In our AST, only some alternative branches are represented as a node. This has historical
    /// reasons, e.g. we added a node for elif/else in if statements which was not originally
    /// present in the parser.
    pub const fn is_alternative_branch_with_node(self) -> bool {
        matches!(
            self,
            AnyNodeRef::ExceptHandlerExceptHandler(_) | AnyNodeRef::ElifElseClause(_)
        )
    }

    pub fn visit_preorder<'b, V>(self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'b> + ?Sized,
        'a: 'b,
    {
        match self {
            AnyNodeRef::ModModule(node) => node.visit_source_order(visitor),
            AnyNodeRef::ModExpression(node) => node.visit_source_order(visitor),
            AnyNodeRef::StmtFunctionDef(node) => node.visit_source_order(visitor),
            AnyNodeRef::StmtClassDef(node) => node.visit_source_order(visitor),
            AnyNodeRef::StmtReturn(node) => node.visit_source_order(visitor),
            AnyNodeRef::StmtDelete(node) => node.visit_source_order(visitor),
            AnyNodeRef::StmtTypeAlias(node) => node.visit_source_order(visitor),
            AnyNodeRef::StmtAssign(node) => node.visit_source_order(visitor),
            AnyNodeRef::StmtAugAssign(node) => node.visit_source_order(visitor),
            AnyNodeRef::StmtAnnAssign(node) => node.visit_source_order(visitor),
            AnyNodeRef::StmtFor(node) => node.visit_source_order(visitor),
            AnyNodeRef::StmtWhile(node) => node.visit_source_order(visitor),
            AnyNodeRef::StmtIf(node) => node.visit_source_order(visitor),
            AnyNodeRef::StmtWith(node) => node.visit_source_order(visitor),
            AnyNodeRef::StmtMatch(node) => node.visit_source_order(visitor),
            AnyNodeRef::StmtRaise(node) => node.visit_source_order(visitor),
            AnyNodeRef::StmtTry(node) => node.visit_source_order(visitor),
            AnyNodeRef::StmtAssert(node) => node.visit_source_order(visitor),
            AnyNodeRef::StmtImport(node) => node.visit_source_order(visitor),
            AnyNodeRef::StmtImportFrom(node) => node.visit_source_order(visitor),
            AnyNodeRef::StmtGlobal(node) => node.visit_source_order(visitor),
            AnyNodeRef::StmtNonlocal(node) => node.visit_source_order(visitor),
            AnyNodeRef::StmtExpr(node) => node.visit_source_order(visitor),
            AnyNodeRef::StmtPass(node) => node.visit_source_order(visitor),
            AnyNodeRef::StmtBreak(node) => node.visit_source_order(visitor),
            AnyNodeRef::StmtContinue(node) => node.visit_source_order(visitor),
            AnyNodeRef::StmtIpyEscapeCommand(node) => node.visit_source_order(visitor),
            AnyNodeRef::ExprBoolOp(node) => node.visit_source_order(visitor),
            AnyNodeRef::ExprNamed(node) => node.visit_source_order(visitor),
            AnyNodeRef::ExprBinOp(node) => node.visit_source_order(visitor),
            AnyNodeRef::ExprUnaryOp(node) => node.visit_source_order(visitor),
            AnyNodeRef::ExprLambda(node) => node.visit_source_order(visitor),
            AnyNodeRef::ExprIf(node) => node.visit_source_order(visitor),
            AnyNodeRef::ExprDict(node) => node.visit_source_order(visitor),
            AnyNodeRef::ExprSet(node) => node.visit_source_order(visitor),
            AnyNodeRef::ExprListComp(node) => node.visit_source_order(visitor),
            AnyNodeRef::ExprSetComp(node) => node.visit_source_order(visitor),
            AnyNodeRef::ExprDictComp(node) => node.visit_source_order(visitor),
            AnyNodeRef::ExprGenerator(node) => node.visit_source_order(visitor),
            AnyNodeRef::ExprAwait(node) => node.visit_source_order(visitor),
            AnyNodeRef::ExprYield(node) => node.visit_source_order(visitor),
            AnyNodeRef::ExprYieldFrom(node) => node.visit_source_order(visitor),
            AnyNodeRef::ExprCompare(node) => node.visit_source_order(visitor),
            AnyNodeRef::ExprCall(node) => node.visit_source_order(visitor),
            AnyNodeRef::FStringExpressionElement(node) => node.visit_source_order(visitor),
            AnyNodeRef::FStringLiteralElement(node) => node.visit_source_order(visitor),
            AnyNodeRef::FStringFormatSpec(node) => node.visit_source_order(visitor),
            AnyNodeRef::ExprFString(node) => node.visit_source_order(visitor),
            AnyNodeRef::ExprStringLiteral(node) => node.visit_source_order(visitor),
            AnyNodeRef::ExprBytesLiteral(node) => node.visit_source_order(visitor),
            AnyNodeRef::ExprNumberLiteral(node) => node.visit_source_order(visitor),
            AnyNodeRef::ExprBooleanLiteral(node) => node.visit_source_order(visitor),
            AnyNodeRef::ExprNoneLiteral(node) => node.visit_source_order(visitor),
            AnyNodeRef::ExprEllipsisLiteral(node) => node.visit_source_order(visitor),
            AnyNodeRef::ExprAttribute(node) => node.visit_source_order(visitor),
            AnyNodeRef::ExprSubscript(node) => node.visit_source_order(visitor),
            AnyNodeRef::ExprStarred(node) => node.visit_source_order(visitor),
            AnyNodeRef::ExprName(node) => node.visit_source_order(visitor),
            AnyNodeRef::ExprList(node) => node.visit_source_order(visitor),
            AnyNodeRef::ExprTuple(node) => node.visit_source_order(visitor),
            AnyNodeRef::ExprSlice(node) => node.visit_source_order(visitor),
            AnyNodeRef::ExprIpyEscapeCommand(node) => node.visit_source_order(visitor),
            AnyNodeRef::ExceptHandlerExceptHandler(node) => node.visit_source_order(visitor),
            AnyNodeRef::PatternMatchValue(node) => node.visit_source_order(visitor),
            AnyNodeRef::PatternMatchSingleton(node) => node.visit_source_order(visitor),
            AnyNodeRef::PatternMatchSequence(node) => node.visit_source_order(visitor),
            AnyNodeRef::PatternMatchMapping(node) => node.visit_source_order(visitor),
            AnyNodeRef::PatternMatchClass(node) => node.visit_source_order(visitor),
            AnyNodeRef::PatternMatchStar(node) => node.visit_source_order(visitor),
            AnyNodeRef::PatternMatchAs(node) => node.visit_source_order(visitor),
            AnyNodeRef::PatternMatchOr(node) => node.visit_source_order(visitor),
            AnyNodeRef::PatternArguments(node) => node.visit_source_order(visitor),
            AnyNodeRef::PatternKeyword(node) => node.visit_source_order(visitor),
            AnyNodeRef::Comprehension(node) => node.visit_source_order(visitor),
            AnyNodeRef::Arguments(node) => node.visit_source_order(visitor),
            AnyNodeRef::Parameters(node) => node.visit_source_order(visitor),
            AnyNodeRef::Parameter(node) => node.visit_source_order(visitor),
            AnyNodeRef::ParameterWithDefault(node) => node.visit_source_order(visitor),
            AnyNodeRef::Keyword(node) => node.visit_source_order(visitor),
            AnyNodeRef::Alias(node) => node.visit_source_order(visitor),
            AnyNodeRef::WithItem(node) => node.visit_source_order(visitor),
            AnyNodeRef::MatchCase(node) => node.visit_source_order(visitor),
            AnyNodeRef::Decorator(node) => node.visit_source_order(visitor),
            AnyNodeRef::TypeParams(node) => node.visit_source_order(visitor),
            AnyNodeRef::TypeParamTypeVar(node) => node.visit_source_order(visitor),
            AnyNodeRef::TypeParamTypeVarTuple(node) => node.visit_source_order(visitor),
            AnyNodeRef::TypeParamParamSpec(node) => node.visit_source_order(visitor),
            AnyNodeRef::FString(node) => node.visit_source_order(visitor),
            AnyNodeRef::StringLiteral(node) => node.visit_source_order(visitor),
            AnyNodeRef::BytesLiteral(node) => node.visit_source_order(visitor),
            AnyNodeRef::ElifElseClause(node) => node.visit_source_order(visitor),
        }
    }

    /// The last child of the last branch, if the node has multiple branches.
    pub fn last_child_in_body(&self) -> Option<AnyNodeRef<'a>> {
        let body = match self {
            AnyNodeRef::StmtFunctionDef(ast::StmtFunctionDef { body, .. })
            | AnyNodeRef::StmtClassDef(ast::StmtClassDef { body, .. })
            | AnyNodeRef::StmtWith(ast::StmtWith { body, .. })
            | AnyNodeRef::MatchCase(MatchCase { body, .. })
            | AnyNodeRef::ExceptHandlerExceptHandler(ast::ExceptHandlerExceptHandler {
                body,
                ..
            })
            | AnyNodeRef::ElifElseClause(ast::ElifElseClause { body, .. }) => body,
            AnyNodeRef::StmtIf(ast::StmtIf {
                body,
                elif_else_clauses,
                ..
            }) => elif_else_clauses.last().map_or(body, |clause| &clause.body),

            AnyNodeRef::StmtFor(ast::StmtFor { body, orelse, .. })
            | AnyNodeRef::StmtWhile(ast::StmtWhile { body, orelse, .. }) => {
                if orelse.is_empty() {
                    body
                } else {
                    orelse
                }
            }

            AnyNodeRef::StmtMatch(ast::StmtMatch { cases, .. }) => {
                return cases.last().map(AnyNodeRef::from);
            }

            AnyNodeRef::StmtTry(ast::StmtTry {
                body,
                handlers,
                orelse,
                finalbody,
                ..
            }) => {
                if finalbody.is_empty() {
                    if orelse.is_empty() {
                        if handlers.is_empty() {
                            body
                        } else {
                            return handlers.last().map(AnyNodeRef::from);
                        }
                    } else {
                        orelse
                    }
                } else {
                    finalbody
                }
            }

            // Not a node that contains an indented child node.
            _ => return None,
        };

        body.last().map(AnyNodeRef::from)
    }

    /// Check if the given statement is the first statement after the colon of a branch, be it in if
    /// statements, for statements, after each part of a try-except-else-finally or function/class
    /// definitions.
    ///
    ///
    /// ```python
    /// if True:    <- has body
    ///     a       <- first statement
    ///     b
    /// elif b:     <- has body
    ///     c       <- first statement
    ///     d
    /// else:       <- has body
    ///     e       <- first statement
    ///     f
    ///
    /// class:      <- has body
    ///     a: int  <- first statement
    ///     b: int
    ///
    /// ```
    ///
    /// For nodes with multiple bodies, we check all bodies that don't have their own node. For
    /// try-except-else-finally, each except branch has it's own node, so for the `StmtTry`, we check
    /// the `try:`, `else:` and `finally:`, bodies, while `ExceptHandlerExceptHandler` has it's own
    /// check. For for-else and while-else, we check both branches for the whole statement.
    ///
    /// ```python
    /// try:        <- has body (a)
    ///     6/8     <- first statement (a)
    ///     1/0
    /// except:     <- has body (b)
    ///     a       <- first statement (b)
    ///     b
    /// else:
    ///     c       <- first statement (a)
    ///     d
    /// finally:
    ///     e       <- first statement (a)
    ///     f
    /// ```
    pub fn is_first_statement_in_body(&self, body: AnyNodeRef) -> bool {
        match body {
            AnyNodeRef::StmtFor(ast::StmtFor { body, orelse, .. })
            | AnyNodeRef::StmtWhile(ast::StmtWhile { body, orelse, .. }) => {
                are_same_optional(*self, body.first()) || are_same_optional(*self, orelse.first())
            }

            AnyNodeRef::StmtTry(ast::StmtTry {
                body,
                orelse,
                finalbody,
                ..
            }) => {
                are_same_optional(*self, body.first())
                    || are_same_optional(*self, orelse.first())
                    || are_same_optional(*self, finalbody.first())
            }

            AnyNodeRef::StmtIf(ast::StmtIf { body, .. })
            | AnyNodeRef::ElifElseClause(ast::ElifElseClause { body, .. })
            | AnyNodeRef::StmtWith(ast::StmtWith { body, .. })
            | AnyNodeRef::ExceptHandlerExceptHandler(ast::ExceptHandlerExceptHandler {
                body,
                ..
            })
            | AnyNodeRef::MatchCase(MatchCase { body, .. })
            | AnyNodeRef::StmtFunctionDef(ast::StmtFunctionDef { body, .. })
            | AnyNodeRef::StmtClassDef(ast::StmtClassDef { body, .. }) => {
                are_same_optional(*self, body.first())
            }

            AnyNodeRef::StmtMatch(ast::StmtMatch { cases, .. }) => {
                are_same_optional(*self, cases.first())
            }

            _ => false,
        }
    }

    /// Returns `true` if `statement` is the first statement in an alternate `body` (e.g. the else of an if statement)
    pub fn is_first_statement_in_alternate_body(&self, body: AnyNodeRef) -> bool {
        match body {
            AnyNodeRef::StmtFor(ast::StmtFor { orelse, .. })
            | AnyNodeRef::StmtWhile(ast::StmtWhile { orelse, .. }) => {
                are_same_optional(*self, orelse.first())
            }

            AnyNodeRef::StmtTry(ast::StmtTry {
                handlers,
                orelse,
                finalbody,
                ..
            }) => {
                are_same_optional(*self, handlers.first())
                    || are_same_optional(*self, orelse.first())
                    || are_same_optional(*self, finalbody.first())
            }

            AnyNodeRef::StmtIf(ast::StmtIf {
                elif_else_clauses, ..
            }) => are_same_optional(*self, elif_else_clauses.first()),
            _ => false,
        }
    }
}

/// Returns `true` if `right` is `Some` and `left` and `right` are referentially equal.
fn are_same_optional<'a, T>(left: AnyNodeRef, right: Option<T>) -> bool
where
    T: Into<AnyNodeRef<'a>>,
{
    right.is_some_and(|right| left.ptr_eq(right.into()))
}

impl<'a> From<&'a ast::ModModule> for AnyNodeRef<'a> {
    fn from(node: &'a ast::ModModule) -> Self {
        AnyNodeRef::ModModule(node)
    }
}

impl<'a> From<&'a ast::ModExpression> for AnyNodeRef<'a> {
    fn from(node: &'a ast::ModExpression) -> Self {
        AnyNodeRef::ModExpression(node)
    }
}

impl<'a> From<&'a ast::StmtFunctionDef> for AnyNodeRef<'a> {
    fn from(node: &'a ast::StmtFunctionDef) -> Self {
        AnyNodeRef::StmtFunctionDef(node)
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

impl<'a> From<&'a ast::StmtTypeAlias> for AnyNodeRef<'a> {
    fn from(node: &'a ast::StmtTypeAlias) -> Self {
        AnyNodeRef::StmtTypeAlias(node)
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

impl<'a> From<&'a ast::ElifElseClause> for AnyNodeRef<'a> {
    fn from(node: &'a ast::ElifElseClause) -> Self {
        AnyNodeRef::ElifElseClause(node)
    }
}

impl<'a> From<&'a ast::StmtWith> for AnyNodeRef<'a> {
    fn from(node: &'a ast::StmtWith) -> Self {
        AnyNodeRef::StmtWith(node)
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

impl<'a> From<&'a ast::StmtIpyEscapeCommand> for AnyNodeRef<'a> {
    fn from(node: &'a ast::StmtIpyEscapeCommand) -> Self {
        AnyNodeRef::StmtIpyEscapeCommand(node)
    }
}

impl<'a> From<&'a ast::ExprBoolOp> for AnyNodeRef<'a> {
    fn from(node: &'a ast::ExprBoolOp) -> Self {
        AnyNodeRef::ExprBoolOp(node)
    }
}

impl<'a> From<&'a ast::ExprNamed> for AnyNodeRef<'a> {
    fn from(node: &'a ast::ExprNamed) -> Self {
        AnyNodeRef::ExprNamed(node)
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

impl<'a> From<&'a ast::ExprIf> for AnyNodeRef<'a> {
    fn from(node: &'a ast::ExprIf) -> Self {
        AnyNodeRef::ExprIf(node)
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

impl<'a> From<&'a ast::ExprGenerator> for AnyNodeRef<'a> {
    fn from(node: &'a ast::ExprGenerator) -> Self {
        AnyNodeRef::ExprGenerator(node)
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

impl<'a> From<&'a ast::FStringExpressionElement> for AnyNodeRef<'a> {
    fn from(node: &'a ast::FStringExpressionElement) -> Self {
        AnyNodeRef::FStringExpressionElement(node)
    }
}

impl<'a> From<&'a ast::FStringLiteralElement> for AnyNodeRef<'a> {
    fn from(node: &'a ast::FStringLiteralElement) -> Self {
        AnyNodeRef::FStringLiteralElement(node)
    }
}

impl<'a> From<&'a ast::FStringFormatSpec> for AnyNodeRef<'a> {
    fn from(node: &'a ast::FStringFormatSpec) -> Self {
        AnyNodeRef::FStringFormatSpec(node)
    }
}

impl<'a> From<&'a ast::ExprFString> for AnyNodeRef<'a> {
    fn from(node: &'a ast::ExprFString) -> Self {
        AnyNodeRef::ExprFString(node)
    }
}

impl<'a> From<&'a ast::ExprStringLiteral> for AnyNodeRef<'a> {
    fn from(node: &'a ast::ExprStringLiteral) -> Self {
        AnyNodeRef::ExprStringLiteral(node)
    }
}

impl<'a> From<&'a ast::ExprBytesLiteral> for AnyNodeRef<'a> {
    fn from(node: &'a ast::ExprBytesLiteral) -> Self {
        AnyNodeRef::ExprBytesLiteral(node)
    }
}

impl<'a> From<&'a ast::ExprNumberLiteral> for AnyNodeRef<'a> {
    fn from(node: &'a ast::ExprNumberLiteral) -> Self {
        AnyNodeRef::ExprNumberLiteral(node)
    }
}

impl<'a> From<&'a ast::ExprBooleanLiteral> for AnyNodeRef<'a> {
    fn from(node: &'a ast::ExprBooleanLiteral) -> Self {
        AnyNodeRef::ExprBooleanLiteral(node)
    }
}

impl<'a> From<&'a ast::ExprNoneLiteral> for AnyNodeRef<'a> {
    fn from(node: &'a ast::ExprNoneLiteral) -> Self {
        AnyNodeRef::ExprNoneLiteral(node)
    }
}

impl<'a> From<&'a ast::ExprEllipsisLiteral> for AnyNodeRef<'a> {
    fn from(node: &'a ast::ExprEllipsisLiteral) -> Self {
        AnyNodeRef::ExprEllipsisLiteral(node)
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

impl<'a> From<&'a ast::ExprIpyEscapeCommand> for AnyNodeRef<'a> {
    fn from(node: &'a ast::ExprIpyEscapeCommand) -> Self {
        AnyNodeRef::ExprIpyEscapeCommand(node)
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

impl<'a> From<&'a ast::PatternArguments> for AnyNodeRef<'a> {
    fn from(node: &'a ast::PatternArguments) -> Self {
        AnyNodeRef::PatternArguments(node)
    }
}

impl<'a> From<&'a ast::PatternKeyword> for AnyNodeRef<'a> {
    fn from(node: &'a ast::PatternKeyword) -> Self {
        AnyNodeRef::PatternKeyword(node)
    }
}

impl<'a> From<&'a Decorator> for AnyNodeRef<'a> {
    fn from(node: &'a Decorator) -> Self {
        AnyNodeRef::Decorator(node)
    }
}

impl<'a> From<&'a ast::TypeParams> for AnyNodeRef<'a> {
    fn from(node: &'a ast::TypeParams) -> Self {
        AnyNodeRef::TypeParams(node)
    }
}
impl<'a> From<&'a TypeParamTypeVar> for AnyNodeRef<'a> {
    fn from(node: &'a TypeParamTypeVar) -> Self {
        AnyNodeRef::TypeParamTypeVar(node)
    }
}

impl<'a> From<&'a TypeParamTypeVarTuple> for AnyNodeRef<'a> {
    fn from(node: &'a TypeParamTypeVarTuple) -> Self {
        AnyNodeRef::TypeParamTypeVarTuple(node)
    }
}

impl<'a> From<&'a TypeParamParamSpec> for AnyNodeRef<'a> {
    fn from(node: &'a TypeParamParamSpec) -> Self {
        AnyNodeRef::TypeParamParamSpec(node)
    }
}

impl<'a> From<&'a ast::FString> for AnyNodeRef<'a> {
    fn from(node: &'a ast::FString) -> Self {
        AnyNodeRef::FString(node)
    }
}

impl<'a> From<&'a ast::StringLiteral> for AnyNodeRef<'a> {
    fn from(node: &'a ast::StringLiteral) -> Self {
        AnyNodeRef::StringLiteral(node)
    }
}

impl<'a> From<&'a ast::BytesLiteral> for AnyNodeRef<'a> {
    fn from(node: &'a ast::BytesLiteral) -> Self {
        AnyNodeRef::BytesLiteral(node)
    }
}

impl<'a> From<&'a Stmt> for AnyNodeRef<'a> {
    fn from(stmt: &'a Stmt) -> Self {
        match stmt {
            Stmt::FunctionDef(node) => AnyNodeRef::StmtFunctionDef(node),
            Stmt::ClassDef(node) => AnyNodeRef::StmtClassDef(node),
            Stmt::Return(node) => AnyNodeRef::StmtReturn(node),
            Stmt::Delete(node) => AnyNodeRef::StmtDelete(node),
            Stmt::TypeAlias(node) => AnyNodeRef::StmtTypeAlias(node),
            Stmt::Assign(node) => AnyNodeRef::StmtAssign(node),
            Stmt::AugAssign(node) => AnyNodeRef::StmtAugAssign(node),
            Stmt::AnnAssign(node) => AnyNodeRef::StmtAnnAssign(node),
            Stmt::For(node) => AnyNodeRef::StmtFor(node),
            Stmt::While(node) => AnyNodeRef::StmtWhile(node),
            Stmt::If(node) => AnyNodeRef::StmtIf(node),
            Stmt::With(node) => AnyNodeRef::StmtWith(node),
            Stmt::Match(node) => AnyNodeRef::StmtMatch(node),
            Stmt::Raise(node) => AnyNodeRef::StmtRaise(node),
            Stmt::Try(node) => AnyNodeRef::StmtTry(node),
            Stmt::Assert(node) => AnyNodeRef::StmtAssert(node),
            Stmt::Import(node) => AnyNodeRef::StmtImport(node),
            Stmt::ImportFrom(node) => AnyNodeRef::StmtImportFrom(node),
            Stmt::Global(node) => AnyNodeRef::StmtGlobal(node),
            Stmt::Nonlocal(node) => AnyNodeRef::StmtNonlocal(node),
            Stmt::Expr(node) => AnyNodeRef::StmtExpr(node),
            Stmt::Pass(node) => AnyNodeRef::StmtPass(node),
            Stmt::Break(node) => AnyNodeRef::StmtBreak(node),
            Stmt::Continue(node) => AnyNodeRef::StmtContinue(node),
            Stmt::IpyEscapeCommand(node) => AnyNodeRef::StmtIpyEscapeCommand(node),
        }
    }
}

impl<'a> From<&'a Expr> for AnyNodeRef<'a> {
    fn from(expr: &'a Expr) -> Self {
        match expr {
            Expr::BoolOp(node) => AnyNodeRef::ExprBoolOp(node),
            Expr::Named(node) => AnyNodeRef::ExprNamed(node),
            Expr::BinOp(node) => AnyNodeRef::ExprBinOp(node),
            Expr::UnaryOp(node) => AnyNodeRef::ExprUnaryOp(node),
            Expr::Lambda(node) => AnyNodeRef::ExprLambda(node),
            Expr::If(node) => AnyNodeRef::ExprIf(node),
            Expr::Dict(node) => AnyNodeRef::ExprDict(node),
            Expr::Set(node) => AnyNodeRef::ExprSet(node),
            Expr::ListComp(node) => AnyNodeRef::ExprListComp(node),
            Expr::SetComp(node) => AnyNodeRef::ExprSetComp(node),
            Expr::DictComp(node) => AnyNodeRef::ExprDictComp(node),
            Expr::Generator(node) => AnyNodeRef::ExprGenerator(node),
            Expr::Await(node) => AnyNodeRef::ExprAwait(node),
            Expr::Yield(node) => AnyNodeRef::ExprYield(node),
            Expr::YieldFrom(node) => AnyNodeRef::ExprYieldFrom(node),
            Expr::Compare(node) => AnyNodeRef::ExprCompare(node),
            Expr::Call(node) => AnyNodeRef::ExprCall(node),
            Expr::FString(node) => AnyNodeRef::ExprFString(node),
            Expr::StringLiteral(node) => AnyNodeRef::ExprStringLiteral(node),
            Expr::BytesLiteral(node) => AnyNodeRef::ExprBytesLiteral(node),
            Expr::NumberLiteral(node) => AnyNodeRef::ExprNumberLiteral(node),
            Expr::BooleanLiteral(node) => AnyNodeRef::ExprBooleanLiteral(node),
            Expr::NoneLiteral(node) => AnyNodeRef::ExprNoneLiteral(node),
            Expr::EllipsisLiteral(node) => AnyNodeRef::ExprEllipsisLiteral(node),
            Expr::Attribute(node) => AnyNodeRef::ExprAttribute(node),
            Expr::Subscript(node) => AnyNodeRef::ExprSubscript(node),
            Expr::Starred(node) => AnyNodeRef::ExprStarred(node),
            Expr::Name(node) => AnyNodeRef::ExprName(node),
            Expr::List(node) => AnyNodeRef::ExprList(node),
            Expr::Tuple(node) => AnyNodeRef::ExprTuple(node),
            Expr::Slice(node) => AnyNodeRef::ExprSlice(node),
            Expr::IpyEscapeCommand(node) => AnyNodeRef::ExprIpyEscapeCommand(node),
        }
    }
}

impl<'a> From<&'a Mod> for AnyNodeRef<'a> {
    fn from(module: &'a Mod) -> Self {
        match module {
            Mod::Module(node) => AnyNodeRef::ModModule(node),
            Mod::Expression(node) => AnyNodeRef::ModExpression(node),
        }
    }
}

impl<'a> From<&'a FStringElement> for AnyNodeRef<'a> {
    fn from(element: &'a FStringElement) -> Self {
        match element {
            FStringElement::Expression(node) => AnyNodeRef::FStringExpressionElement(node),
            FStringElement::Literal(node) => AnyNodeRef::FStringLiteralElement(node),
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

impl<'a> From<&'a TypeParam> for AnyNodeRef<'a> {
    fn from(type_param: &'a TypeParam) -> Self {
        match type_param {
            TypeParam::TypeVar(node) => AnyNodeRef::TypeParamTypeVar(node),
            TypeParam::TypeVarTuple(node) => AnyNodeRef::TypeParamTypeVarTuple(node),
            TypeParam::ParamSpec(node) => AnyNodeRef::TypeParamParamSpec(node),
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
impl<'a> From<&'a Parameters> for AnyNodeRef<'a> {
    fn from(node: &'a Parameters) -> Self {
        AnyNodeRef::Parameters(node)
    }
}
impl<'a> From<&'a Parameter> for AnyNodeRef<'a> {
    fn from(node: &'a Parameter) -> Self {
        AnyNodeRef::Parameter(node)
    }
}
impl<'a> From<&'a ParameterWithDefault> for AnyNodeRef<'a> {
    fn from(node: &'a ParameterWithDefault) -> Self {
        AnyNodeRef::ParameterWithDefault(node)
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
            AnyNodeRef::ModExpression(node) => node.range(),
            AnyNodeRef::StmtFunctionDef(node) => node.range(),
            AnyNodeRef::StmtClassDef(node) => node.range(),
            AnyNodeRef::StmtReturn(node) => node.range(),
            AnyNodeRef::StmtDelete(node) => node.range(),
            AnyNodeRef::StmtTypeAlias(node) => node.range(),
            AnyNodeRef::StmtAssign(node) => node.range(),
            AnyNodeRef::StmtAugAssign(node) => node.range(),
            AnyNodeRef::StmtAnnAssign(node) => node.range(),
            AnyNodeRef::StmtFor(node) => node.range(),
            AnyNodeRef::StmtWhile(node) => node.range(),
            AnyNodeRef::StmtIf(node) => node.range(),
            AnyNodeRef::StmtWith(node) => node.range(),
            AnyNodeRef::StmtMatch(node) => node.range(),
            AnyNodeRef::StmtRaise(node) => node.range(),
            AnyNodeRef::StmtTry(node) => node.range(),
            AnyNodeRef::StmtAssert(node) => node.range(),
            AnyNodeRef::StmtImport(node) => node.range(),
            AnyNodeRef::StmtImportFrom(node) => node.range(),
            AnyNodeRef::StmtGlobal(node) => node.range(),
            AnyNodeRef::StmtNonlocal(node) => node.range(),
            AnyNodeRef::StmtExpr(node) => node.range(),
            AnyNodeRef::StmtPass(node) => node.range(),
            AnyNodeRef::StmtBreak(node) => node.range(),
            AnyNodeRef::StmtContinue(node) => node.range(),
            AnyNodeRef::StmtIpyEscapeCommand(node) => node.range(),
            AnyNodeRef::ExprBoolOp(node) => node.range(),
            AnyNodeRef::ExprNamed(node) => node.range(),
            AnyNodeRef::ExprBinOp(node) => node.range(),
            AnyNodeRef::ExprUnaryOp(node) => node.range(),
            AnyNodeRef::ExprLambda(node) => node.range(),
            AnyNodeRef::ExprIf(node) => node.range(),
            AnyNodeRef::ExprDict(node) => node.range(),
            AnyNodeRef::ExprSet(node) => node.range(),
            AnyNodeRef::ExprListComp(node) => node.range(),
            AnyNodeRef::ExprSetComp(node) => node.range(),
            AnyNodeRef::ExprDictComp(node) => node.range(),
            AnyNodeRef::ExprGenerator(node) => node.range(),
            AnyNodeRef::ExprAwait(node) => node.range(),
            AnyNodeRef::ExprYield(node) => node.range(),
            AnyNodeRef::ExprYieldFrom(node) => node.range(),
            AnyNodeRef::ExprCompare(node) => node.range(),
            AnyNodeRef::ExprCall(node) => node.range(),
            AnyNodeRef::FStringExpressionElement(node) => node.range(),
            AnyNodeRef::FStringLiteralElement(node) => node.range(),
            AnyNodeRef::FStringFormatSpec(node) => node.range(),
            AnyNodeRef::ExprFString(node) => node.range(),
            AnyNodeRef::ExprStringLiteral(node) => node.range(),
            AnyNodeRef::ExprBytesLiteral(node) => node.range(),
            AnyNodeRef::ExprNumberLiteral(node) => node.range(),
            AnyNodeRef::ExprBooleanLiteral(node) => node.range(),
            AnyNodeRef::ExprNoneLiteral(node) => node.range(),
            AnyNodeRef::ExprEllipsisLiteral(node) => node.range(),
            AnyNodeRef::ExprAttribute(node) => node.range(),
            AnyNodeRef::ExprSubscript(node) => node.range(),
            AnyNodeRef::ExprStarred(node) => node.range(),
            AnyNodeRef::ExprName(node) => node.range(),
            AnyNodeRef::ExprList(node) => node.range(),
            AnyNodeRef::ExprTuple(node) => node.range(),
            AnyNodeRef::ExprSlice(node) => node.range(),
            AnyNodeRef::ExprIpyEscapeCommand(node) => node.range(),
            AnyNodeRef::ExceptHandlerExceptHandler(node) => node.range(),
            AnyNodeRef::PatternMatchValue(node) => node.range(),
            AnyNodeRef::PatternMatchSingleton(node) => node.range(),
            AnyNodeRef::PatternMatchSequence(node) => node.range(),
            AnyNodeRef::PatternMatchMapping(node) => node.range(),
            AnyNodeRef::PatternMatchClass(node) => node.range(),
            AnyNodeRef::PatternMatchStar(node) => node.range(),
            AnyNodeRef::PatternMatchAs(node) => node.range(),
            AnyNodeRef::PatternMatchOr(node) => node.range(),
            AnyNodeRef::PatternArguments(node) => node.range(),
            AnyNodeRef::PatternKeyword(node) => node.range(),
            AnyNodeRef::Comprehension(node) => node.range(),
            AnyNodeRef::Arguments(node) => node.range(),
            AnyNodeRef::Parameters(node) => node.range(),
            AnyNodeRef::Parameter(node) => node.range(),
            AnyNodeRef::ParameterWithDefault(node) => node.range(),
            AnyNodeRef::Keyword(node) => node.range(),
            AnyNodeRef::Alias(node) => node.range(),
            AnyNodeRef::WithItem(node) => node.range(),
            AnyNodeRef::MatchCase(node) => node.range(),
            AnyNodeRef::Decorator(node) => node.range(),
            AnyNodeRef::ElifElseClause(node) => node.range(),
            AnyNodeRef::TypeParams(node) => node.range(),
            AnyNodeRef::TypeParamTypeVar(node) => node.range(),
            AnyNodeRef::TypeParamTypeVarTuple(node) => node.range(),
            AnyNodeRef::TypeParamParamSpec(node) => node.range(),
            AnyNodeRef::FString(node) => node.range(),
            AnyNodeRef::StringLiteral(node) => node.range(),
            AnyNodeRef::BytesLiteral(node) => node.range(),
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
    StmtIpyEscapeCommand,
    StmtExpr,
    StmtPass,
    StmtBreak,
    StmtContinue,
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
    FStringExpressionElement,
    FStringLiteralElement,
    FStringFormatSpec,
    ExprFString,
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
    ExprIpyEscapeCommand,
    ExceptHandlerExceptHandler,
    PatternMatchValue,
    PatternMatchSingleton,
    PatternMatchSequence,
    PatternMatchMapping,
    PatternMatchClass,
    PatternMatchStar,
    PatternMatchAs,
    PatternMatchOr,
    PatternArguments,
    PatternKeyword,
    TypeIgnoreTypeIgnore,
    Comprehension,
    Arguments,
    Parameters,
    Parameter,
    ParameterWithDefault,
    Keyword,
    Alias,
    WithItem,
    MatchCase,
    Decorator,
    ElifElseClause,
    TypeParams,
    TypeParamTypeVar,
    TypeParamTypeVarTuple,
    TypeParamParamSpec,
    FString,
    StringLiteral,
    BytesLiteral,
}

// FIXME: The `StatementRef` here allows us to implement `AstNode` for `Stmt` which otherwise wouldn't be possible
//  because of the `cast_ref` method that needs to return a `&Stmt` for a specific statement node.
//  Implementing `AstNode` for `Stmt` is desired to have `AstId.upcast` work where the Id then represents
//  any `Stmt` instead of a specific statement.
//  The existing solution "works" in the sense that `upcast` etc can be implemented. However, `StatementRef`
//  doesn't implement `AstNode` itself and thus, can't be used as `AstNodeKey` or passed to query the `ast_id` (because that requires that the node implements `HasAstId` which extends `AstNode`).
//  I don't know how a solution to this would look like but this isn't the first time where this problem has come up.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum StatementRef<'a> {
    FunctionDef(&'a StmtFunctionDef),
    ClassDef(&'a StmtClassDef),
    Return(&'a StmtReturn),
    Delete(&'a StmtDelete),
    Assign(&'a StmtAssign),
    AugAssign(&'a StmtAugAssign),
    AnnAssign(&'a StmtAnnAssign),
    TypeAlias(&'a StmtTypeAlias),
    For(&'a StmtFor),
    While(&'a StmtWhile),
    If(&'a StmtIf),
    With(&'a StmtWith),
    Match(&'a StmtMatch),
    Raise(&'a StmtRaise),
    Try(&'a StmtTry),
    Assert(&'a StmtAssert),
    Import(&'a StmtImport),
    ImportFrom(&'a StmtImportFrom),
    Global(&'a StmtGlobal),
    Nonlocal(&'a StmtNonlocal),
    Expr(&'a StmtExpr),
    Pass(&'a StmtPass),
    Break(&'a StmtBreak),
    Continue(&'a StmtContinue),
    IpyEscapeCommand(&'a StmtIpyEscapeCommand),
}

impl<'a> From<&'a StmtFunctionDef> for StatementRef<'a> {
    fn from(value: &'a StmtFunctionDef) -> Self {
        Self::FunctionDef(value)
    }
}
impl<'a> From<&'a StmtClassDef> for StatementRef<'a> {
    fn from(value: &'a StmtClassDef) -> Self {
        Self::ClassDef(value)
    }
}
impl<'a> From<&'a StmtReturn> for StatementRef<'a> {
    fn from(value: &'a StmtReturn) -> Self {
        Self::Return(value)
    }
}
impl<'a> From<&'a StmtDelete> for StatementRef<'a> {
    fn from(value: &'a StmtDelete) -> Self {
        Self::Delete(value)
    }
}
impl<'a> From<&'a StmtAssign> for StatementRef<'a> {
    fn from(value: &'a StmtAssign) -> Self {
        Self::Assign(value)
    }
}
impl<'a> From<&'a StmtAugAssign> for StatementRef<'a> {
    fn from(value: &'a StmtAugAssign) -> Self {
        Self::AugAssign(value)
    }
}
impl<'a> From<&'a StmtAnnAssign> for StatementRef<'a> {
    fn from(value: &'a StmtAnnAssign) -> Self {
        Self::AnnAssign(value)
    }
}
impl<'a> From<&'a StmtTypeAlias> for StatementRef<'a> {
    fn from(value: &'a StmtTypeAlias) -> Self {
        Self::TypeAlias(value)
    }
}
impl<'a> From<&'a StmtFor> for StatementRef<'a> {
    fn from(value: &'a StmtFor) -> Self {
        Self::For(value)
    }
}
impl<'a> From<&'a StmtWhile> for StatementRef<'a> {
    fn from(value: &'a StmtWhile) -> Self {
        Self::While(value)
    }
}
impl<'a> From<&'a StmtIf> for StatementRef<'a> {
    fn from(value: &'a StmtIf) -> Self {
        Self::If(value)
    }
}
impl<'a> From<&'a StmtWith> for StatementRef<'a> {
    fn from(value: &'a StmtWith) -> Self {
        Self::With(value)
    }
}
impl<'a> From<&'a StmtMatch> for StatementRef<'a> {
    fn from(value: &'a StmtMatch) -> Self {
        Self::Match(value)
    }
}
impl<'a> From<&'a StmtRaise> for StatementRef<'a> {
    fn from(value: &'a StmtRaise) -> Self {
        Self::Raise(value)
    }
}
impl<'a> From<&'a StmtTry> for StatementRef<'a> {
    fn from(value: &'a StmtTry) -> Self {
        Self::Try(value)
    }
}
impl<'a> From<&'a StmtAssert> for StatementRef<'a> {
    fn from(value: &'a StmtAssert) -> Self {
        Self::Assert(value)
    }
}
impl<'a> From<&'a StmtImport> for StatementRef<'a> {
    fn from(value: &'a StmtImport) -> Self {
        Self::Import(value)
    }
}
impl<'a> From<&'a StmtImportFrom> for StatementRef<'a> {
    fn from(value: &'a StmtImportFrom) -> Self {
        Self::ImportFrom(value)
    }
}
impl<'a> From<&'a StmtGlobal> for StatementRef<'a> {
    fn from(value: &'a StmtGlobal) -> Self {
        Self::Global(value)
    }
}
impl<'a> From<&'a StmtNonlocal> for StatementRef<'a> {
    fn from(value: &'a StmtNonlocal) -> Self {
        Self::Nonlocal(value)
    }
}
impl<'a> From<&'a StmtExpr> for StatementRef<'a> {
    fn from(value: &'a StmtExpr) -> Self {
        Self::Expr(value)
    }
}
impl<'a> From<&'a StmtPass> for StatementRef<'a> {
    fn from(value: &'a StmtPass) -> Self {
        Self::Pass(value)
    }
}
impl<'a> From<&'a StmtBreak> for StatementRef<'a> {
    fn from(value: &'a StmtBreak) -> Self {
        Self::Break(value)
    }
}
impl<'a> From<&'a StmtContinue> for StatementRef<'a> {
    fn from(value: &'a StmtContinue) -> Self {
        Self::Continue(value)
    }
}
impl<'a> From<&'a StmtIpyEscapeCommand> for StatementRef<'a> {
    fn from(value: &'a StmtIpyEscapeCommand) -> Self {
        Self::IpyEscapeCommand(value)
    }
}

impl<'a> From<&'a Stmt> for StatementRef<'a> {
    fn from(value: &'a Stmt) -> Self {
        match value {
            Stmt::FunctionDef(statement) => Self::FunctionDef(statement),
            Stmt::ClassDef(statement) => Self::ClassDef(statement),
            Stmt::Return(statement) => Self::Return(statement),
            Stmt::Delete(statement) => Self::Delete(statement),
            Stmt::Assign(statement) => Self::Assign(statement),
            Stmt::AugAssign(statement) => Self::AugAssign(statement),
            Stmt::AnnAssign(statement) => Self::AnnAssign(statement),
            Stmt::TypeAlias(statement) => Self::TypeAlias(statement),
            Stmt::For(statement) => Self::For(statement),
            Stmt::While(statement) => Self::While(statement),
            Stmt::If(statement) => Self::If(statement),
            Stmt::With(statement) => Self::With(statement),
            Stmt::Match(statement) => Self::Match(statement),
            Stmt::Raise(statement) => Self::Raise(statement),
            Stmt::Try(statement) => Self::Try(statement),
            Stmt::Assert(statement) => Self::Assert(statement),
            Stmt::Import(statement) => Self::Import(statement),
            Stmt::ImportFrom(statement) => Self::ImportFrom(statement),
            Stmt::Global(statement) => Self::Global(statement),
            Stmt::Nonlocal(statement) => Self::Nonlocal(statement),
            Stmt::Expr(statement) => Self::Expr(statement),
            Stmt::Pass(statement) => Self::Pass(statement),
            Stmt::Break(statement) => Self::Break(statement),
            Stmt::Continue(statement) => Self::Continue(statement),
            Stmt::IpyEscapeCommand(statement) => Self::IpyEscapeCommand(statement),
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum TypeParamRef<'a> {
    TypeVar(&'a TypeParamTypeVar),
    ParamSpec(&'a TypeParamParamSpec),
    TypeVarTuple(&'a TypeParamTypeVarTuple),
}

impl<'a> From<&'a TypeParamTypeVar> for TypeParamRef<'a> {
    fn from(value: &'a TypeParamTypeVar) -> Self {
        Self::TypeVar(value)
    }
}

impl<'a> From<&'a TypeParamParamSpec> for TypeParamRef<'a> {
    fn from(value: &'a TypeParamParamSpec) -> Self {
        Self::ParamSpec(value)
    }
}

impl<'a> From<&'a TypeParamTypeVarTuple> for TypeParamRef<'a> {
    fn from(value: &'a TypeParamTypeVarTuple) -> Self {
        Self::TypeVarTuple(value)
    }
}

impl<'a> From<&'a TypeParam> for TypeParamRef<'a> {
    fn from(value: &'a TypeParam) -> Self {
        match value {
            TypeParam::TypeVar(value) => Self::TypeVar(value),
            TypeParam::ParamSpec(value) => Self::ParamSpec(value),
            TypeParam::TypeVarTuple(value) => Self::TypeVarTuple(value),
        }
    }
}
