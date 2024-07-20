#![allow(clippy::derive_partial_eq_without_eq)]

use std::fmt;
use std::fmt::Debug;
use std::iter::FusedIterator;
use std::ops::{Deref, DerefMut};
use std::slice::{Iter, IterMut};
use std::sync::OnceLock;

use bitflags::bitflags;
use itertools::Itertools;
use ruff_allocator::{Allocator, Box, CloneIn};

use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};

use crate::{
    int,
    str::Quote,
    str_prefix::{AnyStringPrefix, ByteStringPrefix, FStringPrefix, StringLiteralPrefix},
    LiteralExpressionRef,
};

/// See also [mod](https://docs.python.org/3/library/ast.html#ast.mod)
#[derive(Debug, PartialEq, is_macro::Is)]
pub enum Mod<'ast> {
    Module(ModModule<'ast>),
    Expression(ModExpression<'ast>),
}

/// See also [Module](https://docs.python.org/3/library/ast.html#ast.Module)
#[derive(Debug, PartialEq)]
pub struct ModModule<'ast> {
    pub range: TextRange,
    pub body: Vec<Stmt<'ast>>,
}

impl<'ast> From<ModModule<'ast>> for Mod<'ast> {
    fn from(payload: ModModule<'ast>) -> Self {
        Mod::Module(payload)
    }
}

/// See also [Expression](https://docs.python.org/3/library/ast.html#ast.Expression)
#[derive(Debug, PartialEq)]
pub struct ModExpression<'ast> {
    pub range: TextRange,
    pub body: Expr<'ast>,
}

impl<'ast> From<ModExpression<'ast>> for Mod<'ast> {
    fn from(payload: ModExpression<'ast>) -> Self {
        Mod::Expression(payload)
    }
}

/// See also [stmt](https://docs.python.org/3/library/ast.html#ast.stmt)
#[derive(Debug, PartialEq, is_macro::Is)]
pub enum Stmt<'ast> {
    #[is(name = "function_def_stmt")]
    FunctionDef(StmtFunctionDef<'ast>),
    #[is(name = "class_def_stmt")]
    ClassDef(StmtClassDef<'ast>),
    #[is(name = "return_stmt")]
    Return(StmtReturn<'ast>),
    #[is(name = "delete_stmt")]
    Delete(StmtDelete<'ast>),
    #[is(name = "assign_stmt")]
    Assign(StmtAssign<'ast>),
    #[is(name = "aug_assign_stmt")]
    AugAssign(StmtAugAssign<'ast>),
    #[is(name = "ann_assign_stmt")]
    AnnAssign(StmtAnnAssign<'ast>),
    #[is(name = "type_alias_stmt")]
    TypeAlias(StmtTypeAlias<'ast>),
    #[is(name = "for_stmt")]
    For(StmtFor<'ast>),
    #[is(name = "while_stmt")]
    While(StmtWhile<'ast>),
    #[is(name = "if_stmt")]
    If(StmtIf<'ast>),
    #[is(name = "with_stmt")]
    With(StmtWith<'ast>),
    #[is(name = "match_stmt")]
    Match(StmtMatch<'ast>),
    #[is(name = "raise_stmt")]
    Raise(StmtRaise<'ast>),
    #[is(name = "try_stmt")]
    Try(StmtTry<'ast>),
    #[is(name = "assert_stmt")]
    Assert(StmtAssert<'ast>),
    #[is(name = "import_stmt")]
    Import(StmtImport<'ast>),
    #[is(name = "import_from_stmt")]
    ImportFrom(StmtImportFrom<'ast>),
    #[is(name = "global_stmt")]
    Global(StmtGlobal<'ast>),
    #[is(name = "nonlocal_stmt")]
    Nonlocal(StmtNonlocal<'ast>),
    #[is(name = "expr_stmt")]
    Expr(StmtExpr<'ast>),
    #[is(name = "pass_stmt")]
    Pass(StmtPass),
    #[is(name = "break_stmt")]
    Break(StmtBreak),
    #[is(name = "continue_stmt")]
    Continue(StmtContinue),

    // Jupyter notebook specific
    #[is(name = "ipy_escape_command_stmt")]
    IpyEscapeCommand(StmtIpyEscapeCommand<'ast>),
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
#[derive(Debug, PartialEq)]
pub struct StmtIpyEscapeCommand<'ast> {
    pub range: TextRange,
    pub kind: IpyEscapeKind,
    pub value: &'ast str,
}

impl<'ast> From<StmtIpyEscapeCommand<'ast>> for Stmt<'ast> {
    fn from(payload: StmtIpyEscapeCommand<'ast>) -> Self {
        Stmt::IpyEscapeCommand(payload)
    }
}

/// See also [FunctionDef](https://docs.python.org/3/library/ast.html#ast.FunctionDef) and
/// [AsyncFunctionDef](https://docs.python.org/3/library/ast.html#ast.AsyncFunctionDef).
///
/// This type differs from the original Python AST, as it collapses the
/// synchronous and asynchronous variants into a single type.
#[derive(Debug, PartialEq)]
pub struct StmtFunctionDef<'ast> {
    pub range: TextRange,
    pub is_async: bool,
    pub decorator_list: Vec<Decorator<'ast>>,
    pub name: Identifier<'ast>,
    pub type_params: Option<Box<'ast, TypeParams<'ast>>>,
    pub parameters: Box<'ast, Parameters<'ast>>,
    pub returns: Option<Box<'ast, Expr<'ast>>>,
    pub body: Vec<Stmt<'ast>>,
}

impl<'ast> From<StmtFunctionDef<'ast>> for Stmt<'ast> {
    fn from(payload: StmtFunctionDef<'ast>) -> Self {
        Stmt::FunctionDef(payload)
    }
}

/// See also [ClassDef](https://docs.python.org/3/library/ast.html#ast.ClassDef)
#[derive(Debug, PartialEq)]
pub struct StmtClassDef<'ast> {
    pub range: TextRange,
    pub decorator_list: Vec<Decorator<'ast>>,
    pub name: Identifier<'ast>,
    pub type_params: Option<Box<'ast, TypeParams<'ast>>>,
    pub arguments: Option<Box<'ast, Arguments<'ast>>>,
    pub body: Vec<Stmt<'ast>>,
}

impl<'ast> StmtClassDef<'ast> {
    /// Return an iterator over the bases of the class.
    pub fn bases(&self) -> &[Expr<'ast>] {
        match &self.arguments {
            Some(arguments) => &arguments.args,
            None => &[],
        }
    }

    /// Return an iterator over the metaclass keywords of the class.
    pub fn keywords(&self) -> &[Keyword<'ast>] {
        match &self.arguments {
            Some(arguments) => &arguments.keywords,
            None => &[],
        }
    }
}

impl<'ast> From<StmtClassDef<'ast>> for Stmt<'ast> {
    fn from(payload: StmtClassDef<'ast>) -> Self {
        Stmt::ClassDef(payload)
    }
}

/// See also [Return](https://docs.python.org/3/library/ast.html#ast.Return)
#[derive(Debug, PartialEq)]
pub struct StmtReturn<'ast> {
    pub range: TextRange,
    pub value: Option<Box<'ast, Expr<'ast>>>,
}

impl<'ast> From<StmtReturn<'ast>> for Stmt<'ast> {
    fn from(payload: StmtReturn<'ast>) -> Self {
        Stmt::Return(payload)
    }
}

/// See also [Delete](https://docs.python.org/3/library/ast.html#ast.Delete)
#[derive(Debug, PartialEq)]
pub struct StmtDelete<'ast> {
    pub range: TextRange,
    pub targets: Vec<Expr<'ast>>,
}

impl<'ast> From<StmtDelete<'ast>> for Stmt<'ast> {
    fn from(payload: StmtDelete<'ast>) -> Self {
        Stmt::Delete(payload)
    }
}

/// See also [TypeAlias](https://docs.python.org/3/library/ast.html#ast.TypeAlias)
#[derive(Debug, PartialEq)]
pub struct StmtTypeAlias<'ast> {
    pub range: TextRange,
    pub name: Box<'ast, Expr<'ast>>,
    pub type_params: Option<TypeParams<'ast>>,
    pub value: Box<'ast, Expr<'ast>>,
}

impl<'ast> From<StmtTypeAlias<'ast>> for Stmt<'ast> {
    fn from(payload: StmtTypeAlias<'ast>) -> Self {
        Stmt::TypeAlias(payload)
    }
}

/// See also [Assign](https://docs.python.org/3/library/ast.html#ast.Assign)
#[derive(Debug, PartialEq)]
pub struct StmtAssign<'ast> {
    pub range: TextRange,
    pub targets: Vec<Expr<'ast>>,
    pub value: Box<'ast, Expr<'ast>>,
}

impl<'ast> From<StmtAssign<'ast>> for Stmt<'ast> {
    fn from(payload: StmtAssign<'ast>) -> Self {
        Stmt::Assign(payload)
    }
}

/// See also [AugAssign](https://docs.python.org/3/library/ast.html#ast.AugAssign)
#[derive(Debug, PartialEq)]
pub struct StmtAugAssign<'ast> {
    pub range: TextRange,
    pub target: Box<'ast, Expr<'ast>>,
    pub op: Operator,
    pub value: Box<'ast, Expr<'ast>>,
}

impl<'ast> From<StmtAugAssign<'ast>> for Stmt<'ast> {
    fn from(payload: StmtAugAssign<'ast>) -> Self {
        Stmt::AugAssign(payload)
    }
}

/// See also [AnnAssign](https://docs.python.org/3/library/ast.html#ast.AnnAssign)
#[derive(Debug, PartialEq)]
pub struct StmtAnnAssign<'ast> {
    pub range: TextRange,
    pub target: Box<'ast, Expr<'ast>>,
    pub annotation: Box<'ast, Expr<'ast>>,
    pub value: Option<Box<'ast, Expr<'ast>>>,
    pub simple: bool,
}

impl<'ast> From<StmtAnnAssign<'ast>> for Stmt<'ast> {
    fn from(payload: StmtAnnAssign<'ast>) -> Self {
        Stmt::AnnAssign(payload)
    }
}

/// See also [For](https://docs.python.org/3/library/ast.html#ast.For) and
/// [AsyncFor](https://docs.python.org/3/library/ast.html#ast.AsyncFor).
///
/// This type differs from the original Python AST, as it collapses the
/// synchronous and asynchronous variants into a single type.
#[derive(Debug, PartialEq)]
pub struct StmtFor<'ast> {
    pub range: TextRange,
    pub is_async: bool,
    pub target: Box<'ast, Expr<'ast>>,
    pub iter: Box<'ast, Expr<'ast>>,
    pub body: Vec<Stmt<'ast>>,
    pub orelse: Vec<Stmt<'ast>>,
}

impl<'ast> From<StmtFor<'ast>> for Stmt<'ast> {
    fn from(payload: StmtFor<'ast>) -> Self {
        Stmt::For(payload)
    }
}

/// See also [While](https://docs.python.org/3/library/ast.html#ast.While) and
/// [AsyncWhile](https://docs.python.org/3/library/ast.html#ast.AsyncWhile).
#[derive(Debug, PartialEq)]
pub struct StmtWhile<'ast> {
    pub range: TextRange,
    pub test: Box<'ast, Expr<'ast>>,
    pub body: Vec<Stmt<'ast>>,
    pub orelse: Vec<Stmt<'ast>>,
}

impl<'ast> From<StmtWhile<'ast>> for Stmt<'ast> {
    fn from(payload: StmtWhile<'ast>) -> Self {
        Stmt::While(payload)
    }
}

