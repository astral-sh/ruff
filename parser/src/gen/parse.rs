// This file was originally generated from asdl by a python script, but we now edit it manually

impl Parse for ast::StmtFunctionDef {
    fn lex_starts_at(
        source: &str,
        offset: TextSize,
    ) -> SoftKeywordTransformer<Lexer<std::str::Chars>> {
        ast::Stmt::lex_starts_at(source, offset)
    }
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
    fn lex_starts_at(
        source: &str,
        offset: TextSize,
    ) -> SoftKeywordTransformer<Lexer<std::str::Chars>> {
        ast::Stmt::lex_starts_at(source, offset)
    }
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
    fn lex_starts_at(
        source: &str,
        offset: TextSize,
    ) -> SoftKeywordTransformer<Lexer<std::str::Chars>> {
        ast::Stmt::lex_starts_at(source, offset)
    }
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
    fn lex_starts_at(
        source: &str,
        offset: TextSize,
    ) -> SoftKeywordTransformer<Lexer<std::str::Chars>> {
        ast::Stmt::lex_starts_at(source, offset)
    }
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
    fn lex_starts_at(
        source: &str,
        offset: TextSize,
    ) -> SoftKeywordTransformer<Lexer<std::str::Chars>> {
        ast::Stmt::lex_starts_at(source, offset)
    }
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
    fn lex_starts_at(
        source: &str,
        offset: TextSize,
    ) -> SoftKeywordTransformer<Lexer<std::str::Chars>> {
        ast::Stmt::lex_starts_at(source, offset)
    }
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

impl Parse for ast::StmtAugAssign {
    fn lex_starts_at(
        source: &str,
        offset: TextSize,
    ) -> SoftKeywordTransformer<Lexer<std::str::Chars>> {
        ast::Stmt::lex_starts_at(source, offset)
    }
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
    fn lex_starts_at(
        source: &str,
        offset: TextSize,
    ) -> SoftKeywordTransformer<Lexer<std::str::Chars>> {
        ast::Stmt::lex_starts_at(source, offset)
    }
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
    fn lex_starts_at(
        source: &str,
        offset: TextSize,
    ) -> SoftKeywordTransformer<Lexer<std::str::Chars>> {
        ast::Stmt::lex_starts_at(source, offset)
    }
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
    fn lex_starts_at(
        source: &str,
        offset: TextSize,
    ) -> SoftKeywordTransformer<Lexer<std::str::Chars>> {
        ast::Stmt::lex_starts_at(source, offset)
    }
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
    fn lex_starts_at(
        source: &str,
        offset: TextSize,
    ) -> SoftKeywordTransformer<Lexer<std::str::Chars>> {
        ast::Stmt::lex_starts_at(source, offset)
    }
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
    fn lex_starts_at(
        source: &str,
        offset: TextSize,
    ) -> SoftKeywordTransformer<Lexer<std::str::Chars>> {
        ast::Stmt::lex_starts_at(source, offset)
    }
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
    fn lex_starts_at(
        source: &str,
        offset: TextSize,
    ) -> SoftKeywordTransformer<Lexer<std::str::Chars>> {
        ast::Stmt::lex_starts_at(source, offset)
    }
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
    fn lex_starts_at(
        source: &str,
        offset: TextSize,
    ) -> SoftKeywordTransformer<Lexer<std::str::Chars>> {
        ast::Stmt::lex_starts_at(source, offset)
    }
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
    fn lex_starts_at(
        source: &str,
        offset: TextSize,
    ) -> SoftKeywordTransformer<Lexer<std::str::Chars>> {
        ast::Stmt::lex_starts_at(source, offset)
    }
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
    fn lex_starts_at(
        source: &str,
        offset: TextSize,
    ) -> SoftKeywordTransformer<Lexer<std::str::Chars>> {
        ast::Stmt::lex_starts_at(source, offset)
    }
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
    fn lex_starts_at(
        source: &str,
        offset: TextSize,
    ) -> SoftKeywordTransformer<Lexer<std::str::Chars>> {
        ast::Stmt::lex_starts_at(source, offset)
    }
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
    fn lex_starts_at(
        source: &str,
        offset: TextSize,
    ) -> SoftKeywordTransformer<Lexer<std::str::Chars>> {
        ast::Stmt::lex_starts_at(source, offset)
    }
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
    fn lex_starts_at(
        source: &str,
        offset: TextSize,
    ) -> SoftKeywordTransformer<Lexer<std::str::Chars>> {
        ast::Stmt::lex_starts_at(source, offset)
    }
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
    fn lex_starts_at(
        source: &str,
        offset: TextSize,
    ) -> SoftKeywordTransformer<Lexer<std::str::Chars>> {
        ast::Stmt::lex_starts_at(source, offset)
    }
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
    fn lex_starts_at(
        source: &str,
        offset: TextSize,
    ) -> SoftKeywordTransformer<Lexer<std::str::Chars>> {
        ast::Stmt::lex_starts_at(source, offset)
    }
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
    fn lex_starts_at(
        source: &str,
        offset: TextSize,
    ) -> SoftKeywordTransformer<Lexer<std::str::Chars>> {
        ast::Stmt::lex_starts_at(source, offset)
    }
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
    fn lex_starts_at(
        source: &str,
        offset: TextSize,
    ) -> SoftKeywordTransformer<Lexer<std::str::Chars>> {
        ast::Stmt::lex_starts_at(source, offset)
    }
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
    fn lex_starts_at(
        source: &str,
        offset: TextSize,
    ) -> SoftKeywordTransformer<Lexer<std::str::Chars>> {
        ast::Stmt::lex_starts_at(source, offset)
    }
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
    fn lex_starts_at(
        source: &str,
        offset: TextSize,
    ) -> SoftKeywordTransformer<Lexer<std::str::Chars>> {
        ast::Stmt::lex_starts_at(source, offset)
    }
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
    fn lex_starts_at(
        source: &str,
        offset: TextSize,
    ) -> SoftKeywordTransformer<Lexer<std::str::Chars>> {
        ast::Stmt::lex_starts_at(source, offset)
    }
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
    fn lex_starts_at(
        source: &str,
        offset: TextSize,
    ) -> SoftKeywordTransformer<Lexer<std::str::Chars>> {
        ast::Stmt::lex_starts_at(source, offset)
    }
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
    fn lex_starts_at(
        source: &str,
        offset: TextSize,
    ) -> SoftKeywordTransformer<Lexer<std::str::Chars>> {
        ast::Expr::lex_starts_at(source, offset)
    }
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
    fn lex_starts_at(
        source: &str,
        offset: TextSize,
    ) -> SoftKeywordTransformer<Lexer<std::str::Chars>> {
        ast::Expr::lex_starts_at(source, offset)
    }
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
    fn lex_starts_at(
        source: &str,
        offset: TextSize,
    ) -> SoftKeywordTransformer<Lexer<std::str::Chars>> {
        ast::Expr::lex_starts_at(source, offset)
    }
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
    fn lex_starts_at(
        source: &str,
        offset: TextSize,
    ) -> SoftKeywordTransformer<Lexer<std::str::Chars>> {
        ast::Expr::lex_starts_at(source, offset)
    }
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
    fn lex_starts_at(
        source: &str,
        offset: TextSize,
    ) -> SoftKeywordTransformer<Lexer<std::str::Chars>> {
        ast::Expr::lex_starts_at(source, offset)
    }
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
    fn lex_starts_at(
        source: &str,
        offset: TextSize,
    ) -> SoftKeywordTransformer<Lexer<std::str::Chars>> {
        ast::Expr::lex_starts_at(source, offset)
    }
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
    fn lex_starts_at(
        source: &str,
        offset: TextSize,
    ) -> SoftKeywordTransformer<Lexer<std::str::Chars>> {
        ast::Expr::lex_starts_at(source, offset)
    }
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
    fn lex_starts_at(
        source: &str,
        offset: TextSize,
    ) -> SoftKeywordTransformer<Lexer<std::str::Chars>> {
        ast::Expr::lex_starts_at(source, offset)
    }
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
    fn lex_starts_at(
        source: &str,
        offset: TextSize,
    ) -> SoftKeywordTransformer<Lexer<std::str::Chars>> {
        ast::Expr::lex_starts_at(source, offset)
    }
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
    fn lex_starts_at(
        source: &str,
        offset: TextSize,
    ) -> SoftKeywordTransformer<Lexer<std::str::Chars>> {
        ast::Expr::lex_starts_at(source, offset)
    }
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
    fn lex_starts_at(
        source: &str,
        offset: TextSize,
    ) -> SoftKeywordTransformer<Lexer<std::str::Chars>> {
        ast::Expr::lex_starts_at(source, offset)
    }
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
    fn lex_starts_at(
        source: &str,
        offset: TextSize,
    ) -> SoftKeywordTransformer<Lexer<std::str::Chars>> {
        ast::Expr::lex_starts_at(source, offset)
    }
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
    fn lex_starts_at(
        source: &str,
        offset: TextSize,
    ) -> SoftKeywordTransformer<Lexer<std::str::Chars>> {
        ast::Expr::lex_starts_at(source, offset)
    }
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
    fn lex_starts_at(
        source: &str,
        offset: TextSize,
    ) -> SoftKeywordTransformer<Lexer<std::str::Chars>> {
        ast::Expr::lex_starts_at(source, offset)
    }
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
    fn lex_starts_at(
        source: &str,
        offset: TextSize,
    ) -> SoftKeywordTransformer<Lexer<std::str::Chars>> {
        ast::Expr::lex_starts_at(source, offset)
    }
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
    fn lex_starts_at(
        source: &str,
        offset: TextSize,
    ) -> SoftKeywordTransformer<Lexer<std::str::Chars>> {
        ast::Expr::lex_starts_at(source, offset)
    }
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
    fn lex_starts_at(
        source: &str,
        offset: TextSize,
    ) -> SoftKeywordTransformer<Lexer<std::str::Chars>> {
        ast::Expr::lex_starts_at(source, offset)
    }
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
    fn lex_starts_at(
        source: &str,
        offset: TextSize,
    ) -> SoftKeywordTransformer<Lexer<std::str::Chars>> {
        ast::Expr::lex_starts_at(source, offset)
    }
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
    fn lex_starts_at(
        source: &str,
        offset: TextSize,
    ) -> SoftKeywordTransformer<Lexer<std::str::Chars>> {
        ast::Expr::lex_starts_at(source, offset)
    }
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
    fn lex_starts_at(
        source: &str,
        offset: TextSize,
    ) -> SoftKeywordTransformer<Lexer<std::str::Chars>> {
        ast::Expr::lex_starts_at(source, offset)
    }
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
    fn lex_starts_at(
        source: &str,
        offset: TextSize,
    ) -> SoftKeywordTransformer<Lexer<std::str::Chars>> {
        ast::Expr::lex_starts_at(source, offset)
    }
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
    fn lex_starts_at(
        source: &str,
        offset: TextSize,
    ) -> SoftKeywordTransformer<Lexer<std::str::Chars>> {
        ast::Expr::lex_starts_at(source, offset)
    }
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
    fn lex_starts_at(
        source: &str,
        offset: TextSize,
    ) -> SoftKeywordTransformer<Lexer<std::str::Chars>> {
        ast::Expr::lex_starts_at(source, offset)
    }
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
    fn lex_starts_at(
        source: &str,
        offset: TextSize,
    ) -> SoftKeywordTransformer<Lexer<std::str::Chars>> {
        ast::Expr::lex_starts_at(source, offset)
    }
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
    fn lex_starts_at(
        source: &str,
        offset: TextSize,
    ) -> SoftKeywordTransformer<Lexer<std::str::Chars>> {
        ast::Expr::lex_starts_at(source, offset)
    }
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
    fn lex_starts_at(
        source: &str,
        offset: TextSize,
    ) -> SoftKeywordTransformer<Lexer<std::str::Chars>> {
        ast::Expr::lex_starts_at(source, offset)
    }
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
    fn lex_starts_at(
        source: &str,
        offset: TextSize,
    ) -> SoftKeywordTransformer<Lexer<std::str::Chars>> {
        ast::Expr::lex_starts_at(source, offset)
    }
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
