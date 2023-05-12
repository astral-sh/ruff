//! Extract docstrings from an AST.

use rustpython_parser::ast::{self, Constant, Expr, ExprKind, Stmt, StmtKind};

use ruff_python_semantic::definition::{Definition, DefinitionId, Definitions, Member, MemberKind};

/// Extract a docstring from a function or class body.
pub(crate) fn docstring_from(suite: &[Stmt]) -> Option<&Expr> {
    let stmt = suite.first()?;
    // Require the docstring to be a standalone expression.
    let StmtKind::Expr(ast::StmtExpr { value }) = &stmt.node else {
        return None;
    };
    // Only match strings.
    if !matches!(
        &value.node,
        ExprKind::Constant(ast::ExprConstant {
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
            if let StmtKind::ClassDef(ast::StmtClassDef { body, .. })
            | StmtKind::FunctionDef(ast::StmtFunctionDef { body, .. })
            | StmtKind::AsyncFunctionDef(ast::StmtAsyncFunctionDef { body, .. }) =
                &member.stmt.node
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