/// See also [If](https://docs.python.org/3/library/ast.html#ast.If)
#[derive(Debug, PartialEq)]
pub struct StmtIf<'ast> {
    pub range: TextRange,
    pub test: Box<'ast, Expr<'ast>>,
    pub body: Vec<Stmt<'ast>>,
    pub elif_else_clauses: Vec<ElifElseClause<'ast>>,
}

impl<'ast> From<StmtIf<'ast>> for Stmt<'ast> {
    fn from(payload: StmtIf<'ast>) -> Self {
        Stmt::If(payload)
    }
}

#[derive(Debug, PartialEq)]
pub struct ElifElseClause<'ast> {
    pub range: TextRange,
    pub test: Option<Expr<'ast>>,
    pub body: Vec<Stmt<'ast>>,
}

/// See also [With](https://docs.python.org/3/library/ast.html#ast.With) and
/// [AsyncWith](https://docs.python.org/3/library/ast.html#ast.AsyncWith).
///
/// This type differs from the original Python AST, as it collapses the
/// synchronous and asynchronous variants into a single type.
#[derive(Debug, PartialEq)]
pub struct StmtWith<'ast> {
    pub range: TextRange,
    pub is_async: bool,
    pub items: Vec<WithItem<'ast>>,
    pub body: Vec<Stmt<'ast>>,
}

impl<'ast> From<StmtWith<'ast>> for Stmt<'ast> {
    fn from(payload: StmtWith<'ast>) -> Self {
        Stmt::With(payload)
    }
}

/// See also [Match](https://docs.python.org/3/library/ast.html#ast.Match)
#[derive(Debug, PartialEq)]
pub struct StmtMatch<'ast> {
    pub range: TextRange,
    pub subject: Box<'ast, Expr<'ast>>,
    pub cases: Vec<MatchCase<'ast>>,
}

impl<'ast> From<StmtMatch<'ast>> for Stmt<'ast> {
    fn from(payload: StmtMatch<'ast>) -> Self {
        Stmt::Match(payload)
    }
}

/// See also [Raise](https://docs.python.org/3/library/ast.html#ast.Raise)
#[derive(Debug, PartialEq)]
pub struct StmtRaise<'ast> {
    pub range: TextRange,
    pub exc: Option<Box<'ast, Expr<'ast>>>,
    pub cause: Option<Box<'ast, Expr<'ast>>>,
}

impl<'ast> From<StmtRaise<'ast>> for Stmt<'ast> {
    fn from(payload: StmtRaise<'ast>) -> Self {
        Stmt::Raise(payload)
    }
}

/// See also [Try](https://docs.python.org/3/library/ast.html#ast.Try) and
/// [TryStar](https://docs.python.org/3/library/ast.html#ast.TryStar)
#[derive(Debug, PartialEq)]
pub struct StmtTry<'ast> {
    pub range: TextRange,
    pub body: Vec<Stmt<'ast>>,
    pub handlers: Vec<ExceptHandler<'ast>>,
    pub orelse: Vec<Stmt<'ast>>,
    pub finalbody: Vec<Stmt<'ast>>,
    pub is_star: bool,
}

impl<'ast> From<StmtTry<'ast>> for Stmt<'ast> {
    fn from(payload: StmtTry<'ast>) -> Self {
        Stmt::Try(payload)
    }
}

/// See also [Assert](https://docs.python.org/3/library/ast.html#ast.Assert)
#[derive(Debug, PartialEq)]
pub struct StmtAssert<'ast> {
    pub range: TextRange,
    pub test: Box<'ast, Expr<'ast>>,
    pub msg: Option<Box<'ast, Expr<'ast>>>,
}

impl<'ast> From<StmtAssert<'ast>> for Stmt<'ast> {
    fn from(payload: StmtAssert<'ast>) -> Self {
        Stmt::Assert(payload)
    }
}

/// See also [Import](https://docs.python.org/3/library/ast.html#ast.Import)
#[derive(Clone, Debug, PartialEq)]
pub struct StmtImport<'ast> {
    pub range: TextRange,
    pub names: Vec<Alias<'ast>>,
}

impl<'ast> From<StmtImport<'ast>> for Stmt<'ast> {
    fn from(payload: StmtImport<'ast>) -> Self {
        Stmt::Import(payload)
    }
}

/// See also [ImportFrom](https://docs.python.org/3/library/ast.html#ast.ImportFrom)
#[derive(Clone, Debug, PartialEq)]
pub struct StmtImportFrom<'ast> {
    pub range: TextRange,
    pub module: Option<Identifier<'ast>>,
    pub names: Vec<Alias<'ast>>,
    pub level: u32,
}

impl<'ast> From<StmtImportFrom<'ast>> for Stmt<'ast> {
    fn from(payload: StmtImportFrom<'ast>) -> Self {
        Stmt::ImportFrom(payload)
    }
}

/// See also [Global](https://docs.python.org/3/library/ast.html#ast.Global)
#[derive(Clone, Debug, PartialEq)]
pub struct StmtGlobal<'ast> {
    pub range: TextRange,
    pub names: Vec<Identifier<'ast>>,
}

impl<'ast> From<StmtGlobal<'ast>> for Stmt<'ast> {
    fn from(payload: StmtGlobal<'ast>) -> Self {
        Stmt::Global(payload)
    }
}

/// See also [Nonlocal](https://docs.python.org/3/library/ast.html#ast.Nonlocal)
#[derive(Clone, Debug, PartialEq)]
pub struct StmtNonlocal<'ast> {
    pub range: TextRange,
    pub names: Vec<Identifier<'ast>>,
}

impl<'ast> From<StmtNonlocal<'ast>> for Stmt<'ast> {
    fn from(payload: StmtNonlocal<'ast>) -> Self {
        Stmt::Nonlocal(payload)
    }
}

/// See also [Expr](https://docs.python.org/3/library/ast.html#ast.Expr)
#[derive(Debug, PartialEq)]
pub struct StmtExpr<'ast> {
    pub range: TextRange,
    pub value: Box<'ast, Expr<'ast>>,
}

impl<'ast> From<StmtExpr<'ast>> for Stmt<'ast> {
    fn from(payload: StmtExpr<'ast>) -> Self {
        Stmt::Expr(payload)
    }
}

/// See also [Pass](https://docs.python.org/3/library/ast.html#ast.Pass)
#[derive(Clone, Debug, PartialEq)]
pub struct StmtPass {
    pub range: TextRange,
}

impl From<StmtPass> for Stmt<'_> {
    fn from(payload: StmtPass) -> Self {
        Stmt::Pass(payload)
    }
}

/// See also [Break](https://docs.python.org/3/library/ast.html#ast.Break)
#[derive(Clone, Debug, PartialEq)]
pub struct StmtBreak {
    pub range: TextRange,
}

impl From<StmtBreak> for Stmt<'_> {
    fn from(payload: StmtBreak) -> Self {
        Stmt::Break(payload)
    }
}

/// See also [Continue](https://docs.python.org/3/library/ast.html#ast.Continue)
#[derive(Clone, Debug, PartialEq)]
pub struct StmtContinue {
    pub range: TextRange,
}

impl From<StmtContinue> for Stmt<'_> {
    fn from(payload: StmtContinue) -> Self {
        Stmt::Continue(payload)
    }
}

