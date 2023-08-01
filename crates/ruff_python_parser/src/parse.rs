use crate::lexer::{lex, lex_starts_at, LexResult};
use crate::{parse_tokens, Mode, ParseError, ParseErrorType};
use ruff_python_ast as ast;
use ruff_python_ast::Ranged;
use ruff_text_size::TextSize;

/// Parse Python code string to implementor's type.
///
/// # Example
///
/// For example, parsing a simple function definition and a call to that function:
///
/// ```
/// use ruff_python_parser::{self as parser, Parse};
/// use ruff_python_ast as ast;
/// let source = r#"
/// def foo():
///    return 42
///
/// print(foo())
/// "#;
/// let program = ast::Suite::parse(source, "<embedded>");
/// assert!(program.is_ok());
/// ```
///
/// Parsing a single expression denoting the addition of two numbers, but this time specifying a different,
/// somewhat silly, location:
///
/// ```
/// # use ruff_text_size::TextSize;
/// # use ruff_python_ast as ast;
/// # use ruff_python_parser::{self as parser, Parse};
///
/// let expr = ast::Expr::parse_starts_at("1 + 2", "<embedded>", TextSize::from(400));
/// assert!(expr.is_ok());
pub trait Parse
where
    Self: Sized,
{
    const MODE: Mode;

    fn parse(source: &str, source_path: &str) -> Result<Self, ParseError> {
        let tokens = lex(source, Self::MODE);

        Self::parse_tokens(tokens, source_path)
    }

    fn parse_without_path(source: &str) -> Result<Self, ParseError> {
        Self::parse(source, "<unknown>")
    }

    fn parse_starts_at(
        source: &str,
        source_path: &str,
        offset: TextSize,
    ) -> Result<Self, ParseError> {
        let tokens = lex_starts_at(source, Self::MODE, offset);

        Self::parse_tokens(tokens, source_path)
    }

    fn parse_tokens(
        lxr: impl IntoIterator<Item = LexResult>,
        source_path: &str,
    ) -> Result<Self, ParseError>;
}

impl Parse for ast::ModModule {
    const MODE: Mode = Mode::Module;

    fn parse_tokens(
        lxr: impl IntoIterator<Item = LexResult>,
        source_path: &str,
    ) -> Result<Self, ParseError> {
        match parse_tokens(lxr, Mode::Module, source_path)? {
            ast::Mod::Module(m) => Ok(m),
            ast::Mod::Expression(_) => unreachable!("Mode::Module doesn't return other variant"),
        }
    }
}

impl Parse for ast::ModExpression {
    const MODE: Mode = Mode::Expression;

    fn parse_tokens(
        lxr: impl IntoIterator<Item = LexResult>,
        source_path: &str,
    ) -> Result<Self, ParseError> {
        match parse_tokens(lxr, Mode::Expression, source_path)? {
            ast::Mod::Expression(m) => Ok(m),
            ast::Mod::Module(_) => unreachable!("Mode::Module doesn't return other variant"),
        }
    }
}

impl Parse for ast::Suite {
    const MODE: Mode = Mode::Module;

    fn parse_tokens(
        lxr: impl IntoIterator<Item = LexResult>,
        source_path: &str,
    ) -> Result<Self, ParseError> {
        Ok(ast::ModModule::parse_tokens(lxr, source_path)?.body)
    }
}

impl Parse for ast::Stmt {
    const MODE: Mode = Mode::Module;

    fn parse_tokens(
        lxr: impl IntoIterator<Item = LexResult>,
        source_path: &str,
    ) -> Result<Self, ParseError> {
        let mut statements = ast::ModModule::parse_tokens(lxr, source_path)?.body;
        let statement = match statements.len() {
            0 => {
                return Err(ParseError {
                    error: ParseErrorType::Eof,
                    offset: TextSize::default(),
                    source_path: source_path.to_owned(),
                })
            }
            1 => statements.pop().unwrap(),
            _ => {
                return Err(ParseError {
                    error: ParseErrorType::InvalidToken,
                    offset: statements[1].range().start(),
                    source_path: source_path.to_owned(),
                })
            }
        };
        Ok(statement)
    }
}

impl Parse for ast::Expr {
    const MODE: Mode = Mode::Expression;

    fn parse_tokens(
        lxr: impl IntoIterator<Item = LexResult>,
        source_path: &str,
    ) -> Result<Self, ParseError> {
        Ok(*ast::ModExpression::parse_tokens(lxr, source_path)?.body)
    }
}

impl Parse for ast::Identifier {
    const MODE: Mode = Mode::Expression;

