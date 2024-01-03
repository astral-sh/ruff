use ruff_python_ast::{self as ast, ExceptHandler, Stmt};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Terminal {
    /// There is no known terminal.
    None,
    /// There is an implicit return (e.g., a path that doesn't return).
    Implicit,
    /// Every path through the function ends with a `raise` statement.
    Raise,
    /// No path through the function ends with a `return` statement.
    Return,
    /// Every path through the function ends with a `return` or `raise` statement.
    Explicit,
    /// At least one path through the function ends with a `return` statement.
    ConditionalReturn,
}

impl Terminal {
    /// Returns the [`Terminal`] behavior of the function, if it can be determined.
    pub fn from_function(function: &ast::StmtFunctionDef) -> Terminal {
        Self::from_body(&function.body)
    }

    /// Returns the [`Terminal`] behavior of the body, if it can be determined.
    fn from_body(stmts: &[Stmt]) -> Terminal {
        let mut terminal = Terminal::None;

        for stmt in stmts {
            match stmt {
                Stmt::For(ast::StmtFor { body, orelse, .. })
                | Stmt::While(ast::StmtWhile { body, orelse, .. }) => {
                    if always_breaks(body) {
                        continue;
                    }

                    terminal = terminal.and_then(Self::from_body(body));

                    if !sometimes_breaks(body) {
                        terminal = terminal.and_then(Self::from_body(orelse));
                    }
                }
                Stmt::If(ast::StmtIf {
                    body,
                    elif_else_clauses,
                    ..
                }) => {
                    let branch_terminal = Terminal::branches(
                        std::iter::once(Self::from_body(body)).chain(
                            elif_else_clauses
                                .iter()
                                .map(|clause| Self::from_body(&clause.body)),
                        ),
                    );

                    // If the `if` statement is known to be exhaustive (by way of including an
                    // `else`)...
                    if elif_else_clauses.iter().any(|clause| clause.test.is_none()) {
                        // And all branches return, then the `if` statement returns.
                        terminal = terminal.and_then(branch_terminal);
                    } else if branch_terminal.has_return() {
                        // Otherwise, if any branch returns, we know this can't be a
                        // non-returning function.
                        terminal = terminal.and_then(Terminal::ConditionalReturn);
                    }
                }
                Stmt::Match(ast::StmtMatch { cases, .. }) => {
                    let branch_terminal = terminal.and_then(Terminal::branches(
                        cases.iter().map(|case| Self::from_body(&case.body)),
                    ));

                    // If the `match` is known to be exhaustive (by way of including a wildcard
                    // pattern)...
                    if cases.iter().any(is_wildcard) {
                        // And all branches return, then the `match` statement returns.
                        terminal = terminal.and_then(branch_terminal);
                    } else {
                        // Otherwise, if any branch returns, we know this can't be a
                        // non-returning function.
                        if branch_terminal.has_return() {
                            terminal = terminal.and_then(Terminal::ConditionalReturn);
                        }
                    }
                }
                Stmt::Try(ast::StmtTry {
                    body,
                    handlers,
                    orelse,
                    finalbody,
                    ..
                }) => {
                    // If the body returns, then this can't be a non-returning function.
                    let body_terminal = Self::from_body(body);
                    if body_terminal.has_return() {
                        terminal = terminal.and_then(Terminal::ConditionalReturn);
                    }

                    // If the `finally` block returns, the `try` block must also return.
                    terminal = terminal.and_then(Self::from_body(finalbody));

                    // If all the handlers return, the `try` block may also return.
                    let branch_terminal = Terminal::branches(handlers.iter().map(|handler| {
                        let ExceptHandler::ExceptHandler(ast::ExceptHandlerExceptHandler {
                            body,
                            ..
                        }) = handler;
                        Self::from_body(body)
                    }));

                    if orelse.is_empty() {
                        // If there's no `else`, we may fall through.
                        if branch_terminal.has_return() {
                            terminal = terminal.and_then(Terminal::ConditionalReturn);
                        }
                    } else {
                        // If there's an `else`, we won't fall through.
                        terminal =
                            terminal.and_then(branch_terminal.branch(Terminal::from_body(orelse)));
                    }
                }
                Stmt::With(ast::StmtWith { body, .. }) => {
                    terminal = terminal.and_then(Self::from_body(body));
                }
                Stmt::Return(_) => {
                    terminal = terminal.and_then(Terminal::Explicit);
                }
                Stmt::Raise(_) => {
                    terminal = terminal.and_then(Terminal::Raise);
                }
                _ => {}
            }
        }

        match terminal {
            Terminal::None => Terminal::Implicit,
            _ => terminal,
        }
    }