/// See also [expr](https://docs.python.org/3/library/ast.html#ast.expr)
#[derive(Debug, PartialEq, is_macro::Is)]
pub enum Expr<'ast> {
    #[is(name = "bool_op_expr")]
    BoolOp(ExprBoolOp<'ast>),
    #[is(name = "named_expr")]
    Named(ExprNamed<'ast>),
    #[is(name = "bin_op_expr")]
    BinOp(ExprBinOp<'ast>),
    #[is(name = "unary_op_expr")]
    UnaryOp(ExprUnaryOp<'ast>),
    #[is(name = "lambda_expr")]
    Lambda(ExprLambda<'ast>),
    #[is(name = "if_expr")]
    If(ExprIf<'ast>),
    #[is(name = "dict_expr")]
    Dict(ExprDict<'ast>),
    #[is(name = "set_expr")]
    Set(ExprSet<'ast>),
    #[is(name = "list_comp_expr")]
    ListComp(ExprListComp<'ast>),
    #[is(name = "set_comp_expr")]
    SetComp(ExprSetComp<'ast>),
    #[is(name = "dict_comp_expr")]
    DictComp(ExprDictComp<'ast>),
    #[is(name = "generator_expr")]
    Generator(ExprGenerator<'ast>),
    #[is(name = "await_expr")]
    Await(ExprAwait<'ast>),
    #[is(name = "yield_expr")]
    Yield(ExprYield<'ast>),
    #[is(name = "yield_from_expr")]
    YieldFrom(ExprYieldFrom<'ast>),
    #[is(name = "compare_expr")]
    Compare(ExprCompare<'ast>),
    #[is(name = "call_expr")]
    Call(ExprCall<'ast>),
    #[is(name = "f_string_expr")]
    FString(ExprFString<'ast>),
    #[is(name = "string_literal_expr")]
    StringLiteral(ExprStringLiteral<'ast>),
    #[is(name = "bytes_literal_expr")]
    BytesLiteral(ExprBytesLiteral<'ast>),
    #[is(name = "number_literal_expr")]
    NumberLiteral(ExprNumberLiteral),
    #[is(name = "boolean_literal_expr")]
    BooleanLiteral(ExprBooleanLiteral),
    #[is(name = "none_literal_expr")]
    NoneLiteral(ExprNoneLiteral),
    #[is(name = "ellipsis_literal_expr")]
    EllipsisLiteral(ExprEllipsisLiteral),
    #[is(name = "attribute_expr")]
    Attribute(ExprAttribute<'ast>),
    #[is(name = "subscript_expr")]
    Subscript(ExprSubscript<'ast>),
    #[is(name = "starred_expr")]
    Starred(ExprStarred<'ast>),
    #[is(name = "name_expr")]
    Name(ExprName<'ast>),
    #[is(name = "list_expr")]
    List(ExprList<'ast>),
    #[is(name = "tuple_expr")]
    Tuple(ExprTuple<'ast>),
    #[is(name = "slice_expr")]
    Slice(ExprSlice<'ast>),

    // Jupyter notebook specific
    #[is(name = "ipy_escape_command_expr")]
    IpyEscapeCommand(ExprIpyEscapeCommand<'ast>),
}

impl<'ast> Expr<'ast> {
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
    pub fn as_literal_expr(&self) -> Option<LiteralExpressionRef<'_, 'ast>> {
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

impl<'ast> CloneIn<'ast> for Expr<'ast> {
    fn clone_in(&self, allocator: &'ast Allocator) -> Self {
        todo!();
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
#[derive(Debug, PartialEq)]
pub struct ExprIpyEscapeCommand<'ast> {
    pub range: TextRange,
    pub kind: IpyEscapeKind,
    pub value: &'ast str,
}

impl<'ast> From<ExprIpyEscapeCommand<'ast>> for Expr<'ast> {
    fn from(payload: ExprIpyEscapeCommand<'ast>) -> Self {
        Expr::IpyEscapeCommand(payload)
    }
}

/// See also [BoolOp](https://docs.python.org/3/library/ast.html#ast.BoolOp)
#[derive(Debug, PartialEq)]
pub struct ExprBoolOp<'ast> {
    pub range: TextRange,
    pub op: BoolOp,
    pub values: Vec<Expr<'ast>>,
}

impl<'ast> From<ExprBoolOp<'ast>> for Expr<'ast> {
    fn from(payload: ExprBoolOp<'ast>) -> Self {
        Expr::BoolOp(payload)
    }
}

/// See also [NamedExpr](https://docs.python.org/3/library/ast.html#ast.NamedExpr)
#[derive(Debug, PartialEq)]
pub struct ExprNamed<'ast> {
    pub range: TextRange,
    pub target: Box<'ast, Expr<'ast>>,
    pub value: Box<'ast, Expr<'ast>>,
}

impl<'ast> From<ExprNamed<'ast>> for Expr<'ast> {
    fn from(payload: ExprNamed<'ast>) -> Self {
        Expr::Named(payload)
    }
}

/// See also [BinOp](https://docs.python.org/3/library/ast.html#ast.BinOp)
#[derive(Debug, PartialEq)]
pub struct ExprBinOp<'ast> {
    pub range: TextRange,
    pub left: Box<'ast, Expr<'ast>>,
    pub op: Operator,
    pub right: Box<'ast, Expr<'ast>>,
}

impl<'ast> From<ExprBinOp<'ast>> for Expr<'ast> {
    fn from(payload: ExprBinOp<'ast>) -> Self {
        Expr::BinOp(payload)
    }
}

/// See also [UnaryOp](https://docs.python.org/3/library/ast.html#ast.UnaryOp)
#[derive(Debug, PartialEq)]
pub struct ExprUnaryOp<'ast> {
    pub range: TextRange,
    pub op: UnaryOp,
    pub operand: Box<'ast, Expr<'ast>>,
}

impl<'ast> From<ExprUnaryOp<'ast>> for Expr<'ast> {
    fn from(payload: ExprUnaryOp<'ast>) -> Self {
        Expr::UnaryOp(payload)
    }
}

/// See also [Lambda](https://docs.python.org/3/library/ast.html#ast.Lambda)
#[derive(Debug, PartialEq)]
pub struct ExprLambda<'ast> {
    pub range: TextRange,
    pub parameters: Option<Box<'ast, Parameters<'ast>>>,
    pub body: Box<'ast, Expr<'ast>>,
}

impl<'ast> From<ExprLambda<'ast>> for Expr<'ast> {
    fn from(payload: ExprLambda<'ast>) -> Self {
        Expr::Lambda(payload)
    }
}

/// See also [IfExp](https://docs.python.org/3/library/ast.html#ast.IfExp)
#[derive(Debug, PartialEq)]
pub struct ExprIf<'ast> {
    pub range: TextRange,
    pub test: Box<'ast, Expr<'ast>>,
    pub body: Box<'ast, Expr<'ast>>,
    pub orelse: Box<'ast, Expr<'ast>>,
}

impl<'ast> From<ExprIf<'ast>> for Expr<'ast> {
    fn from(payload: ExprIf<'ast>) -> Self {
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
#[derive(Debug, PartialEq)]
pub struct DictItem<'ast> {
    pub key: Option<Expr<'ast>>,
    pub value: Expr<'ast>,
}

impl<'ast> DictItem<'ast> {
    fn key(&self) -> Option<&Expr<'ast>> {
        self.key.as_ref()
    }

    fn value(&self) -> &Expr<'ast> {
        &self.value
    }
}

impl Ranged for DictItem<'_> {
    fn range(&self) -> TextRange {
        TextRange::new(
            self.key.as_ref().map_or(self.value.start(), Ranged::start),
            self.value.end(),
        )
    }
}

/// See also [Dict](https://docs.python.org/3/library/ast.html#ast.Dict)
#[derive(Debug, PartialEq)]
pub struct ExprDict<'ast> {
    pub range: TextRange,
    pub items: Vec<DictItem<'ast>>,
}

impl<'ast> ExprDict<'ast> {
    /// Returns an `Iterator` over the AST nodes representing the
    /// dictionary's keys.
    pub fn iter_keys(&self) -> DictKeyIterator<'_, 'ast> {
        DictKeyIterator::new(&self.items)
    }

    /// Returns an `Iterator` over the AST nodes representing the
    /// dictionary's values.
    pub fn iter_values(&self) -> DictValueIterator<'_, 'ast> {
        DictValueIterator::new(&self.items)
    }

    /// Returns the AST node representing the *n*th key of this
    /// dictionary.
    ///
    /// Panics: If the index `n` is out of bounds.
    pub fn key(&self, n: usize) -> Option<&Expr<'ast>> {
        self.items[n].key()
    }

    /// Returns the AST node representing the *n*th value of this
    /// dictionary.
    ///
    /// Panics: If the index `n` is out of bounds.
    pub fn value(&self, n: usize) -> &Expr<'ast> {
        self.items[n].value()
    }
}

impl<'ast> From<ExprDict<'ast>> for Expr<'ast> {
    fn from(payload: ExprDict<'ast>) -> Self {
        Expr::Dict(payload)
    }
}

#[derive(Debug, Clone)]
pub struct DictKeyIterator<'a, 'ast> {
    items: Iter<'a, DictItem<'ast>>,
}

impl<'a, 'ast> DictKeyIterator<'a, 'ast> {
    fn new(items: &'a [DictItem<'ast>]) -> Self {
        Self {
            items: items.iter(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl<'a, 'ast> Iterator for DictKeyIterator<'a, 'ast> {
    type Item = Option<&'a Expr<'ast>>;

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

impl DoubleEndedIterator for DictKeyIterator<'_, '_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.items.next_back().map(DictItem::key)
    }
}

impl FusedIterator for DictKeyIterator<'_, '_> {}
impl ExactSizeIterator for DictKeyIterator<'_, '_> {}

#[derive(Debug, Clone)]
pub struct DictValueIterator<'a, 'ast> {
    items: Iter<'a, DictItem<'ast>>,
}

impl<'a, 'ast> DictValueIterator<'a, 'ast> {
    fn new(items: &'a [DictItem<'ast>]) -> Self {
        Self {
            items: items.iter(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl<'a, 'ast> Iterator for DictValueIterator<'a, 'ast> {
    type Item = &'a Expr<'ast>;

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

impl DoubleEndedIterator for DictValueIterator<'_, '_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.items.next_back().map(DictItem::value)
    }
}

impl FusedIterator for DictValueIterator<'_, '_> {}
impl ExactSizeIterator for DictValueIterator<'_, '_> {}

/// See also [Set](https://docs.python.org/3/library/ast.html#ast.Set)
#[derive(Debug, PartialEq)]
pub struct ExprSet<'ast> {
    pub range: TextRange,
    pub elts: Vec<Expr<'ast>>,
}

impl<'ast> From<ExprSet<'ast>> for Expr<'ast> {
    fn from(payload: ExprSet<'ast>) -> Self {
        Expr::Set(payload)
    }
}

/// See also [ListComp](https://docs.python.org/3/library/ast.html#ast.ListComp)
#[derive(Debug, PartialEq)]
pub struct ExprListComp<'ast> {
    pub range: TextRange,
    pub elt: Box<'ast, Expr<'ast>>,
    pub generators: Vec<Comprehension<'ast>>,
}

impl<'ast> From<ExprListComp<'ast>> for Expr<'ast> {
    fn from(payload: ExprListComp<'ast>) -> Self {
        Expr::ListComp(payload)
    }
}

/// See also [SetComp](https://docs.python.org/3/library/ast.html#ast.SetComp)
#[derive(Debug, PartialEq)]
pub struct ExprSetComp<'ast> {
    pub range: TextRange,
    pub elt: Box<'ast, Expr<'ast>>,
    pub generators: Vec<Comprehension<'ast>>,
}

impl<'ast> From<ExprSetComp<'ast>> for Expr<'ast> {
    fn from(payload: ExprSetComp<'ast>) -> Self {
        Expr::SetComp(payload)
    }
}

/// See also [DictComp](https://docs.python.org/3/library/ast.html#ast.DictComp)
#[derive(Debug, PartialEq)]
pub struct ExprDictComp<'ast> {
    pub range: TextRange,
    pub key: Box<'ast, Expr<'ast>>,
    pub value: Box<'ast, Expr<'ast>>,
    pub generators: Vec<Comprehension<'ast>>,
}

impl<'ast> From<ExprDictComp<'ast>> for Expr<'ast> {
    fn from(payload: ExprDictComp<'ast>) -> Self {
        Expr::DictComp(payload)
    }
}

/// See also [GeneratorExp](https://docs.python.org/3/library/ast.html#ast.GeneratorExp)
#[derive(Debug, PartialEq)]
pub struct ExprGenerator<'ast> {
    pub range: TextRange,
    pub elt: Box<'ast, Expr<'ast>>,
    pub generators: Vec<Comprehension<'ast>>,
    pub parenthesized: bool,
}

impl<'ast> From<ExprGenerator<'ast>> for Expr<'ast> {
    fn from(payload: ExprGenerator<'ast>) -> Self {
        Expr::Generator(payload)
    }
}

/// See also [Await](https://docs.python.org/3/library/ast.html#ast.Await)
#[derive(Debug, PartialEq)]
pub struct ExprAwait<'ast> {
    pub range: TextRange,
    pub value: Box<'ast, Expr<'ast>>,
}

impl<'ast> From<ExprAwait<'ast>> for Expr<'ast> {
    fn from(payload: ExprAwait<'ast>) -> Self {
        Expr::Await(payload)
    }
}

/// See also [Yield](https://docs.python.org/3/library/ast.html#ast.Yield)
#[derive(Debug, PartialEq)]
pub struct ExprYield<'ast> {
    pub range: TextRange,
    pub value: Option<Box<'ast, Expr<'ast>>>,
}

impl<'ast> From<ExprYield<'ast>> for Expr<'ast> {
    fn from(payload: ExprYield<'ast>) -> Self {
        Expr::Yield(payload)
    }
}

