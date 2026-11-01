use ruff_python_ast::helpers::map_callable;
use ruff_python_ast::{
    self as ast, ElifElseClause, MatchCase, Pattern, PatternKind, StmtFunctionDef, StmtKind, Suite,
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
    pub fn from_function(function: StmtFunctionDef<'_>, semantic: &SemanticModel) -> Terminal {
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
    fn from_body(stmts: Suite<'_>, semantic: &SemanticModel) -> Terminal {
        let mut terminal = Terminal::None;

        for stmt in stmts {
            match stmt.fields() {
                ast::StmtFields::For(ast::StmtForFields { body, orelse, .. })
                | ast::StmtFields::While(ast::StmtWhileFields { body, orelse, .. }) => {
                    if always_breaks(body) {
                        continue;
                    }

                    terminal = terminal.and_then(Self::from_body(body, semantic));

                    if !sometimes_breaks(body, semantic) {
                        terminal = terminal.and_then(Self::from_body(orelse, semantic));
                    }
                }
                ast::StmtFields::If(ast::StmtIfFields {
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
                ast::StmtFields::Match(ast::StmtMatchFields { cases, .. }) => {
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
                ast::StmtFields::Try(ast::StmtTryFields {
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
                        let ast::ExceptHandlerFields::ExceptHandler(
                            ast::ExceptHandlerExceptHandlerFields { body, .. },
                        ) = handler.fields();
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
                ast::StmtFields::With(ast::StmtWithFields { body, .. }) => {
                    terminal = terminal.and_then(Self::from_body(body, semantic));
                }
                ast::StmtFields::Return(_) => {
                    terminal = terminal.and_then(Terminal::RaiseOrReturn);
                }
                ast::StmtFields::Raise(ast::StmtRaiseFields { node: raise, .. }) => {
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
fn sometimes_breaks(stmts: Suite<'_>, semantic: &SemanticModel) -> bool {
    for stmt in stmts {
        match stmt.classify() {
            StmtKind::For(stmt_for) => {
                let body = stmt_for.body();
                let orelse = stmt_for.orelse();

                if Terminal::from_body(body, semantic).has_any_return() {
                    return false;
                }
                if sometimes_breaks(orelse, semantic) {
                    return true;
                }
            }
            StmtKind::While(stmt_while) => {
                let body = stmt_while.body();
                let orelse = stmt_while.orelse();

                if Terminal::from_body(body, semantic).has_any_return() {
                    return false;
                }
                if sometimes_breaks(orelse, semantic) {
                    return true;
                }
            }
            StmtKind::If(stmt_if)
                if std::iter::once(stmt_if.body())
                    .chain(stmt_if.elif_else_clauses().iter().map(ElifElseClause::body))
                    .any(|body| sometimes_breaks(body, semantic)) =>
            {
                return true;
            }
            StmtKind::Match(stmt_match)
                if stmt_match
                    .cases()
                    .iter()
                    .any(|case| sometimes_breaks(case.body(), semantic)) =>
            {
                return true;
            }
            StmtKind::Try(stmt_try)
                if (sometimes_breaks(stmt_try.body(), semantic)
                    || stmt_try.handlers().iter().any(|handler| {
                        let ast::ExceptHandlerFields::ExceptHandler(
                            ast::ExceptHandlerExceptHandlerFields { body, .. },
                        ) = handler.fields();
                        sometimes_breaks(body, semantic)
                    })
                    || sometimes_breaks(stmt_try.orelse(), semantic)
                    || sometimes_breaks(stmt_try.finalbody(), semantic)) =>
            {
                return true;
            }
            StmtKind::With(stmt_with) if sometimes_breaks(stmt_with.body(), semantic) => {
                return true;
            }
            StmtKind::Break(_) => return true,
            StmtKind::Return(_) => return false,
            StmtKind::Raise(_) => return false,
            _ => {}
        }
    }
    false
}

/// Returns `true` if the body may break via a `break` statement.
fn always_breaks(stmts: Suite<'_>) -> bool {
    for stmt in stmts {
        match stmt.classify() {
            StmtKind::Break(_) => return true,
            StmtKind::Return(_) => return false,
            StmtKind::Raise(_) => return false,
            _ => {}
        }
    }
    false
}

/// Returns true if the [`MatchCase`] is a wildcard pattern.
fn is_wildcard(pattern: MatchCase<'_>) -> bool {
    /// Returns true if the [`Pattern`] is a wildcard pattern.
    fn is_wildcard_pattern(pattern: Pattern<'_>) -> bool {
        match pattern.classify() {
            PatternKind::MatchValue(_)
            | PatternKind::MatchSingleton(_)
            | PatternKind::MatchSequence(_)
            | PatternKind::MatchMapping(_)
            | PatternKind::MatchClass(_)
            | PatternKind::MatchStar(_) => false,

            PatternKind::MatchAs(pattern_match_as) => {
                let pattern = pattern_match_as.pattern();
                pattern.is_none()
            }
            PatternKind::MatchOr(pattern_match_or) => {
                let patterns = pattern_match_or.patterns();

                patterns.iter().all(is_wildcard_pattern)
            }
        }
    }

    pattern.guard().is_none() && is_wildcard_pattern(pattern.pattern())
}
