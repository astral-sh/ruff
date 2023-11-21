#![allow(clippy::derive_partial_eq_without_eq)]

use std::cell::OnceCell;
use std::fmt;
use std::fmt::Debug;
use std::ops::Deref;

use itertools::Either::{Left, Right};
use itertools::Itertools;

use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::{int, LiteralExpressionRef};

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
    #[is(name = "while_stmt")]
    While(StmtWhile),
    #[is(name = "if_stmt")]
    If(StmtIf),
    #[is(name = "with_stmt")]
    With(StmtWith),
    #[is(name = "match_stmt")]
    Match(StmtMatch),
    #[is(name = "raise_stmt")]
    Raise(StmtRaise),
    #[is(name = "try_stmt")]
    Try(StmtTry),
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
    #[is(name = "ipy_escape_command_stmt")]
    IpyEscapeCommand(StmtIpyEscapeCommand),
}

/// An AST node used to represent a IPython escape command at the statement level.
///
/// For example,
/// ```python
/// %matplotlib inline
/// ```
///
/// ## Terminology
///
/// Escape commands are special IPython syntax which starts with a token to identify
/// the escape kind followed by the command value itself. [Escape kind] are the kind
/// of escape commands that are recognized by the token: `%`, `%%`, `!`, `!!`,
/// `?`, `??`, `/`, `;`, and `,`.
///
/// Help command (or Dynamic Object Introspection as it's called) are the escape commands
/// of the kind `?` and `??`. For example, `?str.replace`. Help end command are a subset
/// of Help command where the token can be at the end of the line i.e., after the value.
/// For example, `str.replace?`.
///
/// Here's where things get tricky. I'll divide the help end command into two types for
/// better understanding:
/// 1. Strict version: The token is _only_ at the end of the line. For example,
///    `str.replace?` or `str.replace??`.
/// 2. Combined version: Along with the `?` or `??` token, which are at the end of the
///    line, there are other escape kind tokens that are present at the start as well.
///    For example, `%matplotlib?` or `%%timeit?`.
///
/// Priority comes into picture for the "Combined version" mentioned above. How do
/// we determine the escape kind if there are tokens on both side of the value, i.e., which
/// token to choose? The Help end command always takes priority over any other token which
/// means that if there is `?`/`??` at the end then that is used to determine the kind.
/// For example, in `%matplotlib?` the escape kind is determined using the `?` token
/// instead of `%` token.
///
/// ## Syntax
///
/// `<IpyEscapeKind><Command value>`
///
/// The simplest form is an escape kind token followed by the command value. For example,
/// `%matplotlib inline`, `/foo`, `!pwd`, etc.
///
/// `<Command value><IpyEscapeKind ("?" or "??")>`
///
/// The help end escape command would be the reverse of the above syntax. Here, the
/// escape kind token can only be either `?` or `??` and it is at the end of the line.
/// For example, `str.replace?`, `math.pi??`, etc.
///
/// `<IpyEscapeKind><Command value><EscapeKind ("?" or "??")>`
///
/// The final syntax is the combined version of the above two. For example, `%matplotlib?`,
/// `%%timeit??`, etc.
///
/// [Escape kind]: IpyEscapeKind
#[derive(Clone, Debug, PartialEq)]
pub struct StmtIpyEscapeCommand {
    pub range: TextRange,
    pub kind: IpyEscapeKind,
    pub value: String,
}

impl From<StmtIpyEscapeCommand> for Stmt {
    fn from(payload: StmtIpyEscapeCommand) -> Self {
        Stmt::IpyEscapeCommand(payload)
    }
}

/// See also [FunctionDef](https://docs.python.org/3/library/ast.html#ast.FunctionDef) and
/// [AsyncFunctionDef](https://docs.python.org/3/library/ast.html#ast.AsyncFunctionDef).
///
/// This type differs from the original Python AST, as it collapses the
/// synchronous and asynchronous variants into a single type.
#[derive(Clone, Debug, PartialEq)]
pub struct StmtFunctionDef {
    pub range: TextRange,
    pub is_async: bool,
    pub decorator_list: Vec<Decorator>,
    pub name: Identifier,
    pub type_params: Option<TypeParams>,
    pub parameters: Box<Parameters>,
    pub returns: Option<Box<Expr>>,
    pub body: Vec<Stmt>,
}

impl From<StmtFunctionDef> for Stmt {
    fn from(payload: StmtFunctionDef) -> Self {
        Stmt::FunctionDef(payload)
    }
}

/// See also [ClassDef](https://docs.python.org/3/library/ast.html#ast.ClassDef)
#[derive(Clone, Debug, PartialEq)]
pub struct StmtClassDef {
    pub range: TextRange,
    pub decorator_list: Vec<Decorator>,
    pub name: Identifier,
    pub type_params: Option<Box<TypeParams>>,
    pub arguments: Option<Box<Arguments>>,
    pub body: Vec<Stmt>,
}

impl StmtClassDef {
    /// Return an iterator over the bases of the class.
    pub fn bases(&self) -> &[Expr] {
        match &self.arguments {
            Some(arguments) => &arguments.args,
            None => &[],
        }
    }