/// See also [YieldFrom](https://docs.python.org/3/library/ast.html#ast.YieldFrom)
#[derive(Debug, PartialEq)]
pub struct ExprYieldFrom<'ast> {
    pub range: TextRange,
    pub value: Box<'ast, Expr<'ast>>,
}

impl<'ast> From<ExprYieldFrom<'ast>> for Expr<'ast> {
    fn from(payload: ExprYieldFrom<'ast>) -> Self {
        Expr::YieldFrom(payload)
    }
}

/// See also [Compare](https://docs.python.org/3/library/ast.html#ast.Compare)
#[derive(Debug, PartialEq)]
pub struct ExprCompare<'ast> {
    pub range: TextRange,
    pub left: Box<'ast, Expr<'ast>>,
    pub ops: &'ast mut [CmpOp],
    pub comparators: &'ast mut [Expr<'ast>],
}

impl<'ast> From<ExprCompare<'ast>> for Expr<'ast> {
    fn from(payload: ExprCompare<'ast>) -> Self {
        Expr::Compare(payload)
    }
}

/// See also [Call](https://docs.python.org/3/library/ast.html#ast.Call)
#[derive(Debug, PartialEq)]
pub struct ExprCall<'ast> {
    pub range: TextRange,
    pub func: Box<'ast, Expr<'ast>>,
    pub arguments: Arguments<'ast>,
}

impl<'ast> From<ExprCall<'ast>> for Expr<'ast> {
    fn from(payload: ExprCall<'ast>) -> Self {
        Expr::Call(payload)
    }
}

#[derive(Debug, PartialEq)]
pub struct FStringFormatSpec<'ast> {
    pub range: TextRange,
    pub elements: FStringElements<'ast>,
}

impl Ranged for FStringFormatSpec<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}

/// See also [FormattedValue](https://docs.python.org/3/library/ast.html#ast.FormattedValue)
#[derive(Debug, PartialEq)]
pub struct FStringExpressionElement<'ast> {
    pub range: TextRange,
    pub expression: Box<'ast, Expr<'ast>>,
    pub debug_text: Option<DebugText>,
    pub conversion: ConversionFlag,
    pub format_spec: Option<Box<'ast, FStringFormatSpec<'ast>>>,
}

impl Ranged for FStringExpressionElement<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}

/// An `FStringLiteralElement` with an empty `value` is an invalid f-string element.
#[derive(Debug, PartialEq)]
pub struct FStringLiteralElement<'ast> {
    pub range: TextRange,
    pub value: &'ast str,
}

impl FStringLiteralElement<'_> {
    pub fn is_valid(&self) -> bool {
        !self.value.is_empty()
    }
}

impl Ranged for FStringLiteralElement<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}

impl Deref for FStringLiteralElement<'_> {
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

#[derive(Debug, PartialEq, Eq, Hash)]
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
#[derive(Debug, PartialEq)]
pub struct ExprFString<'ast> {
    pub range: TextRange,
    pub value: FStringValue<'ast>,
}

impl<'ast> From<ExprFString<'ast>> for Expr<'ast> {
    fn from(payload: ExprFString<'ast>) -> Self {
        Expr::FString(payload)
    }
}

/// The value representing an [`ExprFString`].
#[derive(Debug, PartialEq)]
pub struct FStringValue<'ast> {
    inner: FStringValueInner<'ast>,
}

impl<'ast> FStringValue<'ast> {
    /// Creates a new f-string with the given value.
    pub fn single(value: FString<'ast>) -> Self {
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
    pub fn concatenated(values: Vec<FStringPart<'ast>>) -> Self {
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
    pub fn as_slice(&self) -> &[FStringPart<'ast>] {
        match &self.inner {
            FStringValueInner::Single(part) => std::slice::from_ref(part),
            FStringValueInner::Concatenated(parts) => parts,
        }
    }

    /// Returns a mutable slice of all the [`FStringPart`]s contained in this value.
    fn as_mut_slice(&mut self) -> &mut [FStringPart<'ast>] {
        match &mut self.inner {
            FStringValueInner::Single(part) => std::slice::from_mut(part),
            FStringValueInner::Concatenated(parts) => parts,
        }
    }

    /// Returns an iterator over all the [`FStringPart`]s contained in this value.
    pub fn iter(&self) -> Iter<FStringPart<'ast>> {
        self.as_slice().iter()
    }

    /// Returns an iterator over all the [`FStringPart`]s contained in this value
    /// that allows modification.
    pub(crate) fn iter_mut(&mut self) -> IterMut<FStringPart<'ast>> {
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
    pub fn literals(&self) -> impl Iterator<Item = &StringLiteral<'ast>> {
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
    pub fn f_strings(&self) -> impl Iterator<Item = &FString<'ast>> {
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
    pub fn elements(&self) -> impl Iterator<Item = &FStringElement<'ast>> {
        self.f_strings().flat_map(|fstring| fstring.elements.iter())
    }
}

impl<'a, 'ast> IntoIterator for &'a FStringValue<'ast> {
    type Item = &'a FStringPart<'ast>;
    type IntoIter = Iter<'a, FStringPart<'ast>>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a, 'ast> IntoIterator for &'a mut FStringValue<'ast> {
    type Item = &'a mut FStringPart<'ast>;
    type IntoIter = IterMut<'a, FStringPart<'ast>>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

/// An internal representation of [`FStringValue`].
#[derive(Debug, PartialEq)]
enum FStringValueInner<'ast> {
    /// A single f-string i.e., `f"foo"`.
    ///
    /// This is always going to be `FStringPart::FString` variant which is
    /// maintained by the `FStringValue::single` constructor.
    Single(FStringPart<'ast>),

    /// An implicitly concatenated f-string i.e., `"foo" f"bar {x}"`.
    Concatenated(Vec<FStringPart<'ast>>),
}

/// An f-string part which is either a string literal or an f-string.
#[derive(Debug, PartialEq, is_macro::Is)]
pub enum FStringPart<'ast> {
    Literal(StringLiteral<'ast>),
    FString(FString<'ast>),
}

impl FStringPart<'_> {
    pub fn quote_style(&self) -> Quote {
        match self {
            Self::Literal(string_literal) => string_literal.flags.quote_style(),
            Self::FString(f_string) => f_string.flags.quote_style(),
        }
    }
}

impl Ranged for FStringPart<'_> {
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
    #[derive(Default, Copy,  Clone, PartialEq, Eq, Hash)]
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
#[derive(Debug, PartialEq)]
pub struct FString<'ast> {
    pub range: TextRange,
    pub elements: FStringElements<'ast>,
    pub flags: FStringFlags,
}

impl Ranged for FString<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}

