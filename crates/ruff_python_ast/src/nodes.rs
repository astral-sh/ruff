#![allow(clippy::derive_partial_eq_without_eq)]

use std::borrow::Cow;
use std::fmt;
use std::fmt::Debug;
use std::iter::FusedIterator;
use std::ops::{Deref, DerefMut};
use std::slice::{Iter, IterMut};
use std::sync::OnceLock;

use bitflags::bitflags;
use itertools::Itertools;

use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};

use crate::name::Name;
use crate::{
    int,
    str::Quote,
    str_prefix::{AnyStringPrefix, ByteStringPrefix, FStringPrefix, StringLiteralPrefix},
    LiteralExpressionRef,
};

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
    pub value: Box<str>,
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
    pub type_params: Option<Box<TypeParams>>,
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
    pub level: u32,
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
    #[is(name = "named_expr")]
    Named(ExprNamed),
    #[is(name = "bin_op_expr")]
    BinOp(ExprBinOp),
    #[is(name = "unary_op_expr")]
    UnaryOp(ExprUnaryOp),
    #[is(name = "lambda_expr")]
    Lambda(ExprLambda),
    #[is(name = "if_expr")]
    If(ExprIf),
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
    #[is(name = "generator_expr")]
    Generator(ExprGenerator),
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
    pub value: Box<str>,
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
pub struct ExprNamed {
    pub range: TextRange,
    pub target: Box<Expr>,
    pub value: Box<Expr>,
}

