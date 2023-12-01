//! Extract docstrings from an AST.

use ruff_python_ast::{self as ast, Stmt};
use ruff_python_semantic::{Definition, DefinitionId, Definitions, Member, MemberKind};

/// Extract a docstring from a function or class body.
pub(crate) fn docstring_from(suite: &[Stmt]) -> Option<&ast::ExprStringLiteral> {
    let stmt = suite.first()?;
    // Require the docstring to be a standalone expression.
    let Stmt::Expr(ast::StmtExpr { value, range: _ }) = stmt else {
        return None;
    };
    // Only match strings.
    value.as_string_literal_expr()
}

/// Extract a docstring from a `Definition`.
pub(crate) fn extract_docstring<'a>(
    definition: &'a Definition<'a>,
) -> Option<&'a ast::ExprStringLiteral> {
    match definition {
        Definition::Module(module) => docstring_from(module.python_ast),
        Definition::Member(member) => docstring_from(member.body()),
    }
}

#[derive(Copy, Clone)]
pub(crate) enum ExtractionTarget<'a> {
    Class(&'a ast::StmtClassDef),
    Function(&'a ast::StmtFunctionDef),
}

/// Extract a `Definition` from the AST node defined by a `Stmt`.
pub(crate) fn extract_definition<'a>(
    target: ExtractionTarget<'a>,
    parent: DefinitionId,
    definitions: &Definitions<'a>,
) -> Member<'a> {
    match target {
        ExtractionTarget::Function(function) => match &definitions[parent] {
            Definition::Module(..) => Member {
                parent,
                kind: MemberKind::Function(function),
            },
            Definition::Member(Member {
                kind: MemberKind::Class(_) | MemberKind::NestedClass(_),
                ..
            }) => Member {
                parent,
                kind: MemberKind::Method(function),
            },
            Definition::Member(_) => Member {
                parent,
                kind: MemberKind::NestedFunction(function),
            },
        },
        ExtractionTarget::Class(class) => match &definitions[parent] {
            Definition::Module(_) => Member {
                parent,
                kind: MemberKind::Class(class),
            },
            Definition::Member(_) => Member {
                parent,
                kind: MemberKind::NestedClass(class),
            },
        },
    }
}
