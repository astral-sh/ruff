#![allow(clippy::derive_partial_eq_without_eq)]

use crate::Ranged;
use num_bigint::BigInt;
use ruff_text_size::{TextRange, TextSize};
use std::fmt;
use std::fmt::Debug;

/// See also [mod](https://docs.python.org/3/library/ast.html#ast.mod)
#[derive(Clone, Debug, PartialEq, is_macro::Is)]
pub enum Mod {
    Module(ModModule),
    Expression(ModExpression),
}

/// See also [Module](https://docs.python.org/3/library/ast.html#ast.Module)
#[derive(Clone, Debug, PartialEq)]
pub struct ModModule {
    pub range: TextRange,
    pub body: Vec<Stmt>,
}

impl From<ModModule> for Mod {
    fn from(payload: ModModule) -> Self {
        Mod::Module(payload)
    }
}

/// See also [Expression](https://docs.python.org/3/library/ast.html#ast.Expression)
#[derive(Clone, Debug, PartialEq)]
pub struct ModExpression {
    pub range: TextRange,
    pub body: Box<Expr>,
}

impl From<ModExpression> for Mod {
    fn from(payload: ModExpression) -> Self {
        Mod::Expression(payload)
    }
}

/// See also [stmt](https://docs.python.org/3/library/ast.html#ast.stmt)
#[derive(Clone, Debug, PartialEq, is_macro::Is)]
pub enum Stmt {
    #[is(name = "function_def_stmt")]
    FunctionDef(StmtFunctionDef),
    #[is(name = "async_function_def_stmt")]
    AsyncFunctionDef(StmtAsyncFunctionDef),
    #[is(name = "class_def_stmt")]
    ClassDef(StmtClassDef),
    #[is(name = "return_stmt")]
    Return(StmtReturn),
    #[is(name = "delete_stmt")]
    Delete(StmtDelete),
    #[is(name = "assign_stmt")]
    Assign(StmtAssign),
    #[is(name = "aug_assign_stmt")]
    AugAssign(StmtAugAssign),
    #[is(name = "ann_assign_stmt")]
    AnnAssign(StmtAnnAssign),
    #[is(name = "type_alias_stmt")]
    TypeAlias(StmtTypeAlias),
    #[is(name = "for_stmt")]
    For(StmtFor),
    #[is(name = "async_for_stmt")]
    AsyncFor(StmtAsyncFor),
    #[is(name = "while_stmt")]
    While(StmtWhile),
    #[is(name = "if_stmt")]
    If(StmtIf),
    #[is(name = "with_stmt")]
    With(StmtWith),
    #[is(name = "async_with_stmt")]
    AsyncWith(StmtAsyncWith),
    #[is(name = "match_stmt")]
    Match(StmtMatch),
    #[is(name = "raise_stmt")]
    Raise(StmtRaise),
    #[is(name = "try_stmt")]
    Try(StmtTry),
    #[is(name = "try_star_stmt")]
    TryStar(StmtTryStar),
    #[is(name = "assert_stmt")]
    Assert(StmtAssert),
    #[is(name = "import_stmt")]
    Import(StmtImport),
    #[is(name = "import_from_stmt")]
    ImportFrom(StmtImportFrom),
    #[is(name = "global_stmt")]
    Global(StmtGlobal),
    #[is(name = "nonlocal_stmt")]
    Nonlocal(StmtNonlocal),
    #[is(name = "expr_stmt")]
    Expr(StmtExpr),
    #[is(name = "pass_stmt")]
    Pass(StmtPass),
    #[is(name = "break_stmt")]
    Break(StmtBreak),
    #[is(name = "continue_stmt")]
    Continue(StmtContinue),

    // Jupyter notebook specific
    #[is(name = "line_magic_stmt")]
    LineMagic(StmtLineMagic),
}

#[derive(Clone, Debug, PartialEq)]
pub struct StmtLineMagic {
    pub range: TextRange,
    pub kind: MagicKind,
    pub value: String,
}

impl From<StmtLineMagic> for Stmt {
    fn from(payload: StmtLineMagic) -> Self {
        Stmt::LineMagic(payload)
    }
}

/// See also [FunctionDef](https://docs.python.org/3/library/ast.html#ast.FunctionDef)
#[derive(Clone, Debug, PartialEq)]
pub struct StmtFunctionDef {
    pub range: TextRange,
    pub name: Identifier,
    pub args: Box<Arguments>,
    pub body: Vec<Stmt>,
    pub decorator_list: Vec<Decorator>,
    pub returns: Option<Box<Expr>>,
    pub type_params: Vec<TypeParam>,
}

impl From<StmtFunctionDef> for Stmt {
    fn from(payload: StmtFunctionDef) -> Self {
        Stmt::FunctionDef(payload)
    }
}

/// See also [AsyncFunctionDef](https://docs.python.org/3/library/ast.html#ast.AsyncFunctionDef)
#[derive(Clone, Debug, PartialEq)]
pub struct StmtAsyncFunctionDef {
    pub range: TextRange,
    pub name: Identifier,
    pub args: Box<Arguments>,
    pub body: Vec<Stmt>,
    pub decorator_list: Vec<Decorator>,
    pub returns: Option<Box<Expr>>,
    pub type_params: Vec<TypeParam>,
}

impl From<StmtAsyncFunctionDef> for Stmt {
    fn from(payload: StmtAsyncFunctionDef) -> Self {
        Stmt::AsyncFunctionDef(payload)
    }
}

/// See also [ClassDef](https://docs.python.org/3/library/ast.html#ast.ClassDef)
#[derive(Clone, Debug, PartialEq)]
pub struct StmtClassDef {
    pub range: TextRange,
    pub name: Identifier,
    pub bases: Vec<Expr>,
    pub keywords: Vec<Keyword>,
    pub body: Vec<Stmt>,
    pub type_params: Vec<TypeParam>,
    pub decorator_list: Vec<Decorator>,
}

impl From<StmtClassDef> for Stmt {
    fn from(payload: StmtClassDef) -> Self {
        Stmt::ClassDef(payload)
    }
}

/// See also [Return](https://docs.python.org/3/library/ast.html#ast.Return)
#[derive(Clone, Debug, PartialEq)]
pub struct StmtReturn {
    pub range: TextRange,
    pub value: Option<Box<Expr>>,
}

impl From<StmtReturn> for Stmt {
    fn from(payload: StmtReturn) -> Self {
        Stmt::Return(payload)
    }
}

/// See also [Delete](https://docs.python.org/3/library/ast.html#ast.Delete)
#[derive(Clone, Debug, PartialEq)]
pub struct StmtDelete {
    pub range: TextRange,
    pub targets: Vec<Expr>,
}

impl From<StmtDelete> for Stmt {
    fn from(payload: StmtDelete) -> Self {
        Stmt::Delete(payload)
    }
}

/// See also [TypeAlias](https://docs.python.org/3/library/ast.html#ast.TypeAlias)
#[derive(Clone, Debug, PartialEq)]
pub struct StmtTypeAlias {
    pub range: TextRange,
    pub name: Box<Expr>,
    pub type_params: Vec<TypeParam>,
    pub value: Box<Expr>,
}

impl From<StmtTypeAlias> for Stmt {
    fn from(payload: StmtTypeAlias) -> Self {
        Stmt::TypeAlias(payload)
    }
}

/// See also [Assign](https://docs.python.org/3/library/ast.html#ast.Assign)
#[derive(Clone, Debug, PartialEq)]
pub struct StmtAssign {
    pub range: TextRange,
    pub targets: Vec<Expr>,
    pub value: Box<Expr>,
}

impl From<StmtAssign> for Stmt {
    fn from(payload: StmtAssign) -> Self {
        Stmt::Assign(payload)
    }
}

/// See also [AugAssign](https://docs.python.org/3/library/ast.html#ast.AugAssign)
#[derive(Clone, Debug, PartialEq)]
pub struct StmtAugAssign {
    pub range: TextRange,
    pub target: Box<Expr>,
    pub op: Operator,
    pub value: Box<Expr>,
}

impl From<StmtAugAssign> for Stmt {
    fn from(payload: StmtAugAssign) -> Self {
        Stmt::AugAssign(payload)
    }
}

/// See also [AnnAssign](https://docs.python.org/3/library/ast.html#ast.AnnAssign)
#[derive(Clone, Debug, PartialEq)]
pub struct StmtAnnAssign {
    pub range: TextRange,
    pub target: Box<Expr>,
    pub annotation: Box<Expr>,
    pub value: Option<Box<Expr>>,
    pub simple: bool,
}

impl From<StmtAnnAssign> for Stmt {
    fn from(payload: StmtAnnAssign) -> Self {
        Stmt::AnnAssign(payload)
    }
}

/// See also [For](https://docs.python.org/3/library/ast.html#ast.For)
#[derive(Clone, Debug, PartialEq)]
pub struct StmtFor {
    pub range: TextRange,
    pub target: Box<Expr>,
    pub iter: Box<Expr>,
    pub body: Vec<Stmt>,
    pub orelse: Vec<Stmt>,
}

impl From<StmtFor> for Stmt {
    fn from(payload: StmtFor) -> Self {
        Stmt::For(payload)
    }
}

/// See also [AsyncFor](https://docs.python.org/3/library/ast.html#ast.AsyncFor)
#[derive(Clone, Debug, PartialEq)]
pub struct StmtAsyncFor {
    pub range: TextRange,
    pub target: Box<Expr>,
    pub iter: Box<Expr>,
    pub body: Vec<Stmt>,
    pub orelse: Vec<Stmt>,
}