    /// Return an iterator over the metaclass keywords of the class.
    pub fn keywords(&self) -> &[Keyword] {
        match &self.arguments {
            Some(arguments) => &arguments.keywords,
            None => &[],
        }
    }
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
    pub type_params: Option<TypeParams>,
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

/// See also [For](https://docs.python.org/3/library/ast.html#ast.For) and
/// [AsyncFor](https://docs.python.org/3/library/ast.html#ast.AsyncFor).
///
/// This type differs from the original Python AST, as it collapses the
/// synchronous and asynchronous variants into a single type.
#[derive(Clone, Debug, PartialEq)]
pub struct StmtFor {
    pub range: TextRange,
    pub is_async: bool,
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

/// See also [While](https://docs.python.org/3/library/ast.html#ast.While) and
/// [AsyncWhile](https://docs.python.org/3/library/ast.html#ast.AsyncWhile).
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

/// See also [With](https://docs.python.org/3/library/ast.html#ast.With) and
/// [AsyncWith](https://docs.python.org/3/library/ast.html#ast.AsyncWith).
///
/// This type differs from the original Python AST, as it collapses the
/// synchronous and asynchronous variants into a single type.
#[derive(Clone, Debug, PartialEq)]
pub struct StmtWith {
    pub range: TextRange,
    pub is_async: bool,
    pub items: Vec<WithItem>,
    pub body: Vec<Stmt>,
}

impl From<StmtWith> for Stmt {
    fn from(payload: StmtWith) -> Self {
        Stmt::With(payload)
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

/// See also [Try](https://docs.python.org/3/library/ast.html#ast.Try) and
/// [TryStar](https://docs.python.org/3/library/ast.html#ast.TryStar)
#[derive(Clone, Debug, PartialEq)]
pub struct StmtTry {
    pub range: TextRange,
    pub body: Vec<Stmt>,
    pub handlers: Vec<ExceptHandler>,
    pub orelse: Vec<Stmt>,
    pub finalbody: Vec<Stmt>,
    pub is_star: bool,
}

impl From<StmtTry> for Stmt {
    fn from(payload: StmtTry) -> Self {
        Stmt::Try(payload)
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
    pub level: Option<u32>,
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
    #[is(name = "f_string_expr")]
    FString(ExprFString),
    #[is(name = "string_literal_expr")]
    StringLiteral(ExprStringLiteral),
    #[is(name = "bytes_literal_expr")]
    BytesLiteral(ExprBytesLiteral),
    #[is(name = "number_literal_expr")]
    NumberLiteral(ExprNumberLiteral),
    #[is(name = "boolean_literal_expr")]
    BooleanLiteral(ExprBooleanLiteral),
    #[is(name = "none_literal_expr")]
    NoneLiteral(ExprNoneLiteral),
    #[is(name = "ellipsis_literal_expr")]
    EllipsisLiteral(ExprEllipsisLiteral),
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
    #[is(name = "ipy_escape_command_expr")]
    IpyEscapeCommand(ExprIpyEscapeCommand),
}

impl Expr {
    /// Returns `true` if the expression is a literal expression.
    ///
    /// A literal expression is either a string literal, bytes literal,
    /// integer, float, complex number, boolean, `None`, or ellipsis (`...`).
    pub fn is_literal_expr(&self) -> bool {
        matches!(
            self,
            Expr::StringLiteral(_)
                | Expr::BytesLiteral(_)
                | Expr::NumberLiteral(_)
                | Expr::BooleanLiteral(_)
                | Expr::NoneLiteral(_)
                | Expr::EllipsisLiteral(_)
        )
    }

    /// Returns [`LiteralExpressionRef`] if the expression is a literal expression.
    pub fn as_literal_expr(&self) -> Option<LiteralExpressionRef<'_>> {
        match self {
            Expr::StringLiteral(expr) => Some(LiteralExpressionRef::StringLiteral(expr)),
            Expr::BytesLiteral(expr) => Some(LiteralExpressionRef::BytesLiteral(expr)),
            Expr::NumberLiteral(expr) => Some(LiteralExpressionRef::NumberLiteral(expr)),
            Expr::BooleanLiteral(expr) => Some(LiteralExpressionRef::BooleanLiteral(expr)),
            Expr::NoneLiteral(expr) => Some(LiteralExpressionRef::NoneLiteral(expr)),
            Expr::EllipsisLiteral(expr) => Some(LiteralExpressionRef::EllipsisLiteral(expr)),
            _ => None,
        }
    }
}

/// An AST node used to represent a IPython escape command at the expression level.
///
/// For example,
/// ```python
/// dir = !pwd
/// ```
///
/// Here, the escape kind can only be `!` or `%` otherwise it is a syntax error.
///
/// For more information related to terminology and syntax of escape commands,
/// see [`StmtIpyEscapeCommand`].
#[derive(Clone, Debug, PartialEq)]
pub struct ExprIpyEscapeCommand {
    pub range: TextRange,
    pub kind: IpyEscapeKind,
    pub value: String,
}

impl From<ExprIpyEscapeCommand> for Expr {
    fn from(payload: ExprIpyEscapeCommand) -> Self {
        Expr::IpyEscapeCommand(payload)
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
    pub parameters: Option<Box<Parameters>>,
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
    pub arguments: Arguments,
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

/// An AST node used to represent an f-string.
///
/// This type differs from the original Python AST ([JoinedStr]) in that it
/// doesn't join the implicitly concatenated parts into a single string. Instead,
/// it keeps them separate and provide various methods to access the parts.
///
/// [JoinedStr]: https://docs.python.org/3/library/ast.html#ast.JoinedStr
#[derive(Clone, Debug, PartialEq)]
pub struct ExprFString {
    pub range: TextRange,
    pub value: FStringValue,
}

impl From<ExprFString> for Expr {
    fn from(payload: ExprFString) -> Self {
        Expr::FString(payload)
    }
}

/// The value representing an [`ExprFString`].
#[derive(Clone, Debug, PartialEq)]
pub struct FStringValue {
    inner: FStringValueInner,
}

impl FStringValue {
    /// Creates a new f-string with the given value.
    pub fn single(value: FString) -> Self {
        Self {
            inner: FStringValueInner::Single(FStringPart::FString(value)),
        }
    }

    /// Creates a new f-string with the given values that represents an implicitly
    /// concatenated f-string.
    ///
    /// # Panics
    ///
    /// Panics if `values` is less than 2. Use [`FStringValue::single`] instead.
    pub fn concatenated(values: Vec<FStringPart>) -> Self {
        assert!(values.len() > 1);
        Self {
            inner: FStringValueInner::Concatenated(values),
        }
    }

    /// Returns `true` if the f-string is implicitly concatenated, `false` otherwise.
    pub fn is_implicit_concatenated(&self) -> bool {
        matches!(self.inner, FStringValueInner::Concatenated(_))
    }

    /// Returns an iterator over all the [`FStringPart`]s contained in this value.
    pub fn parts(&self) -> impl Iterator<Item = &FStringPart> {
        match &self.inner {
            FStringValueInner::Single(part) => Left(std::iter::once(part)),
            FStringValueInner::Concatenated(parts) => Right(parts.iter()),
        }
    }

    /// Returns an iterator over all the [`FStringPart`]s contained in this value
    /// that allows modification.
    pub(crate) fn parts_mut(&mut self) -> impl Iterator<Item = &mut FStringPart> {
        match &mut self.inner {
            FStringValueInner::Single(part) => Left(std::iter::once(part)),
            FStringValueInner::Concatenated(parts) => Right(parts.iter_mut()),
        }
    }

    /// Returns an iterator over the [`StringLiteral`] parts contained in this value.
    ///
    /// Note that this doesn't nest into the f-string parts. For example,
    ///
    /// ```python
    /// "foo" f"bar {x}" "baz" f"qux"
    /// ```
    ///
    /// Here, the string literal parts returned would be `"foo"` and `"baz"`.
    pub fn literals(&self) -> impl Iterator<Item = &StringLiteral> {
        self.parts().filter_map(|part| part.as_literal())
    }

    /// Returns an iterator over the [`FString`] parts contained in this value.
    ///
    /// Note that this doesn't nest into the f-string parts. For example,
    ///
    /// ```python
    /// "foo" f"bar {x}" "baz" f"qux"
    /// ```
    ///
    /// Here, the f-string parts returned would be `f"bar {x}"` and `f"qux"`.
    pub fn f_strings(&self) -> impl Iterator<Item = &FString> {
        self.parts().filter_map(|part| part.as_f_string())
    }

    /// Returns an iterator over all the f-string elements contained in this value.
    ///
    /// An f-string element is what makes up an [`FString`] i.e., it is either a
    /// string literal or an expression. In the following example,
    ///
    /// ```python
    /// "foo" f"bar {x}" "baz" f"qux"
    /// ```
    ///
    /// The f-string elements returned would be string literal (`"bar "`),
    /// expression (`x`) and string literal (`"qux"`).
    pub fn elements(&self) -> impl Iterator<Item = &Expr> {
        self.f_strings().flat_map(|fstring| fstring.values.iter())
    }
}

/// An internal representation of [`FStringValue`].
#[derive(Clone, Debug, PartialEq)]
enum FStringValueInner {
    /// A single f-string i.e., `f"foo"`.
    ///
    /// This is always going to be `FStringPart::FString` variant which is
    /// maintained by the `FStringValue::single` constructor.
    Single(FStringPart),

    /// An implicitly concatenated f-string i.e., `"foo" f"bar {x}"`.
    Concatenated(Vec<FStringPart>),
}

/// An f-string part which is either a string literal or an f-string.
#[derive(Clone, Debug, PartialEq, is_macro::Is)]
pub enum FStringPart {
    Literal(StringLiteral),
    FString(FString),
}

impl Ranged for FStringPart {
    fn range(&self) -> TextRange {
        match self {
            FStringPart::Literal(string_literal) => string_literal.range(),
            FStringPart::FString(f_string) => f_string.range(),
        }
    }
}

/// An AST node that represents a single f-string which is part of an [`ExprFString`].
#[derive(Clone, Debug, PartialEq)]
pub struct FString {
    pub range: TextRange,
    pub values: Vec<Expr>,
}

impl Ranged for FString {
    fn range(&self) -> TextRange {
        self.range
    }
}

impl From<FString> for Expr {
    fn from(payload: FString) -> Self {
        ExprFString {
            range: payload.range,
            value: FStringValue::single(payload),
        }
        .into()
    }
}

/// An AST node that represents either a single string literal or an implicitly
/// concatenated string literals.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ExprStringLiteral {
    pub range: TextRange,
    pub value: StringLiteralValue,
}

impl From<ExprStringLiteral> for Expr {
    fn from(payload: ExprStringLiteral) -> Self {
        Expr::StringLiteral(payload)
    }
}

impl Ranged for ExprStringLiteral {
    fn range(&self) -> TextRange {
        self.range
    }
}

/// The value representing a [`ExprStringLiteral`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StringLiteralValue {
    inner: StringLiteralValueInner,
}

impl StringLiteralValue {
    /// Creates a new single string literal with the given value.
    pub fn single(string: StringLiteral) -> Self {
        Self {
            inner: StringLiteralValueInner::Single(string),
        }
    }

    /// Creates a new string literal with the given values that represents an
    /// implicitly concatenated strings.
    ///
    /// # Panics
    ///
    /// Panics if `strings` is less than 2. Use [`StringLiteralValue::single`]
    /// instead.
    pub fn concatenated(strings: Vec<StringLiteral>) -> Self {
        assert!(strings.len() > 1);
        Self {
            inner: StringLiteralValueInner::Concatenated(ConcatenatedStringLiteral {
                strings,
                value: OnceCell::new(),
            }),
        }
    }

    /// Returns `true` if the string literal is implicitly concatenated.
    pub const fn is_implicit_concatenated(&self) -> bool {
        matches!(self.inner, StringLiteralValueInner::Concatenated(_))
    }

    /// Returns `true` if the string literal is a unicode string.
    ///
    /// For an implicitly concatenated string, it returns `true` only if the first
    /// string literal is a unicode string.
    pub fn is_unicode(&self) -> bool {
        self.parts().next().map_or(false, |part| part.unicode)
    }

    /// Returns an iterator over all the [`StringLiteral`] parts contained in this value.
    pub fn parts(&self) -> impl Iterator<Item = &StringLiteral> {
        match &self.inner {
            StringLiteralValueInner::Single(value) => Left(std::iter::once(value)),
            StringLiteralValueInner::Concatenated(value) => Right(value.strings.iter()),
        }
    }

    /// Returns an iterator over all the [`StringLiteral`] parts contained in this value
    /// that allows modification.
    pub(crate) fn parts_mut(&mut self) -> impl Iterator<Item = &mut StringLiteral> {
        match &mut self.inner {
            StringLiteralValueInner::Single(value) => Left(std::iter::once(value)),
            StringLiteralValueInner::Concatenated(value) => Right(value.strings.iter_mut()),
        }
    }

    /// Returns the concatenated string value as a [`str`].
    pub fn as_str(&self) -> &str {
        match &self.inner {
            StringLiteralValueInner::Single(value) => value.as_str(),
            StringLiteralValueInner::Concatenated(value) => value.as_str(),
        }
    }
}

impl PartialEq<str> for StringLiteralValue {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<String> for StringLiteralValue {
    fn eq(&self, other: &String) -> bool {
        self.as_str() == other
    }
}

impl Deref for StringLiteralValue {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

impl fmt::Display for StringLiteralValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// An internal representation of [`StringLiteralValue`].
#[derive(Clone, Debug, PartialEq)]
enum StringLiteralValueInner {
    /// A single string literal i.e., `"foo"`.
    Single(StringLiteral),

    /// An implicitly concatenated string literals i.e., `"foo" "bar"`.
    Concatenated(ConcatenatedStringLiteral),
}

impl Default for StringLiteralValueInner {
    fn default() -> Self {
        Self::Single(StringLiteral::default())
    }
}

/// An AST node that represents a single string literal which is part of an
/// [`ExprStringLiteral`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StringLiteral {
    pub range: TextRange,
    pub value: String,
    pub unicode: bool,
}

impl Ranged for StringLiteral {
    fn range(&self) -> TextRange {
        self.range
    }
}

impl Deref for StringLiteral {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.value.as_str()
    }
}

impl StringLiteral {
    /// Extracts a string slice containing the entire `String`.
    pub fn as_str(&self) -> &str {
        self
    }
}

impl From<StringLiteral> for Expr {
    fn from(payload: StringLiteral) -> Self {
        ExprStringLiteral {
            range: payload.range,
            value: StringLiteralValue::single(payload),
        }
        .into()
    }
}

/// An internal representation of [`StringLiteral`] that represents an
/// implicitly concatenated string.
#[derive(Clone, PartialEq)]
struct ConcatenatedStringLiteral {
    /// Each string literal that makes up the concatenated string.
    strings: Vec<StringLiteral>,
    /// The concatenated string value.
    value: OnceCell<String>,
}

impl ConcatenatedStringLiteral {
    /// Extracts a string slice containing the entire concatenated string.
    fn as_str(&self) -> &str {
        self.value
            .get_or_init(|| self.strings.iter().map(StringLiteral::as_str).collect())
    }
}

impl Debug for ConcatenatedStringLiteral {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ConcatenatedStringLiteral")
            .field("strings", &self.strings)
            .field("value", &self.as_str())
            .finish()
    }
}

/// An AST node that represents either a single bytes literal or an implicitly
/// concatenated bytes literals.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ExprBytesLiteral {
    pub range: TextRange,
    pub value: BytesLiteralValue,
}

impl From<ExprBytesLiteral> for Expr {
    fn from(payload: ExprBytesLiteral) -> Self {
        Expr::BytesLiteral(payload)
    }
}

impl Ranged for ExprBytesLiteral {
    fn range(&self) -> TextRange {
        self.range
    }
}

/// The value representing a [`ExprBytesLiteral`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BytesLiteralValue {
    inner: BytesLiteralValueInner,
}

impl BytesLiteralValue {
    /// Creates a new single bytes literal with the given value.
    pub fn single(value: BytesLiteral) -> Self {
        Self {
            inner: BytesLiteralValueInner::Single(value),
        }
    }

