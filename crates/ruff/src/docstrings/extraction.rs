//! Extract docstrings from an AST.

use rustpython_parser::ast::{self, Constant, Expr, Stmt};

use ruff_python_semantic::{Definition, DefinitionId, Definitions, Member, MemberKind};

/// Extract a docstring from a function or class body.
pub(crate) fn docstring_from(suite: &[Stmt]) -> Option<&Expr> {
    let stmt = suite.first()?;
    // Require the docstring to be a standalone expression.
    let Stmt::Expr(ast::StmtExpr { value, range: _ }) = stmt else {
        return None;
    };
    // Only match strings.
    if !matches!(
        value.as_ref(),
        Expr::Constant(ast::ExprConstant {
            value: Constant::Str(_),
            ..
        })
    ) {
        return None;
    }
    Some(value)
}

/// Extract a docstring from a `Definition`.
pub(crate) fn extract_docstring<'a>(definition: &'a Definition<'a>) -> Option<&'a Expr> {
    match definition {
        Definition::Module(module) => docstring_from(module.python_ast),
        Definition::Member(member) => {
            if let Stmt::ClassDef(ast::StmtClassDef { body, .. })
            | Stmt::FunctionDef(ast::StmtFunctionDef { body, .. })
            | Stmt::AsyncFunctionDef(ast::StmtAsyncFunctionDef { body, .. }) = &member.stmt
            {
                docstring_from(body)
            } else {
                None
            }
        }
    }
}

#[derive(Copy, Clone)]
pub(crate) enum ExtractionTarget {
    Class,
    Function,
}

/// Extract a `Definition` from the AST node defined by a `Stmt`.
pub(crate) fn extract_definition<'a>(
    target: ExtractionTarget,
    stmt: &'a Stmt,
    parent: DefinitionId,
    definitions: &Definitions<'a>,
) -> Member<'a> {
    match target {
        ExtractionTarget::Function => match &definitions[parent] {
            Definition::Module(..) => Member {
                parent,
                kind: MemberKind::Function,
                stmt,
            },
            Definition::Member(Member {
                kind: MemberKind::Class | MemberKind::NestedClass,
                ..
            }) => Member {
                parent,
                kind: MemberKind::Method,
                stmt,
            },
            Definition::Member(..) => Member {
                parent,
                kind: MemberKind::NestedFunction,
                stmt,
            },
        },
        ExtractionTarget::Class => match &definitions[parent] {
            Definition::Module(..) => Member {
                parent,
                kind: MemberKind::Class,
                stmt,
            },
            Definition::Member(..) => Member {
                parent,
                kind: MemberKind::NestedClass,
                stmt,
            },
        },
    }
}