    fn parse_tokens(
        lxr: impl IntoIterator<Item = LexResult>,
        source_path: &str,
    ) -> Result<Self, ParseError> {
        let expr = ast::Expr::parse_tokens(lxr, source_path)?;
        match expr {
            ast::Expr::Name(name) => {
                let range = name.range();
                Ok(ast::Identifier::new(name.id, range))
            }
            expr => Err(ParseError {
                error: ParseErrorType::InvalidToken,
                offset: expr.range().start(),
                source_path: source_path.to_owned(),
            }),
        }
    }
}

impl Parse for ast::Constant {
    const MODE: Mode = Mode::Expression;

    fn parse_tokens(
        lxr: impl IntoIterator<Item = LexResult>,
        source_path: &str,
    ) -> Result<Self, ParseError> {
        let expr = ast::Expr::parse_tokens(lxr, source_path)?;
        match expr {
            ast::Expr::Constant(c) => Ok(c.value),
            expr => Err(ParseError {
                error: ParseErrorType::InvalidToken,
                offset: expr.range().start(),
                source_path: source_path.to_owned(),
            }),
        }
    }
}

impl Parse for ast::StmtFunctionDef {
    const MODE: Mode = Mode::Module;

    fn parse_tokens(
        lxr: impl IntoIterator<Item = LexResult>,
        source_path: &str,
    ) -> Result<Self, ParseError> {
        let node = ast::Stmt::parse_tokens(lxr, source_path)?;
        match node {
            ast::Stmt::FunctionDef(node) => Ok(node),
            node => Err(ParseError {
                error: ParseErrorType::InvalidToken,
                offset: node.range().start(),
                source_path: source_path.to_owned(),
            }),
        }
    }
}

impl Parse for ast::StmtAsyncFunctionDef {
    const MODE: Mode = Mode::Module;
    fn parse_tokens(
        lxr: impl IntoIterator<Item = LexResult>,
        source_path: &str,
    ) -> Result<Self, ParseError> {
        let node = ast::Stmt::parse_tokens(lxr, source_path)?;
        match node {
            ast::Stmt::AsyncFunctionDef(node) => Ok(node),
            node => Err(ParseError {
                error: ParseErrorType::InvalidToken,
                offset: node.range().start(),
                source_path: source_path.to_owned(),
            }),
        }
    }
}

impl Parse for ast::StmtClassDef {
    const MODE: Mode = Mode::Module;
    fn parse_tokens(
        lxr: impl IntoIterator<Item = LexResult>,
        source_path: &str,
    ) -> Result<Self, ParseError> {
        let node = ast::Stmt::parse_tokens(lxr, source_path)?;
        match node {
            ast::Stmt::ClassDef(node) => Ok(node),
            node => Err(ParseError {
                error: ParseErrorType::InvalidToken,
                offset: node.range().start(),
                source_path: source_path.to_owned(),
            }),
        }
    }
}

impl Parse for ast::StmtReturn {
    const MODE: Mode = Mode::Module;
    fn parse_tokens(
        lxr: impl IntoIterator<Item = LexResult>,
        source_path: &str,
    ) -> Result<Self, ParseError> {
        let node = ast::Stmt::parse_tokens(lxr, source_path)?;
        match node {
            ast::Stmt::Return(node) => Ok(node),
            node => Err(ParseError {
                error: ParseErrorType::InvalidToken,
                offset: node.range().start(),
                source_path: source_path.to_owned(),
            }),
        }
    }
}

impl Parse for ast::StmtDelete {
    const MODE: Mode = Mode::Module;
    fn parse_tokens(
        lxr: impl IntoIterator<Item = LexResult>,
        source_path: &str,
    ) -> Result<Self, ParseError> {
        let node = ast::Stmt::parse_tokens(lxr, source_path)?;
        match node {
            ast::Stmt::Delete(node) => Ok(node),
            node => Err(ParseError {
                error: ParseErrorType::InvalidToken,
                offset: node.range().start(),
                source_path: source_path.to_owned(),
            }),
        }
    }
}

impl Parse for ast::StmtAssign {
    const MODE: Mode = Mode::Module;
    fn parse_tokens(
        lxr: impl IntoIterator<Item = LexResult>,
        source_path: &str,
    ) -> Result<Self, ParseError> {
        let node = ast::Stmt::parse_tokens(lxr, source_path)?;
        match node {
            ast::Stmt::Assign(node) => Ok(node),
            node => Err(ParseError {
                error: ParseErrorType::InvalidToken,
                offset: node.range().start(),
                source_path: source_path.to_owned(),
            }),
        }
    }
}