    /// Creates a new bytes literal with the given values that represents an
    /// implicitly concatenated bytes.
    ///
    /// # Panics
    ///
    /// Panics if `values` is less than 2. Use [`BytesLiteralValue::single`]
    /// instead.
    pub fn concatenated(values: Vec<BytesLiteral>) -> Self {
        assert!(values.len() > 1);
        Self {
            inner: BytesLiteralValueInner::Concatenated(values),
        }
    }

    /// Returns `true` if the bytes literal is implicitly concatenated.
    pub const fn is_implicit_concatenated(&self) -> bool {
        matches!(self.inner, BytesLiteralValueInner::Concatenated(_))
    }

    /// Returns an iterator over all the [`BytesLiteral`] parts contained in this value.
    pub fn parts(&self) -> impl Iterator<Item = &BytesLiteral> {
        match &self.inner {
            BytesLiteralValueInner::Single(value) => Left(std::iter::once(value)),
            BytesLiteralValueInner::Concatenated(values) => Right(values.iter()),
        }
    }

    /// Returns an iterator over all the [`BytesLiteral`] parts contained in this value
    /// that allows modification.
    pub(crate) fn parts_mut(&mut self) -> impl Iterator<Item = &mut BytesLiteral> {
        match &mut self.inner {
            BytesLiteralValueInner::Single(value) => Left(std::iter::once(value)),
            BytesLiteralValueInner::Concatenated(values) => Right(values.iter_mut()),
        }
    }

