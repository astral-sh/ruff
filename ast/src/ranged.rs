// This file was originally generated from asdl by a python script, but we now edit it manually

use crate::text_size::{TextRange, TextSize};

pub trait Ranged {
    fn range(&self) -> TextRange;

    fn start(&self) -> TextSize {
        self.range().start()
    }

    fn end(&self) -> TextSize {
        self.range().end()
    }
}

impl Ranged for TextRange {
    fn range(&self) -> TextRange {
        *self
    }
}

impl<T> Ranged for &T
where
    T: Ranged,
{
    fn range(&self) -> TextRange {
        T::range(self)
    }
}

impl Ranged for crate::generic::ModModule {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::generic::ModInteractive {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::generic::ModExpression {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::generic::ModFunctionType {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::Mod {
    fn range(&self) -> TextRange {
        match self {
            Self::Module(node) => node.range(),
            Self::Interactive(node) => node.range(),
            Self::Expression(node) => node.range(),
            Self::FunctionType(node) => node.range(),
        }
    }
}

impl Ranged for crate::generic::StmtFunctionDef {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::generic::StmtAsyncFunctionDef {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::generic::StmtClassDef {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::generic::StmtReturn {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::generic::StmtDelete {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::generic::StmtTypeAlias {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::generic::StmtAssign {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::generic::StmtAugAssign {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::generic::StmtAnnAssign {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::generic::StmtFor {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::generic::StmtAsyncFor {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::generic::StmtWhile {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::generic::StmtIf {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::generic::ElifElseClause {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::generic::StmtWith {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::generic::StmtAsyncWith {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::generic::StmtMatch {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::generic::StmtRaise {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::generic::StmtTry {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::generic::StmtTryStar {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::generic::StmtAssert {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::generic::StmtImport {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::generic::StmtImportFrom {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::generic::StmtGlobal {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::generic::StmtNonlocal {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::generic::StmtExpr {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::generic::StmtPass {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::generic::StmtBreak {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::generic::StmtContinue {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::generic::StmtLineMagic {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::Stmt {
    fn range(&self) -> TextRange {
        match self {
            Self::FunctionDef(node) => node.range(),
            Self::AsyncFunctionDef(node) => node.range(),
            Self::ClassDef(node) => node.range(),
            Self::Return(node) => node.range(),
            Self::Delete(node) => node.range(),
            Self::TypeAlias(node) => node.range(),
            Self::Assign(node) => node.range(),
            Self::AugAssign(node) => node.range(),
            Self::AnnAssign(node) => node.range(),
            Self::For(node) => node.range(),
            Self::AsyncFor(node) => node.range(),
            Self::While(node) => node.range(),
            Self::If(node) => node.range(),
            Self::With(node) => node.range(),
            Self::AsyncWith(node) => node.range(),
            Self::Match(node) => node.range(),
            Self::Raise(node) => node.range(),
            Self::Try(node) => node.range(),
            Self::TryStar(node) => node.range(),
            Self::Assert(node) => node.range(),
            Self::Import(node) => node.range(),
            Self::ImportFrom(node) => node.range(),
            Self::Global(node) => node.range(),
            Self::Nonlocal(node) => node.range(),
            Self::Expr(node) => node.range(),
            Self::Pass(node) => node.range(),
            Self::Break(node) => node.range(),
            Self::Continue(node) => node.range(),
            Self::LineMagic(node) => node.range(),
        }
    }
}

impl Ranged for crate::generic::ExprBoolOp {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::generic::ExprNamedExpr {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::generic::ExprBinOp {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::generic::ExprUnaryOp {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::generic::ExprLambda {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::generic::ExprIfExp {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::generic::ExprDict {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::generic::ExprSet {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::generic::ExprListComp {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::generic::ExprSetComp {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::generic::ExprDictComp {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::generic::ExprGeneratorExp {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::generic::ExprAwait {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::generic::ExprYield {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::generic::ExprYieldFrom {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::generic::ExprCompare {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::generic::ExprCall {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::generic::ExprFormattedValue {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::generic::ExprJoinedStr {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::generic::ExprConstant {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::generic::ExprAttribute {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::generic::ExprSubscript {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::generic::ExprStarred {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::generic::ExprName {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::generic::ExprList {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::generic::ExprTuple {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::generic::ExprSlice {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::generic::ExprLineMagic {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::Expr {
    fn range(&self) -> TextRange {
        match self {
            Self::BoolOp(node) => node.range(),
            Self::NamedExpr(node) => node.range(),
            Self::BinOp(node) => node.range(),
            Self::UnaryOp(node) => node.range(),
            Self::Lambda(node) => node.range(),
            Self::IfExp(node) => node.range(),
            Self::Dict(node) => node.range(),
            Self::Set(node) => node.range(),
            Self::ListComp(node) => node.range(),
            Self::SetComp(node) => node.range(),
            Self::DictComp(node) => node.range(),
            Self::GeneratorExp(node) => node.range(),
            Self::Await(node) => node.range(),
            Self::Yield(node) => node.range(),
            Self::YieldFrom(node) => node.range(),
            Self::Compare(node) => node.range(),
            Self::Call(node) => node.range(),
            Self::FormattedValue(node) => node.range(),
            Self::JoinedStr(node) => node.range(),
            Self::Constant(node) => node.range(),
            Self::Attribute(node) => node.range(),
            Self::Subscript(node) => node.range(),
            Self::Starred(node) => node.range(),
            Self::Name(node) => node.range(),
            Self::List(node) => node.range(),
            Self::Tuple(node) => node.range(),
            Self::Slice(node) => node.range(),
            Self::LineMagic(node) => node.range(),
        }
    }
}

impl Ranged for crate::generic::Comprehension {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::generic::ExceptHandlerExceptHandler {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::ExceptHandler {
    fn range(&self) -> TextRange {
        match self {
            Self::ExceptHandler(node) => node.range(),
        }
    }
}

impl Ranged for crate::generic::PythonArguments {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::generic::Arg {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::generic::Keyword {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::generic::Alias {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::generic::WithItem {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::generic::MatchCase {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::generic::PatternMatchValue {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::generic::PatternMatchSingleton {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::generic::PatternMatchSequence {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::generic::PatternMatchMapping {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::generic::PatternMatchClass {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::generic::PatternMatchStar {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::generic::PatternMatchAs {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::generic::PatternMatchOr {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::Pattern {
    fn range(&self) -> TextRange {
        match self {
            Self::MatchValue(node) => node.range(),
            Self::MatchSingleton(node) => node.range(),
            Self::MatchSequence(node) => node.range(),
            Self::MatchMapping(node) => node.range(),
            Self::MatchClass(node) => node.range(),
            Self::MatchStar(node) => node.range(),
            Self::MatchAs(node) => node.range(),
            Self::MatchOr(node) => node.range(),
        }
    }
}

impl Ranged for crate::generic::TypeIgnoreTypeIgnore {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::TypeIgnore {
    fn range(&self) -> TextRange {
        match self {
            Self::TypeIgnore(node) => node.range(),
        }
    }
}
impl Ranged for crate::generic::TypeParamTypeVar {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::generic::TypeParamTypeVarTuple {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::generic::TypeParamParamSpec {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::TypeParam {
    fn range(&self) -> TextRange {
        match self {
            Self::TypeVar(node) => node.range(),
            Self::TypeVarTuple(node) => node.range(),
            Self::ParamSpec(node) => node.range(),
        }
    }
}
impl Ranged for crate::generic::Decorator {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::generic::Arguments {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::generic::ArgWithDefault {
    fn range(&self) -> TextRange {
        self.range
    }
}