impl From<StmtAsyncFor> for Stmt {
    fn from(payload: StmtAsyncFor) -> Self {
        Stmt::AsyncFor(payload)
    }
}

/// See also [While](https://docs.python.org/3/library/ast.html#ast.While)
#[derive(Clone, Debug, PartialEq)]
pub struct StmtWhile {
    pub range: TextRange,
    pub test: Box<Expr>,
    pub body: Vec<Stmt>,
    pub orelse: Vec<Stmt>,
}

impl From<StmtWhile> for Stmt {
    fn from(payload: StmtWhile) -> Self {
        Stmt::While(payload)
    }
}

/// See also [If](https://docs.python.org/3/library/ast.html#ast.If)
#[derive(Clone, Debug, PartialEq)]
pub struct StmtIf {
    pub range: TextRange,
    pub test: Box<Expr>,
    pub body: Vec<Stmt>,
    pub elif_else_clauses: Vec<ElifElseClause>,
}

impl From<StmtIf> for Stmt {
    fn from(payload: StmtIf) -> Self {
        Stmt::If(payload)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ElifElseClause {
    pub range: TextRange,
    pub test: Option<Expr>,
    pub body: Vec<Stmt>,
}

/// See also [With](https://docs.python.org/3/library/ast.html#ast.With)
#[derive(Clone, Debug, PartialEq)]
pub struct StmtWith {
    pub range: TextRange,
    pub items: Vec<WithItem>,
    pub body: Vec<Stmt>,
}

impl From<StmtWith> for Stmt {
    fn from(payload: StmtWith) -> Self {
        Stmt::With(payload)
    }
}

/// See also [AsyncWith](https://docs.python.org/3/library/ast.html#ast.AsyncWith)
#[derive(Clone, Debug, PartialEq)]
pub struct StmtAsyncWith {
    pub range: TextRange,
    pub items: Vec<WithItem>,
    pub body: Vec<Stmt>,
}

impl From<StmtAsyncWith> for Stmt {
    fn from(payload: StmtAsyncWith) -> Self {
        Stmt::AsyncWith(payload)
    }
}

/// See also [Match](https://docs.python.org/3/library/ast.html#ast.Match)
#[derive(Clone, Debug, PartialEq)]
pub struct StmtMatch {
    pub range: TextRange,
    pub subject: Box<Expr>,
    pub cases: Vec<MatchCase>,
}

impl From<StmtMatch> for Stmt {
    fn from(payload: StmtMatch) -> Self {
        Stmt::Match(payload)
    }
}

/// See also [Raise](https://docs.python.org/3/library/ast.html#ast.Raise)
#[derive(Clone, Debug, PartialEq)]
pub struct StmtRaise {
    pub range: TextRange,
    pub exc: Option<Box<Expr>>,
    pub cause: Option<Box<Expr>>,
}

impl From<StmtRaise> for Stmt {
    fn from(payload: StmtRaise) -> Self {
        Stmt::Raise(payload)
    }
}

/// See also [Try](https://docs.python.org/3/library/ast.html#ast.Try)
#[derive(Clone, Debug, PartialEq)]
pub struct StmtTry {
    pub range: TextRange,
    pub body: Vec<Stmt>,
    pub handlers: Vec<ExceptHandler>,
    pub orelse: Vec<Stmt>,
    pub finalbody: Vec<Stmt>,
}

impl From<StmtTry> for Stmt {
    fn from(payload: StmtTry) -> Self {
        Stmt::Try(payload)
    }
}

/// See also [TryStar](https://docs.python.org/3/library/ast.html#ast.TryStar)
#[derive(Clone, Debug, PartialEq)]
pub struct StmtTryStar {
    pub range: TextRange,
    pub body: Vec<Stmt>,
    pub handlers: Vec<ExceptHandler>,
    pub orelse: Vec<Stmt>,
    pub finalbody: Vec<Stmt>,
}

impl From<StmtTryStar> for Stmt {
    fn from(payload: StmtTryStar) -> Self {
        Stmt::TryStar(payload)
    }
}

/// See also [Assert](https://docs.python.org/3/library/ast.html#ast.Assert)
#[derive(Clone, Debug, PartialEq)]
pub struct StmtAssert {
    pub range: TextRange,
    pub test: Box<Expr>,
    pub msg: Option<Box<Expr>>,
}

impl From<StmtAssert> for Stmt {
    fn from(payload: StmtAssert) -> Self {
        Stmt::Assert(payload)
    }
}

/// See also [Import](https://docs.python.org/3/library/ast.html#ast.Import)
#[derive(Clone, Debug, PartialEq)]
pub struct StmtImport {
    pub range: TextRange,
    pub names: Vec<Alias>,
}

impl From<StmtImport> for Stmt {
    fn from(payload: StmtImport) -> Self {
        Stmt::Import(payload)
    }
}

/// See also [ImportFrom](https://docs.python.org/3/library/ast.html#ast.ImportFrom)
#[derive(Clone, Debug, PartialEq)]
pub struct StmtImportFrom {
    pub range: TextRange,
    pub module: Option<Identifier>,
    pub names: Vec<Alias>,
    pub level: Option<Int>,
}

impl From<StmtImportFrom> for Stmt {
    fn from(payload: StmtImportFrom) -> Self {
        Stmt::ImportFrom(payload)
    }
}

/// See also [Global](https://docs.python.org/3/library/ast.html#ast.Global)
#[derive(Clone, Debug, PartialEq)]
pub struct StmtGlobal {
    pub range: TextRange,
    pub names: Vec<Identifier>,
}

impl From<StmtGlobal> for Stmt {
    fn from(payload: StmtGlobal) -> Self {
        Stmt::Global(payload)
    }
}

/// See also [Nonlocal](https://docs.python.org/3/library/ast.html#ast.Nonlocal)
#[derive(Clone, Debug, PartialEq)]
pub struct StmtNonlocal {
    pub range: TextRange,
    pub names: Vec<Identifier>,
}

impl From<StmtNonlocal> for Stmt {
    fn from(payload: StmtNonlocal) -> Self {
        Stmt::Nonlocal(payload)
    }
}

/// See also [Expr](https://docs.python.org/3/library/ast.html#ast.Expr)
#[derive(Clone, Debug, PartialEq)]
pub struct StmtExpr {
    pub range: TextRange,
    pub value: Box<Expr>,
}

impl From<StmtExpr> for Stmt {
    fn from(payload: StmtExpr) -> Self {
        Stmt::Expr(payload)
    }
}

/// See also [Pass](https://docs.python.org/3/library/ast.html#ast.Pass)
#[derive(Clone, Debug, PartialEq)]
pub struct StmtPass {
    pub range: TextRange,
}

impl From<StmtPass> for Stmt {
    fn from(payload: StmtPass) -> Self {
        Stmt::Pass(payload)
    }
}

/// See also [Break](https://docs.python.org/3/library/ast.html#ast.Break)
#[derive(Clone, Debug, PartialEq)]
pub struct StmtBreak {
    pub range: TextRange,
}

impl From<StmtBreak> for Stmt {
    fn from(payload: StmtBreak) -> Self {
        Stmt::Break(payload)
    }
}

/// See also [Continue](https://docs.python.org/3/library/ast.html#ast.Continue)
#[derive(Clone, Debug, PartialEq)]
pub struct StmtContinue {
    pub range: TextRange,
}

impl From<StmtContinue> for Stmt {
    fn from(payload: StmtContinue) -> Self {
        Stmt::Continue(payload)
    }
}

/// See also [expr](https://docs.python.org/3/library/ast.html#ast.expr)
#[derive(Clone, Debug, PartialEq, is_macro::Is)]
pub enum Expr {
    #[is(name = "bool_op_expr")]
    BoolOp(ExprBoolOp),
    #[is(name = "named_expr_expr")]
    NamedExpr(ExprNamedExpr),
    #[is(name = "bin_op_expr")]
    BinOp(ExprBinOp),
    #[is(name = "unary_op_expr")]
    UnaryOp(ExprUnaryOp),
    #[is(name = "lambda_expr")]
    Lambda(ExprLambda),
    #[is(name = "if_exp_expr")]
    IfExp(ExprIfExp),
    #[is(name = "dict_expr")]
    Dict(ExprDict),
    #[is(name = "set_expr")]
    Set(ExprSet),
    #[is(name = "list_comp_expr")]
    ListComp(ExprListComp),
    #[is(name = "set_comp_expr")]
    SetComp(ExprSetComp),
    #[is(name = "dict_comp_expr")]
    DictComp(ExprDictComp),
    #[is(name = "generator_exp_expr")]
    GeneratorExp(ExprGeneratorExp),
    #[is(name = "await_expr")]
    Await(ExprAwait),
    #[is(name = "yield_expr")]
    Yield(ExprYield),
    #[is(name = "yield_from_expr")]
    YieldFrom(ExprYieldFrom),
    #[is(name = "compare_expr")]
    Compare(ExprCompare),
    #[is(name = "call_expr")]
    Call(ExprCall),
    #[is(name = "formatted_value_expr")]
    FormattedValue(ExprFormattedValue),
    #[is(name = "joined_str_expr")]
    JoinedStr(ExprJoinedStr),
    #[is(name = "constant_expr")]
    Constant(ExprConstant),
    #[is(name = "attribute_expr")]
    Attribute(ExprAttribute),
    #[is(name = "subscript_expr")]
    Subscript(ExprSubscript),
    #[is(name = "starred_expr")]
    Starred(ExprStarred),
    #[is(name = "name_expr")]
    Name(ExprName),
    #[is(name = "list_expr")]
    List(ExprList),
    #[is(name = "tuple_expr")]
    Tuple(ExprTuple),
    #[is(name = "slice_expr")]
    Slice(ExprSlice),

    // Jupyter notebook specific
    #[is(name = "line_magic_expr")]
    LineMagic(ExprLineMagic),
}

#[derive(Clone, Debug, PartialEq)]
pub struct ExprLineMagic {
    pub range: TextRange,
    pub kind: MagicKind,
    pub value: String,
}

impl From<ExprLineMagic> for Expr {
    fn from(payload: ExprLineMagic) -> Self {
        Expr::LineMagic(payload)
    }
}

/// See also [BoolOp](https://docs.python.org/3/library/ast.html#ast.BoolOp)
#[derive(Clone, Debug, PartialEq)]
pub struct ExprBoolOp {
    pub range: TextRange,
    pub op: BoolOp,
    pub values: Vec<Expr>,
}

impl From<ExprBoolOp> for Expr {
    fn from(payload: ExprBoolOp) -> Self {
        Expr::BoolOp(payload)
    }
}

/// See also [NamedExpr](https://docs.python.org/3/library/ast.html#ast.NamedExpr)
#[derive(Clone, Debug, PartialEq)]
pub struct ExprNamedExpr {
    pub range: TextRange,
    pub target: Box<Expr>,
    pub value: Box<Expr>,
}

impl From<ExprNamedExpr> for Expr {
    fn from(payload: ExprNamedExpr) -> Self {
        Expr::NamedExpr(payload)
    }
}

/// See also [BinOp](https://docs.python.org/3/library/ast.html#ast.BinOp)
#[derive(Clone, Debug, PartialEq)]
pub struct ExprBinOp {
    pub range: TextRange,
    pub left: Box<Expr>,
    pub op: Operator,
    pub right: Box<Expr>,
}

impl From<ExprBinOp> for Expr {
    fn from(payload: ExprBinOp) -> Self {
        Expr::BinOp(payload)
    }
}

/// See also [UnaryOp](https://docs.python.org/3/library/ast.html#ast.UnaryOp)
#[derive(Clone, Debug, PartialEq)]
pub struct ExprUnaryOp {
    pub range: TextRange,
    pub op: UnaryOp,
    pub operand: Box<Expr>,
}

impl From<ExprUnaryOp> for Expr {
    fn from(payload: ExprUnaryOp) -> Self {
        Expr::UnaryOp(payload)
    }
}

/// See also [Lambda](https://docs.python.org/3/library/ast.html#ast.Lambda)
#[derive(Clone, Debug, PartialEq)]
pub struct ExprLambda {
    pub range: TextRange,
    pub args: Box<Arguments>,
    pub body: Box<Expr>,
}

impl From<ExprLambda> for Expr {
    fn from(payload: ExprLambda) -> Self {
        Expr::Lambda(payload)
    }
}

/// See also [IfExp](https://docs.python.org/3/library/ast.html#ast.IfExp)
#[derive(Clone, Debug, PartialEq)]
pub struct ExprIfExp {
    pub range: TextRange,
    pub test: Box<Expr>,
    pub body: Box<Expr>,
    pub orelse: Box<Expr>,
}

impl From<ExprIfExp> for Expr {
    fn from(payload: ExprIfExp) -> Self {
        Expr::IfExp(payload)
    }
}

/// See also [Dict](https://docs.python.org/3/library/ast.html#ast.Dict)
#[derive(Clone, Debug, PartialEq)]
pub struct ExprDict {
    pub range: TextRange,
    pub keys: Vec<Option<Expr>>,
    pub values: Vec<Expr>,
}

impl From<ExprDict> for Expr {
    fn from(payload: ExprDict) -> Self {
        Expr::Dict(payload)
    }
}

/// See also [Set](https://docs.python.org/3/library/ast.html#ast.Set)
#[derive(Clone, Debug, PartialEq)]
pub struct ExprSet {
    pub range: TextRange,
    pub elts: Vec<Expr>,
}

impl From<ExprSet> for Expr {
    fn from(payload: ExprSet) -> Self {
        Expr::Set(payload)
    }
}

/// See also [ListComp](https://docs.python.org/3/library/ast.html#ast.ListComp)
#[derive(Clone, Debug, PartialEq)]
pub struct ExprListComp {
    pub range: TextRange,
    pub elt: Box<Expr>,
    pub generators: Vec<Comprehension>,
}

impl From<ExprListComp> for Expr {
    fn from(payload: ExprListComp) -> Self {
        Expr::ListComp(payload)
    }
}

/// See also [SetComp](https://docs.python.org/3/library/ast.html#ast.SetComp)
#[derive(Clone, Debug, PartialEq)]
pub struct ExprSetComp {
    pub range: TextRange,
    pub elt: Box<Expr>,
    pub generators: Vec<Comprehension>,
}

impl From<ExprSetComp> for Expr {
    fn from(payload: ExprSetComp) -> Self {
        Expr::SetComp(payload)
    }
}

/// See also [DictComp](https://docs.python.org/3/library/ast.html#ast.DictComp)
#[derive(Clone, Debug, PartialEq)]
pub struct ExprDictComp {
    pub range: TextRange,
    pub key: Box<Expr>,
    pub value: Box<Expr>,
    pub generators: Vec<Comprehension>,
}

impl From<ExprDictComp> for Expr {
    fn from(payload: ExprDictComp) -> Self {
        Expr::DictComp(payload)
    }
}

/// See also [GeneratorExp](https://docs.python.org/3/library/ast.html#ast.GeneratorExp)
#[derive(Clone, Debug, PartialEq)]
pub struct ExprGeneratorExp {
    pub range: TextRange,
    pub elt: Box<Expr>,
    pub generators: Vec<Comprehension>,
}

impl From<ExprGeneratorExp> for Expr {
    fn from(payload: ExprGeneratorExp) -> Self {
        Expr::GeneratorExp(payload)
    }
}

/// See also [Await](https://docs.python.org/3/library/ast.html#ast.Await)
#[derive(Clone, Debug, PartialEq)]
pub struct ExprAwait {
    pub range: TextRange,
    pub value: Box<Expr>,
}

impl From<ExprAwait> for Expr {
    fn from(payload: ExprAwait) -> Self {
        Expr::Await(payload)
    }
}

/// See also [Yield](https://docs.python.org/3/library/ast.html#ast.Yield)
#[derive(Clone, Debug, PartialEq)]
pub struct ExprYield {
    pub range: TextRange,
    pub value: Option<Box<Expr>>,
}

impl From<ExprYield> for Expr {
    fn from(payload: ExprYield) -> Self {
        Expr::Yield(payload)
    }
}

/// See also [YieldFrom](https://docs.python.org/3/library/ast.html#ast.YieldFrom)
#[derive(Clone, Debug, PartialEq)]
pub struct ExprYieldFrom {
    pub range: TextRange,
    pub value: Box<Expr>,
}

impl From<ExprYieldFrom> for Expr {
    fn from(payload: ExprYieldFrom) -> Self {
        Expr::YieldFrom(payload)
    }
}

/// See also [Compare](https://docs.python.org/3/library/ast.html#ast.Compare)
#[derive(Clone, Debug, PartialEq)]
pub struct ExprCompare {
    pub range: TextRange,
    pub left: Box<Expr>,
    pub ops: Vec<CmpOp>,
    pub comparators: Vec<Expr>,
}

impl From<ExprCompare> for Expr {
    fn from(payload: ExprCompare) -> Self {
        Expr::Compare(payload)
    }
}

/// See also [Call](https://docs.python.org/3/library/ast.html#ast.Call)
#[derive(Clone, Debug, PartialEq)]
pub struct ExprCall {
    pub range: TextRange,
    pub func: Box<Expr>,
    pub args: Vec<Expr>,
    pub keywords: Vec<Keyword>,
}

impl From<ExprCall> for Expr {
    fn from(payload: ExprCall) -> Self {
        Expr::Call(payload)
    }
}

/// See also [FormattedValue](https://docs.python.org/3/library/ast.html#ast.FormattedValue)
#[derive(Clone, Debug, PartialEq)]
pub struct ExprFormattedValue {
    pub range: TextRange,
    pub value: Box<Expr>,
    pub debug_text: Option<DebugText>,
    pub conversion: ConversionFlag,
    pub format_spec: Option<Box<Expr>>,
}

impl From<ExprFormattedValue> for Expr {
    fn from(payload: ExprFormattedValue) -> Self {
        Expr::FormattedValue(payload)
    }
}

/// Transforms a value prior to formatting it.
#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq, is_macro::Is)]
#[repr(i8)]
#[allow(clippy::cast_possible_wrap)]
pub enum ConversionFlag {
    /// No conversion
    None = -1, // CPython uses -1
    /// Converts by calling `str(<value>)`.
    Str = b's' as i8,
    /// Converts by calling `ascii(<value>)`.
    Ascii = b'a' as i8,
    /// Converts by calling `repr(<value>)`.
    Repr = b'r' as i8,
}

impl ConversionFlag {
    pub fn to_byte(&self) -> Option<u8> {
        match self {
            Self::None => None,
            flag => Some(*flag as u8),
        }
    }
    pub fn to_char(&self) -> Option<char> {
        Some(self.to_byte()? as char)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct DebugText {
    /// The text between the `{` and the expression node.
    pub leading: String,
    /// The text between the expression and the conversion, the format_spec, or the `}`, depending on what's present in the source
    pub trailing: String,
}

/// See also [JoinedStr](https://docs.python.org/3/library/ast.html#ast.JoinedStr)
#[derive(Clone, Debug, PartialEq)]
pub struct ExprJoinedStr {
    pub range: TextRange,
    pub values: Vec<Expr>,
}

impl From<ExprJoinedStr> for Expr {
    fn from(payload: ExprJoinedStr) -> Self {
        Expr::JoinedStr(payload)
    }
}

/// See also [Constant](https://docs.python.org/3/library/ast.html#ast.Constant)
#[derive(Clone, Debug, PartialEq)]
pub struct ExprConstant {
    pub range: TextRange,
    pub value: Constant,
    pub kind: Option<String>,
}

impl From<ExprConstant> for Expr {
    fn from(payload: ExprConstant) -> Self {
        Expr::Constant(payload)
    }
}

/// See also [Attribute](https://docs.python.org/3/library/ast.html#ast.Attribute)
#[derive(Clone, Debug, PartialEq)]
pub struct ExprAttribute {
    pub range: TextRange,
    pub value: Box<Expr>,
    pub attr: Identifier,
    pub ctx: ExprContext,
}

impl From<ExprAttribute> for Expr {
    fn from(payload: ExprAttribute) -> Self {
        Expr::Attribute(payload)
    }
}

/// See also [Subscript](https://docs.python.org/3/library/ast.html#ast.Subscript)
#[derive(Clone, Debug, PartialEq)]
pub struct ExprSubscript {
    pub range: TextRange,
    pub value: Box<Expr>,
    pub slice: Box<Expr>,
    pub ctx: ExprContext,
}

impl From<ExprSubscript> for Expr {
    fn from(payload: ExprSubscript) -> Self {
        Expr::Subscript(payload)
    }
}

/// See also [Starred](https://docs.python.org/3/library/ast.html#ast.Starred)
#[derive(Clone, Debug, PartialEq)]
pub struct ExprStarred {
    pub range: TextRange,
    pub value: Box<Expr>,
    pub ctx: ExprContext,
}

impl From<ExprStarred> for Expr {
    fn from(payload: ExprStarred) -> Self {
        Expr::Starred(payload)
    }
}

/// See also [Name](https://docs.python.org/3/library/ast.html#ast.Name)
#[derive(Clone, Debug, PartialEq)]
pub struct ExprName {
    pub range: TextRange,
    pub id: String,
    pub ctx: ExprContext,
}

impl From<ExprName> for Expr {
    fn from(payload: ExprName) -> Self {
        Expr::Name(payload)
    }
}

/// See also [List](https://docs.python.org/3/library/ast.html#ast.List)
#[derive(Clone, Debug, PartialEq)]
pub struct ExprList {
    pub range: TextRange,
    pub elts: Vec<Expr>,
    pub ctx: ExprContext,
}

impl From<ExprList> for Expr {
    fn from(payload: ExprList) -> Self {
        Expr::List(payload)
    }
}

/// See also [Tuple](https://docs.python.org/3/library/ast.html#ast.Tuple)
#[derive(Clone, Debug, PartialEq)]
pub struct ExprTuple {
    pub range: TextRange,
    pub elts: Vec<Expr>,
    pub ctx: ExprContext,
}

impl From<ExprTuple> for Expr {
    fn from(payload: ExprTuple) -> Self {
        Expr::Tuple(payload)
    }
}

/// See also [Slice](https://docs.python.org/3/library/ast.html#ast.Slice)
#[derive(Clone, Debug, PartialEq)]
pub struct ExprSlice {
    pub range: TextRange,
    pub lower: Option<Box<Expr>>,
    pub upper: Option<Box<Expr>>,
    pub step: Option<Box<Expr>>,
}

impl From<ExprSlice> for Expr {
    fn from(payload: ExprSlice) -> Self {
        Expr::Slice(payload)
    }
}

/// See also [expr_context](https://docs.python.org/3/library/ast.html#ast.expr_context)
#[derive(Clone, Debug, PartialEq, is_macro::Is, Copy, Hash, Eq)]
pub enum ExprContext {
    Load,
    Store,
    Del,
}
impl ExprContext {
    #[inline]
    pub const fn load(&self) -> Option<ExprContextLoad> {
        match self {
            ExprContext::Load => Some(ExprContextLoad),
            _ => None,
        }
    }

    #[inline]
    pub const fn store(&self) -> Option<ExprContextStore> {
        match self {
            ExprContext::Store => Some(ExprContextStore),
            _ => None,
        }
    }

    #[inline]
    pub const fn del(&self) -> Option<ExprContextDel> {
        match self {
            ExprContext::Del => Some(ExprContextDel),
            _ => None,
        }
    }
}

pub struct ExprContextLoad;
impl From<ExprContextLoad> for ExprContext {
    fn from(_: ExprContextLoad) -> Self {
        ExprContext::Load
    }
}

impl std::cmp::PartialEq<ExprContext> for ExprContextLoad {
    #[inline]
    fn eq(&self, other: &ExprContext) -> bool {
        matches!(other, ExprContext::Load)
    }
}

pub struct ExprContextStore;
impl From<ExprContextStore> for ExprContext {
    fn from(_: ExprContextStore) -> Self {
        ExprContext::Store
    }
}

impl std::cmp::PartialEq<ExprContext> for ExprContextStore {
    #[inline]
    fn eq(&self, other: &ExprContext) -> bool {
        matches!(other, ExprContext::Store)
    }
}

pub struct ExprContextDel;
impl From<ExprContextDel> for ExprContext {
    fn from(_: ExprContextDel) -> Self {
        ExprContext::Del
    }
}

impl std::cmp::PartialEq<ExprContext> for ExprContextDel {
    #[inline]
    fn eq(&self, other: &ExprContext) -> bool {
        matches!(other, ExprContext::Del)
    }
}

/// See also [boolop](https://docs.python.org/3/library/ast.html#ast.BoolOp)
#[derive(Clone, Debug, PartialEq, is_macro::Is, Copy, Hash, Eq)]
pub enum BoolOp {
    And,
    Or,
}
impl BoolOp {
    #[inline]
    pub const fn and(&self) -> Option<BoolOpAnd> {
        match self {
            BoolOp::And => Some(BoolOpAnd),
            BoolOp::Or => None,
        }
    }

    #[inline]
    pub const fn or(&self) -> Option<BoolOpOr> {
        match self {
            BoolOp::Or => Some(BoolOpOr),
            BoolOp::And => None,
        }
    }
}

pub struct BoolOpAnd;
impl From<BoolOpAnd> for BoolOp {
    fn from(_: BoolOpAnd) -> Self {
        BoolOp::And
    }
}

impl std::cmp::PartialEq<BoolOp> for BoolOpAnd {
    #[inline]
    fn eq(&self, other: &BoolOp) -> bool {
        matches!(other, BoolOp::And)
    }
}

pub struct BoolOpOr;
impl From<BoolOpOr> for BoolOp {
    fn from(_: BoolOpOr) -> Self {
        BoolOp::Or
    }
}

impl std::cmp::PartialEq<BoolOp> for BoolOpOr {
    #[inline]
    fn eq(&self, other: &BoolOp) -> bool {
        matches!(other, BoolOp::Or)
    }
}

/// See also [operator](https://docs.python.org/3/library/ast.html#ast.operator)
#[derive(Clone, Debug, PartialEq, is_macro::Is, Copy, Hash, Eq)]
pub enum Operator {
    Add,
    Sub,
    Mult,
    MatMult,
    Div,
    Mod,
    Pow,
    LShift,
    RShift,
    BitOr,
    BitXor,
    BitAnd,
    FloorDiv,
}
impl Operator {
    #[inline]
    pub const fn operator_add(&self) -> Option<OperatorAdd> {
        match self {
            Operator::Add => Some(OperatorAdd),
            _ => None,
        }
    }

    #[inline]
    pub const fn operator_sub(&self) -> Option<OperatorSub> {
        match self {
            Operator::Sub => Some(OperatorSub),
            _ => None,
        }
    }

    #[inline]
    pub const fn operator_mult(&self) -> Option<OperatorMult> {
        match self {
            Operator::Mult => Some(OperatorMult),
            _ => None,
        }
    }

    #[inline]
    pub const fn operator_mat_mult(&self) -> Option<OperatorMatMult> {
        match self {
            Operator::MatMult => Some(OperatorMatMult),
            _ => None,
        }
    }

    #[inline]
    pub const fn operator_div(&self) -> Option<OperatorDiv> {
        match self {
            Operator::Div => Some(OperatorDiv),
            _ => None,
        }
    }

    #[inline]
    pub const fn operator_mod(&self) -> Option<OperatorMod> {
        match self {
            Operator::Mod => Some(OperatorMod),
            _ => None,
        }
    }

    #[inline]
    pub const fn operator_pow(&self) -> Option<OperatorPow> {
        match self {
            Operator::Pow => Some(OperatorPow),
            _ => None,
        }
    }

    #[inline]
    pub const fn operator_l_shift(&self) -> Option<OperatorLShift> {
        match self {
            Operator::LShift => Some(OperatorLShift),
            _ => None,
        }
    }

    #[inline]
    pub const fn operator_r_shift(&self) -> Option<OperatorRShift> {
        match self {
            Operator::RShift => Some(OperatorRShift),
            _ => None,
        }
    }

    #[inline]
    pub const fn operator_bit_or(&self) -> Option<OperatorBitOr> {
        match self {
            Operator::BitOr => Some(OperatorBitOr),
            _ => None,
        }
    }

    #[inline]
    pub const fn operator_bit_xor(&self) -> Option<OperatorBitXor> {
        match self {
            Operator::BitXor => Some(OperatorBitXor),
            _ => None,
        }
    }

    #[inline]
    pub const fn operator_bit_and(&self) -> Option<OperatorBitAnd> {
        match self {
            Operator::BitAnd => Some(OperatorBitAnd),
            _ => None,
        }
    }

    #[inline]
    pub const fn operator_floor_div(&self) -> Option<OperatorFloorDiv> {
        match self {
            Operator::FloorDiv => Some(OperatorFloorDiv),
            _ => None,
        }
    }
}

pub struct OperatorAdd;
impl From<OperatorAdd> for Operator {
    fn from(_: OperatorAdd) -> Self {
        Operator::Add
    }
}

impl std::cmp::PartialEq<Operator> for OperatorAdd {
    #[inline]
    fn eq(&self, other: &Operator) -> bool {
        matches!(other, Operator::Add)
    }
}

pub struct OperatorSub;
impl From<OperatorSub> for Operator {
    fn from(_: OperatorSub) -> Self {
        Operator::Sub
    }
}

impl std::cmp::PartialEq<Operator> for OperatorSub {
    #[inline]
    fn eq(&self, other: &Operator) -> bool {
        matches!(other, Operator::Sub)
    }
}

pub struct OperatorMult;
impl From<OperatorMult> for Operator {
    fn from(_: OperatorMult) -> Self {
        Operator::Mult
    }
}

impl std::cmp::PartialEq<Operator> for OperatorMult {
    #[inline]
    fn eq(&self, other: &Operator) -> bool {
        matches!(other, Operator::Mult)
    }
}

pub struct OperatorMatMult;
impl From<OperatorMatMult> for Operator {
    fn from(_: OperatorMatMult) -> Self {
        Operator::MatMult
    }
}

impl std::cmp::PartialEq<Operator> for OperatorMatMult {
    #[inline]
    fn eq(&self, other: &Operator) -> bool {
        matches!(other, Operator::MatMult)
    }
}

pub struct OperatorDiv;
impl From<OperatorDiv> for Operator {
    fn from(_: OperatorDiv) -> Self {
        Operator::Div
    }
}

impl std::cmp::PartialEq<Operator> for OperatorDiv {
    #[inline]
    fn eq(&self, other: &Operator) -> bool {
        matches!(other, Operator::Div)
    }
}

pub struct OperatorMod;
impl From<OperatorMod> for Operator {
    fn from(_: OperatorMod) -> Self {
        Operator::Mod
    }
}

impl std::cmp::PartialEq<Operator> for OperatorMod {
    #[inline]
    fn eq(&self, other: &Operator) -> bool {
        matches!(other, Operator::Mod)
    }
}

pub struct OperatorPow;
impl From<OperatorPow> for Operator {
    fn from(_: OperatorPow) -> Self {
        Operator::Pow
    }
}

impl std::cmp::PartialEq<Operator> for OperatorPow {
    #[inline]
    fn eq(&self, other: &Operator) -> bool {
        matches!(other, Operator::Pow)
    }
}

pub struct OperatorLShift;
impl From<OperatorLShift> for Operator {
    fn from(_: OperatorLShift) -> Self {
        Operator::LShift
    }
}

impl std::cmp::PartialEq<Operator> for OperatorLShift {
    #[inline]
    fn eq(&self, other: &Operator) -> bool {
        matches!(other, Operator::LShift)
    }
}

pub struct OperatorRShift;
impl From<OperatorRShift> for Operator {
    fn from(_: OperatorRShift) -> Self {
        Operator::RShift
    }
}

impl std::cmp::PartialEq<Operator> for OperatorRShift {
    #[inline]
    fn eq(&self, other: &Operator) -> bool {
        matches!(other, Operator::RShift)
    }
}

pub struct OperatorBitOr;
impl From<OperatorBitOr> for Operator {
    fn from(_: OperatorBitOr) -> Self {
        Operator::BitOr
    }
}

impl std::cmp::PartialEq<Operator> for OperatorBitOr {
    #[inline]
    fn eq(&self, other: &Operator) -> bool {
        matches!(other, Operator::BitOr)
    }
}

pub struct OperatorBitXor;
impl From<OperatorBitXor> for Operator {
    fn from(_: OperatorBitXor) -> Self {
        Operator::BitXor
    }
}

impl std::cmp::PartialEq<Operator> for OperatorBitXor {
    #[inline]
    fn eq(&self, other: &Operator) -> bool {
        matches!(other, Operator::BitXor)
    }
}

pub struct OperatorBitAnd;
impl From<OperatorBitAnd> for Operator {
    fn from(_: OperatorBitAnd) -> Self {
        Operator::BitAnd
    }
}

impl std::cmp::PartialEq<Operator> for OperatorBitAnd {
    #[inline]
    fn eq(&self, other: &Operator) -> bool {
        matches!(other, Operator::BitAnd)
    }
}

pub struct OperatorFloorDiv;
impl From<OperatorFloorDiv> for Operator {
    fn from(_: OperatorFloorDiv) -> Self {
        Operator::FloorDiv
    }
}

impl std::cmp::PartialEq<Operator> for OperatorFloorDiv {
    #[inline]
    fn eq(&self, other: &Operator) -> bool {
        matches!(other, Operator::FloorDiv)
    }
}

/// See also [unaryop](https://docs.python.org/3/library/ast.html#ast.unaryop)
#[derive(Clone, Debug, PartialEq, is_macro::Is, Copy, Hash, Eq)]
pub enum UnaryOp {
    Invert,
    Not,
    UAdd,
    USub,
}
impl UnaryOp {
    #[inline]
    pub const fn invert(&self) -> Option<UnaryOpInvert> {
        match self {
            UnaryOp::Invert => Some(UnaryOpInvert),
            _ => None,
        }
    }

    #[inline]
    pub const fn not(&self) -> Option<UnaryOpNot> {
        match self {
            UnaryOp::Not => Some(UnaryOpNot),
            _ => None,
        }
    }

    #[inline]
    pub const fn u_add(&self) -> Option<UnaryOpUAdd> {
        match self {
            UnaryOp::UAdd => Some(UnaryOpUAdd),
            _ => None,
        }
    }

    #[inline]
    pub const fn u_sub(&self) -> Option<UnaryOpUSub> {
        match self {
            UnaryOp::USub => Some(UnaryOpUSub),
            _ => None,
        }
    }
}

pub struct UnaryOpInvert;
impl From<UnaryOpInvert> for UnaryOp {
    fn from(_: UnaryOpInvert) -> Self {
        UnaryOp::Invert
    }
}

impl std::cmp::PartialEq<UnaryOp> for UnaryOpInvert {
    #[inline]
    fn eq(&self, other: &UnaryOp) -> bool {
        matches!(other, UnaryOp::Invert)
    }
}

pub struct UnaryOpNot;
impl From<UnaryOpNot> for UnaryOp {
    fn from(_: UnaryOpNot) -> Self {
        UnaryOp::Not
    }
}

impl std::cmp::PartialEq<UnaryOp> for UnaryOpNot {
    #[inline]
    fn eq(&self, other: &UnaryOp) -> bool {
        matches!(other, UnaryOp::Not)
    }
}

pub struct UnaryOpUAdd;
impl From<UnaryOpUAdd> for UnaryOp {
    fn from(_: UnaryOpUAdd) -> Self {
        UnaryOp::UAdd
    }
}

impl std::cmp::PartialEq<UnaryOp> for UnaryOpUAdd {
    #[inline]
    fn eq(&self, other: &UnaryOp) -> bool {
        matches!(other, UnaryOp::UAdd)
    }
}

pub struct UnaryOpUSub;
impl From<UnaryOpUSub> for UnaryOp {
    fn from(_: UnaryOpUSub) -> Self {
        UnaryOp::USub
    }
}

impl std::cmp::PartialEq<UnaryOp> for UnaryOpUSub {
    #[inline]
    fn eq(&self, other: &UnaryOp) -> bool {
        matches!(other, UnaryOp::USub)
    }
}

/// See also [cmpop](https://docs.python.org/3/library/ast.html#ast.cmpop)
#[derive(Clone, Debug, PartialEq, is_macro::Is, Copy, Hash, Eq)]
pub enum CmpOp {
    Eq,
    NotEq,
    Lt,
    LtE,
    Gt,
    GtE,
    Is,
    IsNot,
    In,
    NotIn,
}
impl CmpOp {
    #[inline]
    pub const fn cmp_op_eq(&self) -> Option<CmpOpEq> {
        match self {
            CmpOp::Eq => Some(CmpOpEq),
            _ => None,
        }
    }

    #[inline]
    pub const fn cmp_op_not_eq(&self) -> Option<CmpOpNotEq> {
        match self {
            CmpOp::NotEq => Some(CmpOpNotEq),
            _ => None,
        }
    }

    #[inline]
    pub const fn cmp_op_lt(&self) -> Option<CmpOpLt> {
        match self {
            CmpOp::Lt => Some(CmpOpLt),
            _ => None,
        }
    }

    #[inline]
    pub const fn cmp_op_lt_e(&self) -> Option<CmpOpLtE> {
        match self {
            CmpOp::LtE => Some(CmpOpLtE),
            _ => None,
        }
    }

    #[inline]
    pub const fn cmp_op_gt(&self) -> Option<CmpOpGt> {
        match self {
            CmpOp::Gt => Some(CmpOpGt),
            _ => None,
        }
    }

    #[inline]
    pub const fn cmp_op_gt_e(&self) -> Option<CmpOpGtE> {
        match self {
            CmpOp::GtE => Some(CmpOpGtE),
            _ => None,
        }
    }

    #[inline]
    pub const fn cmp_op_is(&self) -> Option<CmpOpIs> {
        match self {
            CmpOp::Is => Some(CmpOpIs),
            _ => None,
        }
    }

    #[inline]
    pub const fn cmp_op_is_not(&self) -> Option<CmpOpIsNot> {
        match self {
            CmpOp::IsNot => Some(CmpOpIsNot),
            _ => None,
        }
    }

    #[inline]
    pub const fn cmp_op_in(&self) -> Option<CmpOpIn> {
        match self {
            CmpOp::In => Some(CmpOpIn),
            _ => None,
        }
    }

    #[inline]
    pub const fn cmp_op_not_in(&self) -> Option<CmpOpNotIn> {
        match self {
            CmpOp::NotIn => Some(CmpOpNotIn),
            _ => None,
        }
    }
}

pub struct CmpOpEq;
impl From<CmpOpEq> for CmpOp {
    fn from(_: CmpOpEq) -> Self {
        CmpOp::Eq
    }
}

impl std::cmp::PartialEq<CmpOp> for CmpOpEq {
    #[inline]
    fn eq(&self, other: &CmpOp) -> bool {
        matches!(other, CmpOp::Eq)
    }
}

pub struct CmpOpNotEq;
impl From<CmpOpNotEq> for CmpOp {
    fn from(_: CmpOpNotEq) -> Self {
        CmpOp::NotEq
    }
}

impl std::cmp::PartialEq<CmpOp> for CmpOpNotEq {
    #[inline]
    fn eq(&self, other: &CmpOp) -> bool {
        matches!(other, CmpOp::NotEq)
    }
}

pub struct CmpOpLt;
impl From<CmpOpLt> for CmpOp {
    fn from(_: CmpOpLt) -> Self {
        CmpOp::Lt
    }
}

impl std::cmp::PartialEq<CmpOp> for CmpOpLt {
    #[inline]
    fn eq(&self, other: &CmpOp) -> bool {
        matches!(other, CmpOp::Lt)
    }
}

pub struct CmpOpLtE;
impl From<CmpOpLtE> for CmpOp {
    fn from(_: CmpOpLtE) -> Self {
        CmpOp::LtE
    }
}

impl std::cmp::PartialEq<CmpOp> for CmpOpLtE {
    #[inline]
    fn eq(&self, other: &CmpOp) -> bool {
        matches!(other, CmpOp::LtE)
    }
}

pub struct CmpOpGt;
impl From<CmpOpGt> for CmpOp {
    fn from(_: CmpOpGt) -> Self {
        CmpOp::Gt
    }
}

impl std::cmp::PartialEq<CmpOp> for CmpOpGt {
    #[inline]
    fn eq(&self, other: &CmpOp) -> bool {
        matches!(other, CmpOp::Gt)
    }
}

pub struct CmpOpGtE;
impl From<CmpOpGtE> for CmpOp {
    fn from(_: CmpOpGtE) -> Self {
        CmpOp::GtE
    }
}

impl std::cmp::PartialEq<CmpOp> for CmpOpGtE {
    #[inline]
    fn eq(&self, other: &CmpOp) -> bool {
        matches!(other, CmpOp::GtE)
    }
}

pub struct CmpOpIs;
impl From<CmpOpIs> for CmpOp {
    fn from(_: CmpOpIs) -> Self {
        CmpOp::Is
    }
}

impl std::cmp::PartialEq<CmpOp> for CmpOpIs {
    #[inline]
    fn eq(&self, other: &CmpOp) -> bool {
        matches!(other, CmpOp::Is)
    }
}

pub struct CmpOpIsNot;
impl From<CmpOpIsNot> for CmpOp {
    fn from(_: CmpOpIsNot) -> Self {
        CmpOp::IsNot
    }
}

impl std::cmp::PartialEq<CmpOp> for CmpOpIsNot {
    #[inline]
    fn eq(&self, other: &CmpOp) -> bool {
        matches!(other, CmpOp::IsNot)
    }
}

pub struct CmpOpIn;
impl From<CmpOpIn> for CmpOp {
    fn from(_: CmpOpIn) -> Self {
        CmpOp::In
    }
}

impl std::cmp::PartialEq<CmpOp> for CmpOpIn {
    #[inline]
    fn eq(&self, other: &CmpOp) -> bool {
        matches!(other, CmpOp::In)
    }
}

pub struct CmpOpNotIn;
impl From<CmpOpNotIn> for CmpOp {
    fn from(_: CmpOpNotIn) -> Self {
        CmpOp::NotIn
    }
}

impl std::cmp::PartialEq<CmpOp> for CmpOpNotIn {
    #[inline]
    fn eq(&self, other: &CmpOp) -> bool {
        matches!(other, CmpOp::NotIn)
    }
}

/// See also [comprehension](https://docs.python.org/3/library/ast.html#ast.comprehension)
#[derive(Clone, Debug, PartialEq)]
pub struct Comprehension {
    pub range: TextRange,
    pub target: Expr,
    pub iter: Expr,
    pub ifs: Vec<Expr>,
    pub is_async: bool,
}

/// See also [excepthandler](https://docs.python.org/3/library/ast.html#ast.excepthandler)
#[derive(Clone, Debug, PartialEq, is_macro::Is)]
pub enum ExceptHandler {
    ExceptHandler(ExceptHandlerExceptHandler),
}

/// See also [ExceptHandler](https://docs.python.org/3/library/ast.html#ast.ExceptHandler)
#[derive(Clone, Debug, PartialEq)]
pub struct ExceptHandlerExceptHandler {
    pub range: TextRange,
    pub type_: Option<Box<Expr>>,
    pub name: Option<Identifier>,
    pub body: Vec<Stmt>,
}

impl From<ExceptHandlerExceptHandler> for ExceptHandler {
    fn from(payload: ExceptHandlerExceptHandler) -> Self {
        ExceptHandler::ExceptHandler(payload)
    }
}

/// See also [arguments](https://docs.python.org/3/library/ast.html#ast.arguments)
#[derive(Clone, Debug, PartialEq)]
pub struct PythonArguments {
    pub range: TextRange,
    pub posonlyargs: Vec<Arg>,
    pub args: Vec<Arg>,
    pub vararg: Option<Box<Arg>>,
    pub kwonlyargs: Vec<Arg>,
    pub kw_defaults: Vec<Expr>,
    pub kwarg: Option<Box<Arg>>,
    pub defaults: Vec<Expr>,
}

/// See also [arg](https://docs.python.org/3/library/ast.html#ast.arg)
#[derive(Clone, Debug, PartialEq)]
pub struct Arg {
    pub range: TextRange,
    pub arg: Identifier,
    pub annotation: Option<Box<Expr>>,
}

/// See also [keyword](https://docs.python.org/3/library/ast.html#ast.keyword)
#[derive(Clone, Debug, PartialEq)]
pub struct Keyword {
    pub range: TextRange,
    pub arg: Option<Identifier>,
    pub value: Expr,
}

/// See also [alias](https://docs.python.org/3/library/ast.html#ast.alias)
#[derive(Clone, Debug, PartialEq)]
pub struct Alias {
    pub range: TextRange,
    pub name: Identifier,
    pub asname: Option<Identifier>,
}

/// See also [withitem](https://docs.python.org/3/library/ast.html#ast.withitem)
#[derive(Clone, Debug, PartialEq)]
pub struct WithItem {
    pub range: TextRange,
    pub context_expr: Expr,
    pub optional_vars: Option<Box<Expr>>,
}

/// See also [match_case](https://docs.python.org/3/library/ast.html#ast.match_case)
#[derive(Clone, Debug, PartialEq)]
pub struct MatchCase {
    pub range: TextRange,
    pub pattern: Pattern,
    pub guard: Option<Box<Expr>>,
    pub body: Vec<Stmt>,
}

/// See also [pattern](https://docs.python.org/3/library/ast.html#ast.pattern)
#[derive(Clone, Debug, PartialEq, is_macro::Is)]
pub enum Pattern {
    MatchValue(PatternMatchValue),
    MatchSingleton(PatternMatchSingleton),
    MatchSequence(PatternMatchSequence),
    MatchMapping(PatternMatchMapping),
    MatchClass(PatternMatchClass),
    MatchStar(PatternMatchStar),
    MatchAs(PatternMatchAs),
    MatchOr(PatternMatchOr),
}

/// See also [MatchValue](https://docs.python.org/3/library/ast.html#ast.MatchValue)
#[derive(Clone, Debug, PartialEq)]
pub struct PatternMatchValue {
    pub range: TextRange,
    pub value: Box<Expr>,
}

impl From<PatternMatchValue> for Pattern {
    fn from(payload: PatternMatchValue) -> Self {
        Pattern::MatchValue(payload)
    }
}

/// See also [MatchSingleton](https://docs.python.org/3/library/ast.html#ast.MatchSingleton)
#[derive(Clone, Debug, PartialEq)]
pub struct PatternMatchSingleton {
    pub range: TextRange,
    pub value: Constant,
}

impl From<PatternMatchSingleton> for Pattern {
    fn from(payload: PatternMatchSingleton) -> Self {
        Pattern::MatchSingleton(payload)
    }
}

/// See also [MatchSequence](https://docs.python.org/3/library/ast.html#ast.MatchSequence)
#[derive(Clone, Debug, PartialEq)]
pub struct PatternMatchSequence {
    pub range: TextRange,
    pub patterns: Vec<Pattern>,
}

impl From<PatternMatchSequence> for Pattern {
    fn from(payload: PatternMatchSequence) -> Self {
        Pattern::MatchSequence(payload)
    }
}

/// See also [MatchMapping](https://docs.python.org/3/library/ast.html#ast.MatchMapping)
#[derive(Clone, Debug, PartialEq)]
pub struct PatternMatchMapping {
    pub range: TextRange,
    pub keys: Vec<Expr>,
    pub patterns: Vec<Pattern>,
    pub rest: Option<Identifier>,
}

impl From<PatternMatchMapping> for Pattern {
    fn from(payload: PatternMatchMapping) -> Self {
        Pattern::MatchMapping(payload)
    }
}

/// See also [MatchClass](https://docs.python.org/3/library/ast.html#ast.MatchClass)
#[derive(Clone, Debug, PartialEq)]
pub struct PatternMatchClass {
    pub range: TextRange,
    pub cls: Box<Expr>,
    pub patterns: Vec<Pattern>,
    pub kwd_attrs: Vec<Identifier>,
    pub kwd_patterns: Vec<Pattern>,
}

impl From<PatternMatchClass> for Pattern {
    fn from(payload: PatternMatchClass) -> Self {
        Pattern::MatchClass(payload)
    }
}

/// See also [MatchStar](https://docs.python.org/3/library/ast.html#ast.MatchStar)
#[derive(Clone, Debug, PartialEq)]
pub struct PatternMatchStar {
    pub range: TextRange,
    pub name: Option<Identifier>,
}

impl From<PatternMatchStar> for Pattern {
    fn from(payload: PatternMatchStar) -> Self {
        Pattern::MatchStar(payload)
    }
}

/// See also [MatchAs](https://docs.python.org/3/library/ast.html#ast.MatchAs)
#[derive(Clone, Debug, PartialEq)]
pub struct PatternMatchAs {
    pub range: TextRange,
    pub pattern: Option<Box<Pattern>>,
    pub name: Option<Identifier>,
}

impl From<PatternMatchAs> for Pattern {
    fn from(payload: PatternMatchAs) -> Self {
        Pattern::MatchAs(payload)
    }
}

/// See also [MatchOr](https://docs.python.org/3/library/ast.html#ast.MatchOr)
#[derive(Clone, Debug, PartialEq)]
pub struct PatternMatchOr {
    pub range: TextRange,
    pub patterns: Vec<Pattern>,
}

impl From<PatternMatchOr> for Pattern {
    fn from(payload: PatternMatchOr) -> Self {
        Pattern::MatchOr(payload)
    }
}

/// See also [type_param](https://docs.python.org/3/library/ast.html#ast.type_param)
#[derive(Clone, Debug, PartialEq, is_macro::Is)]
pub enum TypeParam {
    TypeVar(TypeParamTypeVar),
    ParamSpec(TypeParamParamSpec),
    TypeVarTuple(TypeParamTypeVarTuple),
}

/// See also [TypeVar](https://docs.python.org/3/library/ast.html#ast.TypeVar)
#[derive(Clone, Debug, PartialEq)]
pub struct TypeParamTypeVar {
    pub range: TextRange,
    pub name: Identifier,
    pub bound: Option<Box<Expr>>,
}

impl From<TypeParamTypeVar> for TypeParam {
    fn from(payload: TypeParamTypeVar) -> Self {
        TypeParam::TypeVar(payload)
    }
}

/// See also [ParamSpec](https://docs.python.org/3/library/ast.html#ast.ParamSpec)
#[derive(Clone, Debug, PartialEq)]
pub struct TypeParamParamSpec {
    pub range: TextRange,
    pub name: Identifier,
}

impl From<TypeParamParamSpec> for TypeParam {
    fn from(payload: TypeParamParamSpec) -> Self {
        TypeParam::ParamSpec(payload)
    }
}

/// See also [TypeVarTuple](https://docs.python.org/3/library/ast.html#ast.TypeVarTuple)
#[derive(Clone, Debug, PartialEq)]
pub struct TypeParamTypeVarTuple {
    pub range: TextRange,
    pub name: Identifier,
}

impl From<TypeParamTypeVarTuple> for TypeParam {
    fn from(payload: TypeParamTypeVarTuple) -> Self {
        TypeParam::TypeVarTuple(payload)
    }
}

/// See also [decorator](https://docs.python.org/3/library/ast.html#ast.decorator)
#[derive(Clone, Debug, PartialEq)]
pub struct Decorator {
    pub range: TextRange,
    pub expression: Expr,
}

/// An alternative type of AST `arguments`. This is ruff_python_parser-friendly and human-friendly definition of function arguments.
/// This form also has advantage to implement pre-order traverse.
/// `defaults` and `kw_defaults` fields are removed and the default values are placed under each `arg_with_default` typed argument.
/// `vararg` and `kwarg` are still typed as `arg` because they never can have a default value.
///
/// The matching Python style AST type is [`PythonArguments`]. While [`PythonArguments`] has ordered `kwonlyargs` fields by
/// default existence, [Arguments] has location-ordered kwonlyargs fields.
///
/// NOTE: This type is different from original Python AST.

#[derive(Clone, Debug, PartialEq)]
pub struct Arguments {
    pub range: TextRange,
    pub posonlyargs: Vec<ArgWithDefault>,
    pub args: Vec<ArgWithDefault>,
    pub vararg: Option<Box<Arg>>,
    pub kwonlyargs: Vec<ArgWithDefault>,
    pub kwarg: Option<Box<Arg>>,
}

/// An alternative type of AST `arg`. This is used for each function argument that might have a default value.
/// Used by `Arguments` original type.
///
/// NOTE: This type is different from original Python AST.

#[derive(Clone, Debug, PartialEq)]
pub struct ArgWithDefault {
    pub range: TextRange,
    pub def: Arg,
    pub default: Option<Box<Expr>>,
}

pub type Suite = Vec<Stmt>;

impl CmpOp {
    pub fn as_str(&self) -> &'static str {
        match self {
            CmpOp::Eq => "==",
            CmpOp::NotEq => "!=",
            CmpOp::Lt => "<",
            CmpOp::LtE => "<=",
            CmpOp::Gt => ">",
            CmpOp::GtE => ">=",
            CmpOp::Is => "is",
            CmpOp::IsNot => "is not",
            CmpOp::In => "in",
            CmpOp::NotIn => "not in",
        }
    }
}

impl Arguments {
    pub fn empty(range: TextRange) -> Self {
        Self {
            range,
            posonlyargs: Vec::new(),
            args: Vec::new(),
            vararg: None,
            kwonlyargs: Vec::new(),
            kwarg: None,
        }
    }
}

#[allow(clippy::borrowed_box)] // local utility
fn clone_boxed_expr(expr: &Box<Expr>) -> Box<Expr> {
    let expr: &Expr = expr.as_ref();
    Box::new(expr.clone())
}

impl ArgWithDefault {
    pub fn as_arg(&self) -> &Arg {
        &self.def
    }

    pub fn to_arg(&self) -> (Arg, Option<Box<Expr>>) {
        let ArgWithDefault {
            range: _,
            def,
            default,
        } = self;
        (def.clone(), default.as_ref().map(clone_boxed_expr))
    }
    pub fn into_arg(self) -> (Arg, Option<Box<Expr>>) {
        let ArgWithDefault {
            range: _,
            def,
            default,
        } = self;
        (def, default)
    }
}

impl Arguments {
    pub fn defaults(&self) -> impl std::iter::Iterator<Item = &Expr> {
        self.posonlyargs
            .iter()
            .chain(self.args.iter())
            .filter_map(|arg| arg.default.as_ref().map(std::convert::AsRef::as_ref))
    }

    #[allow(clippy::type_complexity)]
    pub fn split_kwonlyargs(&self) -> (Vec<&Arg>, Vec<(&Arg, &Expr)>) {
        let mut args = Vec::new();
        let mut with_defaults = Vec::new();
        for arg in &self.kwonlyargs {
            if let Some(ref default) = arg.default {
                with_defaults.push((arg.as_arg(), &**default));
            } else {
                args.push(arg.as_arg());
            }
        }
        (args, with_defaults)
    }
}

/// The kind of magic command as defined in [IPython Syntax] in the IPython codebase.
///
/// [IPython Syntax]: https://github.com/ipython/ipython/blob/635815e8f1ded5b764d66cacc80bbe25e9e2587f/IPython/core/inputtransformer2.py#L335-L343
#[derive(PartialEq, Eq, Debug, Clone, Hash, Copy)]
pub enum MagicKind {
    /// Send line to underlying system shell.
    Shell,
    /// Send line to system shell and capture output.
    ShCap,
    /// Show help on object.
    Help,
    /// Show help on object, with extra verbosity.
    Help2,
    /// Call magic function.
    Magic,
    /// Call cell magic function.
    Magic2,
    /// Call first argument with rest of line as arguments after splitting on whitespace
    /// and quote each as string.
    Quote,
    /// Call first argument with rest of line as an argument quoted as a single string.
    Quote2,
    /// Call first argument with rest of line as arguments.
    Paren,
}

impl TryFrom<char> for MagicKind {
    type Error = String;

    fn try_from(ch: char) -> Result<Self, Self::Error> {
        match ch {
            '!' => Ok(MagicKind::Shell),
            '?' => Ok(MagicKind::Help),
            '%' => Ok(MagicKind::Magic),
            ',' => Ok(MagicKind::Quote),
            ';' => Ok(MagicKind::Quote2),
            '/' => Ok(MagicKind::Paren),
            _ => Err(format!("Unexpected magic escape: {ch}")),
        }
    }
}

impl TryFrom<[char; 2]> for MagicKind {
    type Error = String;

    fn try_from(ch: [char; 2]) -> Result<Self, Self::Error> {
        match ch {
            ['!', '!'] => Ok(MagicKind::ShCap),
            ['?', '?'] => Ok(MagicKind::Help2),
            ['%', '%'] => Ok(MagicKind::Magic2),
            [c1, c2] => Err(format!("Unexpected magic escape: {c1}{c2}")),
        }
    }
}

impl fmt::Display for MagicKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MagicKind::Shell => f.write_str("!"),
            MagicKind::ShCap => f.write_str("!!"),
            MagicKind::Help => f.write_str("?"),
            MagicKind::Help2 => f.write_str("??"),
            MagicKind::Magic => f.write_str("%"),
            MagicKind::Magic2 => f.write_str("%%"),
            MagicKind::Quote => f.write_str(","),
            MagicKind::Quote2 => f.write_str(";"),
            MagicKind::Paren => f.write_str("/"),
        }
    }
}