impl<'ast> From<FString<'ast>> for Expr<'ast> {
    fn from(payload: FString<'ast>) -> Self {
        ExprFString {
            range: payload.range,
            value: FStringValue::single(payload),
        }
        .into()
    }
}

/// A newtype wrapper around a list of [`FStringElement`].
#[derive(Default, PartialEq)]
pub struct FStringElements<'ast>(Vec<FStringElement<'ast>>);

impl<'ast> FStringElements<'ast> {
    /// Returns an iterator over all the [`FStringLiteralElement`] nodes contained in this f-string.
    pub fn literals(&self) -> impl Iterator<Item = &FStringLiteralElement<'ast>> {
        self.iter().filter_map(|element| element.as_literal())
    }

    /// Returns an iterator over all the [`FStringExpressionElement`] nodes contained in this f-string.
    pub fn expressions(&self) -> impl Iterator<Item = &FStringExpressionElement<'ast>> {
        self.iter().filter_map(|element| element.as_expression())
    }
}

impl<'ast> From<Vec<FStringElement<'ast>>> for FStringElements<'ast> {
    fn from(elements: Vec<FStringElement<'ast>>) -> Self {
        FStringElements(elements)
    }
}

impl<'a, 'ast> IntoIterator for &'a FStringElements<'ast> {
    type Item = &'a FStringElement<'ast>;
    type IntoIter = Iter<'a, FStringElement<'ast>>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a, 'ast> IntoIterator for &'a mut FStringElements<'ast> {
    type Item = &'a mut FStringElement<'ast>;
    type IntoIter = IterMut<'a, FStringElement<'ast>>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

impl<'ast> Deref for FStringElements<'ast> {
    type Target = [FStringElement<'ast>];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for FStringElements<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl fmt::Debug for FStringElements<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

#[derive(Debug, PartialEq, is_macro::Is)]
pub enum FStringElement<'ast> {
    Literal(FStringLiteralElement<'ast>),
    Expression(FStringExpressionElement<'ast>),
}

impl Ranged for FStringElement<'_> {
    fn range(&self) -> TextRange {
        match self {
            FStringElement::Literal(node) => node.range(),
            FStringElement::Expression(node) => node.range(),
        }
    }
}

/// An AST node that represents either a single string literal or an implicitly
/// concatenated string literals.
#[derive(Debug, Default, PartialEq)]
pub struct ExprStringLiteral<'ast> {
    pub range: TextRange,
    pub value: StringLiteralValue<'ast>,
}

impl<'ast> From<ExprStringLiteral<'ast>> for Expr<'ast> {
    fn from(payload: ExprStringLiteral<'ast>) -> Self {
        Expr::StringLiteral(payload)
    }
}

impl Ranged for ExprStringLiteral<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}

/// The value representing a [`ExprStringLiteral`].
#[derive(Debug, Default, PartialEq)]
pub struct StringLiteralValue<'ast> {
    inner: StringLiteralValueInner<'ast>,
}

impl<'ast> StringLiteralValue<'ast> {
    /// Creates a new single string literal with the given value.
    pub fn single(string: StringLiteral<'ast>) -> Self {
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
    pub fn concatenated(strings: Vec<StringLiteral<'ast>>) -> Self {
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
    pub fn as_slice(&self) -> &[StringLiteral<'ast>] {
        match &self.inner {
            StringLiteralValueInner::Single(value) => std::slice::from_ref(value),
            StringLiteralValueInner::Concatenated(value) => value.strings.as_slice(),
        }
    }

    /// Returns a mutable slice of all the [`StringLiteral`] parts contained in this value.
    fn as_mut_slice(&mut self) -> &mut [StringLiteral<'ast>] {
        match &mut self.inner {
            StringLiteralValueInner::Single(value) => std::slice::from_mut(value),
            StringLiteralValueInner::Concatenated(value) => value.strings.as_mut_slice(),
        }
    }

    /// Returns an iterator over all the [`StringLiteral`] parts contained in this value.
    pub fn iter(&self) -> Iter<StringLiteral<'ast>> {
        self.as_slice().iter()
    }

    /// Returns an iterator over all the [`StringLiteral`] parts contained in this value
    /// that allows modification.
    pub(crate) fn iter_mut(&mut self) -> IterMut<StringLiteral<'ast>> {
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

impl<'a, 'ast> IntoIterator for &'a StringLiteralValue<'ast> {
    type Item = &'a StringLiteral<'ast>;
    type IntoIter = Iter<'a, StringLiteral<'ast>>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a, 'ast> IntoIterator for &'a mut StringLiteralValue<'ast> {
    type Item = &'a mut StringLiteral<'ast>;
    type IntoIter = IterMut<'a, StringLiteral<'ast>>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

impl PartialEq<str> for StringLiteralValue<'_> {
    fn eq(&self, other: &str) -> bool {
        if self.len() != other.len() {
            return false;
        }
        // The `zip` here is safe because we have checked the length of both parts.
        self.chars().zip(other.chars()).all(|(c1, c2)| c1 == c2)
    }
}

impl fmt::Display for StringLiteralValue<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.to_str())
    }
}

/// An internal representation of [`StringLiteralValue`].
#[derive(Debug, PartialEq)]
enum StringLiteralValueInner<'ast> {
    /// A single string literal i.e., `"foo"`.
    Single(StringLiteral<'ast>),

    /// An implicitly concatenated string literals i.e., `"foo" "bar"`.
    Concatenated(ConcatenatedStringLiteral<'ast>),
}

impl Default for StringLiteralValueInner<'_> {
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
#[derive(Default, Debug, PartialEq)]
pub struct StringLiteral<'ast> {
    pub range: TextRange,
    pub value: &'ast str,
    pub flags: StringLiteralFlags,
}

impl Ranged for StringLiteral<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}

impl Deref for StringLiteral<'_> {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<'ast> StringLiteral<'ast> {
    /// Extracts a string slice containing the entire `String`.
    pub fn as_str(&self) -> &str {
        self
    }

    /// Creates an invalid string literal with the given range.
    pub fn invalid(range: TextRange) -> Self {
        Self {
            range,
            value: Default::default(),
            flags: StringLiteralFlags::default().with_invalid(),
        }
    }
}

impl<'ast> From<StringLiteral<'ast>> for Expr<'ast> {
    fn from(payload: StringLiteral<'ast>) -> Self {
        ExprStringLiteral {
            range: payload.range,
            value: StringLiteralValue::single(payload),
        }
        .into()
    }
}

/// An internal representation of [`StringLiteral`] that represents an
/// implicitly concatenated string.
struct ConcatenatedStringLiteral<'ast> {
    /// Each string literal that makes up the concatenated string.
    strings: Vec<StringLiteral<'ast>>,
    /// The concatenated string value.
    value: OnceLock<std::boxed::Box<str>>,
}

impl<'ast> ConcatenatedStringLiteral<'ast> {
    /// Extracts a string slice containing the entire concatenated string.
    fn to_str(&self) -> &str {
        self.value.get_or_init(|| {
            let concatenated: String = self.strings.iter().map(StringLiteral::as_str).collect();
            std::boxed::Box::from(concatenated)
        })
    }
}

impl PartialEq for ConcatenatedStringLiteral<'_> {
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

impl Debug for ConcatenatedStringLiteral<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ConcatenatedStringLiteral")
            .field("strings", &self.strings)
            .field("value", &self.to_str())
            .finish()
    }
}

/// An AST node that represents either a single bytes literal or an implicitly
/// concatenated bytes literals.
#[derive(Debug, Default, PartialEq)]
pub struct ExprBytesLiteral<'ast> {
    pub range: TextRange,
    pub value: BytesLiteralValue<'ast>,
}

impl<'ast> From<ExprBytesLiteral<'ast>> for Expr<'ast> {
    fn from(payload: ExprBytesLiteral<'ast>) -> Self {
        Expr::BytesLiteral(payload)
    }
}

impl Ranged for ExprBytesLiteral<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}

/// The value representing a [`ExprBytesLiteral`].
#[derive(Debug, Default, PartialEq)]
pub struct BytesLiteralValue<'ast> {
    inner: BytesLiteralValueInner<'ast>,
}