impl Parse for ast::StmtTypeAlias {
    const MODE: Mode = Mode::Module;
    fn parse_tokens(
        lxr: impl IntoIterator<Item = LexResult>,
        source_path: &str,
    ) -> Result<Self, ParseError> {
        let node = ast::Stmt::parse_tokens(lxr, source_path)?;
        match node {
            ast::Stmt::TypeAlias(node) => Ok(node),
            node => Err(ParseError {
                error: ParseErrorType::InvalidToken,
                offset: node.range().start(),
                source_path: source_path.to_owned(),
            }),
        }
    }
}

impl Parse for ast::StmtAugAssign {
    const MODE: Mode = Mode::Module;
    fn parse_tokens(
        lxr: impl IntoIterator<Item = LexResult>,
        source_path: &str,
    ) -> Result<Self, ParseError> {
        let node = ast::Stmt::parse_tokens(lxr, source_path)?;
        match node {
            ast::Stmt::AugAssign(node) => Ok(node),
            node => Err(ParseError {
                error: ParseErrorType::InvalidToken,
                offset: node.range().start(),
                source_path: source_path.to_owned(),
            }),
        }
    }
}

impl Parse for ast::StmtAnnAssign {
    const MODE: Mode = Mode::Module;
    fn parse_tokens(
        lxr: impl IntoIterator<Item = LexResult>,
        source_path: &str,
    ) -> Result<Self, ParseError> {
        let node = ast::Stmt::parse_tokens(lxr, source_path)?;
        match node {
            ast::Stmt::AnnAssign(node) => Ok(node),
            node => Err(ParseError {
                error: ParseErrorType::InvalidToken,
                offset: node.range().start(),
                source_path: source_path.to_owned(),
            }),
        }
    }
}

impl Parse for ast::StmtFor {
    const MODE: Mode = Mode::Module;
    fn parse_tokens(
        lxr: impl IntoIterator<Item = LexResult>,
        source_path: &str,
    ) -> Result<Self, ParseError> {
        let node = ast::Stmt::parse_tokens(lxr, source_path)?;
        match node {
            ast::Stmt::For(node) => Ok(node),
            node => Err(ParseError {
                error: ParseErrorType::InvalidToken,
                offset: node.range().start(),
                source_path: source_path.to_owned(),
            }),
        }
    }
}

impl Parse for ast::StmtAsyncFor {
    const MODE: Mode = Mode::Module;
    fn parse_tokens(
        lxr: impl IntoIterator<Item = LexResult>,
        source_path: &str,
    ) -> Result<Self, ParseError> {
        let node = ast::Stmt::parse_tokens(lxr, source_path)?;
        match node {
            ast::Stmt::AsyncFor(node) => Ok(node),
            node => Err(ParseError {
                error: ParseErrorType::InvalidToken,
                offset: node.range().start(),
                source_path: source_path.to_owned(),
            }),
        }
    }
}

impl Parse for ast::StmtWhile {
    const MODE: Mode = Mode::Module;
    fn parse_tokens(
        lxr: impl IntoIterator<Item = LexResult>,
        source_path: &str,
    ) -> Result<Self, ParseError> {
        let node = ast::Stmt::parse_tokens(lxr, source_path)?;
        match node {
            ast::Stmt::While(node) => Ok(node),
            node => Err(ParseError {
                error: ParseErrorType::InvalidToken,
                offset: node.range().start(),
                source_path: source_path.to_owned(),
            }),
        }
    }
}

impl Parse for ast::StmtIf {
    const MODE: Mode = Mode::Module;
    fn parse_tokens(
        lxr: impl IntoIterator<Item = LexResult>,
        source_path: &str,
    ) -> Result<Self, ParseError> {
        let node = ast::Stmt::parse_tokens(lxr, source_path)?;
        match node {
            ast::Stmt::If(node) => Ok(node),
            node => Err(ParseError {
                error: ParseErrorType::InvalidToken,
                offset: node.range().start(),
                source_path: source_path.to_owned(),
            }),
        }
    }
}

impl Parse for ast::StmtWith {
    const MODE: Mode = Mode::Module;
    fn parse_tokens(
        lxr: impl IntoIterator<Item = LexResult>,
        source_path: &str,
    ) -> Result<Self, ParseError> {
        let node = ast::Stmt::parse_tokens(lxr, source_path)?;
        match node {
            ast::Stmt::With(node) => Ok(node),
            node => Err(ParseError {
                error: ParseErrorType::InvalidToken,
                offset: node.range().start(),
                source_path: source_path.to_owned(),
            }),
        }
    }
}