    /// Returns `true` if the concatenated bytes has a length of zero.
    pub fn is_empty(&self) -> bool {
        self.parts().all(|part| part.is_empty())
    }

    /// Returns the length of the concatenated bytes.
    pub fn len(&self) -> usize {
        self.parts().map(|part| part.len()).sum()
    }

    /// Returns an iterator over the bytes of the concatenated bytes.
    fn bytes(&self) -> impl Iterator<Item = u8> + '_ {
        self.parts()
            .flat_map(|part| part.as_slice().iter().copied())
    }
}

impl PartialEq<[u8]> for BytesLiteralValue {
    fn eq(&self, other: &[u8]) -> bool {
        if self.len() != other.len() {
            return false;
        }
        // The `zip` here is safe because we have checked the length of both parts.
        self.bytes()
            .zip(other.iter().copied())
            .all(|(b1, b2)| b1 == b2)
    }
}

/// An internal representation of [`BytesLiteralValue`].
#[derive(Clone, Debug, PartialEq)]
enum BytesLiteralValueInner {
    /// A single bytes literal i.e., `b"foo"`.
    Single(BytesLiteral),

    /// An implicitly concatenated bytes literals i.e., `b"foo" b"bar"`.
    Concatenated(Vec<BytesLiteral>),
}

impl Default for BytesLiteralValueInner {
    fn default() -> Self {
        Self::Single(BytesLiteral::default())
    }
}

/// An AST node that represents a single bytes literal which is part of an
/// [`ExprBytesLiteral`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BytesLiteral {
    pub range: TextRange,
    pub value: Vec<u8>,
}

impl Ranged for BytesLiteral {
    fn range(&self) -> TextRange {
        self.range
    }
}

impl Deref for BytesLiteral {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.value.as_slice()
    }
}

impl BytesLiteral {
    /// Extracts a byte slice containing the entire [`BytesLiteral`].
    pub fn as_slice(&self) -> &[u8] {
        self
    }
}