impl From<ExprNamed> for Expr {
    fn from(payload: ExprNamed) -> Self {
        Expr::Named(payload)
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
pub struct ExprIf {
    pub range: TextRange,
    pub test: Box<Expr>,
    pub body: Box<Expr>,
    pub orelse: Box<Expr>,
}

impl From<ExprIf> for Expr {
    fn from(payload: ExprIf) -> Self {
        Expr::If(payload)
    }
}

/// Represents an item in a [dictionary literal display][1].
///
/// Consider the following Python dictionary literal:
/// ```python
/// {key1: value1, **other_dictionary}
/// ```
///
/// In our AST, this would be represented using an `ExprDict` node containing
/// two `DictItem` nodes inside it:
/// ```ignore
/// [
///     DictItem {
///         key: Some(Expr::Name(ExprName { id: "key1" })),
///         value: Expr::Name(ExprName { id: "value1" }),
///     },
///     DictItem {
///         key: None,
///         value: Expr::Name(ExprName { id: "other_dictionary" }),
///     }
/// ]
/// ```
///
/// [1]: https://docs.python.org/3/reference/expressions.html#displays-for-lists-sets-and-dictionaries
#[derive(Debug, Clone, PartialEq)]
pub struct DictItem {
    pub key: Option<Expr>,
    pub value: Expr,
}

impl DictItem {
    fn key(&self) -> Option<&Expr> {
        self.key.as_ref()
    }

    fn value(&self) -> &Expr {
        &self.value
    }
}

impl Ranged for DictItem {
    fn range(&self) -> TextRange {
        TextRange::new(
            self.key.as_ref().map_or(self.value.start(), Ranged::start),
            self.value.end(),
        )
    }
}

/// See also [Dict](https://docs.python.org/3/library/ast.html#ast.Dict)
#[derive(Clone, Debug, PartialEq)]
pub struct ExprDict {
    pub range: TextRange,
    pub items: Vec<DictItem>,
}

impl ExprDict {
    /// Returns an `Iterator` over the AST nodes representing the
    /// dictionary's keys.
    pub fn iter_keys(&self) -> DictKeyIterator {
        DictKeyIterator::new(&self.items)
    }

    /// Returns an `Iterator` over the AST nodes representing the
    /// dictionary's values.
    pub fn iter_values(&self) -> DictValueIterator {
        DictValueIterator::new(&self.items)
    }

    /// Returns the AST node representing the *n*th key of this
    /// dictionary.
    ///
    /// Panics: If the index `n` is out of bounds.
    pub fn key(&self, n: usize) -> Option<&Expr> {
        self.items[n].key()
    }

    /// Returns the AST node representing the *n*th value of this
    /// dictionary.
    ///
    /// Panics: If the index `n` is out of bounds.
    pub fn value(&self, n: usize) -> &Expr {
        self.items[n].value()
    }

    pub fn iter(&self) -> std::slice::Iter<'_, DictItem> {
        self.items.iter()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

impl<'a> IntoIterator for &'a ExprDict {
    type IntoIter = std::slice::Iter<'a, DictItem>;
    type Item = &'a DictItem;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl From<ExprDict> for Expr {
    fn from(payload: ExprDict) -> Self {
        Expr::Dict(payload)
    }
}

#[derive(Debug, Clone)]
pub struct DictKeyIterator<'a> {
    items: Iter<'a, DictItem>,
}

impl<'a> DictKeyIterator<'a> {
    fn new(items: &'a [DictItem]) -> Self {
        Self {
            items: items.iter(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl<'a> Iterator for DictKeyIterator<'a> {
    type Item = Option<&'a Expr>;

    fn next(&mut self) -> Option<Self::Item> {
        self.items.next().map(DictItem::key)
    }

    fn last(mut self) -> Option<Self::Item> {
        self.next_back()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.items.size_hint()
    }
}

impl<'a> DoubleEndedIterator for DictKeyIterator<'a> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.items.next_back().map(DictItem::key)
    }
}

impl<'a> FusedIterator for DictKeyIterator<'a> {}
impl<'a> ExactSizeIterator for DictKeyIterator<'a> {}

#[derive(Debug, Clone)]
pub struct DictValueIterator<'a> {
    items: Iter<'a, DictItem>,
}

impl<'a> DictValueIterator<'a> {
    fn new(items: &'a [DictItem]) -> Self {
        Self {
            items: items.iter(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl<'a> Iterator for DictValueIterator<'a> {
    type Item = &'a Expr;

    fn next(&mut self) -> Option<Self::Item> {
        self.items.next().map(DictItem::value)
    }

    fn last(mut self) -> Option<Self::Item> {
        self.next_back()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.items.size_hint()
    }
}

impl<'a> DoubleEndedIterator for DictValueIterator<'a> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.items.next_back().map(DictItem::value)
    }
}

impl<'a> FusedIterator for DictValueIterator<'a> {}
impl<'a> ExactSizeIterator for DictValueIterator<'a> {}

/// See also [Set](https://docs.python.org/3/library/ast.html#ast.Set)
#[derive(Clone, Debug, PartialEq)]
pub struct ExprSet {
    pub range: TextRange,
    pub elts: Vec<Expr>,
}

impl ExprSet {
    pub fn iter(&self) -> std::slice::Iter<'_, Expr> {
        self.elts.iter()
    }

    pub fn len(&self) -> usize {
        self.elts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.elts.is_empty()
    }
}

impl<'a> IntoIterator for &'a ExprSet {
    type IntoIter = std::slice::Iter<'a, Expr>;
    type Item = &'a Expr;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
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
pub struct ExprGenerator {
    pub range: TextRange,
    pub elt: Box<Expr>,
    pub generators: Vec<Comprehension>,
    pub parenthesized: bool,
}

impl From<ExprGenerator> for Expr {
    fn from(payload: ExprGenerator) -> Self {
        Expr::Generator(payload)
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
    pub ops: Box<[CmpOp]>,
    pub comparators: Box<[Expr]>,
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

#[derive(Clone, Debug, PartialEq)]
pub struct FStringFormatSpec {
    pub range: TextRange,
    pub elements: FStringElements,
}

impl Ranged for FStringFormatSpec {
    fn range(&self) -> TextRange {
        self.range
    }
}

/// See also [FormattedValue](https://docs.python.org/3/library/ast.html#ast.FormattedValue)
#[derive(Clone, Debug, PartialEq)]
pub struct FStringExpressionElement {
    pub range: TextRange,
    pub expression: Box<Expr>,
    pub debug_text: Option<DebugText>,
    pub conversion: ConversionFlag,
    pub format_spec: Option<Box<FStringFormatSpec>>,
}

impl Ranged for FStringExpressionElement {
    fn range(&self) -> TextRange {
        self.range
    }
}

/// An `FStringLiteralElement` with an empty `value` is an invalid f-string element.
#[derive(Clone, Debug, PartialEq)]
pub struct FStringLiteralElement {
    pub range: TextRange,
    pub value: Box<str>,
}

impl FStringLiteralElement {
    pub fn is_valid(&self) -> bool {
        !self.value.is_empty()
    }
}

impl Ranged for FStringLiteralElement {
    fn range(&self) -> TextRange {
        self.range
    }
}

impl Deref for FStringLiteralElement {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.value
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
    /// The text between the expression and the conversion, the `format_spec`, or the `}`, depending on what's present in the source
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

    /// Returns a slice of all the [`FStringPart`]s contained in this value.
    pub fn as_slice(&self) -> &[FStringPart] {
        match &self.inner {
            FStringValueInner::Single(part) => std::slice::from_ref(part),
            FStringValueInner::Concatenated(parts) => parts,
        }
    }

    /// Returns a mutable slice of all the [`FStringPart`]s contained in this value.
    fn as_mut_slice(&mut self) -> &mut [FStringPart] {
        match &mut self.inner {
            FStringValueInner::Single(part) => std::slice::from_mut(part),
            FStringValueInner::Concatenated(parts) => parts,
        }
    }

    /// Returns an iterator over all the [`FStringPart`]s contained in this value.
    pub fn iter(&self) -> Iter<FStringPart> {
        self.as_slice().iter()
    }

    /// Returns an iterator over all the [`FStringPart`]s contained in this value
    /// that allows modification.
    pub(crate) fn iter_mut(&mut self) -> IterMut<FStringPart> {
        self.as_mut_slice().iter_mut()
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
        self.iter().filter_map(|part| part.as_literal())
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
        self.iter().filter_map(|part| part.as_f_string())
    }

    /// Returns an iterator over all the [`FStringElement`] contained in this value.
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
    pub fn elements(&self) -> impl Iterator<Item = &FStringElement> {
        self.f_strings().flat_map(|fstring| fstring.elements.iter())
    }
}

impl<'a> IntoIterator for &'a FStringValue {
    type Item = &'a FStringPart;
    type IntoIter = Iter<'a, FStringPart>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a> IntoIterator for &'a mut FStringValue {
    type Item = &'a mut FStringPart;
    type IntoIter = IterMut<'a, FStringPart>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
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

impl FStringPart {
    pub fn quote_style(&self) -> Quote {
        match self {
            Self::Literal(string_literal) => string_literal.flags.quote_style(),
            Self::FString(f_string) => f_string.flags.quote_style(),
        }
    }
}

impl Ranged for FStringPart {
    fn range(&self) -> TextRange {
        match self {
            FStringPart::Literal(string_literal) => string_literal.range(),
            FStringPart::FString(f_string) => f_string.range(),
        }
    }
}

pub trait StringFlags: Copy {
    /// Does the string use single or double quotes in its opener and closer?
    fn quote_style(self) -> Quote;

    /// Is the string triple-quoted, i.e.,
    /// does it begin and end with three consecutive quote characters?
    fn is_triple_quoted(self) -> bool;

    fn prefix(self) -> AnyStringPrefix;

    /// A `str` representation of the quotes used to start and close.
    /// This does not include any prefixes the string has in its opener.
    fn quote_str(self) -> &'static str {
        if self.is_triple_quoted() {
            match self.quote_style() {
                Quote::Single => "'''",
                Quote::Double => r#"""""#,
            }
        } else {
            match self.quote_style() {
                Quote::Single => "'",
                Quote::Double => "\"",
            }
        }
    }

    /// The length of the quotes used to start and close the string.
    /// This does not include the length of any prefixes the string has
    /// in its opener.
    fn quote_len(self) -> TextSize {
        if self.is_triple_quoted() {
            TextSize::new(3)
        } else {
            TextSize::new(1)
        }
    }

    /// The total length of the string's opener,
    /// i.e., the length of the prefixes plus the length
    /// of the quotes used to open the string.
    fn opener_len(self) -> TextSize {
        self.prefix().as_str().text_len() + self.quote_len()
    }

    /// The total length of the string's closer.
    /// This is always equal to `self.quote_len()`,
    /// but is provided here for symmetry with the `opener_len()` method.
    fn closer_len(self) -> TextSize {
        self.quote_len()
    }

    fn format_string_contents(self, contents: &str) -> String {
        let prefix = self.prefix();
        let quote_str = self.quote_str();
        format!("{prefix}{quote_str}{contents}{quote_str}")
    }
}

bitflags! {
    #[derive(Default, Copy, Clone, PartialEq, Eq, Hash)]
    struct FStringFlagsInner: u8 {
        /// The f-string uses double quotes (`"`) for its opener and closer.
        /// If this flag is not set, the f-string uses single quotes (`'`)
        /// for its opener and closer.
        const DOUBLE = 1 << 0;

        /// The f-string is triple-quoted:
        /// it begins and ends with three consecutive quote characters.
        /// For example: `f"""{bar}"""`.
        const TRIPLE_QUOTED = 1 << 1;

        /// The f-string has an `r` prefix, meaning it is a raw f-string
        /// with a lowercase 'r'. For example: `rf"{bar}"`
        const R_PREFIX_LOWER = 1 << 2;

        /// The f-string has an `R` prefix, meaning it is a raw f-string
        /// with an uppercase 'r'. For example: `Rf"{bar}"`.
        /// See https://black.readthedocs.io/en/stable/the_black_code_style/current_style.html#r-strings-and-r-strings
        /// for why we track the casing of the `r` prefix,
        /// but not for any other prefix
        const R_PREFIX_UPPER = 1 << 3;
    }
}

/// Flags that can be queried to obtain information
/// regarding the prefixes and quotes used for an f-string.
#[derive(Default, Copy, Clone, Eq, PartialEq, Hash)]
pub struct FStringFlags(FStringFlagsInner);

impl FStringFlags {
    #[must_use]
    pub fn with_quote_style(mut self, quote_style: Quote) -> Self {
        self.0
            .set(FStringFlagsInner::DOUBLE, quote_style.is_double());
        self
    }

    #[must_use]
    pub fn with_triple_quotes(mut self) -> Self {
        self.0 |= FStringFlagsInner::TRIPLE_QUOTED;
        self
    }

    #[must_use]
    pub fn with_prefix(mut self, prefix: FStringPrefix) -> Self {
        match prefix {
            FStringPrefix::Regular => {
                Self(self.0 - FStringFlagsInner::R_PREFIX_LOWER - FStringFlagsInner::R_PREFIX_UPPER)
            }
            FStringPrefix::Raw { uppercase_r } => {
                self.0.set(FStringFlagsInner::R_PREFIX_UPPER, uppercase_r);
                self.0.set(FStringFlagsInner::R_PREFIX_LOWER, !uppercase_r);
                self
            }
        }
    }

    pub const fn prefix(self) -> FStringPrefix {
        if self.0.contains(FStringFlagsInner::R_PREFIX_LOWER) {
            debug_assert!(!self.0.contains(FStringFlagsInner::R_PREFIX_UPPER));
            FStringPrefix::Raw { uppercase_r: false }
        } else if self.0.contains(FStringFlagsInner::R_PREFIX_UPPER) {
            FStringPrefix::Raw { uppercase_r: true }
        } else {
            FStringPrefix::Regular
        }
    }
}

impl StringFlags for FStringFlags {
    /// Return `true` if the f-string is triple-quoted, i.e.,
    /// it begins and ends with three consecutive quote characters.
    /// For example: `f"""{bar}"""`
    fn is_triple_quoted(self) -> bool {
        self.0.contains(FStringFlagsInner::TRIPLE_QUOTED)
    }

    /// Return the quoting style (single or double quotes)
    /// used by the f-string's opener and closer:
    /// - `f"{"a"}"` -> `QuoteStyle::Double`
    /// - `f'{"a"}'` -> `QuoteStyle::Single`
    fn quote_style(self) -> Quote {
        if self.0.contains(FStringFlagsInner::DOUBLE) {
            Quote::Double
        } else {
            Quote::Single
        }
    }

    fn prefix(self) -> AnyStringPrefix {
        AnyStringPrefix::Format(self.prefix())
    }
}

impl fmt::Debug for FStringFlags {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FStringFlags")
            .field("quote_style", &self.quote_style())
            .field("prefix", &self.prefix())
            .field("triple_quoted", &self.is_triple_quoted())
            .finish()
    }
}

/// An AST node that represents a single f-string which is part of an [`ExprFString`].
#[derive(Clone, Debug, PartialEq)]
pub struct FString {
    pub range: TextRange,
    pub elements: FStringElements,
    pub flags: FStringFlags,
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

/// A newtype wrapper around a list of [`FStringElement`].
#[derive(Clone, Default, PartialEq)]
pub struct FStringElements(Vec<FStringElement>);

impl FStringElements {
    /// Returns an iterator over all the [`FStringLiteralElement`] nodes contained in this f-string.
    pub fn literals(&self) -> impl Iterator<Item = &FStringLiteralElement> {
        self.iter().filter_map(|element| element.as_literal())
    }

    /// Returns an iterator over all the [`FStringExpressionElement`] nodes contained in this f-string.
    pub fn expressions(&self) -> impl Iterator<Item = &FStringExpressionElement> {
        self.iter().filter_map(|element| element.as_expression())
    }
}

impl From<Vec<FStringElement>> for FStringElements {
    fn from(elements: Vec<FStringElement>) -> Self {
        FStringElements(elements)
    }
}

impl<'a> IntoIterator for &'a FStringElements {
    type IntoIter = Iter<'a, FStringElement>;
    type Item = &'a FStringElement;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a> IntoIterator for &'a mut FStringElements {
    type IntoIter = IterMut<'a, FStringElement>;
    type Item = &'a mut FStringElement;

    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

impl Deref for FStringElements {
    type Target = [FStringElement];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for FStringElements {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl fmt::Debug for FStringElements {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

#[derive(Clone, Debug, PartialEq, is_macro::Is)]
pub enum FStringElement {
    Literal(FStringLiteralElement),
    Expression(FStringExpressionElement),
}

impl Ranged for FStringElement {
    fn range(&self) -> TextRange {
        match self {
            FStringElement::Literal(node) => node.range(),
            FStringElement::Expression(node) => node.range(),
        }
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
                value: OnceLock::new(),
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
        self.iter()
            .next()
            .map_or(false, |part| part.flags.prefix().is_unicode())
    }

    /// Returns a slice of all the [`StringLiteral`] parts contained in this value.
    pub fn as_slice(&self) -> &[StringLiteral] {
        match &self.inner {
            StringLiteralValueInner::Single(value) => std::slice::from_ref(value),
            StringLiteralValueInner::Concatenated(value) => value.strings.as_slice(),
        }
    }

    /// Returns a mutable slice of all the [`StringLiteral`] parts contained in this value.
    fn as_mut_slice(&mut self) -> &mut [StringLiteral] {
        match &mut self.inner {
            StringLiteralValueInner::Single(value) => std::slice::from_mut(value),
            StringLiteralValueInner::Concatenated(value) => value.strings.as_mut_slice(),
        }
    }

    /// Returns an iterator over all the [`StringLiteral`] parts contained in this value.
    pub fn iter(&self) -> Iter<StringLiteral> {
        self.as_slice().iter()
    }

    /// Returns an iterator over all the [`StringLiteral`] parts contained in this value
    /// that allows modification.
    pub(crate) fn iter_mut(&mut self) -> IterMut<StringLiteral> {
        self.as_mut_slice().iter_mut()
    }

    /// Returns `true` if the string literal value is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns the total length of the string literal value, in bytes, not
    /// [`char`]s or graphemes.
    pub fn len(&self) -> usize {
        self.iter().fold(0, |acc, part| acc + part.value.len())
    }

    /// Returns an iterator over the [`char`]s of each string literal part.
    pub fn chars(&self) -> impl Iterator<Item = char> + Clone + '_ {
        self.iter().flat_map(|part| part.value.chars())
    }

    /// Returns the concatenated string value as a [`str`].
    ///
    /// Note that this will perform an allocation on the first invocation if the
    /// string value is implicitly concatenated.
    pub fn to_str(&self) -> &str {
        match &self.inner {
            StringLiteralValueInner::Single(value) => value.as_str(),
            StringLiteralValueInner::Concatenated(value) => value.to_str(),
        }
    }
}

impl<'a> IntoIterator for &'a StringLiteralValue {
    type Item = &'a StringLiteral;
    type IntoIter = Iter<'a, StringLiteral>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a> IntoIterator for &'a mut StringLiteralValue {
    type Item = &'a mut StringLiteral;
    type IntoIter = IterMut<'a, StringLiteral>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

impl PartialEq<str> for StringLiteralValue {
    fn eq(&self, other: &str) -> bool {
        if self.len() != other.len() {
            return false;
        }
        // The `zip` here is safe because we have checked the length of both parts.
        self.chars().zip(other.chars()).all(|(c1, c2)| c1 == c2)
    }
}

impl fmt::Display for StringLiteralValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.to_str())
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

bitflags! {
    #[derive(Debug, Default, Copy, Clone, PartialEq, Eq, Hash)]
    struct StringLiteralFlagsInner: u8 {
        /// The string uses double quotes (e.g. `"foo"`).
        /// If this flag is not set, the string uses single quotes (`'foo'`).
        const DOUBLE = 1 << 0;

        /// The string is triple-quoted (`"""foo"""`):
        /// it begins and ends with three consecutive quote characters.
        const TRIPLE_QUOTED = 1 << 1;

        /// The string has a `u` or `U` prefix, e.g. `u"foo"`.
        /// While this prefix is a no-op at runtime,
        /// strings with this prefix can have no other prefixes set;
        /// it is therefore invalid for this flag to be set
        /// if `R_PREFIX` is also set.
        const U_PREFIX = 1 << 2;

        /// The string has an `r` prefix, meaning it is a raw string
        /// with a lowercase 'r' (e.g. `r"foo\."`).
        /// It is invalid to set this flag if `U_PREFIX` is also set.
        const R_PREFIX_LOWER = 1 << 3;

        /// The string has an `R` prefix, meaning it is a raw string
        /// with an uppercase 'R' (e.g. `R'foo\d'`).
        /// See https://black.readthedocs.io/en/stable/the_black_code_style/current_style.html#r-strings-and-r-strings
        /// for why we track the casing of the `r` prefix,
        /// but not for any other prefix
        const R_PREFIX_UPPER = 1 << 4;

        /// The string was deemed invalid by the parser.
        const INVALID = 1 << 5;
    }
}

/// Flags that can be queried to obtain information
/// regarding the prefixes and quotes used for a string literal.
#[derive(Default, Copy, Clone, Eq, PartialEq, Hash)]
pub struct StringLiteralFlags(StringLiteralFlagsInner);

impl StringLiteralFlags {
    #[must_use]
    pub fn with_quote_style(mut self, quote_style: Quote) -> Self {
        self.0
            .set(StringLiteralFlagsInner::DOUBLE, quote_style.is_double());
        self
    }

    #[must_use]
    pub fn with_triple_quotes(mut self) -> Self {
        self.0 |= StringLiteralFlagsInner::TRIPLE_QUOTED;
        self
    }

    #[must_use]
    pub fn with_prefix(self, prefix: StringLiteralPrefix) -> Self {
        let StringLiteralFlags(flags) = self;
        match prefix {
            StringLiteralPrefix::Empty => Self(
                flags
                    - StringLiteralFlagsInner::R_PREFIX_LOWER
                    - StringLiteralFlagsInner::R_PREFIX_UPPER
                    - StringLiteralFlagsInner::U_PREFIX,
            ),
            StringLiteralPrefix::Raw { uppercase: false } => Self(
                (flags | StringLiteralFlagsInner::R_PREFIX_LOWER)
                    - StringLiteralFlagsInner::R_PREFIX_UPPER
                    - StringLiteralFlagsInner::U_PREFIX,
            ),
            StringLiteralPrefix::Raw { uppercase: true } => Self(
                (flags | StringLiteralFlagsInner::R_PREFIX_UPPER)
                    - StringLiteralFlagsInner::R_PREFIX_LOWER
                    - StringLiteralFlagsInner::U_PREFIX,
            ),
            StringLiteralPrefix::Unicode => Self(
                (flags | StringLiteralFlagsInner::U_PREFIX)
                    - StringLiteralFlagsInner::R_PREFIX_LOWER
                    - StringLiteralFlagsInner::R_PREFIX_UPPER,
            ),
        }
    }

    #[must_use]
    pub fn with_invalid(mut self) -> Self {
        self.0 |= StringLiteralFlagsInner::INVALID;
        self
    }

    pub const fn prefix(self) -> StringLiteralPrefix {
        if self.0.contains(StringLiteralFlagsInner::U_PREFIX) {
            debug_assert!(!self.0.intersects(
                StringLiteralFlagsInner::R_PREFIX_LOWER
                    .union(StringLiteralFlagsInner::R_PREFIX_UPPER)
            ));
            StringLiteralPrefix::Unicode
        } else if self.0.contains(StringLiteralFlagsInner::R_PREFIX_LOWER) {
            debug_assert!(!self.0.contains(StringLiteralFlagsInner::R_PREFIX_UPPER));
            StringLiteralPrefix::Raw { uppercase: false }
        } else if self.0.contains(StringLiteralFlagsInner::R_PREFIX_UPPER) {
            StringLiteralPrefix::Raw { uppercase: true }
        } else {
            StringLiteralPrefix::Empty
        }
    }
}

impl StringFlags for StringLiteralFlags {
    /// Return the quoting style (single or double quotes)
    /// used by the string's opener and closer:
    /// - `"a"` -> `QuoteStyle::Double`
    /// - `'a'` -> `QuoteStyle::Single`
    fn quote_style(self) -> Quote {
        if self.0.contains(StringLiteralFlagsInner::DOUBLE) {
            Quote::Double
        } else {
            Quote::Single
        }
    }

    /// Return `true` if the string is triple-quoted, i.e.,
    /// it begins and ends with three consecutive quote characters.
    /// For example: `"""bar"""`
    fn is_triple_quoted(self) -> bool {
        self.0.contains(StringLiteralFlagsInner::TRIPLE_QUOTED)
    }

    fn prefix(self) -> AnyStringPrefix {
        AnyStringPrefix::Regular(self.prefix())
    }
}

impl fmt::Debug for StringLiteralFlags {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StringLiteralFlags")
            .field("quote_style", &self.quote_style())
            .field("prefix", &self.prefix())
            .field("triple_quoted", &self.is_triple_quoted())
            .finish()
    }
}

/// An AST node that represents a single string literal which is part of an
/// [`ExprStringLiteral`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StringLiteral {
    pub range: TextRange,
    pub value: Box<str>,
    pub flags: StringLiteralFlags,
}

impl Ranged for StringLiteral {
    fn range(&self) -> TextRange {
        self.range
    }
}

impl Deref for StringLiteral {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl StringLiteral {
    /// Extracts a string slice containing the entire `String`.
    pub fn as_str(&self) -> &str {
        self
    }

    /// Creates an invalid string literal with the given range.
    pub fn invalid(range: TextRange) -> Self {
        Self {
            range,
            value: "".into(),
            flags: StringLiteralFlags::default().with_invalid(),
        }
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
#[derive(Clone)]
struct ConcatenatedStringLiteral {
    /// Each string literal that makes up the concatenated string.
    strings: Vec<StringLiteral>,
    /// The concatenated string value.
    value: OnceLock<Box<str>>,
}

impl ConcatenatedStringLiteral {
    /// Extracts a string slice containing the entire concatenated string.
    fn to_str(&self) -> &str {
        self.value.get_or_init(|| {
            let concatenated: String = self.strings.iter().map(StringLiteral::as_str).collect();
            concatenated.into_boxed_str()
        })
    }
}

impl PartialEq for ConcatenatedStringLiteral {
    fn eq(&self, other: &Self) -> bool {
        if self.strings.len() != other.strings.len() {
            return false;
        }
        // The `zip` here is safe because we have checked the length of both parts.
        self.strings
            .iter()
            .zip(other.strings.iter())
            .all(|(s1, s2)| s1 == s2)
    }
}

impl Debug for ConcatenatedStringLiteral {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ConcatenatedStringLiteral")
            .field("strings", &self.strings)
            .field("value", &self.to_str())
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

    /// Returns a slice of all the [`BytesLiteral`] parts contained in this value.
    pub fn as_slice(&self) -> &[BytesLiteral] {
        match &self.inner {
            BytesLiteralValueInner::Single(value) => std::slice::from_ref(value),
            BytesLiteralValueInner::Concatenated(value) => value.as_slice(),
        }
    }

    /// Returns a mutable slice of all the [`BytesLiteral`] parts contained in this value.
    fn as_mut_slice(&mut self) -> &mut [BytesLiteral] {
        match &mut self.inner {
            BytesLiteralValueInner::Single(value) => std::slice::from_mut(value),
            BytesLiteralValueInner::Concatenated(value) => value.as_mut_slice(),
        }
    }

    /// Returns an iterator over all the [`BytesLiteral`] parts contained in this value.
    pub fn iter(&self) -> Iter<BytesLiteral> {
        self.as_slice().iter()
    }

    /// Returns an iterator over all the [`BytesLiteral`] parts contained in this value
    /// that allows modification.
    pub(crate) fn iter_mut(&mut self) -> IterMut<BytesLiteral> {
        self.as_mut_slice().iter_mut()
    }

    /// Returns `true` if the concatenated bytes has a length of zero.
    pub fn is_empty(&self) -> bool {
        self.iter().all(|part| part.is_empty())
    }

    /// Returns the length of the concatenated bytes.
    pub fn len(&self) -> usize {
        self.iter().map(|part| part.len()).sum()
    }

    /// Returns an iterator over the bytes of the concatenated bytes.
    pub fn bytes(&self) -> impl Iterator<Item = u8> + '_ {
        self.iter().flat_map(|part| part.as_slice().iter().copied())
    }
}

impl<'a> IntoIterator for &'a BytesLiteralValue {
    type Item = &'a BytesLiteral;
    type IntoIter = Iter<'a, BytesLiteral>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a> IntoIterator for &'a mut BytesLiteralValue {
    type Item = &'a mut BytesLiteral;
    type IntoIter = IterMut<'a, BytesLiteral>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
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

impl<'a> From<&'a BytesLiteralValue> for Cow<'a, [u8]> {
    fn from(value: &'a BytesLiteralValue) -> Self {
        match &value.inner {
            BytesLiteralValueInner::Single(BytesLiteral {
                value: bytes_value, ..
            }) => Cow::from(bytes_value.as_ref()),
            BytesLiteralValueInner::Concatenated(bytes_literal_vec) => Cow::Owned(
                bytes_literal_vec
                    .iter()
                    .flat_map(|bytes_literal| bytes_literal.value.to_vec())
                    .collect::<Vec<u8>>(),
            ),
        }
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

bitflags! {
    #[derive(Default, Copy, Clone, PartialEq, Eq, Hash)]
    struct BytesLiteralFlagsInner: u8 {
        /// The bytestring uses double quotes (e.g. `b"foo"`).
        /// If this flag is not set, the bytestring uses single quotes (e.g. `b'foo'`).
        const DOUBLE = 1 << 0;

        /// The bytestring is triple-quoted (e.g. `b"""foo"""`):
        /// it begins and ends with three consecutive quote characters.
        const TRIPLE_QUOTED = 1 << 1;

        /// The bytestring has an `r` prefix (e.g. `rb"foo"`),
        /// meaning it is a raw bytestring with a lowercase 'r'.
        const R_PREFIX_LOWER = 1 << 2;

        /// The bytestring has an `R` prefix (e.g. `Rb"foo"`),
        /// meaning it is a raw bytestring with an uppercase 'R'.
        /// See https://black.readthedocs.io/en/stable/the_black_code_style/current_style.html#r-strings-and-r-strings
        /// for why we track the casing of the `r` prefix, but not for any other prefix
        const R_PREFIX_UPPER = 1 << 3;

        /// The bytestring was deemed invalid by the parser.
        const INVALID = 1 << 4;
    }
}

/// Flags that can be queried to obtain information
/// regarding the prefixes and quotes used for a bytes literal.
#[derive(Default, Copy, Clone, Eq, PartialEq, Hash)]
pub struct BytesLiteralFlags(BytesLiteralFlagsInner);

impl BytesLiteralFlags {
    #[must_use]
    pub fn with_quote_style(mut self, quote_style: Quote) -> Self {
        self.0
            .set(BytesLiteralFlagsInner::DOUBLE, quote_style.is_double());
        self
    }

    #[must_use]
    pub fn with_triple_quotes(mut self) -> Self {
        self.0 |= BytesLiteralFlagsInner::TRIPLE_QUOTED;
        self
    }

    #[must_use]
    pub fn with_prefix(mut self, prefix: ByteStringPrefix) -> Self {
        match prefix {
            ByteStringPrefix::Regular => {
                self.0 -= BytesLiteralFlagsInner::R_PREFIX_LOWER;
                self.0 -= BytesLiteralFlagsInner::R_PREFIX_UPPER;
            }
            ByteStringPrefix::Raw { uppercase_r } => {
                self.0
                    .set(BytesLiteralFlagsInner::R_PREFIX_UPPER, uppercase_r);
                self.0
                    .set(BytesLiteralFlagsInner::R_PREFIX_LOWER, !uppercase_r);
            }
        };
        self
    }

    #[must_use]
    pub fn with_invalid(mut self) -> Self {
        self.0 |= BytesLiteralFlagsInner::INVALID;
        self
    }

    pub const fn prefix(self) -> ByteStringPrefix {
        if self.0.contains(BytesLiteralFlagsInner::R_PREFIX_LOWER) {
            debug_assert!(!self.0.contains(BytesLiteralFlagsInner::R_PREFIX_UPPER));
            ByteStringPrefix::Raw { uppercase_r: false }
        } else if self.0.contains(BytesLiteralFlagsInner::R_PREFIX_UPPER) {
            ByteStringPrefix::Raw { uppercase_r: true }
        } else {
            ByteStringPrefix::Regular
        }
    }
}

impl StringFlags for BytesLiteralFlags {
    /// Return `true` if the bytestring is triple-quoted, i.e.,
    /// it begins and ends with three consecutive quote characters.
    /// For example: `b"""{bar}"""`
    fn is_triple_quoted(self) -> bool {
        self.0.contains(BytesLiteralFlagsInner::TRIPLE_QUOTED)
    }

    /// Return the quoting style (single or double quotes)
    /// used by the bytestring's opener and closer:
    /// - `b"a"` -> `QuoteStyle::Double`
    /// - `b'a'` -> `QuoteStyle::Single`
    fn quote_style(self) -> Quote {
        if self.0.contains(BytesLiteralFlagsInner::DOUBLE) {
            Quote::Double
        } else {
            Quote::Single
        }
    }

    fn prefix(self) -> AnyStringPrefix {
        AnyStringPrefix::Bytes(self.prefix())
    }
}

impl fmt::Debug for BytesLiteralFlags {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BytesLiteralFlags")
            .field("quote_style", &self.quote_style())
            .field("prefix", &self.prefix())
            .field("triple_quoted", &self.is_triple_quoted())
            .finish()
    }
}

/// An AST node that represents a single bytes literal which is part of an
/// [`ExprBytesLiteral`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BytesLiteral {
    pub range: TextRange,
    pub value: Box<[u8]>,
    pub flags: BytesLiteralFlags,
}

impl Ranged for BytesLiteral {
    fn range(&self) -> TextRange {
        self.range
    }
}

impl Deref for BytesLiteral {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl BytesLiteral {
    /// Extracts a byte slice containing the entire [`BytesLiteral`].
    pub fn as_slice(&self) -> &[u8] {
        self
    }

    /// Creates a new invalid bytes literal with the given range.
    pub fn invalid(range: TextRange) -> Self {
        Self {
            range,
            value: Box::new([]),
            flags: BytesLiteralFlags::default().with_invalid(),
        }
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

bitflags! {
    /// Flags that can be queried to obtain information
    /// regarding the prefixes and quotes used for a string literal.
    ///
    /// Note that not all of these flags can be validly combined -- e.g.,
    /// it is invalid to combine the `U_PREFIX` flag with any other
    /// of the `*_PREFIX` flags. As such, the recommended way to set the
    /// prefix flags is by calling the `as_flags()` method on the
    /// `StringPrefix` enum.
    #[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Hash)]
    struct AnyStringFlagsInner: u8 {
        /// The string uses double quotes (`"`).
        /// If this flag is not set, the string uses single quotes (`'`).
        const DOUBLE = 1 << 0;

        /// The string is triple-quoted:
        /// it begins and ends with three consecutive quote characters.
        const TRIPLE_QUOTED = 1 << 1;

        /// The string has a `u` or `U` prefix.
        /// While this prefix is a no-op at runtime,
        /// strings with this prefix can have no other prefixes set.
        const U_PREFIX = 1 << 2;

        /// The string has a `b` or `B` prefix.
        /// This means that the string is a sequence of `int`s at runtime,
        /// rather than a sequence of `str`s.
        /// Strings with this flag can also be raw strings,
        /// but can have no other prefixes.
        const B_PREFIX = 1 << 3;

        /// The string has a `f` or `F` prefix, meaning it is an f-string.
        /// F-strings can also be raw strings,
        /// but can have no other prefixes.
        const F_PREFIX = 1 << 4;

        /// The string has an `r` prefix, meaning it is a raw string.
        /// F-strings and byte-strings can be raw,
        /// as can strings with no other prefixes.
        /// U-strings cannot be raw.
        const R_PREFIX_LOWER = 1 << 5;

        /// The string has an `R` prefix, meaning it is a raw string.
        /// The casing of the `r`/`R` has no semantic significance at runtime;
        /// see https://black.readthedocs.io/en/stable/the_black_code_style/current_style.html#r-strings-and-r-strings
        /// for why we track the casing of the `r` prefix,
        /// but not for any other prefix
        const R_PREFIX_UPPER = 1 << 6;
    }
}

#[derive(Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AnyStringFlags(AnyStringFlagsInner);

impl AnyStringFlags {
    #[must_use]
    pub fn with_prefix(mut self, prefix: AnyStringPrefix) -> Self {
        self.0 |= match prefix {
            // regular strings
            AnyStringPrefix::Regular(StringLiteralPrefix::Empty) => AnyStringFlagsInner::empty(),
            AnyStringPrefix::Regular(StringLiteralPrefix::Unicode) => AnyStringFlagsInner::U_PREFIX,
            AnyStringPrefix::Regular(StringLiteralPrefix::Raw { uppercase: false }) => {
                AnyStringFlagsInner::R_PREFIX_LOWER
            }
            AnyStringPrefix::Regular(StringLiteralPrefix::Raw { uppercase: true }) => {
                AnyStringFlagsInner::R_PREFIX_UPPER
            }

            // bytestrings
            AnyStringPrefix::Bytes(ByteStringPrefix::Regular) => AnyStringFlagsInner::B_PREFIX,
            AnyStringPrefix::Bytes(ByteStringPrefix::Raw { uppercase_r: false }) => {
                AnyStringFlagsInner::B_PREFIX.union(AnyStringFlagsInner::R_PREFIX_LOWER)
            }
            AnyStringPrefix::Bytes(ByteStringPrefix::Raw { uppercase_r: true }) => {
                AnyStringFlagsInner::B_PREFIX.union(AnyStringFlagsInner::R_PREFIX_UPPER)
            }

            // f-strings
            AnyStringPrefix::Format(FStringPrefix::Regular) => AnyStringFlagsInner::F_PREFIX,
            AnyStringPrefix::Format(FStringPrefix::Raw { uppercase_r: false }) => {
                AnyStringFlagsInner::F_PREFIX.union(AnyStringFlagsInner::R_PREFIX_LOWER)
            }
            AnyStringPrefix::Format(FStringPrefix::Raw { uppercase_r: true }) => {
                AnyStringFlagsInner::F_PREFIX.union(AnyStringFlagsInner::R_PREFIX_UPPER)
            }
        };
        self
    }

    pub fn new(prefix: AnyStringPrefix, quotes: Quote, triple_quoted: bool) -> Self {
        let new = Self::default().with_prefix(prefix).with_quote_style(quotes);
        if triple_quoted {
            new.with_triple_quotes()
        } else {
            new
        }
    }

    /// Does the string have a `u` or `U` prefix?
    pub const fn is_u_string(self) -> bool {
        self.0.contains(AnyStringFlagsInner::U_PREFIX)
    }

    /// Does the string have an `r` or `R` prefix?
    pub const fn is_raw_string(self) -> bool {
        self.0.intersects(
            AnyStringFlagsInner::R_PREFIX_LOWER.union(AnyStringFlagsInner::R_PREFIX_UPPER),
        )
    }

    /// Does the string have an `f` or `F` prefix?
    pub const fn is_f_string(self) -> bool {
        self.0.contains(AnyStringFlagsInner::F_PREFIX)
    }

    /// Does the string have a `b` or `B` prefix?
    pub const fn is_byte_string(self) -> bool {
        self.0.contains(AnyStringFlagsInner::B_PREFIX)
    }

    #[must_use]
    pub fn with_quote_style(mut self, quotes: Quote) -> Self {
        match quotes {
            Quote::Double => self.0 |= AnyStringFlagsInner::DOUBLE,
            Quote::Single => self.0 -= AnyStringFlagsInner::DOUBLE,
        };
        self
    }

    #[must_use]
    pub fn with_triple_quotes(mut self) -> Self {
        self.0 |= AnyStringFlagsInner::TRIPLE_QUOTED;
        self
    }
}

impl StringFlags for AnyStringFlags {
    /// Does the string use single or double quotes in its opener and closer?
    fn quote_style(self) -> Quote {
        if self.0.contains(AnyStringFlagsInner::DOUBLE) {
            Quote::Double
        } else {
            Quote::Single
        }
    }

    /// Is the string triple-quoted, i.e.,
    /// does it begin and end with three consecutive quote characters?
    fn is_triple_quoted(self) -> bool {
        self.0.contains(AnyStringFlagsInner::TRIPLE_QUOTED)
    }

    fn prefix(self) -> AnyStringPrefix {
        let AnyStringFlags(flags) = self;

        // f-strings
        if flags.contains(AnyStringFlagsInner::F_PREFIX) {
            if flags.contains(AnyStringFlagsInner::R_PREFIX_LOWER) {
                return AnyStringPrefix::Format(FStringPrefix::Raw { uppercase_r: false });
            }
            if flags.contains(AnyStringFlagsInner::R_PREFIX_UPPER) {
                return AnyStringPrefix::Format(FStringPrefix::Raw { uppercase_r: true });
            }
            return AnyStringPrefix::Format(FStringPrefix::Regular);
        }

        // bytestrings
        if flags.contains(AnyStringFlagsInner::B_PREFIX) {
            if flags.contains(AnyStringFlagsInner::R_PREFIX_LOWER) {
                return AnyStringPrefix::Bytes(ByteStringPrefix::Raw { uppercase_r: false });
            }
            if flags.contains(AnyStringFlagsInner::R_PREFIX_UPPER) {
                return AnyStringPrefix::Bytes(ByteStringPrefix::Raw { uppercase_r: true });
            }
            return AnyStringPrefix::Bytes(ByteStringPrefix::Regular);
        }

        // all other strings
        if flags.contains(AnyStringFlagsInner::R_PREFIX_LOWER) {
            return AnyStringPrefix::Regular(StringLiteralPrefix::Raw { uppercase: false });
        }
        if flags.contains(AnyStringFlagsInner::R_PREFIX_UPPER) {
            return AnyStringPrefix::Regular(StringLiteralPrefix::Raw { uppercase: true });
        }
        if flags.contains(AnyStringFlagsInner::U_PREFIX) {
            return AnyStringPrefix::Regular(StringLiteralPrefix::Unicode);
        }
        AnyStringPrefix::Regular(StringLiteralPrefix::Empty)
    }
}

impl fmt::Debug for AnyStringFlags {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AnyStringFlags")
            .field("prefix", &self.prefix())
            .field("triple_quoted", &self.is_triple_quoted())
            .field("quote_style", &self.quote_style())
            .finish()
    }
}

impl From<AnyStringFlags> for StringLiteralFlags {
    fn from(value: AnyStringFlags) -> StringLiteralFlags {
        let AnyStringPrefix::Regular(prefix) = value.prefix() else {
            unreachable!(
                "Should never attempt to convert {} into a regular string",
                value.prefix()
            )
        };
        let new = StringLiteralFlags::default()
            .with_quote_style(value.quote_style())
            .with_prefix(prefix);
        if value.is_triple_quoted() {
            new.with_triple_quotes()
        } else {
            new
        }
    }
}

impl From<StringLiteralFlags> for AnyStringFlags {
    fn from(value: StringLiteralFlags) -> Self {
        Self::new(
            AnyStringPrefix::Regular(value.prefix()),
            value.quote_style(),
            value.is_triple_quoted(),
        )
    }
}

impl From<AnyStringFlags> for BytesLiteralFlags {
    fn from(value: AnyStringFlags) -> BytesLiteralFlags {
        let AnyStringPrefix::Bytes(bytestring_prefix) = value.prefix() else {
            unreachable!(
                "Should never attempt to convert {} into a bytestring",
                value.prefix()
            )
        };
        let new = BytesLiteralFlags::default()
            .with_quote_style(value.quote_style())
            .with_prefix(bytestring_prefix);
        if value.is_triple_quoted() {
            new.with_triple_quotes()
        } else {
            new
        }
    }
}

impl From<BytesLiteralFlags> for AnyStringFlags {
    fn from(value: BytesLiteralFlags) -> Self {
        Self::new(
            AnyStringPrefix::Bytes(value.prefix()),
            value.quote_style(),
            value.is_triple_quoted(),
        )
    }
}

impl From<AnyStringFlags> for FStringFlags {
    fn from(value: AnyStringFlags) -> FStringFlags {
        let AnyStringPrefix::Format(fstring_prefix) = value.prefix() else {
            unreachable!(
                "Should never attempt to convert {} into an f-string",
                value.prefix()
            )
        };
        let new = FStringFlags::default()
            .with_quote_style(value.quote_style())
            .with_prefix(fstring_prefix);
        if value.is_triple_quoted() {
            new.with_triple_quotes()
        } else {
            new
        }
    }
}

impl From<FStringFlags> for AnyStringFlags {
    fn from(value: FStringFlags) -> Self {
        Self::new(
            AnyStringPrefix::Format(value.prefix()),
            value.quote_style(),
            value.is_triple_quoted(),
        )
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
    pub id: Name,
    pub ctx: ExprContext,
}

impl ExprName {
    pub fn id(&self) -> &Name {
        &self.id
    }
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

impl ExprList {
    pub fn iter(&self) -> std::slice::Iter<'_, Expr> {
        self.elts.iter()
    }

    pub fn len(&self) -> usize {
        self.elts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.elts.is_empty()
    }
}

impl<'a> IntoIterator for &'a ExprList {
    type IntoIter = std::slice::Iter<'a, Expr>;
    type Item = &'a Expr;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
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

    /// Whether the tuple is parenthesized in the source code.
    pub parenthesized: bool,
}

impl ExprTuple {
    pub fn iter(&self) -> std::slice::Iter<'_, Expr> {
        self.elts.iter()
    }

    pub fn len(&self) -> usize {
        self.elts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.elts.is_empty()
    }
}

impl<'a> IntoIterator for &'a ExprTuple {
    type IntoIter = std::slice::Iter<'a, Expr>;
    type Item = &'a Expr;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
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
    Invalid,
}

/// See also [boolop](https://docs.python.org/3/library/ast.html#ast.BoolOp)
#[derive(Clone, Debug, PartialEq, is_macro::Is, Copy, Hash, Eq)]
pub enum BoolOp {
    And,
    Or,
}

impl BoolOp {
    pub const fn as_str(&self) -> &'static str {
        match self {
            BoolOp::And => "and",
            BoolOp::Or => "or",
        }
    }
}

impl fmt::Display for BoolOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
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
    pub const fn as_str(&self) -> &'static str {
        match self {
            Operator::Add => "+",
            Operator::Sub => "-",
            Operator::Mult => "*",
            Operator::MatMult => "@",
            Operator::Div => "/",
            Operator::Mod => "%",
            Operator::Pow => "**",
            Operator::LShift => "<<",
            Operator::RShift => ">>",
            Operator::BitOr => "|",
            Operator::BitXor => "^",
            Operator::BitAnd => "&",
            Operator::FloorDiv => "//",
        }
    }
}

impl fmt::Display for Operator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
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
    pub const fn as_str(&self) -> &'static str {
        match self {
            UnaryOp::Invert => "~",
            UnaryOp::Not => "not",
            UnaryOp::UAdd => "+",
            UnaryOp::USub => "-",
        }
    }
}

impl fmt::Display for UnaryOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
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
    pub const fn as_str(&self) -> &'static str {
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

impl fmt::Display for CmpOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
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

impl Pattern {
    /// Checks if the [`Pattern`] is an [irrefutable pattern].
    ///
    /// [irrefutable pattern]: https://peps.python.org/pep-0634/#irrefutable-case-blocks
    pub fn is_irrefutable(&self) -> bool {
        match self {
            Pattern::MatchAs(PatternMatchAs { pattern: None, .. }) => true,
            Pattern::MatchOr(PatternMatchOr { patterns, .. }) => {
                patterns.iter().any(Pattern::is_irrefutable)
            }
            _ => false,
        }
    }

    /// Checks if the [`Pattern`] is a [wildcard pattern].
    ///
    /// The following are wildcard patterns:
    /// ```python
    /// match subject:
    ///     case _ as x: ...
    ///     case _ | _: ...
    ///     case _: ...
    /// ```
    ///
    /// [wildcard pattern]: https://docs.python.org/3/reference/compound_stmts.html#wildcard-patterns
    pub fn is_wildcard(&self) -> bool {
        match self {
            Pattern::MatchAs(PatternMatchAs { pattern, .. }) => {
                pattern.as_deref().map_or(true, Pattern::is_wildcard)
            }
            Pattern::MatchOr(PatternMatchOr { patterns, .. }) => {
                patterns.iter().all(Pattern::is_wildcard)
            }
            _ => false,
        }
    }
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
    pub default: Option<Box<Expr>>,
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
    pub default: Option<Box<Expr>>,
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
    pub default: Option<Box<Expr>>,
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

/// Enumeration of the two kinds of parameter
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum AnyParameterRef<'a> {
    /// Variadic parameters cannot have default values,
    /// e.g. both `*args` and `**kwargs` in the following function:
    ///
    /// ```python
    /// def foo(*args, **kwargs): pass
    /// ```
    Variadic(&'a Parameter),

    /// Non-variadic parameters can have default values,
    /// though they won't necessarily always have them:
    ///
    /// ```python
    /// def bar(a=1, /, b=2, *, c=3): pass
    /// ```
    NonVariadic(&'a ParameterWithDefault),
}

impl<'a> AnyParameterRef<'a> {
    pub const fn as_parameter(self) -> &'a Parameter {
        match self {
            Self::NonVariadic(param) => &param.parameter,
            Self::Variadic(param) => param,
        }
    }

    pub const fn name(self) -> &'a Identifier {
        &self.as_parameter().name
    }

    pub const fn is_variadic(self) -> bool {
        matches!(self, Self::Variadic(_))
    }

    pub fn annotation(self) -> Option<&'a Expr> {
        self.as_parameter().annotation.as_deref()
    }

    pub fn default(self) -> Option<&'a Expr> {
        match self {
            Self::NonVariadic(param) => param.default.as_deref(),
            Self::Variadic(_) => None,
        }
    }
}

impl Ranged for AnyParameterRef<'_> {
    fn range(&self) -> TextRange {
        match self {
            Self::NonVariadic(param) => param.range,
            Self::Variadic(param) => param.range,
        }
    }
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

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Parameters {
    pub range: TextRange,
    pub posonlyargs: Vec<ParameterWithDefault>,
    pub args: Vec<ParameterWithDefault>,
    pub vararg: Option<Box<Parameter>>,
    pub kwonlyargs: Vec<ParameterWithDefault>,
    pub kwarg: Option<Box<Parameter>>,
}

impl Parameters {
    /// Returns an iterator over all non-variadic parameters included in this [`Parameters`] node.
    ///
    /// The variadic parameters (`.vararg` and `.kwarg`) can never have default values;
    /// non-variadic parameters sometimes will.
    pub fn iter_non_variadic_params(&self) -> impl Iterator<Item = &ParameterWithDefault> {
        self.posonlyargs
            .iter()
            .chain(&self.args)
            .chain(&self.kwonlyargs)
    }

    /// Returns the [`ParameterWithDefault`] with the given name, or `None` if no such [`ParameterWithDefault`] exists.
    pub fn find(&self, name: &str) -> Option<&ParameterWithDefault> {
        self.iter_non_variadic_params()
            .find(|arg| arg.parameter.name.as_str() == name)
    }

    /// Returns an iterator over all parameters included in this [`Parameters`] node.
    pub fn iter(&self) -> ParametersIterator {
        ParametersIterator::new(self)
    }

    /// Returns the total number of parameters included in this [`Parameters`] node.
    pub fn len(&self) -> usize {
        let Parameters {
            range: _,
            posonlyargs,
            args,
            vararg,
            kwonlyargs,
            kwarg,
        } = self;
        // Safety: a Python function can have an arbitrary number of parameters,
        // so theoretically this could be a number that wouldn't fit into a usize,
        // which would lead to a panic. A Python function with that many parameters
        // is extremely unlikely outside of generated code, however, and it's even
        // more unlikely that we'd find a function with that many parameters in a
        // source-code file <=4GB large (Ruff's maximum).
        posonlyargs
            .len()
            .checked_add(args.len())
            .and_then(|length| length.checked_add(usize::from(vararg.is_some())))
            .and_then(|length| length.checked_add(kwonlyargs.len()))
            .and_then(|length| length.checked_add(usize::from(kwarg.is_some())))
            .expect("Failed to fit the number of parameters into a usize")
    }

    /// Returns `true` if a parameter with the given name is included in this [`Parameters`].
    pub fn includes(&self, name: &str) -> bool {
        self.iter().any(|param| param.name() == name)
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

pub struct ParametersIterator<'a> {
    posonlyargs: Iter<'a, ParameterWithDefault>,
    args: Iter<'a, ParameterWithDefault>,
    vararg: Option<&'a Parameter>,
    kwonlyargs: Iter<'a, ParameterWithDefault>,
    kwarg: Option<&'a Parameter>,
}

impl<'a> ParametersIterator<'a> {
    fn new(parameters: &'a Parameters) -> Self {
        let Parameters {
            range: _,
            posonlyargs,
            args,
            vararg,
            kwonlyargs,
            kwarg,
        } = parameters;
        Self {
            posonlyargs: posonlyargs.iter(),
            args: args.iter(),
            vararg: vararg.as_deref(),
            kwonlyargs: kwonlyargs.iter(),
            kwarg: kwarg.as_deref(),
        }
    }
}

impl<'a> Iterator for ParametersIterator<'a> {
    type Item = AnyParameterRef<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let ParametersIterator {
            posonlyargs,
            args,
            vararg,
            kwonlyargs,
            kwarg,
        } = self;

        if let Some(param) = posonlyargs.next() {
            return Some(AnyParameterRef::NonVariadic(param));
        }
        if let Some(param) = args.next() {
            return Some(AnyParameterRef::NonVariadic(param));
        }
        if let Some(param) = vararg.take() {
            return Some(AnyParameterRef::Variadic(param));
        }
        if let Some(param) = kwonlyargs.next() {
            return Some(AnyParameterRef::NonVariadic(param));
        }
        kwarg.take().map(AnyParameterRef::Variadic)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let ParametersIterator {
            posonlyargs,
            args,
            vararg,
            kwonlyargs,
            kwarg,
        } = self;

        let posonlyargs_len = posonlyargs.len();
        let args_len = args.len();
        let vararg_len = usize::from(vararg.is_some());
        let kwonlyargs_len = kwonlyargs.len();
        let kwarg_len = usize::from(kwarg.is_some());

        let lower = posonlyargs_len
            .saturating_add(args_len)
            .saturating_add(vararg_len)
            .saturating_add(kwonlyargs_len)
            .saturating_add(kwarg_len);

        let upper = posonlyargs_len
            .checked_add(args_len)
            .and_then(|length| length.checked_add(vararg_len))
            .and_then(|length| length.checked_add(kwonlyargs_len))
            .and_then(|length| length.checked_add(kwarg_len));

        (lower, upper)
    }

    fn last(mut self) -> Option<Self::Item> {
        self.next_back()
    }
}

impl<'a> DoubleEndedIterator for ParametersIterator<'a> {
    fn next_back(&mut self) -> Option<Self::Item> {
        let ParametersIterator {
            posonlyargs,
            args,
            vararg,
            kwonlyargs,
            kwarg,
        } = self;

        if let Some(param) = kwarg.take() {
            return Some(AnyParameterRef::Variadic(param));
        }
        if let Some(param) = kwonlyargs.next_back() {
            return Some(AnyParameterRef::NonVariadic(param));
        }
        if let Some(param) = vararg.take() {
            return Some(AnyParameterRef::Variadic(param));
        }
        if let Some(param) = args.next_back() {
            return Some(AnyParameterRef::NonVariadic(param));
        }
        posonlyargs.next_back().map(AnyParameterRef::NonVariadic)
    }
}

impl<'a> FusedIterator for ParametersIterator<'a> {}

/// We rely on the same invariants outlined in the comment above `Parameters::len()`
/// in order to implement `ExactSizeIterator` here
impl<'a> ExactSizeIterator for ParametersIterator<'a> {}

impl<'a> IntoIterator for &'a Parameters {
    type IntoIter = ParametersIterator<'a>;
    type Item = AnyParameterRef<'a>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
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
    pub args: Box<[Expr]>,
    pub keywords: Box<[Keyword]>,
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

/// A suite represents a [Vec] of [Stmt].
///
/// See: <https://docs.python.org/3/reference/compound_stmts.html#grammar-token-python-grammar-suite>
pub type Suite = Vec<Stmt>;

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

/// An `Identifier` with an empty `id` is invalid.
///
/// For example, in the following code `id` will be empty.
/// ```python
/// def 1():
///     ...
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Identifier {
    pub id: Name,
    pub range: TextRange,
}

impl Identifier {
    #[inline]
    pub fn new(id: impl Into<Name>, range: TextRange) -> Self {
        Self {
            id: id.into(),
            range,
        }
    }

    pub fn id(&self) -> &Name {
        &self.id
    }

    pub fn is_valid(&self) -> bool {
        !self.id.is_empty()
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
        self.id == other
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

impl std::fmt::Display for Identifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.id, f)
    }
}

impl From<Identifier> for Name {
    #[inline]
    fn from(identifier: Identifier) -> Name {
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
impl Ranged for crate::nodes::ExprNamed {
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
impl Ranged for crate::nodes::ExprIf {
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
impl Ranged for crate::nodes::ExprGenerator {
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
            Self::Named(node) => node.range(),
            Self::BinOp(node) => node.range(),
            Self::UnaryOp(node) => node.range(),
            Self::Lambda(node) => node.range(),
            Self::If(node) => node.range(),
            Self::Dict(node) => node.range(),
            Self::Set(node) => node.range(),
            Self::ListComp(node) => node.range(),
            Self::SetComp(node) => node.range(),
            Self::DictComp(node) => node.range(),
            Self::Generator(node) => node.range(),
            Self::Await(node) => node.range(),
            Self::Yield(node) => node.range(),
            Self::YieldFrom(node) => node.range(),
            Self::Compare(node) => node.range(),
            Self::Call(node) => node.range(),
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

#[cfg(test)]
mod tests {
    #[allow(clippy::wildcard_imports)]
    use super::*;

    #[test]
    #[cfg(target_pointer_width = "64")]
    fn size() {
        assert!(std::mem::size_of::<Stmt>() <= 120);
        assert!(std::mem::size_of::<StmtFunctionDef>() <= 120);
        assert!(std::mem::size_of::<StmtClassDef>() <= 104);
        assert!(std::mem::size_of::<StmtTry>() <= 112);
        assert!(std::mem::size_of::<Mod>() <= 32);
        // 96 for Rustc < 1.76
        assert!(matches!(std::mem::size_of::<Pattern>(), 88 | 96));

        assert_eq!(std::mem::size_of::<Expr>(), 64);
        assert_eq!(std::mem::size_of::<ExprAttribute>(), 56);
        assert_eq!(std::mem::size_of::<ExprAwait>(), 16);
        assert_eq!(std::mem::size_of::<ExprBinOp>(), 32);
        assert_eq!(std::mem::size_of::<ExprBoolOp>(), 40);
        assert_eq!(std::mem::size_of::<ExprBooleanLiteral>(), 12);
        assert_eq!(std::mem::size_of::<ExprBytesLiteral>(), 40);
        assert_eq!(std::mem::size_of::<ExprCall>(), 56);
        assert_eq!(std::mem::size_of::<ExprCompare>(), 48);
        assert_eq!(std::mem::size_of::<ExprDict>(), 32);
        assert_eq!(std::mem::size_of::<ExprDictComp>(), 48);
        assert_eq!(std::mem::size_of::<ExprEllipsisLiteral>(), 8);
        // 56 for Rustc < 1.76
        assert!(matches!(std::mem::size_of::<ExprFString>(), 48 | 56));
        assert_eq!(std::mem::size_of::<ExprGenerator>(), 48);
        assert_eq!(std::mem::size_of::<ExprIf>(), 32);
        assert_eq!(std::mem::size_of::<ExprIpyEscapeCommand>(), 32);
        assert_eq!(std::mem::size_of::<ExprLambda>(), 24);
        assert_eq!(std::mem::size_of::<ExprList>(), 40);
        assert_eq!(std::mem::size_of::<ExprListComp>(), 40);
        assert_eq!(std::mem::size_of::<ExprName>(), 40);
        assert_eq!(std::mem::size_of::<ExprNamed>(), 24);
        assert_eq!(std::mem::size_of::<ExprNoneLiteral>(), 8);
        assert_eq!(std::mem::size_of::<ExprNumberLiteral>(), 32);
        assert_eq!(std::mem::size_of::<ExprSet>(), 32);
        assert_eq!(std::mem::size_of::<ExprSetComp>(), 40);
        assert_eq!(std::mem::size_of::<ExprSlice>(), 32);
        assert_eq!(std::mem::size_of::<ExprStarred>(), 24);
        assert_eq!(std::mem::size_of::<ExprStringLiteral>(), 56);
        assert_eq!(std::mem::size_of::<ExprSubscript>(), 32);
        assert_eq!(std::mem::size_of::<ExprTuple>(), 40);
        assert_eq!(std::mem::size_of::<ExprUnaryOp>(), 24);
        assert_eq!(std::mem::size_of::<ExprYield>(), 16);
        assert_eq!(std::mem::size_of::<ExprYieldFrom>(), 16);
    }
}