impl Parse for ast::StmtAsyncWith {
    const MODE: Mode = Mode::Module;
    fn parse_tokens(
        lxr: impl IntoIterator<Item = LexResult>,
        source_path: &str,
    ) -> Result<Self, ParseError> {
        let node = ast::Stmt::parse_tokens(lxr, source_path)?;
        match node {
            ast::Stmt::AsyncWith(node) => Ok(node),
            node => Err(ParseError {
                error: ParseErrorType::InvalidToken,
                offset: node.range().start(),
                source_path: source_path.to_owned(),
            }),
        }
    }
}

impl Parse for ast::StmtMatch {
    const MODE: Mode = Mode::Module;
    fn parse_tokens(
        lxr: impl IntoIterator<Item = LexResult>,
        source_path: &str,
    ) -> Result<Self, ParseError> {
        let node = ast::Stmt::parse_tokens(lxr, source_path)?;
        match node {
            ast::Stmt::Match(node) => Ok(node),
            node => Err(ParseError {
                error: ParseErrorType::InvalidToken,
                offset: node.range().start(),
                source_path: source_path.to_owned(),
            }),
        }
    }
}

impl Parse for ast::StmtRaise {
    const MODE: Mode = Mode::Module;
    fn parse_tokens(
        lxr: impl IntoIterator<Item = LexResult>,
        source_path: &str,
    ) -> Result<Self, ParseError> {
        let node = ast::Stmt::parse_tokens(lxr, source_path)?;
        match node {
            ast::Stmt::Raise(node) => Ok(node),
            node => Err(ParseError {
                error: ParseErrorType::InvalidToken,
                offset: node.range().start(),
                source_path: source_path.to_owned(),
            }),
        }
    }
}

impl Parse for ast::StmtTry {
    const MODE: Mode = Mode::Module;
    fn parse_tokens(
        lxr: impl IntoIterator<Item = LexResult>,
        source_path: &str,
    ) -> Result<Self, ParseError> {
        let node = ast::Stmt::parse_tokens(lxr, source_path)?;
        match node {
            ast::Stmt::Try(node) => Ok(node),
            node => Err(ParseError {
                error: ParseErrorType::InvalidToken,
                offset: node.range().start(),
                source_path: source_path.to_owned(),
            }),
        }
    }
}

impl Parse for ast::StmtTryStar {
    const MODE: Mode = Mode::Module;
    fn parse_tokens(
        lxr: impl IntoIterator<Item = LexResult>,
        source_path: &str,
    ) -> Result<Self, ParseError> {
        let node = ast::Stmt::parse_tokens(lxr, source_path)?;
        match node {
            ast::Stmt::TryStar(node) => Ok(node),
            node => Err(ParseError {
                error: ParseErrorType::InvalidToken,
                offset: node.range().start(),
                source_path: source_path.to_owned(),
            }),
        }
    }
}

impl Parse for ast::StmtAssert {
    const MODE: Mode = Mode::Module;
    fn parse_tokens(
        lxr: impl IntoIterator<Item = LexResult>,
        source_path: &str,
    ) -> Result<Self, ParseError> {
        let node = ast::Stmt::parse_tokens(lxr, source_path)?;
        match node {
            ast::Stmt::Assert(node) => Ok(node),
            node => Err(ParseError {
                error: ParseErrorType::InvalidToken,
                offset: node.range().start(),
                source_path: source_path.to_owned(),
            }),
        }
    }
}

impl Parse for ast::StmtImport {
    const MODE: Mode = Mode::Module;
    fn parse_tokens(
        lxr: impl IntoIterator<Item = LexResult>,
        source_path: &str,
    ) -> Result<Self, ParseError> {
        let node = ast::Stmt::parse_tokens(lxr, source_path)?;
        match node {
            ast::Stmt::Import(node) => Ok(node),
            node => Err(ParseError {
                error: ParseErrorType::InvalidToken,
                offset: node.range().start(),
                source_path: source_path.to_owned(),
            }),
        }
    }
}

impl Parse for ast::StmtImportFrom {
    const MODE: Mode = Mode::Module;
    fn parse_tokens(
        lxr: impl IntoIterator<Item = LexResult>,
        source_path: &str,
    ) -> Result<Self, ParseError> {
        let node = ast::Stmt::parse_tokens(lxr, source_path)?;
        match node {
            ast::Stmt::ImportFrom(node) => Ok(node),
            node => Err(ParseError {
                error: ParseErrorType::InvalidToken,
                offset: node.range().start(),
                source_path: source_path.to_owned(),
            }),
        }
    }
}