impl MagicKind {
    /// Returns the length of the magic command prefix.
    pub fn prefix_len(self) -> TextSize {
        let len = match self {
            MagicKind::Shell
            | MagicKind::Magic
            | MagicKind::Help
            | MagicKind::Quote
            | MagicKind::Quote2
            | MagicKind::Paren => 1,
            MagicKind::ShCap | MagicKind::Magic2 | MagicKind::Help2 => 2,
        };
        len.into()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Identifier {
    id: String,
    range: TextRange,
}

impl Identifier {
    #[inline]
    pub fn new(id: impl Into<String>, range: TextRange) -> Self {
        Self {
            id: id.into(),
            range,
        }
    }
}

impl Identifier {
    #[inline]
    pub fn as_str(&self) -> &str {
        self.id.as_str()
    }
}

impl PartialEq<str> for Identifier {
    #[inline]
    fn eq(&self, other: &str) -> bool {
        self.id == other
    }
}

impl PartialEq<String> for Identifier {
    #[inline]
    fn eq(&self, other: &String) -> bool {
        &self.id == other
    }
}

impl std::ops::Deref for Identifier {
    type Target = str;
    #[inline]
    fn deref(&self) -> &Self::Target {
        self.id.as_str()
    }
}

impl AsRef<str> for Identifier {
    #[inline]
    fn as_ref(&self) -> &str {
        self.id.as_str()
    }
}

impl AsRef<String> for Identifier {
    #[inline]
    fn as_ref(&self) -> &String {
        &self.id
    }
}

impl std::fmt::Display for Identifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.id, f)
    }
}