impl<'ast> BytesLiteralValue<'ast> {
    /// Creates a new single bytes literal with the given value.
    pub fn single(value: BytesLiteral<'ast>) -> Self {
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
    pub fn concatenated(values: Vec<BytesLiteral<'ast>>) -> Self {
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
    pub fn as_slice(&self) -> &[BytesLiteral<'ast>] {
        match &self.inner {
            BytesLiteralValueInner::Single(value) => std::slice::from_ref(value),
            BytesLiteralValueInner::Concatenated(value) => value.as_slice(),
        }
    }

    /// Returns a mutable slice of all the [`BytesLiteral`] parts contained in this value.
    fn as_mut_slice(&mut self) -> &mut [BytesLiteral<'ast>] {
        match &mut self.inner {
            BytesLiteralValueInner::Single(value) => std::slice::from_mut(value),
            BytesLiteralValueInner::Concatenated(value) => value.as_mut_slice(),
        }
    }

    /// Returns an iterator over all the [`BytesLiteral`] parts contained in this value.
    pub fn iter(&self) -> Iter<BytesLiteral<'ast>> {
        self.as_slice().iter()
    }

    /// Returns an iterator over all the [`BytesLiteral`] parts contained in this value
    /// that allows modification.
    pub(crate) fn iter_mut(&mut self) -> IterMut<BytesLiteral<'ast>> {
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
    fn bytes(&self) -> impl Iterator<Item = u8> + '_ {
        self.iter().flat_map(|part| part.as_slice().iter().copied())
    }
}

impl<'a, 'ast> IntoIterator for &'a BytesLiteralValue<'ast> {
    type Item = &'a BytesLiteral<'ast>;
    type IntoIter = Iter<'a, BytesLiteral<'ast>>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a, 'ast> IntoIterator for &'a mut BytesLiteralValue<'ast> {
    type Item = &'a mut BytesLiteral<'ast>;
    type IntoIter = IterMut<'a, BytesLiteral<'ast>>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

impl PartialEq<[u8]> for BytesLiteralValue<'_> {
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
#[derive(Debug, PartialEq)]
enum BytesLiteralValueInner<'ast> {
    /// A single bytes literal i.e., `b"foo"`.
    Single(BytesLiteral<'ast>),

    /// An implicitly concatenated bytes literals i.e., `b"foo" b"bar"`.
    Concatenated(Vec<BytesLiteral<'ast>>),
}

impl Default for BytesLiteralValueInner<'_> {
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
#[derive(Default, Debug, PartialEq)]
pub struct BytesLiteral<'ast> {
    pub range: TextRange,
    pub value: &'ast [u8],
    pub flags: BytesLiteralFlags,
}

impl Ranged for BytesLiteral<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}

impl Deref for BytesLiteral<'_> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<'ast> BytesLiteral<'ast> {
    /// Extracts a byte slice containing the entire [`BytesLiteral`].
    pub fn as_slice(&self) -> &[u8] {
        self
    }

    /// Creates a new invalid bytes literal with the given range.
    pub fn invalid(range: TextRange) -> Self {
        Self {
            range,
            value: Default::default(),
            flags: BytesLiteralFlags::default().with_invalid(),
        }
    }
}

impl<'ast> From<BytesLiteral<'ast>> for Expr<'ast> {
    fn from(payload: BytesLiteral<'ast>) -> Self {
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

impl From<ExprNumberLiteral> for Expr<'_> {
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

impl From<ExprBooleanLiteral> for Expr<'_> {
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

impl From<ExprNoneLiteral> for Expr<'_> {
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

impl From<ExprEllipsisLiteral> for Expr<'_> {
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
#[derive(Debug, PartialEq)]
pub struct ExprAttribute<'ast> {
    pub range: TextRange,
    pub value: Box<'ast, Expr<'ast>>,
    pub attr: Identifier<'ast>,
    pub ctx: ExprContext,
}

impl<'ast> From<ExprAttribute<'ast>> for Expr<'ast> {
    fn from(payload: ExprAttribute<'ast>) -> Self {
        Expr::Attribute(payload)
    }
}

/// See also [Subscript](https://docs.python.org/3/library/ast.html#ast.Subscript)
#[derive(Debug, PartialEq)]
pub struct ExprSubscript<'ast> {
    pub range: TextRange,
    pub value: Box<'ast, Expr<'ast>>,
    pub slice: Box<'ast, Expr<'ast>>,
    pub ctx: ExprContext,
}

impl<'ast> From<ExprSubscript<'ast>> for Expr<'ast> {
    fn from(payload: ExprSubscript<'ast>) -> Self {
        Expr::Subscript(payload)
    }
}

/// See also [Starred](https://docs.python.org/3/library/ast.html#ast.Starred)
#[derive(Debug, PartialEq)]
pub struct ExprStarred<'ast> {
    pub range: TextRange,
    pub value: Box<'ast, Expr<'ast>>,
    pub ctx: ExprContext,
}

impl<'ast> From<ExprStarred<'ast>> for Expr<'ast> {
    fn from(payload: ExprStarred<'ast>) -> Self {
        Expr::Starred(payload)
    }
}

/// See also [Name](https://docs.python.org/3/library/ast.html#ast.Name)
#[derive(Clone, Debug, PartialEq)]
pub struct ExprName<'ast> {
    pub range: TextRange,
    pub id: &'ast str,
    pub ctx: ExprContext,
}

impl<'ast> ExprName<'ast> {
    pub fn id(&self) -> &'ast str {
        &self.id
    }
}

impl<'ast> From<ExprName<'ast>> for Expr<'ast> {
    fn from(payload: ExprName<'ast>) -> Self {
        Expr::Name(payload)
    }
}

/// See also [List](https://docs.python.org/3/library/ast.html#ast.List)
#[derive(Debug, PartialEq)]
pub struct ExprList<'ast> {
    pub range: TextRange,
    pub elts: Vec<Expr<'ast>>,
    pub ctx: ExprContext,
}

impl<'ast> From<ExprList<'ast>> for Expr<'ast> {
    fn from(payload: ExprList<'ast>) -> Self {
        Expr::List(payload)
    }
}

/// See also [Tuple](https://docs.python.org/3/library/ast.html#ast.Tuple)
#[derive(Debug, PartialEq)]
pub struct ExprTuple<'ast> {
    pub range: TextRange,
    pub elts: Vec<Expr<'ast>>,
    pub ctx: ExprContext,

    /// Whether the tuple is parenthesized in the source code.
    pub parenthesized: bool,
}

impl<'ast> From<ExprTuple<'ast>> for Expr<'ast> {
    fn from(payload: ExprTuple<'ast>) -> Self {
        Expr::Tuple(payload)
    }
}

/// See also [Slice](https://docs.python.org/3/library/ast.html#ast.Slice)
#[derive(Debug, PartialEq)]
pub struct ExprSlice<'ast> {
    pub range: TextRange,
    pub lower: Option<Box<'ast, Expr<'ast>>>,
    pub upper: Option<Box<'ast, Expr<'ast>>>,
    pub step: Option<Box<'ast, Expr<'ast>>>,
}

impl<'ast> From<ExprSlice<'ast>> for Expr<'ast> {
    fn from(payload: ExprSlice<'ast>) -> Self {
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
#[derive(Debug, PartialEq)]
pub struct Comprehension<'ast> {
    pub range: TextRange,
    pub target: Expr<'ast>,
    pub iter: Expr<'ast>,
    pub ifs: Vec<Expr<'ast>>,
    pub is_async: bool,
}

/// See also [excepthandler](https://docs.python.org/3/library/ast.html#ast.excepthandler)
#[derive(Debug, PartialEq, is_macro::Is)]
pub enum ExceptHandler<'ast> {
    ExceptHandler(ExceptHandlerExceptHandler<'ast>),
}

/// See also [ExceptHandler](https://docs.python.org/3/library/ast.html#ast.ExceptHandler)
#[derive(Debug, PartialEq)]
pub struct ExceptHandlerExceptHandler<'ast> {
    pub range: TextRange,
    pub type_: Option<Box<'ast, Expr<'ast>>>,
    pub name: Option<Identifier<'ast>>,
    pub body: Vec<Stmt<'ast>>,
}

impl<'ast> From<ExceptHandlerExceptHandler<'ast>> for ExceptHandler<'ast> {
    fn from(payload: ExceptHandlerExceptHandler<'ast>) -> Self {
        ExceptHandler::ExceptHandler(payload)
    }
}

/// See also [arg](https://docs.python.org/3/library/ast.html#ast.arg)
#[derive(Debug, PartialEq)]
pub struct Parameter<'ast> {
    pub range: TextRange,
    pub name: Identifier<'ast>,
    pub annotation: Option<Box<'ast, Expr<'ast>>>,
}

/// See also [keyword](https://docs.python.org/3/library/ast.html#ast.keyword)
#[derive(Debug, PartialEq)]
pub struct Keyword<'ast> {
    pub range: TextRange,
    pub arg: Option<Identifier<'ast>>,
    pub value: Expr<'ast>,
}

/// See also [alias](https://docs.python.org/3/library/ast.html#ast.alias)
#[derive(Clone, Debug, PartialEq)]
pub struct Alias<'ast> {
    pub range: TextRange,
    pub name: Identifier<'ast>,
    pub asname: Option<Identifier<'ast>>,
}

/// See also [withitem](https://docs.python.org/3/library/ast.html#ast.withitem)
#[derive(Debug, PartialEq)]
pub struct WithItem<'ast> {
    pub range: TextRange,
    pub context_expr: Expr<'ast>,
    pub optional_vars: Option<Box<'ast, Expr<'ast>>>,
}

/// See also [match_case](https://docs.python.org/3/library/ast.html#ast.match_case)
#[derive(Debug, PartialEq)]
pub struct MatchCase<'ast> {
    pub range: TextRange,
    pub pattern: Pattern<'ast>,
    pub guard: Option<Box<'ast, Expr<'ast>>>,
    pub body: Vec<Stmt<'ast>>,
}