impl Parse for ast::StmtGlobal {
    const MODE: Mode = Mode::Module;
    fn parse_tokens(
        lxr: impl IntoIterator<Item = LexResult>,
        source_path: &str,
    ) -> Result<Self, ParseError> {
        let node = ast::Stmt::parse_tokens(lxr, source_path)?;
        match node {
            ast::Stmt::Global(node) => Ok(node),
            node => Err(ParseError {
                error: ParseErrorType::InvalidToken,
                offset: node.range().start(),
                source_path: source_path.to_owned(),
            }),
        }
    }
}

impl Parse for ast::StmtNonlocal {
    const MODE: Mode = Mode::Module;
    fn parse_tokens(
        lxr: impl IntoIterator<Item = LexResult>,
        source_path: &str,
    ) -> Result<Self, ParseError> {
        let node = ast::Stmt::parse_tokens(lxr, source_path)?;
        match node {
            ast::Stmt::Nonlocal(node) => Ok(node),
            node => Err(ParseError {
                error: ParseErrorType::InvalidToken,
                offset: node.range().start(),
                source_path: source_path.to_owned(),
            }),
        }
    }
}

impl Parse for ast::StmtExpr {
    const MODE: Mode = Mode::Module;
    fn parse_tokens(
        lxr: impl IntoIterator<Item = LexResult>,
        source_path: &str,
    ) -> Result<Self, ParseError> {
        let node = ast::Stmt::parse_tokens(lxr, source_path)?;
        match node {
            ast::Stmt::Expr(node) => Ok(node),
            node => Err(ParseError {
                error: ParseErrorType::InvalidToken,
                offset: node.range().start(),
                source_path: source_path.to_owned(),
            }),
        }
    }
}

impl Parse for ast::StmtPass {
    const MODE: Mode = Mode::Module;
    fn parse_tokens(
        lxr: impl IntoIterator<Item = LexResult>,
        source_path: &str,
    ) -> Result<Self, ParseError> {
        let node = ast::Stmt::parse_tokens(lxr, source_path)?;
        match node {
            ast::Stmt::Pass(node) => Ok(node),
            node => Err(ParseError {
                error: ParseErrorType::InvalidToken,
                offset: node.range().start(),
                source_path: source_path.to_owned(),
            }),
        }
    }
}

impl Parse for ast::StmtBreak {
    const MODE: Mode = Mode::Module;
    fn parse_tokens(
        lxr: impl IntoIterator<Item = LexResult>,
        source_path: &str,
    ) -> Result<Self, ParseError> {
        let node = ast::Stmt::parse_tokens(lxr, source_path)?;
        match node {
            ast::Stmt::Break(node) => Ok(node),
            node => Err(ParseError {
                error: ParseErrorType::InvalidToken,
                offset: node.range().start(),
                source_path: source_path.to_owned(),
            }),
        }
    }
}

impl Parse for ast::StmtContinue {
    const MODE: Mode = Mode::Module;
    fn parse_tokens(
        lxr: impl IntoIterator<Item = LexResult>,
        source_path: &str,
    ) -> Result<Self, ParseError> {
        let node = ast::Stmt::parse_tokens(lxr, source_path)?;
        match node {
            ast::Stmt::Continue(node) => Ok(node),
            node => Err(ParseError {
                error: ParseErrorType::InvalidToken,
                offset: node.range().start(),
                source_path: source_path.to_owned(),
            }),
        }
    }
}

impl Parse for ast::ExprBoolOp {
    const MODE: Mode = Mode::Expression;
    fn parse_tokens(
        lxr: impl IntoIterator<Item = LexResult>,
        source_path: &str,
    ) -> Result<Self, ParseError> {
        let node = ast::Expr::parse_tokens(lxr, source_path)?;
        match node {
            ast::Expr::BoolOp(node) => Ok(node),
            node => Err(ParseError {
                error: ParseErrorType::InvalidToken,
                offset: node.range().start(),
                source_path: source_path.to_owned(),
            }),
        }
    }
}

impl Parse for ast::ExprNamedExpr {
    const MODE: Mode = Mode::Expression;
    fn parse_tokens(
        lxr: impl IntoIterator<Item = LexResult>,
        source_path: &str,
    ) -> Result<Self, ParseError> {
        let node = ast::Expr::parse_tokens(lxr, source_path)?;
        match node {
            ast::Expr::NamedExpr(node) => Ok(node),
            node => Err(ParseError {
                error: ParseErrorType::InvalidToken,
                offset: node.range().start(),
                source_path: source_path.to_owned(),
            }),
        }
    }
}

impl Parse for ast::ExprBinOp {
    const MODE: Mode = Mode::Expression;
    fn parse_tokens(
        lxr: impl IntoIterator<Item = LexResult>,
        source_path: &str,
    ) -> Result<Self, ParseError> {
        let node = ast::Expr::parse_tokens(lxr, source_path)?;
        match node {
            ast::Expr::BinOp(node) => Ok(node),
            node => Err(ParseError {
                error: ParseErrorType::InvalidToken,
                offset: node.range().start(),
                source_path: source_path.to_owned(),
            }),
        }
    }
}