impl From<BytesLiteral> for Expr {
    fn from(payload: BytesLiteral) -> Self {
        ExprBytesLiteral {
            range: payload.range,
            value: BytesLiteralValue::single(payload),
        }
        .into()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ExprNumberLiteral {
    pub range: TextRange,
    pub value: Number,
}

impl From<ExprNumberLiteral> for Expr {
    fn from(payload: ExprNumberLiteral) -> Self {
        Expr::NumberLiteral(payload)
    }
}

impl Ranged for ExprNumberLiteral {
    fn range(&self) -> TextRange {
        self.range
    }
}

#[derive(Clone, Debug, PartialEq, is_macro::Is)]
pub enum Number {
    Int(int::Int),
    Float(f64),
    Complex { real: f64, imag: f64 },
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ExprBooleanLiteral {
    pub range: TextRange,
    pub value: bool,
}

impl From<ExprBooleanLiteral> for Expr {
    fn from(payload: ExprBooleanLiteral) -> Self {
        Expr::BooleanLiteral(payload)
    }
}

impl Ranged for ExprBooleanLiteral {
    fn range(&self) -> TextRange {
        self.range
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ExprNoneLiteral {
    pub range: TextRange,
}

impl From<ExprNoneLiteral> for Expr {
    fn from(payload: ExprNoneLiteral) -> Self {
        Expr::NoneLiteral(payload)
    }
}

impl Ranged for ExprNoneLiteral {
    fn range(&self) -> TextRange {
        self.range
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ExprEllipsisLiteral {
    pub range: TextRange,
}

impl From<ExprEllipsisLiteral> for Expr {
    fn from(payload: ExprEllipsisLiteral) -> Self {
        Expr::EllipsisLiteral(payload)
    }
}

impl Ranged for ExprEllipsisLiteral {
    fn range(&self) -> TextRange {
        self.range
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

/// See also [arg](https://docs.python.org/3/library/ast.html#ast.arg)
#[derive(Clone, Debug, PartialEq)]
pub struct Parameter {
    pub range: TextRange,
    pub name: Identifier,
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
    pub value: Singleton,
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
    pub arguments: PatternArguments,
}

impl From<PatternMatchClass> for Pattern {
    fn from(payload: PatternMatchClass) -> Self {
        Pattern::MatchClass(payload)
    }
}

/// An AST node to represent the arguments to a [`PatternMatchClass`], i.e., the
/// parenthesized contents in `case Point(1, x=0, y=0)`.
///
/// Like [`Arguments`], but for [`PatternMatchClass`].
#[derive(Clone, Debug, PartialEq)]
pub struct PatternArguments {
    pub range: TextRange,
    pub patterns: Vec<Pattern>,
    pub keywords: Vec<PatternKeyword>,
}

/// An AST node to represent the keyword arguments to a [`PatternMatchClass`], i.e., the
/// `x=0` and `y=0` in `case Point(x=0, y=0)`.
///
/// Like [`Keyword`], but for [`PatternMatchClass`].
#[derive(Clone, Debug, PartialEq)]
pub struct PatternKeyword {
    pub range: TextRange,
    pub attr: Identifier,
    pub pattern: Pattern,
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
///
/// `defaults` and `kw_defaults` fields are removed and the default values are placed under each [`ParameterWithDefault`] typed argument.
/// `vararg` and `kwarg` are still typed as `arg` because they never can have a default value.
///
/// The original Python-style AST type orders `kwonlyargs` fields by default existence; [Parameters] has location-ordered `kwonlyargs` fields.
///
/// NOTE: This type differs from the original Python AST. See: [arguments](https://docs.python.org/3/library/ast.html#ast.arguments).

#[derive(Clone, Debug, PartialEq)]
pub struct Parameters {
    pub range: TextRange,
    pub posonlyargs: Vec<ParameterWithDefault>,
    pub args: Vec<ParameterWithDefault>,
    pub vararg: Option<Box<Parameter>>,
    pub kwonlyargs: Vec<ParameterWithDefault>,
    pub kwarg: Option<Box<Parameter>>,
}

impl Parameters {
    /// Returns the [`ParameterWithDefault`] with the given name, or `None` if no such [`ParameterWithDefault`] exists.
    pub fn find(&self, name: &str) -> Option<&ParameterWithDefault> {
        self.posonlyargs
            .iter()
            .chain(&self.args)
            .chain(&self.kwonlyargs)
            .find(|arg| arg.parameter.name.as_str() == name)
    }

    /// Returns `true` if a parameter with the given name included in this [`Parameters`].
    pub fn includes(&self, name: &str) -> bool {
        if self
            .posonlyargs
            .iter()
            .chain(&self.args)
            .chain(&self.kwonlyargs)
            .any(|arg| arg.parameter.name.as_str() == name)
        {
            return true;
        }
        if let Some(arg) = &self.vararg {
            if arg.name.as_str() == name {
                return true;
            }
        }
        if let Some(arg) = &self.kwarg {
            if arg.name.as_str() == name {
                return true;
            }
        }
        false
    }

    /// Returns `true` if the [`Parameters`] is empty.
    pub fn is_empty(&self) -> bool {
        self.posonlyargs.is_empty()
            && self.args.is_empty()
            && self.kwonlyargs.is_empty()
            && self.vararg.is_none()
            && self.kwarg.is_none()
    }
}

/// An alternative type of AST `arg`. This is used for each function argument that might have a default value.
/// Used by `Arguments` original type.
///
/// NOTE: This type is different from original Python AST.

#[derive(Clone, Debug, PartialEq)]
pub struct ParameterWithDefault {
    pub range: TextRange,
    pub parameter: Parameter,
    pub default: Option<Box<Expr>>,
}

/// An AST node used to represent the arguments passed to a function call or class definition.
///
/// For example, given:
/// ```python
/// foo(1, 2, 3, bar=4, baz=5)
/// ```
/// The `Arguments` node would span from the left to right parentheses (inclusive), and contain
/// the arguments and keyword arguments in the order they appear in the source code.
///
/// Similarly, given:
/// ```python
/// class Foo(Bar, baz=1, qux=2):
///     pass
/// ```
/// The `Arguments` node would again span from the left to right parentheses (inclusive), and
/// contain the `Bar` argument and the `baz` and `qux` keyword arguments in the order they
/// appear in the source code.
///
/// In the context of a class definition, the Python-style AST refers to the arguments as `bases`,
/// as they represent the "explicitly specified base classes", while the keyword arguments are
/// typically used for `metaclass`, with any additional arguments being passed to the `metaclass`.

#[derive(Clone, Debug, PartialEq)]
pub struct Arguments {
    pub range: TextRange,
    pub args: Vec<Expr>,
    pub keywords: Vec<Keyword>,
}

/// An entry in the argument list of a function call.
#[derive(Clone, Debug, PartialEq)]
pub enum ArgOrKeyword<'a> {
    Arg(&'a Expr),
    Keyword(&'a Keyword),
}

impl<'a> From<&'a Expr> for ArgOrKeyword<'a> {
    fn from(arg: &'a Expr) -> Self {
        Self::Arg(arg)
    }
}

impl<'a> From<&'a Keyword> for ArgOrKeyword<'a> {
    fn from(keyword: &'a Keyword) -> Self {
        Self::Keyword(keyword)
    }
}

impl Ranged for ArgOrKeyword<'_> {
    fn range(&self) -> TextRange {
        match self {
            Self::Arg(arg) => arg.range(),
            Self::Keyword(keyword) => keyword.range(),
        }
    }
}

impl Arguments {
    /// Return the number of positional and keyword arguments.
    pub fn len(&self) -> usize {
        self.args.len() + self.keywords.len()
    }

    /// Return `true` if there are no positional or keyword arguments.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Return the [`Keyword`] with the given name, or `None` if no such [`Keyword`] exists.
    pub fn find_keyword(&self, keyword_name: &str) -> Option<&Keyword> {
        self.keywords.iter().find(|keyword| {
            let Keyword { arg, .. } = keyword;
            arg.as_ref().is_some_and(|arg| arg == keyword_name)
        })
    }

    /// Return the positional argument at the given index, or `None` if no such argument exists.
    pub fn find_positional(&self, position: usize) -> Option<&Expr> {
        self.args
            .iter()
            .take_while(|expr| !expr.is_starred_expr())
            .nth(position)
    }

    /// Return the argument with the given name or at the given position, or `None` if no such
    /// argument exists. Used to retrieve arguments that can be provided _either_ as keyword or
    /// positional arguments.
    pub fn find_argument(&self, name: &str, position: usize) -> Option<&Expr> {
        self.find_keyword(name)
            .map(|keyword| &keyword.value)
            .or_else(|| self.find_positional(position))
    }

    /// Return the positional and keyword arguments in the order of declaration.
    ///
    /// Positional arguments are generally before keyword arguments, but star arguments are an
    /// exception:
    /// ```python
    /// class A(*args, a=2, *args2, **kwargs):
    ///     pass
    ///
    /// f(*args, a=2, *args2, **kwargs)
    /// ```
    /// where `*args` and `args2` are `args` while `a=1` and `kwargs` are `keywords`.
    ///
    /// If you would just chain `args` and `keywords` the call would get reordered which we don't
    /// want. This function instead "merge sorts" them into the correct order.
    ///
    /// Note that the order of evaluation is always first `args`, then `keywords`:
    /// ```python
    /// def f(*args, **kwargs):
    ///     pass
    ///
    /// def g(x):
    ///     print(x)
    ///     return x
    ///
    ///
    /// f(*g([1]), a=g(2), *g([3]), **g({"4": 5}))
    /// ```
    /// Output:
    /// ```text
    /// [1]
    /// [3]
    /// 2
    /// {'4': 5}
    /// ```
    pub fn arguments_source_order(&self) -> impl Iterator<Item = ArgOrKeyword<'_>> {
        let args = self.args.iter().map(ArgOrKeyword::Arg);
        let keywords = self.keywords.iter().map(ArgOrKeyword::Keyword);
        args.merge_by(keywords, |left, right| left.start() < right.start())
    }
}

/// An AST node used to represent a sequence of type parameters.
///
/// For example, given:
/// ```python
/// class C[T, U, V]: ...
/// ```
/// The `TypeParams` node would span from the left to right brackets (inclusive), and contain
/// the `T`, `U`, and `V` type parameters in the order they appear in the source code.

#[derive(Clone, Debug, PartialEq)]
pub struct TypeParams {
    pub range: TextRange,
    pub type_params: Vec<TypeParam>,
}

impl Deref for TypeParams {
    type Target = [TypeParam];

    fn deref(&self) -> &Self::Target {
        &self.type_params
    }
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

impl Parameters {
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

impl ParameterWithDefault {
    pub fn as_parameter(&self) -> &Parameter {
        &self.parameter
    }
}

impl Parameters {
    pub fn defaults(&self) -> impl std::iter::Iterator<Item = &Expr> {
        self.posonlyargs
            .iter()
            .chain(self.args.iter())
            .filter_map(|arg| arg.default.as_ref().map(std::convert::AsRef::as_ref))
    }

    #[allow(clippy::type_complexity)]
    pub fn split_kwonlyargs(&self) -> (Vec<&Parameter>, Vec<(&Parameter, &Expr)>) {
        let mut args = Vec::new();
        let mut with_defaults = Vec::new();
        for arg in &self.kwonlyargs {
            if let Some(ref default) = arg.default {
                with_defaults.push((arg.as_parameter(), &**default));
            } else {
                args.push(arg.as_parameter());
            }
        }
        (args, with_defaults)
    }
}

/// The kind of escape command as defined in [IPython Syntax] in the IPython codebase.
///
/// [IPython Syntax]: https://github.com/ipython/ipython/blob/635815e8f1ded5b764d66cacc80bbe25e9e2587f/IPython/core/inputtransformer2.py#L335-L343
#[derive(PartialEq, Eq, Debug, Clone, Hash, Copy)]
pub enum IpyEscapeKind {
    /// Send line to underlying system shell (`!`).
    Shell,
    /// Send line to system shell and capture output (`!!`).
    ShCap,
    /// Show help on object (`?`).
    Help,
    /// Show help on object, with extra verbosity (`??`).
    Help2,
    /// Call magic function (`%`).
    Magic,
    /// Call cell magic function (`%%`).
    Magic2,
    /// Call first argument with rest of line as arguments after splitting on whitespace
    /// and quote each as string (`,`).
    Quote,
    /// Call first argument with rest of line as an argument quoted as a single string (`;`).
    Quote2,
    /// Call first argument with rest of line as arguments (`/`).
    Paren,
}

impl TryFrom<char> for IpyEscapeKind {
    type Error = String;

    fn try_from(ch: char) -> Result<Self, Self::Error> {
        match ch {
            '!' => Ok(IpyEscapeKind::Shell),
            '?' => Ok(IpyEscapeKind::Help),
            '%' => Ok(IpyEscapeKind::Magic),
            ',' => Ok(IpyEscapeKind::Quote),
            ';' => Ok(IpyEscapeKind::Quote2),
            '/' => Ok(IpyEscapeKind::Paren),
            _ => Err(format!("Unexpected magic escape: {ch}")),
        }
    }
}

impl TryFrom<[char; 2]> for IpyEscapeKind {
    type Error = String;

    fn try_from(ch: [char; 2]) -> Result<Self, Self::Error> {
        match ch {
            ['!', '!'] => Ok(IpyEscapeKind::ShCap),
            ['?', '?'] => Ok(IpyEscapeKind::Help2),
            ['%', '%'] => Ok(IpyEscapeKind::Magic2),
            [c1, c2] => Err(format!("Unexpected magic escape: {c1}{c2}")),
        }
    }
}

impl fmt::Display for IpyEscapeKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl IpyEscapeKind {
    /// Returns the length of the escape kind token.
    pub fn prefix_len(self) -> TextSize {
        let len = match self {
            IpyEscapeKind::Shell
            | IpyEscapeKind::Magic
            | IpyEscapeKind::Help
            | IpyEscapeKind::Quote
            | IpyEscapeKind::Quote2
            | IpyEscapeKind::Paren => 1,
            IpyEscapeKind::ShCap | IpyEscapeKind::Magic2 | IpyEscapeKind::Help2 => 2,
        };
        len.into()
    }

    /// Returns `true` if the escape kind is help i.e., `?` or `??`.
    pub const fn is_help(self) -> bool {
        matches!(self, IpyEscapeKind::Help | IpyEscapeKind::Help2)
    }

    /// Returns `true` if the escape kind is magic i.e., `%` or `%%`.
    pub const fn is_magic(self) -> bool {
        matches!(self, IpyEscapeKind::Magic | IpyEscapeKind::Magic2)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            IpyEscapeKind::Shell => "!",
            IpyEscapeKind::ShCap => "!!",
            IpyEscapeKind::Help => "?",
            IpyEscapeKind::Help2 => "??",
            IpyEscapeKind::Magic => "%",
            IpyEscapeKind::Magic2 => "%%",
            IpyEscapeKind::Quote => ",",
            IpyEscapeKind::Quote2 => ";",
            IpyEscapeKind::Paren => "/",
        }
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

#[derive(Clone, Debug, PartialEq)]
pub enum Singleton {
    None,
    True,
    False,
}

impl From<bool> for Singleton {
    fn from(value: bool) -> Self {
        if value {
            Singleton::True
        } else {
            Singleton::False
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
impl Ranged for crate::nodes::StmtIpyEscapeCommand {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::Stmt {
    fn range(&self) -> TextRange {
        match self {
            Self::FunctionDef(node) => node.range(),
            Self::ClassDef(node) => node.range(),
            Self::Return(node) => node.range(),
            Self::Delete(node) => node.range(),
            Self::TypeAlias(node) => node.range(),
            Self::Assign(node) => node.range(),
            Self::AugAssign(node) => node.range(),
            Self::AnnAssign(node) => node.range(),
            Self::For(node) => node.range(),
            Self::While(node) => node.range(),
            Self::If(node) => node.range(),
            Self::With(node) => node.range(),
            Self::Match(node) => node.range(),
            Self::Raise(node) => node.range(),
            Self::Try(node) => node.range(),
            Self::Assert(node) => node.range(),
            Self::Import(node) => node.range(),
            Self::ImportFrom(node) => node.range(),
            Self::Global(node) => node.range(),
            Self::Nonlocal(node) => node.range(),
            Self::Expr(node) => node.range(),
            Self::Pass(node) => node.range(),
            Self::Break(node) => node.range(),
            Self::Continue(node) => node.range(),
            Stmt::IpyEscapeCommand(node) => node.range(),
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
impl Ranged for crate::nodes::ExprFString {
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
impl Ranged for crate::nodes::ExprIpyEscapeCommand {
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
            Self::FString(node) => node.range(),
            Self::StringLiteral(node) => node.range(),
            Self::BytesLiteral(node) => node.range(),
            Self::NumberLiteral(node) => node.range(),
            Self::BooleanLiteral(node) => node.range(),
            Self::NoneLiteral(node) => node.range(),
            Self::EllipsisLiteral(node) => node.range(),
            Self::Attribute(node) => node.range(),
            Self::Subscript(node) => node.range(),
            Self::Starred(node) => node.range(),
            Self::Name(node) => node.range(),
            Self::List(node) => node.range(),
            Self::Tuple(node) => node.range(),
            Self::Slice(node) => node.range(),
            Self::IpyEscapeCommand(node) => node.range(),
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
impl Ranged for crate::nodes::Parameter {
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
impl Ranged for crate::nodes::PatternArguments {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::PatternKeyword {
    fn range(&self) -> TextRange {
        self.range
    }
}

impl Ranged for crate::nodes::TypeParams {
    fn range(&self) -> TextRange {
        self.range
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
impl Ranged for crate::nodes::Parameters {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::ParameterWithDefault {
    fn range(&self) -> TextRange {
        self.range
    }
}

/// An expression that may be parenthesized.
#[derive(Clone, Debug)]
pub struct ParenthesizedExpr {
    /// The range of the expression, including any parentheses.
    pub range: TextRange,
    /// The underlying expression.
    pub expr: Expr,
}
impl ParenthesizedExpr {
    /// Returns `true` if the expression is may be parenthesized.
    pub fn is_parenthesized(&self) -> bool {
        self.range != self.expr.range()
    }
}
impl Ranged for ParenthesizedExpr {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl From<Expr> for ParenthesizedExpr {
    fn from(expr: Expr) -> Self {
        ParenthesizedExpr {
            range: expr.range(),
            expr,
        }
    }
}
impl From<ParenthesizedExpr> for Expr {
    fn from(parenthesized_expr: ParenthesizedExpr) -> Self {
        parenthesized_expr.expr
    }
}
impl From<ExprIpyEscapeCommand> for ParenthesizedExpr {
    fn from(payload: ExprIpyEscapeCommand) -> Self {
        Expr::IpyEscapeCommand(payload).into()
    }
}
impl From<ExprBoolOp> for ParenthesizedExpr {
    fn from(payload: ExprBoolOp) -> Self {
        Expr::BoolOp(payload).into()
    }
}
impl From<ExprNamedExpr> for ParenthesizedExpr {
    fn from(payload: ExprNamedExpr) -> Self {
        Expr::NamedExpr(payload).into()
    }
}
impl From<ExprBinOp> for ParenthesizedExpr {
    fn from(payload: ExprBinOp) -> Self {
        Expr::BinOp(payload).into()
    }
}
impl From<ExprUnaryOp> for ParenthesizedExpr {
    fn from(payload: ExprUnaryOp) -> Self {
        Expr::UnaryOp(payload).into()
    }
}
impl From<ExprLambda> for ParenthesizedExpr {
    fn from(payload: ExprLambda) -> Self {
        Expr::Lambda(payload).into()
    }
}
impl From<ExprIfExp> for ParenthesizedExpr {
    fn from(payload: ExprIfExp) -> Self {
        Expr::IfExp(payload).into()
    }
}
impl From<ExprDict> for ParenthesizedExpr {
    fn from(payload: ExprDict) -> Self {
        Expr::Dict(payload).into()
    }
}
impl From<ExprSet> for ParenthesizedExpr {
    fn from(payload: ExprSet) -> Self {
        Expr::Set(payload).into()
    }
}
impl From<ExprListComp> for ParenthesizedExpr {
    fn from(payload: ExprListComp) -> Self {
        Expr::ListComp(payload).into()
    }
}
impl From<ExprSetComp> for ParenthesizedExpr {
    fn from(payload: ExprSetComp) -> Self {
        Expr::SetComp(payload).into()
    }
}
impl From<ExprDictComp> for ParenthesizedExpr {
    fn from(payload: ExprDictComp) -> Self {
        Expr::DictComp(payload).into()
    }
}
impl From<ExprGeneratorExp> for ParenthesizedExpr {
    fn from(payload: ExprGeneratorExp) -> Self {
        Expr::GeneratorExp(payload).into()
    }
}
impl From<ExprAwait> for ParenthesizedExpr {
    fn from(payload: ExprAwait) -> Self {
        Expr::Await(payload).into()
    }
}
impl From<ExprYield> for ParenthesizedExpr {
    fn from(payload: ExprYield) -> Self {
        Expr::Yield(payload).into()
    }
}
impl From<ExprYieldFrom> for ParenthesizedExpr {
    fn from(payload: ExprYieldFrom) -> Self {
        Expr::YieldFrom(payload).into()
    }
}
impl From<ExprCompare> for ParenthesizedExpr {
    fn from(payload: ExprCompare) -> Self {
        Expr::Compare(payload).into()
    }
}
impl From<ExprCall> for ParenthesizedExpr {
    fn from(payload: ExprCall) -> Self {
        Expr::Call(payload).into()
    }
}
impl From<ExprFormattedValue> for ParenthesizedExpr {
    fn from(payload: ExprFormattedValue) -> Self {
        Expr::FormattedValue(payload).into()
    }
}
impl From<ExprFString> for ParenthesizedExpr {
    fn from(payload: ExprFString) -> Self {
        Expr::FString(payload).into()
    }
}
impl From<ExprStringLiteral> for ParenthesizedExpr {
    fn from(payload: ExprStringLiteral) -> Self {
        Expr::StringLiteral(payload).into()
    }
}
impl From<ExprBytesLiteral> for ParenthesizedExpr {
    fn from(payload: ExprBytesLiteral) -> Self {
        Expr::BytesLiteral(payload).into()
    }
}
impl From<ExprNumberLiteral> for ParenthesizedExpr {
    fn from(payload: ExprNumberLiteral) -> Self {
        Expr::NumberLiteral(payload).into()
    }
}
impl From<ExprBooleanLiteral> for ParenthesizedExpr {
    fn from(payload: ExprBooleanLiteral) -> Self {
        Expr::BooleanLiteral(payload).into()
    }
}
impl From<ExprNoneLiteral> for ParenthesizedExpr {
    fn from(payload: ExprNoneLiteral) -> Self {
        Expr::NoneLiteral(payload).into()
    }
}
impl From<ExprEllipsisLiteral> for ParenthesizedExpr {
    fn from(payload: ExprEllipsisLiteral) -> Self {
        Expr::EllipsisLiteral(payload).into()
    }
}
impl From<ExprAttribute> for ParenthesizedExpr {
    fn from(payload: ExprAttribute) -> Self {
        Expr::Attribute(payload).into()
    }
}
impl From<ExprSubscript> for ParenthesizedExpr {
    fn from(payload: ExprSubscript) -> Self {
        Expr::Subscript(payload).into()
    }
}
impl From<ExprStarred> for ParenthesizedExpr {
    fn from(payload: ExprStarred) -> Self {
        Expr::Starred(payload).into()
    }
}
impl From<ExprName> for ParenthesizedExpr {
    fn from(payload: ExprName) -> Self {
        Expr::Name(payload).into()
    }
}
impl From<ExprList> for ParenthesizedExpr {
    fn from(payload: ExprList) -> Self {
        Expr::List(payload).into()
    }
}
impl From<ExprTuple> for ParenthesizedExpr {
    fn from(payload: ExprTuple) -> Self {
        Expr::Tuple(payload).into()
    }
}
impl From<ExprSlice> for ParenthesizedExpr {
    fn from(payload: ExprSlice) -> Self {
        Expr::Slice(payload).into()
    }
}

#[cfg(target_pointer_width = "64")]
mod size_assertions {
    use static_assertions::assert_eq_size;

    #[allow(clippy::wildcard_imports)]
    use super::*;

    assert_eq_size!(Stmt, [u8; 144]);
    assert_eq_size!(StmtFunctionDef, [u8; 144]);
    assert_eq_size!(StmtClassDef, [u8; 104]);
    assert_eq_size!(StmtTry, [u8; 112]);
    assert_eq_size!(Expr, [u8; 80]);
    assert_eq_size!(Pattern, [u8; 96]);
    assert_eq_size!(Mod, [u8; 32]);
}