/// See also [pattern](https://docs.python.org/3/library/ast.html#ast.pattern)
#[derive(Debug, PartialEq, is_macro::Is)]
pub enum Pattern<'ast> {
    MatchValue(PatternMatchValue<'ast>),
    MatchSingleton(PatternMatchSingleton),
    MatchSequence(PatternMatchSequence<'ast>),
    MatchMapping(PatternMatchMapping<'ast>),
    MatchClass(PatternMatchClass<'ast>),
    MatchStar(PatternMatchStar<'ast>),
    MatchAs(PatternMatchAs<'ast>),
    MatchOr(PatternMatchOr<'ast>),
}

impl Pattern<'_> {
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
}

/// See also [MatchValue](https://docs.python.org/3/library/ast.html#ast.MatchValue)
#[derive(Debug, PartialEq)]
pub struct PatternMatchValue<'ast> {
    pub range: TextRange,
    pub value: Box<'ast, Expr<'ast>>,
}

impl<'ast> From<PatternMatchValue<'ast>> for Pattern<'ast> {
    fn from(payload: PatternMatchValue<'ast>) -> Self {
        Pattern::MatchValue(payload)
    }
}

/// See also [MatchSingleton](https://docs.python.org/3/library/ast.html#ast.MatchSingleton)
#[derive(Debug, PartialEq)]
pub struct PatternMatchSingleton {
    pub range: TextRange,
    pub value: Singleton,
}

impl From<PatternMatchSingleton> for Pattern<'_> {
    fn from(payload: PatternMatchSingleton) -> Self {
        Pattern::MatchSingleton(payload)
    }
}

/// See also [MatchSequence](https://docs.python.org/3/library/ast.html#ast.MatchSequence)
#[derive(Debug, PartialEq)]
pub struct PatternMatchSequence<'ast> {
    pub range: TextRange,
    pub patterns: Vec<Pattern<'ast>>,
}

impl<'ast> From<PatternMatchSequence<'ast>> for Pattern<'ast> {
    fn from(payload: PatternMatchSequence<'ast>) -> Self {
        Pattern::MatchSequence(payload)
    }
}

/// See also [MatchMapping](https://docs.python.org/3/library/ast.html#ast.MatchMapping)
#[derive(Debug, PartialEq)]
pub struct PatternMatchMapping<'ast> {
    pub range: TextRange,
    pub keys: Vec<Expr<'ast>>,
    pub patterns: Vec<Pattern<'ast>>,
    pub rest: Option<Identifier<'ast>>,
}

impl<'ast> From<PatternMatchMapping<'ast>> for Pattern<'ast> {
    fn from(payload: PatternMatchMapping<'ast>) -> Self {
        Pattern::MatchMapping(payload)
    }
}

/// See also [MatchClass](https://docs.python.org/3/library/ast.html#ast.MatchClass)
#[derive(Debug, PartialEq)]
pub struct PatternMatchClass<'ast> {
    pub range: TextRange,
    pub cls: Box<'ast, Expr<'ast>>,
    pub arguments: PatternArguments<'ast>,
}

impl<'ast> From<PatternMatchClass<'ast>> for Pattern<'ast> {
    fn from(payload: PatternMatchClass<'ast>) -> Self {
        Pattern::MatchClass(payload)
    }
}

/// An AST node to represent the arguments to a [`PatternMatchClass`], i.e., the
/// parenthesized contents in `case Point(1, x=0, y=0)`.
///
/// Like [`Arguments`], but for [`PatternMatchClass`].
#[derive(Debug, PartialEq)]
pub struct PatternArguments<'ast> {
    pub range: TextRange,
    pub patterns: Vec<Pattern<'ast>>,
    pub keywords: Vec<PatternKeyword<'ast>>,
}

/// An AST node to represent the keyword arguments to a [`PatternMatchClass`], i.e., the
/// `x=0` and `y=0` in `case Point(x=0, y=0)`.
///
/// Like [`Keyword`], but for [`PatternMatchClass`].
#[derive(Debug, PartialEq)]
pub struct PatternKeyword<'ast> {
    pub range: TextRange,
    pub attr: Identifier<'ast>,
    pub pattern: Pattern<'ast>,
}

/// See also [MatchStar](https://docs.python.org/3/library/ast.html#ast.MatchStar)
#[derive(Debug, PartialEq)]
pub struct PatternMatchStar<'ast> {
    pub range: TextRange,
    pub name: Option<Identifier<'ast>>,
}

impl<'ast> From<PatternMatchStar<'ast>> for Pattern<'ast> {
    fn from(payload: PatternMatchStar<'ast>) -> Self {
        Pattern::MatchStar(payload)
    }
}

/// See also [MatchAs](https://docs.python.org/3/library/ast.html#ast.MatchAs)
#[derive(Debug, PartialEq)]
pub struct PatternMatchAs<'ast> {
    pub range: TextRange,
    pub pattern: Option<Box<'ast, Pattern<'ast>>>,
    pub name: Option<Identifier<'ast>>,
}

impl<'ast> From<PatternMatchAs<'ast>> for Pattern<'ast> {
    fn from(payload: PatternMatchAs<'ast>) -> Self {
        Pattern::MatchAs(payload)
    }
}

/// See also [MatchOr](https://docs.python.org/3/library/ast.html#ast.MatchOr)
#[derive(Debug, PartialEq)]
pub struct PatternMatchOr<'ast> {
    pub range: TextRange,
    pub patterns: Vec<Pattern<'ast>>,
}

impl<'ast> From<PatternMatchOr<'ast>> for Pattern<'ast> {
    fn from(payload: PatternMatchOr<'ast>) -> Self {
        Pattern::MatchOr(payload)
    }
}

/// See also [type_param](https://docs.python.org/3/library/ast.html#ast.type_param)
#[derive(Debug, PartialEq, is_macro::Is)]
pub enum TypeParam<'ast> {
    TypeVar(TypeParamTypeVar<'ast>),
    ParamSpec(TypeParamParamSpec<'ast>),
    TypeVarTuple(TypeParamTypeVarTuple<'ast>),
}

/// See also [TypeVar](https://docs.python.org/3/library/ast.html#ast.TypeVar)
#[derive(Debug, PartialEq)]
pub struct TypeParamTypeVar<'ast> {
    pub range: TextRange,
    pub name: Identifier<'ast>,
    pub bound: Option<Box<'ast, Expr<'ast>>>,
    pub default: Option<Box<'ast, Expr<'ast>>>,
}

impl<'ast> From<TypeParamTypeVar<'ast>> for TypeParam<'ast> {
    fn from(payload: TypeParamTypeVar<'ast>) -> Self {
        TypeParam::TypeVar(payload)
    }
}

/// See also [ParamSpec](https://docs.python.org/3/library/ast.html#ast.ParamSpec)
#[derive(Debug, PartialEq)]
pub struct TypeParamParamSpec<'ast> {
    pub range: TextRange,
    pub name: Identifier<'ast>,
    pub default: Option<Box<'ast, Expr<'ast>>>,
}

impl<'ast> From<TypeParamParamSpec<'ast>> for TypeParam<'ast> {
    fn from(payload: TypeParamParamSpec<'ast>) -> Self {
        TypeParam::ParamSpec(payload)
    }
}

/// See also [TypeVarTuple](https://docs.python.org/3/library/ast.html#ast.TypeVarTuple)
#[derive(Debug, PartialEq)]
pub struct TypeParamTypeVarTuple<'ast> {
    pub range: TextRange,
    pub name: Identifier<'ast>,
    pub default: Option<Box<'ast, Expr<'ast>>>,
}

impl<'ast> From<TypeParamTypeVarTuple<'ast>> for TypeParam<'ast> {
    fn from(payload: TypeParamTypeVarTuple<'ast>) -> Self {
        TypeParam::TypeVarTuple(payload)
    }
}

/// See also [decorator](https://docs.python.org/3/library/ast.html#ast.decorator)
#[derive(Debug, PartialEq)]
pub struct Decorator<'ast> {
    pub range: TextRange,
    pub expression: Expr<'ast>,
}

/// Enumeration of the two kinds of parameter
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum AnyParameterRef<'a, 'ast> {
    /// Variadic parameters cannot have default values,
    /// e.g. both `*args` and `**kwargs` in the following function:
    ///
    /// ```python
    /// def foo(*args, **kwargs): pass
    /// ```
    Variadic(&'a Parameter<'ast>),

    /// Non-variadic parameters can have default values,
    /// though they won't necessarily always have them:
    ///
    /// ```python
    /// def bar(a=1, /, b=2, *, c=3): pass
    /// ```
    NonVariadic(&'a ParameterWithDefault<'ast>),
}

impl<'a, 'ast> AnyParameterRef<'a, 'ast> {
    pub const fn as_parameter(self) -> &'a Parameter<'ast> {
        match self {
            Self::NonVariadic(param) => &param.parameter,
            Self::Variadic(param) => param,
        }
    }

    pub const fn name(self) -> &'a Identifier<'ast> {
        &self.as_parameter().name
    }

    pub const fn is_variadic(self) -> bool {
        matches!(self, Self::Variadic(_))
    }

    pub fn annotation(self) -> Option<&'a Expr<'ast>> {
        self.as_parameter().annotation.as_deref()
    }

    pub fn default(self) -> Option<&'a Expr<'ast>> {
        match self {
            Self::NonVariadic(param) => param.default.as_deref(),
            Self::Variadic(_) => None,
        }
    }
}

impl Ranged for AnyParameterRef<'_, '_> {
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

#[derive(Debug, PartialEq, Default)]
pub struct Parameters<'ast> {
    pub range: TextRange,
    pub posonlyargs: Vec<ParameterWithDefault<'ast>>,
    pub args: Vec<ParameterWithDefault<'ast>>,
    pub vararg: Option<Box<'ast, Parameter<'ast>>>,
    pub kwonlyargs: Vec<ParameterWithDefault<'ast>>,
    pub kwarg: Option<Box<'ast, Parameter<'ast>>>,
}

