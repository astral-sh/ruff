use ruff_python_ast::{self as ast, ExceptHandler, Stmt};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Terminal {
    /// There is no known terminal (e.g., an implicit return).
    None,
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

                    terminal = terminal.union(Self::from_body(body));

                    if !sometimes_breaks(body) {
                        terminal = terminal.union(Self::from_body(orelse));
                    }
                }
                Stmt::If(ast::StmtIf {
                    body,
                    elif_else_clauses,
                    ..
                }) => {
                    let branch_terminal = Terminal::combine(
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
                        terminal = terminal.union(branch_terminal);
                    } else if branch_terminal.has_return() {
                        // Otherwise, if any branch returns, we know this can't be a
                        // non-returning function.
                        terminal = terminal.union(Terminal::ConditionalReturn);
                    }
                }
                Stmt::Match(ast::StmtMatch { cases, .. }) => {
                    // Note: we assume the `match` is exhaustive.
                    terminal = terminal.union(Terminal::combine(
                        cases.iter().map(|case| Self::from_body(&case.body)),
                    ));
                }
                Stmt::Try(ast::StmtTry {
                    handlers,
                    orelse,
                    finalbody,
                    ..
                }) => {
                    // If the `finally` block returns, the `try` block must also return.
                    terminal = terminal.union(Self::from_body(finalbody));

                    // If the else block and all the handlers return, the `try` block must also
                    // return.
                    let branch_terminal =
                        Terminal::combine(std::iter::once(Self::from_body(orelse)).chain(
                            handlers.iter().map(|handler| {
                                let ExceptHandler::ExceptHandler(ast::ExceptHandlerExceptHandler {
                                    body,
                                    ..
                                }) = handler;
                                Self::from_body(body)
                            }),
                        ));

                    if orelse.is_empty() {
                        // If there's no `else`, we may fall through.
                        if branch_terminal.has_return() {
                            terminal = terminal.union(Terminal::ConditionalReturn);
                        }
                    } else {
                        // If there's an `else`, we may not fall through.
                        terminal = terminal.union(branch_terminal);
                    }
                }
                Stmt::With(ast::StmtWith { body, .. }) => {
                    terminal = terminal.union(Self::from_body(body));
                }
                Stmt::Return(_) => {
                    terminal = terminal.union(Terminal::Explicit);
                }
                Stmt::Raise(_) => {
                    terminal = terminal.union(Terminal::Raise);
                }
                _ => {}
            }
        }
        terminal
    }

    /// Returns `true` if the [`Terminal`] behavior includes at least one `return` path.
    fn has_return(self) -> bool {
        matches!(
            self,
            Self::Return | Self::Explicit | Self::ConditionalReturn
        )
    }

    /// Combine two [`Terminal`] operators.
    fn union(self, other: Self) -> Self {
        match (self, other) {
            (Self::None, other) => other,
            (other, Self::None) => other,
            (Self::Explicit, _) => Self::Explicit,
            (_, Self::Explicit) => Self::Explicit,
            (Self::ConditionalReturn, Self::ConditionalReturn) => Self::ConditionalReturn,
            (Self::Raise, Self::ConditionalReturn) => Self::Explicit,
            (Self::ConditionalReturn, Self::Raise) => Self::Explicit,
            (Self::Return, Self::ConditionalReturn) => Self::Return,
            (Self::ConditionalReturn, Self::Return) => Self::Return,
            (Self::Raise, Self::Raise) => Self::Raise,
            (Self::Return, Self::Return) => Self::Return,
            (Self::Raise, Self::Return) => Self::Explicit,
            (Self::Return, Self::Raise) => Self::Explicit,
        }
    }

    /// Combine a series of [`Terminal`] operators.
    fn combine(iter: impl Iterator<Item = Terminal>) -> Terminal {
        iter.fold(Terminal::None, Self::union)
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
