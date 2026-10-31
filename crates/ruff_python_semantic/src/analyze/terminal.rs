use ruff_python_ast::flat::helpers::map_callable;
use ruff_python_ast::{
    ElifElseClause, MatchCase, Pattern, StmtFunctionDef,
    flat::{self, fields as ast},
};

use crate::model::SemanticModel;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Terminal {
    /// There is no known terminal.
    None,
    /// There is an implicit return (e.g., a path that doesn't return).
    Implicit,
    /// Every path through the function ends with a `raise` statement.
    Raise,
    /// Every path through the function ends with a `raise NotImplementedError` statement.
    RaiseNotImplemented,
    /// No path through the function ends with a `return` statement.
    Return,
    /// Every path through the function ends with a `return` or `raise` statement.
    RaiseOrReturn,
    /// At least one path through the function ends with a `return` statement.
    ConditionalReturn,
}

impl Terminal {
    /// Returns the [`Terminal`] behavior of the function, if it can be determined.
    pub fn from_function(
        function: flat::View<'_, StmtFunctionDef>,
        semantic: &SemanticModel,
    ) -> Terminal {
        Self::from_body(function.body(), semantic)
    }

    /// Returns `true` if the [`Terminal`] behavior includes at least one `return` path.
    fn has_any_return(self) -> bool {
        matches!(
            self,
            Self::Return | Self::RaiseOrReturn | Self::ConditionalReturn
        )
    }

    /// Returns `true` if the [`Terminal`] behavior includes at least one implicit `return` path.
    pub fn has_implicit_return(self) -> bool {
        matches!(self, Self::None | Self::Implicit | Self::ConditionalReturn)
    }

    /// Returns `true` if the [`Terminal`] behavior is `Raise` or `RaiseNotImplemented`.
    pub fn is_always_raise(self) -> bool {
        matches!(self, Self::Raise | Self::RaiseNotImplemented)
    }