impl From<Identifier> for String {
    #[inline]
    fn from(identifier: Identifier) -> String {
        identifier.id
    }
}

impl Ranged for Identifier {
    fn range(&self) -> TextRange {
        self.range
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Int(u32);

impl Int {
    pub fn new(i: u32) -> Self {
        Self(i)
    }
    pub fn to_u32(&self) -> u32 {
        self.0
    }
    pub fn to_usize(&self) -> usize {
        self.0 as _
    }
}

impl std::cmp::PartialEq<u32> for Int {
    #[inline]
    fn eq(&self, other: &u32) -> bool {
        self.0 == *other
    }
}

impl std::cmp::PartialEq<usize> for Int {
    #[inline]
    fn eq(&self, other: &usize) -> bool {
        self.0 as usize == *other
    }
}

#[derive(Clone, Debug, PartialEq, is_macro::Is)]
pub enum Constant {
    None,
    Bool(bool),
    Str(String),
    Bytes(Vec<u8>),
    Int(BigInt),
    Float(f64),
    Complex { real: f64, imag: f64 },
    Ellipsis,
}

impl Constant {
    pub fn is_true(self) -> bool {
        self.bool().map_or(false, |b| b)
    }
    pub fn is_false(self) -> bool {
        self.bool().map_or(false, |b| !b)
    }
    pub fn complex(self) -> Option<(f64, f64)> {
        match self {
            Constant::Complex { real, imag } => Some((real, imag)),
            _ => None,
        }
    }
}

impl From<String> for Constant {
    fn from(s: String) -> Constant {
        Self::Str(s)
    }
}
impl From<Vec<u8>> for Constant {
    fn from(b: Vec<u8>) -> Constant {
        Self::Bytes(b)
    }
}
impl From<bool> for Constant {
    fn from(b: bool) -> Constant {
        Self::Bool(b)
    }
}
impl From<BigInt> for Constant {
    fn from(i: BigInt) -> Constant {
        Self::Int(i)
    }
}

#[cfg(feature = "rustpython-literal")]
impl std::fmt::Display for Constant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Constant::None => f.pad("None"),
            Constant::Bool(b) => f.pad(if *b { "True" } else { "False" }),
            Constant::Str(s) => rustpython_literal::escape::UnicodeEscape::new_repr(s.as_str())
                .str_repr()
                .write(f),
            Constant::Bytes(b) => {
                let escape = rustpython_literal::escape::AsciiEscape::new_repr(b);
                let repr = escape.bytes_repr().to_string().unwrap();
                f.pad(&repr)
            }
            Constant::Int(i) => std::fmt::Display::fmt(&i, f),
            Constant::Float(fp) => f.pad(&rustpython_literal::float::to_string(*fp)),
            Constant::Complex { real, imag } => {
                if *real == 0.0 {
                    write!(f, "{imag}j")
                } else {
                    write!(f, "({real}{imag:+}j)")
                }
            }
            Constant::Ellipsis => f.pad("..."),
        }
    }
}

