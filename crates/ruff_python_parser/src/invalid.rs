/*!
Defines some helper routines for rejecting invalid Python programs.

These routines are named in a way that supports qualified use. For example,
`invalid::assignment_targets`.
*/

use {ruff_python_ast::Expr, ruff_text_size::TextSize};

use crate::lexer::{LexicalError, LexicalErrorType};

/// Returns an error for invalid assignment targets.
///
/// # Errors
///
/// This returns an error when any of the given expressions are themselves
/// or contain an expression that is invalid on the left hand side of an
/// assignment. For example, all literal expressions are invalid assignment
/// targets.
pub(crate) fn assignment_targets(targets: &[Expr]) -> Result<(), LexicalError> {
    for t in targets {
        assignment_target(t)?;
    }
    Ok(())
}

/// Returns an error if the given target is invalid for the left hand side of
/// an assignment.
///
/// # Errors
///
/// This returns an error when the given expression is itself or contains an
/// expression that is invalid on the left hand side of an assignment. For
/// example, all literal expressions are invalid assignment targets.
pub(crate) fn assignment_target(target: &Expr) -> Result<(), LexicalError> {
    // Allowing a glob import here because of its limited scope.
    #[allow(clippy::enum_glob_use)]
    use self::Expr::*;

    let err = |location: TextSize| -> LexicalError {
        let error = LexicalErrorType::AssignmentError;
        LexicalError { error, location }
    };
    match *target {
        BoolOp(ref e) => Err(err(e.range.start())),
        NamedExpr(ref e) => Err(err(e.range.start())),
        BinOp(ref e) => Err(err(e.range.start())),
        UnaryOp(ref e) => Err(err(e.range.start())),
        Lambda(ref e) => Err(err(e.range.start())),
        IfExp(ref e) => Err(err(e.range.start())),
        Dict(ref e) => Err(err(e.range.start())),
        Set(ref e) => Err(err(e.range.start())),
        ListComp(ref e) => Err(err(e.range.start())),
        SetComp(ref e) => Err(err(e.range.start())),
        DictComp(ref e) => Err(err(e.range.start())),
        GeneratorExp(ref e) => Err(err(e.range.start())),
        Await(ref e) => Err(err(e.range.start())),
        Yield(ref e) => Err(err(e.range.start())),
        YieldFrom(ref e) => Err(err(e.range.start())),
        Compare(ref e) => Err(err(e.range.start())),
        Call(ref e) => Err(err(e.range.start())),
        // FString is recursive, but all its forms are invalid as an
        // assignment target, so we can reject it without exploring it.
        FString(ref e) => Err(err(e.range.start())),
        StringLiteral(ref e) => Err(err(e.range.start())),
        BytesLiteral(ref e) => Err(err(e.range.start())),
        NumberLiteral(ref e) => Err(err(e.range.start())),
        BooleanLiteral(ref e) => Err(err(e.range.start())),
        NoneLiteral(ref e) => Err(err(e.range.start())),
        EllipsisLiteral(ref e) => Err(err(e.range.start())),
        // This isn't in the Python grammar but is Jupyter notebook specific.
        // It seems like this should be an error. It does also seem like the
        // parser prevents this from ever appearing as an assignment target
        // anyway. ---AG
        IpyEscapeCommand(ref e) => Err(err(e.range.start())),
        // The only nested expressions allowed as an assignment target
        // are star exprs, lists and tuples.
        Starred(ref e) => assignment_target(&e.value),
        List(ref e) => assignment_targets(&e.elts),
        Tuple(ref e) => assignment_targets(&e.elts),
        // Subscript is recursive and can be invalid, but aren't syntax errors.
        // For example, `5[1] = 42` is a type error.
        Subscript(_) => Ok(()),
        // Similar to Subscript, e.g., `5[1:2] = [42]` is a type error.
        Slice(_) => Ok(()),
        // Similar to Subscript, e.g., `"foo".y = 42` is an attribute error.
        Attribute(_) => Ok(()),
        // These are always valid as assignment targets.
        Name(_) => Ok(()),
    }
}
