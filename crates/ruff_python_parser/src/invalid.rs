/*!
Defines some helper routines for rejecting invalid Python programs.

These routines are named in a way that supports qualified use. For example,
`invalid::assignment_targets`.
*/

use ruff_python_ast::Expr;

use ruff_text_size::TextRange;

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

    let err = |location: TextRange| -> LexicalError {
        let error = LexicalErrorType::AssignmentError;
        LexicalError::new(error, location)
    };
    match *target {
        BoolOp(ref e) => Err(err(e.range)),
        NamedExpr(ref e) => Err(err(e.range)),
        BinOp(ref e) => Err(err(e.range)),
        UnaryOp(ref e) => Err(err(e.range)),
        Lambda(ref e) => Err(err(e.range)),
        IfExp(ref e) => Err(err(e.range)),
        Dict(ref e) => Err(err(e.range)),
        Set(ref e) => Err(err(e.range)),
        ListComp(ref e) => Err(err(e.range)),
        SetComp(ref e) => Err(err(e.range)),
        DictComp(ref e) => Err(err(e.range)),
        GeneratorExp(ref e) => Err(err(e.range)),
        Await(ref e) => Err(err(e.range)),
        Yield(ref e) => Err(err(e.range)),
        YieldFrom(ref e) => Err(err(e.range)),
        Compare(ref e) => Err(err(e.range)),
        Call(ref e) => Err(err(e.range)),
        // FString is recursive, but all its forms are invalid as an
        // assignment target, so we can reject it without exploring it.
        FString(ref e) => Err(err(e.range)),
        StringLiteral(ref e) => Err(err(e.range)),
        BytesLiteral(ref e) => Err(err(e.range)),
        NumberLiteral(ref e) => Err(err(e.range)),
        BooleanLiteral(ref e) => Err(err(e.range)),
        NoneLiteral(ref e) => Err(err(e.range)),
        EllipsisLiteral(ref e) => Err(err(e.range)),
        #[allow(deprecated)]
        Invalid(ref e) => Err(err(e.range)),
        // This isn't in the Python grammar but is Jupyter notebook specific.
        // It seems like this should be an error. It does also seem like the
        // parser prevents this from ever appearing as an assignment target
        // anyway. ---AG
        IpyEscapeCommand(ref e) => Err(err(e.range)),
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