impl Ranged for crate::nodes::ModModule {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::ModExpression {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::Mod {
    fn range(&self) -> TextRange {
        match self {
            Self::Module(node) => node.range(),
            Self::Expression(node) => node.range(),
        }
    }
}

impl Ranged for crate::nodes::StmtFunctionDef {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::StmtAsyncFunctionDef {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::StmtClassDef {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::StmtReturn {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::StmtDelete {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::StmtTypeAlias {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::StmtAssign {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::StmtAugAssign {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::StmtAnnAssign {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::StmtFor {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::StmtAsyncFor {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::StmtWhile {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::StmtIf {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::ElifElseClause {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::StmtWith {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::StmtAsyncWith {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::StmtMatch {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::StmtRaise {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::StmtTry {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::StmtTryStar {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::StmtAssert {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::StmtImport {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::StmtImportFrom {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::StmtGlobal {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::StmtNonlocal {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::StmtExpr {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::StmtPass {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::StmtBreak {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::StmtContinue {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for StmtLineMagic {
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
            Stmt::LineMagic(node) => node.range(),
        }
    }
}

impl Ranged for crate::nodes::ExprBoolOp {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::ExprNamedExpr {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::ExprBinOp {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::ExprUnaryOp {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::ExprLambda {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::ExprIfExp {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::ExprDict {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::ExprSet {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::ExprListComp {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::ExprSetComp {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::ExprDictComp {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::ExprGeneratorExp {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::ExprAwait {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::ExprYield {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::ExprYieldFrom {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::ExprCompare {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::ExprCall {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::ExprFormattedValue {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::ExprJoinedStr {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::ExprConstant {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::ExprAttribute {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::ExprSubscript {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::ExprStarred {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::ExprName {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::ExprList {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::ExprTuple {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::ExprSlice {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for ExprLineMagic {
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
            Expr::LineMagic(node) => node.range(),
        }
    }
}

impl Ranged for crate::nodes::Comprehension {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::ExceptHandlerExceptHandler {
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

impl Ranged for crate::nodes::PythonArguments {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::Arg {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::Keyword {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::Alias {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::WithItem {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::MatchCase {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::PatternMatchValue {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::PatternMatchSingleton {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::PatternMatchSequence {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::PatternMatchMapping {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::PatternMatchClass {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::PatternMatchStar {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::PatternMatchAs {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::PatternMatchOr {
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

impl Ranged for crate::nodes::TypeParamTypeVar {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::TypeParamTypeVarTuple {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::TypeParamParamSpec {
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
impl Ranged for crate::nodes::Decorator {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::Arguments {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::ArgWithDefault {
    fn range(&self) -> TextRange {
        self.range
    }
}

#[cfg(target_pointer_width = "64")]
mod size_assertions {
    #[allow(clippy::wildcard_imports)]
    use super::*;
    use static_assertions::assert_eq_size;

    assert_eq_size!(Stmt, [u8; 168]);
    assert_eq_size!(StmtFunctionDef, [u8; 128]);
    assert_eq_size!(StmtClassDef, [u8; 160]);
    assert_eq_size!(StmtTry, [u8; 104]);
    assert_eq_size!(Expr, [u8; 80]);
    assert_eq_size!(Constant, [u8; 32]);
    assert_eq_size!(Pattern, [u8; 96]);
    assert_eq_size!(Mod, [u8; 32]);
}