impl Parse for ast::ExprUnaryOp {
    const MODE: Mode = Mode::Expression;
    fn parse_tokens(
        lxr: impl IntoIterator<Item = LexResult>,
        source_path: &str,
    ) -> Result<Self, ParseError> {
        let node = ast::Expr::parse_tokens(lxr, source_path)?;
        match node {
            ast::Expr::UnaryOp(node) => Ok(node),
            node => Err(ParseError {
                error: ParseErrorType::InvalidToken,
                offset: node.range().start(),
                source_path: source_path.to_owned(),
            }),
        }
    }
}

impl Parse for ast::ExprLambda {
    const MODE: Mode = Mode::Expression;
    fn parse_tokens(
        lxr: impl IntoIterator<Item = LexResult>,
        source_path: &str,
    ) -> Result<Self, ParseError> {
        let node = ast::Expr::parse_tokens(lxr, source_path)?;
        match node {
            ast::Expr::Lambda(node) => Ok(node),
            node => Err(ParseError {
                error: ParseErrorType::InvalidToken,
                offset: node.range().start(),
                source_path: source_path.to_owned(),
            }),
        }
    }
}

impl Parse for ast::ExprIfExp {
    const MODE: Mode = Mode::Expression;
    fn parse_tokens(
        lxr: impl IntoIterator<Item = LexResult>,
        source_path: &str,
    ) -> Result<Self, ParseError> {
        let node = ast::Expr::parse_tokens(lxr, source_path)?;
        match node {
            ast::Expr::IfExp(node) => Ok(node),
            node => Err(ParseError {
                error: ParseErrorType::InvalidToken,
                offset: node.range().start(),
                source_path: source_path.to_owned(),
            }),
        }
    }
}

impl Parse for ast::ExprDict {
    const MODE: Mode = Mode::Expression;
    fn parse_tokens(
        lxr: impl IntoIterator<Item = LexResult>,
        source_path: &str,
    ) -> Result<Self, ParseError> {
        let node = ast::Expr::parse_tokens(lxr, source_path)?;
        match node {
            ast::Expr::Dict(node) => Ok(node),
            node => Err(ParseError {
                error: ParseErrorType::InvalidToken,
                offset: node.range().start(),
                source_path: source_path.to_owned(),
            }),
        }
    }
}

impl Parse for ast::ExprSet {
    const MODE: Mode = Mode::Expression;
    fn parse_tokens(
        lxr: impl IntoIterator<Item = LexResult>,
        source_path: &str,
    ) -> Result<Self, ParseError> {
        let node = ast::Expr::parse_tokens(lxr, source_path)?;
        match node {
            ast::Expr::Set(node) => Ok(node),
            node => Err(ParseError {
                error: ParseErrorType::InvalidToken,
                offset: node.range().start(),
                source_path: source_path.to_owned(),
            }),
        }
    }
}

impl Parse for ast::ExprListComp {
    const MODE: Mode = Mode::Expression;
    fn parse_tokens(
        lxr: impl IntoIterator<Item = LexResult>,
        source_path: &str,
    ) -> Result<Self, ParseError> {
        let node = ast::Expr::parse_tokens(lxr, source_path)?;
        match node {
            ast::Expr::ListComp(node) => Ok(node),
            node => Err(ParseError {
                error: ParseErrorType::InvalidToken,
                offset: node.range().start(),
                source_path: source_path.to_owned(),
            }),
        }
    }
}

impl Parse for ast::ExprSetComp {
    const MODE: Mode = Mode::Expression;
    fn parse_tokens(
        lxr: impl IntoIterator<Item = LexResult>,
        source_path: &str,
    ) -> Result<Self, ParseError> {
        let node = ast::Expr::parse_tokens(lxr, source_path)?;
        match node {
            ast::Expr::SetComp(node) => Ok(node),
            node => Err(ParseError {
                error: ParseErrorType::InvalidToken,
                offset: node.range().start(),
                source_path: source_path.to_owned(),
            }),
        }
    }
}

impl Parse for ast::ExprDictComp {
    const MODE: Mode = Mode::Expression;
    fn parse_tokens(
        lxr: impl IntoIterator<Item = LexResult>,
        source_path: &str,
    ) -> Result<Self, ParseError> {
        let node = ast::Expr::parse_tokens(lxr, source_path)?;
        match node {
            ast::Expr::DictComp(node) => Ok(node),
            node => Err(ParseError {
                error: ParseErrorType::InvalidToken,
                offset: node.range().start(),
                source_path: source_path.to_owned(),
            }),
        }
    }
}