    /// Returns `true` if the [`Terminal`] behavior includes at least one `return` path.
    fn has_return(self) -> bool {
        matches!(
            self,
            Self::Return | Self::Explicit | Self::ConditionalReturn
        )
    }

    /// Combine two [`Terminal`] operators, with one appearing after the other.
    fn and_then(self, other: Self) -> Self {
        match (self, other) {
            // If one of the operators is `None`, the result is the other operator.
            (Self::None, other) => other,
            (other, Self::None) => other,

            // If one of the operators is `Implicit`, the result is the other operator.
            (Self::Implicit, other) => other,
            (other, Self::Implicit) => other,

            // If both operators are conditional returns, the result is a conditional return.
            (Self::ConditionalReturn, Self::ConditionalReturn) => Self::ConditionalReturn,

            // If one of the operators is `Raise`, then the function ends with an explicit `raise`
            // or `return` statement.
            (Self::Raise, Self::ConditionalReturn) => Self::Explicit,
            (Self::ConditionalReturn, Self::Raise) => Self::Explicit,

            // If one of the operators is `Return`, then the function returns.
            (Self::Return, Self::ConditionalReturn) => Self::Return,
            (Self::ConditionalReturn, Self::Return) => Self::Return,

            // All paths through the function end with a `raise` statement.
            (Self::Raise, Self::Raise) => Self::Raise,

            // All paths through the function end with a `return` statement.
            (Self::Return, Self::Return) => Self::Return,

            // All paths through the function end with a `return` or `raise` statement.
            (Self::Raise, Self::Return) => Self::Explicit,

            // All paths through the function end with a `return` or `raise` statement.
            (Self::Return, Self::Raise) => Self::Explicit,

            // All paths through the function end with a `return` or `raise` statement.
            (Self::Explicit, _) => Self::Explicit,
            (_, Self::Explicit) => Self::Explicit,
        }
    }

    /// Combine two [`Terminal`] operators from different branches.
    fn branch(self, other: Self) -> Self {
        match (self, other) {
            // If one of the operators is `None`, the result is the other operator.
            (Self::None, other) => other,
            (other, Self::None) => other,

            // If one of the operators is `Implicit`, the other operator should be downgraded.
            (Self::Implicit, Self::Implicit) => Self::Implicit,
            (Self::Implicit, Self::Raise) => Self::Implicit,
            (Self::Raise, Self::Implicit) => Self::Implicit,
            (Self::Implicit, Self::Return) => Self::ConditionalReturn,
            (Self::Return, Self::Implicit) => Self::ConditionalReturn,
            (Self::Implicit, Self::Explicit) => Self::ConditionalReturn,
            (Self::Explicit, Self::Implicit) => Self::ConditionalReturn,
            (Self::Implicit, Self::ConditionalReturn) => Self::ConditionalReturn,
            (Self::ConditionalReturn, Self::Implicit) => Self::ConditionalReturn,

            // If both operators are conditional returns, the result is a conditional return.
            (Self::ConditionalReturn, Self::ConditionalReturn) => Self::ConditionalReturn,

            (Self::Raise, Self::ConditionalReturn) => Self::Explicit,
            (Self::ConditionalReturn, Self::Raise) => Self::Explicit,

            (Self::Return, Self::ConditionalReturn) => Self::Return,
            (Self::ConditionalReturn, Self::Return) => Self::Return,

            // All paths through the function end with a `raise` statement.
            (Self::Raise, Self::Raise) => Self::Raise,
            // All paths through the function end with a `return` statement.
            (Self::Return, Self::Return) => Self::Return,
            // All paths through the function end with a `return` or `raise` statement.
            (Self::Raise, Self::Return) => Self::Explicit,
            // All paths through the function end with a `return` or `raise` statement.
            (Self::Return, Self::Raise) => Self::Explicit,
            // All paths through the function end with a `return` or `raise` statement.
            (Self::Explicit, _) => Self::Explicit,
            (_, Self::Explicit) => Self::Explicit,
        }
    }