    /// Returns the [`Terminal`] behavior of the body, if it can be determined.
    fn from_body(stmts: flat::Suite<'_>, semantic: &SemanticModel) -> Terminal {
        let mut terminal = Terminal::None;

        for stmt in stmts {
            match stmt.fields() {
                ast::Stmt::For(ast::StmtFor { body, orelse, .. })
                | ast::Stmt::While(ast::StmtWhile { body, orelse, .. }) => {
                    if always_breaks(body) {
                        continue;
                    }

                    terminal = terminal.and_then(Self::from_body(body, semantic));

                    if !sometimes_breaks(body, semantic) {
                        terminal = terminal.and_then(Self::from_body(orelse, semantic));
                    }
                }
                ast::Stmt::If(ast::StmtIf {
                    body,
                    elif_else_clauses,
                    ..
                }) => {
                    let branch_terminal = Terminal::branches(
                        std::iter::once(Self::from_body(body, semantic)).chain(
                            elif_else_clauses
                                .iter()
                                .map(|clause| Self::from_body(clause.body(), semantic)),
                        ),
                    );

                    // If the `if` statement is known to be exhaustive (by way of including an
                    // `else`)...
                    if elif_else_clauses
                        .iter()
                        .any(|clause| clause.test().is_none())
                    {
                        // And all branches return, then the `if` statement returns.
                        terminal = terminal.and_then(branch_terminal);
                    } else if branch_terminal.has_any_return() {
                        // Otherwise, if any branch returns, we know this can't be a
                        // non-returning function.
                        terminal = terminal.and_then(Terminal::ConditionalReturn);
                    }
                }
                ast::Stmt::Match(ast::StmtMatch { cases, .. }) => {
                    let branch_terminal = terminal.and_then(Terminal::branches(
                        cases
                            .iter()
                            .map(|case| Self::from_body(case.body(), semantic)),
                    ));

                    // If the `match` is known to be exhaustive (by way of including a wildcard
                    // pattern)...
                    if cases.iter().any(is_wildcard) {
                        // And all branches return, then the `match` statement returns.
                        terminal = terminal.and_then(branch_terminal);
                    } else {
                        // Otherwise, if any branch returns, we know this can't be a
                        // non-returning function.
                        if branch_terminal.has_any_return() {
                            terminal = terminal.and_then(Terminal::ConditionalReturn);
                        }
                    }
                }
                ast::Stmt::Try(ast::StmtTry {
                    body,
                    handlers,
                    orelse,
                    finalbody,
                    ..
                }) => {
                    // If the body returns, then this can't be a non-returning function. We assume
                    // that _any_ statement in the body could raise an exception, so we don't
                    // consider the body to be exhaustive. In other words, we assume the exception
                    // handlers exist for a reason.
                    let body_terminal = Self::from_body(body, semantic);
                    if body_terminal.has_any_return() {
                        terminal = terminal.and_then(Terminal::ConditionalReturn);
                    }

                    // If the `finally` block returns, the `try` block must also return. (Similarly,
                    // if the `finally` block raises, the `try` block must also raise.)
                    terminal = terminal.and_then(Self::from_body(finalbody, semantic));

                    let branch_terminal = Terminal::branches(handlers.iter().map(|handler| {
                        let ast::ExceptHandler::ExceptHandler(ast::ExceptHandlerExceptHandler {
                            body,
                            ..
                        }) = handler.fields();
                        Self::from_body(body, semantic)
                    }));

                    if orelse.is_empty() {
                        // If there's no `else`, we may fall through, so only mark that this can't
                        // be a non-returning function if any of the branches return.
                        if branch_terminal.has_any_return() {
                            terminal = terminal.and_then(Terminal::ConditionalReturn);
                        }
                    } else {
                        // If there's an `else`, we won't fall through. If all the handlers and
                        // the `else` block return,, the `try` block also returns.
                        terminal = terminal.and_then(
                            branch_terminal.branch(Terminal::from_body(orelse, semantic)),
                        );
                    }
                }
                ast::Stmt::With(ast::StmtWith { body, .. }) => {
                    terminal = terminal.and_then(Self::from_body(body, semantic));
                }
                ast::Stmt::Return(_) => {
                    terminal = terminal.and_then(Terminal::RaiseOrReturn);
                }
                ast::Stmt::Raise(ast::StmtRaise { node: raise, .. }) => {
                    // Check if this raise is NotImplementedError
                    let is_not_implemented = raise.exc().is_some_and(|exc| {
                        let exc = map_callable(exc);
                        semantic.match_builtin_expr(exc, "NotImplementedError")
                    });

                    if is_not_implemented {
                        terminal = terminal.and_then(Terminal::RaiseNotImplemented);
                    } else {
                        terminal = terminal.and_then(Terminal::Raise);
                    }
                }
                _ => {}
            }
        }

        match terminal {
            Terminal::None => Terminal::Implicit,
            _ => terminal,
        }
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

            // If one of the operators is `Raise` or `RaiseNotImplemented`, then the function ends
            // with an explicit `raise` or `return` statement.
            (Self::Raise | Self::RaiseNotImplemented, Self::ConditionalReturn) => {
                Self::RaiseOrReturn
            }
            (Self::ConditionalReturn, Self::Raise | Self::RaiseNotImplemented) => {
                Self::RaiseOrReturn
            }

            // If one of the operators is `Return`, then the function returns.
            (Self::Return, Self::ConditionalReturn) => Self::Return,
            (Self::ConditionalReturn, Self::Return) => Self::Return,

            // All paths through the function end with a `raise` statement.
            (Self::Raise, Self::Raise) => Self::Raise,
            // All paths through the function end with a `raise NotImplementedError` statement.
            (Self::RaiseNotImplemented, Self::RaiseNotImplemented) => Self::RaiseNotImplemented,
            // Mixed raises: if one is NotImplementedError and one is regular Raise, result is Raise
            (Self::RaiseNotImplemented, Self::Raise) | (Self::Raise, Self::RaiseNotImplemented) => {
                Self::Raise
            }

            // All paths through the function end with a `return` statement.
            (Self::Return, Self::Return) => Self::Return,

            // All paths through the function end with a `return` or `raise` statement.
            (Self::Raise | Self::RaiseNotImplemented, Self::Return) => Self::RaiseOrReturn,

            // All paths through the function end with a `return` or `raise` statement.
            (Self::Return, Self::Raise | Self::RaiseNotImplemented) => Self::RaiseOrReturn,

            // All paths through the function end with a `return` or `raise` statement.
            (Self::RaiseOrReturn, _) => Self::RaiseOrReturn,
            (_, Self::RaiseOrReturn) => Self::RaiseOrReturn,
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
            (Self::Implicit, Self::Raise | Self::RaiseNotImplemented) => Self::Implicit,
            (Self::Raise | Self::RaiseNotImplemented, Self::Implicit) => Self::Implicit,
            (Self::Implicit, Self::Return) => Self::ConditionalReturn,
            (Self::Return, Self::Implicit) => Self::ConditionalReturn,
            (Self::Implicit, Self::RaiseOrReturn) => Self::ConditionalReturn,
            (Self::RaiseOrReturn, Self::Implicit) => Self::ConditionalReturn,
            (Self::Implicit, Self::ConditionalReturn) => Self::ConditionalReturn,
            (Self::ConditionalReturn, Self::Implicit) => Self::ConditionalReturn,

            // If both operators are conditional returns, the result is a conditional return.
            (Self::ConditionalReturn, Self::ConditionalReturn) => Self::ConditionalReturn,

            (Self::Raise | Self::RaiseNotImplemented, Self::ConditionalReturn) => {
                Self::RaiseOrReturn
            }
            (Self::ConditionalReturn, Self::Raise | Self::RaiseNotImplemented) => {
                Self::RaiseOrReturn
            }

            (Self::Return, Self::ConditionalReturn) => Self::Return,
            (Self::ConditionalReturn, Self::Return) => Self::Return,

            // All paths through the function end with a `raise` statement.
            (Self::Raise, Self::Raise) => Self::Raise,
            // All paths through the function end with a `raise NotImplementedError` statement.
            (Self::RaiseNotImplemented, Self::RaiseNotImplemented) => Self::RaiseNotImplemented,
            // Mixed raises: if one is NotImplementedError and one is regular Raise, result is Raise
            (Self::RaiseNotImplemented, Self::Raise) | (Self::Raise, Self::RaiseNotImplemented) => {
                Self::Raise
            }
            // All paths through the function end with a `return` statement.
            (Self::Return, Self::Return) => Self::Return,
            // All paths through the function end with a `return` or `raise` statement.
            (Self::Raise | Self::RaiseNotImplemented, Self::Return) => Self::RaiseOrReturn,
            // All paths through the function end with a `return` or `raise` statement.
            (Self::Return, Self::Raise | Self::RaiseNotImplemented) => Self::RaiseOrReturn,
            // All paths through the function end with a `return` or `raise` statement.
            (Self::RaiseOrReturn, _) => Self::RaiseOrReturn,
            (_, Self::RaiseOrReturn) => Self::RaiseOrReturn,
        }
    }

    /// Combine a series of [`Terminal`] operators.
    fn branches(iter: impl Iterator<Item = Terminal>) -> Terminal {
        iter.fold(Terminal::None, Terminal::branch)
    }
}