impl<'ast> Parameters<'ast> {
    /// Returns an iterator over all non-variadic parameters included in this [`Parameters`] node.
    ///
    /// The variadic parameters (`.vararg` and `.kwarg`) can never have default values;
    /// non-variadic parameters sometimes will.
    pub fn iter_non_variadic_params(&self) -> impl Iterator<Item = &ParameterWithDefault<'ast>> {
        self.posonlyargs
            .iter()
            .chain(&self.args)
            .chain(&self.kwonlyargs)
    }

    /// Returns the [`ParameterWithDefault`] with the given name, or `None` if no such [`ParameterWithDefault`] exists.
    pub fn find(&self, name: &str) -> Option<&ParameterWithDefault<'ast>> {
        self.iter_non_variadic_params()
            .find(|arg| arg.parameter.name.as_str() == name)
    }

    /// Returns an iterator over all parameters included in this [`Parameters`] node.
    pub fn iter(&self) -> ParametersIterator<'_, 'ast> {
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

pub struct ParametersIterator<'a, 'ast> {
    posonlyargs: Iter<'a, ParameterWithDefault<'ast>>,
    args: Iter<'a, ParameterWithDefault<'ast>>,
    vararg: Option<&'a Parameter<'ast>>,
    kwonlyargs: Iter<'a, ParameterWithDefault<'ast>>,
    kwarg: Option<&'a Parameter<'ast>>,
}

impl<'a, 'ast> ParametersIterator<'a, 'ast> {
    fn new(parameters: &'a Parameters<'ast>) -> Self {
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

impl<'a, 'ast> Iterator for ParametersIterator<'a, 'ast> {
    type Item = AnyParameterRef<'a, 'ast>;

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

impl DoubleEndedIterator for ParametersIterator<'_, '_> {
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

impl FusedIterator for ParametersIterator<'_, '_> {}

/// We rely on the same invariants outlined in the comment above `Parameters::len()`
/// in order to implement `ExactSizeIterator` here
impl ExactSizeIterator for ParametersIterator<'_, '_> {}

impl<'a, 'ast> IntoIterator for &'a Parameters<'ast> {
    type IntoIter = ParametersIterator<'a, 'ast>;
    type Item = AnyParameterRef<'a, 'ast>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// An alternative type of AST `arg`. This is used for each function argument that might have a default value.
/// Used by `Arguments` original type.
///
/// NOTE: This type is different from original Python AST.

#[derive(Debug, PartialEq)]
pub struct ParameterWithDefault<'ast> {
    pub range: TextRange,
    pub parameter: Parameter<'ast>,
    pub default: Option<Box<'ast, Expr<'ast>>>,
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

#[derive(Debug, PartialEq)]
pub struct Arguments<'ast> {
    pub range: TextRange,
    pub args: &'ast mut [Expr<'ast>],
    pub keywords: &'ast mut [Keyword<'ast>],
}

/// An entry in the argument list of a function call.
#[derive(Debug, PartialEq)]
pub enum ArgOrKeyword<'a, 'ast> {
    Arg(&'a Expr<'ast>),
    Keyword(&'a Keyword<'ast>),
}

impl<'a, 'ast> From<&'a Expr<'ast>> for ArgOrKeyword<'a, 'ast> {
    fn from(arg: &'a Expr<'ast>) -> Self {
        Self::Arg(arg)
    }
}

impl<'a, 'ast> From<&'a Keyword<'ast>> for ArgOrKeyword<'a, 'ast> {
    fn from(keyword: &'a Keyword<'ast>) -> Self {
        Self::Keyword(keyword)
    }
}

impl Ranged for ArgOrKeyword<'_, '_> {
    fn range(&self) -> TextRange {
        match self {
            Self::Arg(arg) => arg.range(),
            Self::Keyword(keyword) => keyword.range(),
        }
    }
}

impl<'ast> Arguments<'ast> {
    /// Return the number of positional and keyword arguments.
    pub fn len(&self) -> usize {
        self.args.len() + self.keywords.len()
    }

    /// Return `true` if there are no positional or keyword arguments.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Return the [`Keyword`] with the given name, or `None` if no such [`Keyword`] exists.
    pub fn find_keyword(&self, keyword_name: &str) -> Option<&Keyword<'ast>> {
        self.keywords.iter().find(|keyword| {
            let Keyword { arg, .. } = keyword;
            arg.as_ref().is_some_and(|arg| arg == keyword_name)
        })
    }

    /// Return the positional argument at the given index, or `None` if no such argument exists.
    pub fn find_positional(&self, position: usize) -> Option<&Expr<'ast>> {
        self.args
            .iter()
            .take_while(|expr| !expr.is_starred_expr())
            .nth(position)
    }

    /// Return the argument with the given name or at the given position, or `None` if no such
    /// argument exists. Used to retrieve arguments that can be provided _either_ as keyword or
    /// positional arguments.
    pub fn find_argument(&self, name: &str, position: usize) -> Option<&Expr<'ast>> {
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
    pub fn arguments_source_order(&self) -> impl Iterator<Item = ArgOrKeyword<'_, 'ast>> {
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

#[derive(Debug, PartialEq)]
pub struct TypeParams<'ast> {
    pub range: TextRange,
    pub type_params: Vec<TypeParam<'ast>>,
}

impl<'ast> Deref for TypeParams<'ast> {
    type Target = [TypeParam<'ast>];

    fn deref(&self) -> &Self::Target {
        &self.type_params
    }
}

/// A suite represents a [Vec] of [Stmt].
///
/// See: <https://docs.python.org/3/reference/compound_stmts.html#grammar-token-python-grammar-suite>
pub type Suite<'ast> = Vec<Stmt<'ast>>;

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
pub struct Identifier<'ast> {
    pub id: &'ast str,
    pub range: TextRange,
}

impl<'ast> Identifier<'ast> {
    #[inline]
    pub fn new(id: &'ast str, range: TextRange) -> Self {
        Self { id, range }
    }

    pub fn id(&self) -> &'ast str {
        &self.id
    }

    pub fn is_valid(&self) -> bool {
        !self.id.is_empty()
    }
}

impl<'ast> Identifier<'ast> {
    #[inline]
    pub fn as_str(&self) -> &'ast str {
        self.id
    }
}

impl PartialEq<str> for Identifier<'_> {
    #[inline]
    fn eq(&self, other: &str) -> bool {
        self.id == other
    }
}

impl PartialEq<String> for Identifier<'_> {
    #[inline]
    fn eq(&self, other: &String) -> bool {
        self.id == other
    }
}

impl std::ops::Deref for Identifier<'_> {
    type Target = str;
    #[inline]
    fn deref(&self) -> &Self::Target {
        self.id
    }
}

impl AsRef<str> for Identifier<'_> {
    #[inline]
    fn as_ref(&self) -> &str {
        self.id
    }
}

impl std::fmt::Display for Identifier<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.id, f)
    }
}

impl Ranged for Identifier<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
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

impl Ranged for crate::nodes::ModModule<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::ModExpression<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::Mod<'_> {
    fn range(&self) -> TextRange {
        match self {
            Self::Module(node) => node.range(),
            Self::Expression(node) => node.range(),
        }
    }
}

impl Ranged for crate::nodes::StmtFunctionDef<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::StmtClassDef<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::StmtReturn<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::StmtDelete<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::StmtTypeAlias<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::StmtAssign<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::StmtAugAssign<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::StmtAnnAssign<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::StmtFor<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::StmtWhile<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::StmtIf<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::ElifElseClause<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::StmtWith<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::StmtMatch<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::StmtRaise<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::StmtTry<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::StmtAssert<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::StmtImport<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::StmtImportFrom<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::StmtGlobal<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::StmtNonlocal<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::StmtExpr<'_> {
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
impl Ranged for crate::nodes::StmtIpyEscapeCommand<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::Stmt<'_> {
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

impl Ranged for crate::nodes::ExprBoolOp<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::ExprNamed<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::ExprBinOp<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::ExprUnaryOp<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::ExprLambda<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::ExprIf<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::ExprDict<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::ExprSet<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::ExprListComp<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::ExprSetComp<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::ExprDictComp<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::ExprGenerator<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::ExprAwait<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::ExprYield<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::ExprYieldFrom<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::ExprCompare<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::ExprCall<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::ExprFString<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::ExprAttribute<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::ExprSubscript<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::ExprStarred<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::ExprName<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::ExprList<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::ExprTuple<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::ExprSlice<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::ExprIpyEscapeCommand<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::Expr<'_> {
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
impl Ranged for crate::nodes::Comprehension<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::ExceptHandlerExceptHandler<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::ExceptHandler<'_> {
    fn range(&self) -> TextRange {
        match self {
            Self::ExceptHandler(node) => node.range(),
        }
    }
}
impl Ranged for crate::nodes::Parameter<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::Keyword<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::Alias<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::WithItem<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::MatchCase<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::PatternMatchValue<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::PatternMatchSingleton {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::PatternMatchSequence<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::PatternMatchMapping<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::PatternMatchClass<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::PatternMatchStar<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::PatternMatchAs<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::PatternMatchOr<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::Pattern<'_> {
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
impl Ranged for crate::nodes::PatternArguments<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::PatternKeyword<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}

impl Ranged for crate::nodes::TypeParams<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::TypeParamTypeVar<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::TypeParamTypeVarTuple<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::TypeParamParamSpec<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::TypeParam<'_> {
    fn range(&self) -> TextRange {
        match self {
            Self::TypeVar(node) => node.range(),
            Self::TypeVarTuple(node) => node.range(),
            Self::ParamSpec(node) => node.range(),
        }
    }
}
impl Ranged for crate::nodes::Decorator<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::Arguments<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::Parameters<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl Ranged for crate::nodes::ParameterWithDefault<'_> {
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