    /// Combine a series of [`Terminal`] operators.
    fn branches(iter: impl Iterator<Item = Terminal>) -> Terminal {
        iter.fold(Terminal::None, Terminal::branch)
    }
}

/// Returns `true` if the body may break via a `break` statement.
fn sometimes_breaks(stmts: &[Stmt]) -> bool {
    for stmt in stmts {
        match stmt {
            Stmt::For(ast::StmtFor { body, orelse, .. }) => {
                if Terminal::from_body(body).has_return() {
                    return false;
                }
                if sometimes_breaks(orelse) {
                    return true;
                }
            }
            Stmt::While(ast::StmtWhile { body, orelse, .. }) => {
                if Terminal::from_body(body).has_return() {
                    return false;
                }
                if sometimes_breaks(orelse) {
                    return true;
                }
            }
            Stmt::If(ast::StmtIf {
                body,
                elif_else_clauses,
                ..
            }) => {
                if std::iter::once(body)
                    .chain(elif_else_clauses.iter().map(|clause| &clause.body))
                    .any(|body| sometimes_breaks(body))
                {
                    return true;
                }
            }
            Stmt::Match(ast::StmtMatch { cases, .. }) => {
                if cases.iter().any(|case| sometimes_breaks(&case.body)) {
                    return true;
                }
            }
            Stmt::Try(ast::StmtTry {
                body,
                handlers,
                orelse,
                finalbody,
                ..
            }) => {
                if sometimes_breaks(body)
                    || handlers.iter().any(|handler| {
                        let ExceptHandler::ExceptHandler(ast::ExceptHandlerExceptHandler {
                            body,
                            ..
                        }) = handler;
                        sometimes_breaks(body)
                    })
                    || sometimes_breaks(orelse)
                    || sometimes_breaks(finalbody)
                {
                    return true;
                }
            }
            Stmt::With(ast::StmtWith { body, .. }) => {
                if sometimes_breaks(body) {
                    return true;
                }
            }
            Stmt::Break(_) => return true,
            Stmt::Return(_) => return false,
            Stmt::Raise(_) => return false,
            _ => {}
        }
    }
    false
}

/// Returns `true` if the body may break via a `break` statement.
fn always_breaks(stmts: &[Stmt]) -> bool {
    for stmt in stmts {
        match stmt {
            Stmt::Break(_) => return true,
            Stmt::Return(_) => return false,
            Stmt::Raise(_) => return false,
            _ => {}
        }
    }
    false
}

/// Returns true if the [`MatchCase`] is a wildcard pattern.
fn is_wildcard(pattern: &ast::MatchCase) -> bool {
    /// Returns true if the [`Pattern`] is a wildcard pattern.
    fn is_wildcard_pattern(pattern: &ast::Pattern) -> bool {
        match pattern {
            ast::Pattern::MatchValue(_)
            | ast::Pattern::MatchSingleton(_)
            | ast::Pattern::MatchSequence(_)
            | ast::Pattern::MatchMapping(_)
            | ast::Pattern::MatchClass(_)
            | ast::Pattern::MatchStar(_) => false,
            ast::Pattern::MatchAs(ast::PatternMatchAs { pattern, .. }) => pattern.is_none(),
            ast::Pattern::MatchOr(ast::PatternMatchOr { patterns, .. }) => {
                patterns.iter().all(is_wildcard_pattern)
            }
        }
    }

    pattern.guard.is_none() && is_wildcard_pattern(&pattern.pattern)
}