/// Returns `true` if the body may break via a `break` statement.
fn sometimes_breaks(stmts: flat::Suite<'_>, semantic: &SemanticModel) -> bool {
    for stmt in stmts {
        match stmt.classify() {
            flat::StmtKind::For(stmt_for) => {
                let body = stmt_for.body();
                let orelse = stmt_for.orelse();

                if Terminal::from_body(body, semantic).has_any_return() {
                    return false;
                }
                if sometimes_breaks(orelse, semantic) {
                    return true;
                }
            }
            flat::StmtKind::While(stmt_while) => {
                let body = stmt_while.body();
                let orelse = stmt_while.orelse();

                if Terminal::from_body(body, semantic).has_any_return() {
                    return false;
                }
                if sometimes_breaks(orelse, semantic) {
                    return true;
                }
            }
            flat::StmtKind::If(stmt_if)
                if std::iter::once(stmt_if.body())
                    .chain(
                        stmt_if
                            .elif_else_clauses()
                            .iter()
                            .map(flat::View::<ElifElseClause>::body),
                    )
                    .any(|body| sometimes_breaks(body, semantic)) =>
            {
                return true;
            }
            flat::StmtKind::Match(stmt_match)
                if stmt_match
                    .cases()
                    .iter()
                    .any(|case| sometimes_breaks(case.body(), semantic)) =>
            {
                return true;
            }
            flat::StmtKind::Try(stmt_try)
                if (sometimes_breaks(stmt_try.body(), semantic)
                    || stmt_try.handlers().iter().any(|handler| {
                        let ast::ExceptHandler::ExceptHandler(ast::ExceptHandlerExceptHandler {
                            body,
                            ..
                        }) = handler.fields();
                        sometimes_breaks(body, semantic)
                    })
                    || sometimes_breaks(stmt_try.orelse(), semantic)
                    || sometimes_breaks(stmt_try.finalbody(), semantic)) =>
            {
                return true;
            }
            flat::StmtKind::With(stmt_with) if sometimes_breaks(stmt_with.body(), semantic) => {
                return true;
            }
            flat::StmtKind::Break(_) => return true,
            flat::StmtKind::Return(_) => return false,
            flat::StmtKind::Raise(_) => return false,
            _ => {}
        }
    }
    false
}

/// Returns `true` if the body may break via a `break` statement.
fn always_breaks(stmts: flat::Suite<'_>) -> bool {
    for stmt in stmts {
        match stmt.classify() {
            flat::StmtKind::Break(_) => return true,
            flat::StmtKind::Return(_) => return false,
            flat::StmtKind::Raise(_) => return false,
            _ => {}
        }
    }
    false
}

/// Returns true if the [`MatchCase`] is a wildcard pattern.
fn is_wildcard(pattern: flat::View<'_, MatchCase>) -> bool {
    /// Returns true if the [`Pattern`] is a wildcard pattern.
    fn is_wildcard_pattern(pattern: flat::View<'_, Pattern>) -> bool {
        match pattern.classify() {
            flat::PatternKind::MatchValue(_)
            | flat::PatternKind::MatchSingleton(_)
            | flat::PatternKind::MatchSequence(_)
            | flat::PatternKind::MatchMapping(_)
            | flat::PatternKind::MatchClass(_)
            | flat::PatternKind::MatchStar(_) => false,

            flat::PatternKind::MatchAs(pattern_match_as) => {
                let pattern = pattern_match_as.pattern();
                pattern.is_none()
            }
            flat::PatternKind::MatchOr(pattern_match_or) => {
                let patterns = pattern_match_or.patterns();

                patterns.iter().all(is_wildcard_pattern)
            }
        }
    }

    pattern.guard().is_none() && is_wildcard_pattern(pattern.pattern())
}