impl Parse for ast::ExprGeneratorExp {
    const MODE: Mode = Mode::Expression;
    fn parse_tokens(
        lxr: impl IntoIterator<Item = LexResult>,
        source_path: &str,
    ) -> Result<Self, ParseError> {
        let node = ast::Expr::parse_tokens(lxr, source_path)?;
        match node {
            ast::Expr::GeneratorExp(node) => Ok(node),
            node => Err(ParseError {
                error: ParseErrorType::InvalidToken,
                offset: node.range().start(),
                source_path: source_path.to_owned(),
            }),
        }
    }
}

impl Parse for ast::ExprAwait {
    const MODE: Mode = Mode::Expression;
    fn parse_tokens(
        lxr: impl IntoIterator<Item = LexResult>,
        source_path: &str,
    ) -> Result<Self, ParseError> {
        let node = ast::Expr::parse_tokens(lxr, source_path)?;
        match node {
            ast::Expr::Await(node) => Ok(node),
            node => Err(ParseError {
                error: ParseErrorType::InvalidToken,
                offset: node.range().start(),
                source_path: source_path.to_owned(),
            }),
        }
    }
}

impl Parse for ast::ExprYield {
    const MODE: Mode = Mode::Expression;
    fn parse_tokens(
        lxr: impl IntoIterator<Item = LexResult>,
        source_path: &str,
    ) -> Result<Self, ParseError> {
        let node = ast::Expr::parse_tokens(lxr, source_path)?;
        match node {
            ast::Expr::Yield(node) => Ok(node),
            node => Err(ParseError {
                error: ParseErrorType::InvalidToken,
                offset: node.range().start(),
                source_path: source_path.to_owned(),
            }),
        }
    }
}

impl Parse for ast::ExprYieldFrom {
    const MODE: Mode = Mode::Expression;
    fn parse_tokens(
        lxr: impl IntoIterator<Item = LexResult>,
        source_path: &str,
    ) -> Result<Self, ParseError> {
        let node = ast::Expr::parse_tokens(lxr, source_path)?;
        match node {
            ast::Expr::YieldFrom(node) => Ok(node),
            node => Err(ParseError {
                error: ParseErrorType::InvalidToken,
                offset: node.range().start(),
                source_path: source_path.to_owned(),
            }),
        }
    }
}

impl Parse for ast::ExprCompare {
    const MODE: Mode = Mode::Expression;
    fn parse_tokens(
        lxr: impl IntoIterator<Item = LexResult>,
        source_path: &str,
    ) -> Result<Self, ParseError> {
        let node = ast::Expr::parse_tokens(lxr, source_path)?;
        match node {
            ast::Expr::Compare(node) => Ok(node),
            node => Err(ParseError {
                error: ParseErrorType::InvalidToken,
                offset: node.range().start(),
                source_path: source_path.to_owned(),
            }),
        }
    }
}

impl Parse for ast::ExprCall {
    const MODE: Mode = Mode::Expression;
    fn parse_tokens(
        lxr: impl IntoIterator<Item = LexResult>,
        source_path: &str,
    ) -> Result<Self, ParseError> {
        let node = ast::Expr::parse_tokens(lxr, source_path)?;
        match node {
            ast::Expr::Call(node) => Ok(node),
            node => Err(ParseError {
                error: ParseErrorType::InvalidToken,
                offset: node.range().start(),
                source_path: source_path.to_owned(),
            }),
        }
    }
}

impl Parse for ast::ExprFormattedValue {
    const MODE: Mode = Mode::Expression;
    fn parse_tokens(
        lxr: impl IntoIterator<Item = LexResult>,
        source_path: &str,
    ) -> Result<Self, ParseError> {
        let node = ast::Expr::parse_tokens(lxr, source_path)?;
        match node {
            ast::Expr::FormattedValue(node) => Ok(node),
            node => Err(ParseError {
                error: ParseErrorType::InvalidToken,
                offset: node.range().start(),
                source_path: source_path.to_owned(),
            }),
        }
    }
}

impl Parse for ast::ExprJoinedStr {
    const MODE: Mode = Mode::Expression;
    fn parse_tokens(
        lxr: impl IntoIterator<Item = LexResult>,
        source_path: &str,
    ) -> Result<Self, ParseError> {
        let node = ast::Expr::parse_tokens(lxr, source_path)?;
        match node {
            ast::Expr::JoinedStr(node) => Ok(node),
            node => Err(ParseError {
                error: ParseErrorType::InvalidToken,
                offset: node.range().start(),
                source_path: source_path.to_owned(),
            }),
        }
    }
}

impl Parse for ast::ExprConstant {
    const MODE: Mode = Mode::Expression;
    fn parse_tokens(
        lxr: impl IntoIterator<Item = LexResult>,
        source_path: &str,
    ) -> Result<Self, ParseError> {
        let node = ast::Expr::parse_tokens(lxr, source_path)?;
        match node {
            ast::Expr::Constant(node) => Ok(node),
            node => Err(ParseError {
                error: ParseErrorType::InvalidToken,
                offset: node.range().start(),
                source_path: source_path.to_owned(),
            }),
        }
    }
}

impl Parse for ast::ExprAttribute {
    const MODE: Mode = Mode::Expression;
    fn parse_tokens(
        lxr: impl IntoIterator<Item = LexResult>,
        source_path: &str,
    ) -> Result<Self, ParseError> {
        let node = ast::Expr::parse_tokens(lxr, source_path)?;
        match node {
            ast::Expr::Attribute(node) => Ok(node),
            node => Err(ParseError {
                error: ParseErrorType::InvalidToken,
                offset: node.range().start(),
                source_path: source_path.to_owned(),
            }),
        }
    }
}

impl Parse for ast::ExprSubscript {
    const MODE: Mode = Mode::Expression;
    fn parse_tokens(
        lxr: impl IntoIterator<Item = LexResult>,
        source_path: &str,
    ) -> Result<Self, ParseError> {
        let node = ast::Expr::parse_tokens(lxr, source_path)?;
        match node {
            ast::Expr::Subscript(node) => Ok(node),
            node => Err(ParseError {
                error: ParseErrorType::InvalidToken,
                offset: node.range().start(),
                source_path: source_path.to_owned(),
            }),
        }
    }
}

impl Parse for ast::ExprStarred {
    const MODE: Mode = Mode::Expression;
    fn parse_tokens(
        lxr: impl IntoIterator<Item = LexResult>,
        source_path: &str,
    ) -> Result<Self, ParseError> {
        let node = ast::Expr::parse_tokens(lxr, source_path)?;
        match node {
            ast::Expr::Starred(node) => Ok(node),
            node => Err(ParseError {
                error: ParseErrorType::InvalidToken,
                offset: node.range().start(),
                source_path: source_path.to_owned(),
            }),
        }
    }
}

impl Parse for ast::ExprName {
    const MODE: Mode = Mode::Expression;
    fn parse_tokens(
        lxr: impl IntoIterator<Item = LexResult>,
        source_path: &str,
    ) -> Result<Self, ParseError> {
        let node = ast::Expr::parse_tokens(lxr, source_path)?;
        match node {
            ast::Expr::Name(node) => Ok(node),
            node => Err(ParseError {
                error: ParseErrorType::InvalidToken,
                offset: node.range().start(),
                source_path: source_path.to_owned(),
            }),
        }
    }
}

impl Parse for ast::ExprList {
    const MODE: Mode = Mode::Expression;
    fn parse_tokens(
        lxr: impl IntoIterator<Item = LexResult>,
        source_path: &str,
    ) -> Result<Self, ParseError> {
        let node = ast::Expr::parse_tokens(lxr, source_path)?;
        match node {
            ast::Expr::List(node) => Ok(node),
            node => Err(ParseError {
                error: ParseErrorType::InvalidToken,
                offset: node.range().start(),
                source_path: source_path.to_owned(),
            }),
        }
    }
}

impl Parse for ast::ExprTuple {
    const MODE: Mode = Mode::Expression;
    fn parse_tokens(
        lxr: impl IntoIterator<Item = LexResult>,
        source_path: &str,
    ) -> Result<Self, ParseError> {
        let node = ast::Expr::parse_tokens(lxr, source_path)?;
        match node {
            ast::Expr::Tuple(node) => Ok(node),
            node => Err(ParseError {
                error: ParseErrorType::InvalidToken,
                offset: node.range().start(),
                source_path: source_path.to_owned(),
            }),
        }
    }
}

impl Parse for ast::ExprSlice {
    const MODE: Mode = Mode::Expression;
    fn parse_tokens(
        lxr: impl IntoIterator<Item = LexResult>,
        source_path: &str,
    ) -> Result<Self, ParseError> {
        let node = ast::Expr::parse_tokens(lxr, source_path)?;
        match node {
            ast::Expr::Slice(node) => Ok(node),
            node => Err(ParseError {
                error: ParseErrorType::InvalidToken,
                offset: node.range().start(),
                source_path: source_path.to_owned(),
            }),
        }
    }
}
