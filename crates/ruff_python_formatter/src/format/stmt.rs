#![allow(unused_variables, clippy::too_many_arguments)]

use ruff_formatter::prelude::*;
use ruff_formatter::{format_args, write};
use ruff_text_size::TextSize;

use crate::builders::literal;
use crate::context::ASTFormatContext;
use crate::cst::{
    Alias, Arguments, Excepthandler, Expr, ExprKind, Keyword, Stmt, StmtKind, Withitem,
};
use crate::format::builders::{block, join_names};
use crate::format::helpers::is_self_closing;
use crate::shared_traits::AsFormat;
use crate::trivia::{Parenthesize, Relationship, TriviaKind};

fn format_break(f: &mut Formatter<ASTFormatContext<'_>>) -> FormatResult<()> {
    write!(f, [text("break")])
}

fn format_pass(f: &mut Formatter<ASTFormatContext<'_>>, stmt: &Stmt) -> FormatResult<()> {
    // Write the statement body.
    write!(f, [text("pass")])?;

    // Format any end-of-line comments.
    let mut first = true;
    for range in stmt.trivia.iter().filter_map(|trivia| {
        if matches!(trivia.relationship, Relationship::Trailing) {
            if let TriviaKind::EndOfLineComment(range) = trivia.kind {
                Some(range)
            } else {
                None
            }
        } else {
            None
        }
    }) {
        if std::mem::take(&mut first) {
            write!(f, [line_suffix(&text("  "))])?;
        }
        write!(f, [line_suffix(&literal(range))])?;
    }

    Ok(())
}

fn format_continue(f: &mut Formatter<ASTFormatContext<'_>>) -> FormatResult<()> {
    write!(f, [text("continue")])
}

fn format_global(f: &mut Formatter<ASTFormatContext<'_>>, names: &[String]) -> FormatResult<()> {
    write!(f, [text("global")])?;
    if !names.is_empty() {
        write!(f, [space(), join_names(names)])?;
    }
    Ok(())
}

fn format_nonlocal(f: &mut Formatter<ASTFormatContext<'_>>, names: &[String]) -> FormatResult<()> {
    write!(f, [text("nonlocal")])?;
    if !names.is_empty() {
        write!(f, [space(), join_names(names)])?;
    }
    Ok(())
}

fn format_delete(f: &mut Formatter<ASTFormatContext<'_>>, targets: &[Expr]) -> FormatResult<()> {
    write!(f, [text("del")])?;

    match targets.len() {
        0 => Ok(()),
        1 => write!(f, [space(), targets[0].format()]),
        _ => {
            write!(
                f,
                [
                    space(),
                    group(&format_args![
                        if_group_breaks(&text("(")),
                        soft_block_indent(&format_with(|f| {
                            for (i, target) in targets.iter().enumerate() {
                                write!(f, [target.format()])?;

                                if i < targets.len() - 1 {
                                    write!(f, [text(","), soft_line_break_or_space()])?;
                                } else {
                                    write!(f, [if_group_breaks(&text(","))])?;
                                }
                            }
                            Ok(())
                        })),
                        if_group_breaks(&text(")")),
                    ])
                ]
            )
        }
    }
}

fn format_class_def(
    f: &mut Formatter<ASTFormatContext<'_>>,
    name: &str,
    bases: &[Expr],
    keywords: &[Keyword],
    body: &[Stmt],
    decorator_list: &[Expr],
) -> FormatResult<()> {
    for decorator in decorator_list {
        write!(f, [text("@"), decorator.format(), hard_line_break()])?;
    }

    write!(
        f,
        [
            text("class"),
            space(),
            dynamic_text(name, TextSize::default())
        ]
    )?;

    if !bases.is_empty() || !keywords.is_empty() {
        let format_bases = format_with(|f| {
            for (i, expr) in bases.iter().enumerate() {
                write!(f, [expr.format()])?;

                if i < bases.len() - 1 || !keywords.is_empty() {
                    write!(f, [text(","), soft_line_break_or_space()])?;
                } else {
                    write!(f, [if_group_breaks(&text(","))])?;
                }

                for (i, keyword) in keywords.iter().enumerate() {
                    if let Some(arg) = &keyword.node.arg {
                        write!(
                            f,
                            [
                                dynamic_text(arg, TextSize::default()),
                                text("="),
                                keyword.node.value.format()
                            ]
                        )?;
                    } else {
                        write!(f, [text("**"), keyword.node.value.format()])?;
                    }
                    if i < keywords.len() - 1 {
                        write!(f, [text(","), soft_line_break_or_space()])?;
                    } else {
                        write!(f, [if_group_breaks(&text(","))])?;
                    }
                }
            }
            Ok(())
        });

        write!(
            f,
            [
                text("("),
                group(&soft_block_indent(&format_bases)),
                text(")")
            ]
        )?;
    }

    write!(f, [text(":"), block_indent(&block(body))])
}

fn format_func_def(
    f: &mut Formatter<ASTFormatContext<'_>>,
    stmt: &Stmt,
    name: &str,
    args: &Arguments,
    returns: Option<&Expr>,
    body: &[Stmt],
    decorator_list: &[Expr],
    async_: bool,
) -> FormatResult<()> {
    for decorator in decorator_list {
        write!(f, [text("@"), decorator.format(), hard_line_break()])?;
    }
    if async_ {
        write!(f, [text("async"), space()])?;
    }
    write!(
        f,
        [
            text("def"),
            space(),
            dynamic_text(name, TextSize::default()),
            text("("),
            group(&soft_block_indent(&format_with(|f| {
                if stmt
                    .trivia
                    .iter()
                    .any(|c| matches!(c.kind, TriviaKind::MagicTrailingComma))
                {
                    write!(f, [expand_parent()])?;
                }
                write!(f, [args.format()])
            }))),
            text(")")
        ]
    )?;

    if let Some(returns) = returns {
        write!(f, [text(" -> "), returns.format()])?;
    }

    write!(f, [text(":")])?;

    // Format any end-of-line comments.
    let mut first = true;
    for range in stmt.trivia.iter().filter_map(|trivia| {
        if matches!(trivia.relationship, Relationship::Trailing) {
            if let TriviaKind::EndOfLineComment(range) = trivia.kind {
                Some(range)
            } else {
                None
            }
        } else {
            None
        }
    }) {
        if std::mem::take(&mut first) {
            write!(f, [line_suffix(&text("  "))])?;
        }
        write!(f, [line_suffix(&literal(range))])?;
    }

    write!(f, [block_indent(&format_args![block(body)])])
}

fn format_assign(
    f: &mut Formatter<ASTFormatContext<'_>>,
    stmt: &Stmt,
    targets: &[Expr],
    value: &Expr,
) -> FormatResult<()> {
    write!(f, [targets[0].format()])?;

    for target in &targets[1..] {
        // TODO(charlie): This doesn't match Black's behavior. We need to parenthesize
        // this expression sometimes.
        write!(f, [text(" = "), target.format()])?;
    }
    write!(f, [text(" = ")])?;
    if is_self_closing(value) {
        write!(f, [group(&value.format())])?;
    } else {
        write!(
            f,
            [group(&format_args![
                if_group_breaks(&text("(")),
                soft_block_indent(&value.format()),
                if_group_breaks(&text(")")),
            ])]
        )?;
    }

    // Format any end-of-line comments.
    let mut first = true;
    for range in stmt.trivia.iter().filter_map(|trivia| {
        if matches!(trivia.relationship, Relationship::Trailing) {
            if let TriviaKind::EndOfLineComment(range) = trivia.kind {
                Some(range)
            } else {
                None
            }
        } else {
            None
        }
    }) {
        if std::mem::take(&mut first) {
            write!(f, [line_suffix(&text("  "))])?;
        }
        write!(f, [line_suffix(&literal(range))])?;
    }

    Ok(())
}

fn format_ann_assign(
    f: &mut Formatter<ASTFormatContext<'_>>,
    stmt: &Stmt,
    target: &Expr,
    annotation: &Expr,
    value: Option<&Expr>,
    simple: usize,
) -> FormatResult<()> {
    let need_parens = matches!(target.node, ExprKind::Name { .. }) && simple == 0;
    if need_parens {
        write!(f, [text("(")])?;
    }
    write!(f, [target.format()])?;
    if need_parens {
        write!(f, [text(")")])?;
    }
    write!(f, [text(": "), annotation.format()])?;

    if let Some(value) = value {
        write!(
            f,
            [
                space(),
                text("="),
                space(),
                group(&format_args![
                    if_group_breaks(&text("(")),
                    soft_block_indent(&value.format()),
                    if_group_breaks(&text(")")),
                ])
            ]
        )?;
    }

    Ok(())
}

fn format_for(
    f: &mut Formatter<ASTFormatContext<'_>>,
    stmt: &Stmt,
    target: &Expr,
    iter: &Expr,
    body: &[Stmt],
    orelse: &[Stmt],
    _type_comment: Option<&str>,
) -> FormatResult<()> {
    write!(
        f,
        [
            text("for"),
            space(),
            group(&target.format()),
            space(),
            text("in"),
            space(),
            group(&iter.format()),
            text(":"),
            block_indent(&block(body))
        ]
    )?;
    if !orelse.is_empty() {
        write!(f, [text("else:"), block_indent(&block(orelse))])?;
    }
    Ok(())
}

fn format_while(
    f: &mut Formatter<ASTFormatContext<'_>>,
    stmt: &Stmt,
    test: &Expr,
    body: &[Stmt],
    orelse: &[Stmt],
) -> FormatResult<()> {
    write!(f, [text("while"), space()])?;
    if is_self_closing(test) {
        write!(f, [test.format()])?;
    } else {
        write!(
            f,
            [group(&format_args![
                if_group_breaks(&text("(")),
                soft_block_indent(&test.format()),
                if_group_breaks(&text(")")),
            ])]
        )?;
    }
    write!(f, [text(":"), block_indent(&block(body))])?;
    if !orelse.is_empty() {
        write!(f, [text("else:"), block_indent(&block(orelse))])?;
    }
    Ok(())
}

fn format_if(
    f: &mut Formatter<ASTFormatContext<'_>>,
    test: &Expr,
    body: &[Stmt],
    orelse: &[Stmt],
) -> FormatResult<()> {
    write!(f, [text("if"), space()])?;
    if is_self_closing(test) {
        write!(f, [test.format()])?;
    } else {
        write!(
            f,
            [group(&format_args![
                if_group_breaks(&text("(")),
                soft_block_indent(&test.format()),
                if_group_breaks(&text(")")),
            ])]
        )?;
    }
    write!(f, [text(":"), block_indent(&block(body))])?;
    if !orelse.is_empty() {
        if orelse.len() == 1 {
            if let StmtKind::If { test, body, orelse } = &orelse[0].node {
                write!(f, [text("el")])?;
                format_if(f, test, body, orelse)?;
            } else {
                write!(f, [text("else:"), block_indent(&block(orelse))])?;
            }
        } else {
            write!(f, [text("else:"), block_indent(&block(orelse))])?;
        }
    }
    Ok(())
}

fn format_raise(
    f: &mut Formatter<ASTFormatContext<'_>>,
    stmt: &Stmt,
    exc: Option<&Expr>,
    cause: Option<&Expr>,
) -> FormatResult<()> {
    write!(f, [text("raise")])?;
    if let Some(exc) = exc {
        write!(f, [space(), exc.format()])?;
        if let Some(cause) = cause {
            write!(f, [space(), text("from"), space(), cause.format()])?;
        }
    }
    Ok(())
}

fn format_return(
    f: &mut Formatter<ASTFormatContext<'_>>,
    stmt: &Stmt,
    value: Option<&Expr>,
) -> FormatResult<()> {
    write!(f, [text("return")])?;
    if let Some(value) = value {
        write!(f, [space(), value.format()])?;
    }

    // Format any end-of-line comments.
    let mut first = true;
    for range in stmt.trivia.iter().filter_map(|trivia| {
        if matches!(trivia.relationship, Relationship::Trailing) {
            if let TriviaKind::EndOfLineComment(range) = trivia.kind {
                Some(range)
            } else {
                None
            }
        } else {
            None
        }
    }) {
        if std::mem::take(&mut first) {
            write!(f, [line_suffix(&text("  "))])?;
        }
        write!(f, [line_suffix(&literal(range))])?;
    }

    Ok(())
}

fn format_try(
    f: &mut Formatter<ASTFormatContext<'_>>,
    stmt: &Stmt,
    body: &[Stmt],
    handlers: &[Excepthandler],
    orelse: &[Stmt],
    finalbody: &[Stmt],
) -> FormatResult<()> {
    write!(f, [text("try:"), block_indent(&block(body))])?;
    for handler in handlers {
        write!(f, [handler.format()])?;
    }
    if !orelse.is_empty() {
        write!(f, [text("else:"), block_indent(&block(orelse))])?;
    }
    if !finalbody.is_empty() {
        write!(f, [text("finally:"), block_indent(&block(finalbody))])?;
    }
    Ok(())
}

fn format_try_star(
    f: &mut Formatter<ASTFormatContext<'_>>,
    stmt: &Stmt,
    body: &[Stmt],
    handlers: &[Excepthandler],
    orelse: &[Stmt],
    finalbody: &[Stmt],
) -> FormatResult<()> {
    write!(f, [text("try:"), block_indent(&block(body))])?;
    for handler in handlers {
        // TODO(charlie): Include `except*`.
        write!(f, [handler.format()])?;
    }
    if !orelse.is_empty() {
        write!(f, [text("else:"), block_indent(&block(orelse))])?;
    }
    if !finalbody.is_empty() {
        write!(f, [text("finally:"), block_indent(&block(finalbody))])?;
    }
    Ok(())
}

fn format_assert(
    f: &mut Formatter<ASTFormatContext<'_>>,
    stmt: &Stmt,
    test: &Expr,
    msg: Option<&Expr>,
) -> FormatResult<()> {
    write!(f, [text("assert"), space()])?;
    write!(
        f,
        [group(&format_args![
            if_group_breaks(&text("(")),
            soft_block_indent(&test.format()),
            if_group_breaks(&text(")")),
        ])]
    )?;
    if let Some(msg) = msg {
        write!(
            f,
            [
                text(","),
                space(),
                group(&format_args![
                    if_group_breaks(&text("(")),
                    soft_block_indent(&msg.format()),
                    if_group_breaks(&text(")")),
                ])
            ]
        )?;
    }
    Ok(())
}

fn format_import(
    f: &mut Formatter<ASTFormatContext<'_>>,
    stmt: &Stmt,
    names: &[Alias],
) -> FormatResult<()> {
    write!(
        f,
        [
            text("import"),
            space(),
            group(&format_args![
                if_group_breaks(&text("(")),
                soft_block_indent(&format_with(|f| {
                    for (i, name) in names.iter().enumerate() {
                        write!(f, [name.format()])?;
                        if i < names.len() - 1 {
                            write!(f, [text(","), soft_line_break_or_space()])?;
                        } else {
                            write!(f, [if_group_breaks(&text(","))])?;
                        }
                    }
                    Ok(())
                })),
                if_group_breaks(&text(")")),
            ])
        ]
    )
}

fn format_import_from(
    f: &mut Formatter<ASTFormatContext<'_>>,
    stmt: &Stmt,
    module: Option<&str>,
    names: &[Alias],
    level: Option<&usize>,
) -> FormatResult<()> {
    write!(f, [text("from")])?;
    write!(f, [space()])?;

    if let Some(level) = level {
        for _ in 0..*level {
            write!(f, [text(".")])?;
        }
    }
    if let Some(module) = module {
        write!(f, [dynamic_text(module, TextSize::default())])?;
    }
    write!(f, [space()])?;

    write!(f, [text("import")])?;
    write!(f, [space()])?;

    if names.iter().any(|name| name.node.name == "*") {
        write!(f, [text("*")])?;
    } else {
        let magic_trailing_comma = stmt
            .trivia
            .iter()
            .any(|c| matches!(c.kind, TriviaKind::MagicTrailingComma));
        write!(
            f,
            [group(&format_args![
                if_group_breaks(&text("(")),
                soft_block_indent(&format_with(|f| {
                    if magic_trailing_comma {
                        write!(f, [expand_parent()])?;
                    }
                    for (i, name) in names.iter().enumerate() {
                        write!(f, [name.format()])?;
                        if i < names.len() - 1 {
                            write!(f, [text(",")])?;
                            write!(f, [soft_line_break_or_space()])?;
                        } else {
                            write!(f, [if_group_breaks(&text(","))])?;
                        }
                    }
                    Ok(())
                })),
                if_group_breaks(&text(")")),
            ])]
        )?;
    }

    // Format any end-of-line comments.
    let mut first = true;
    for range in stmt.trivia.iter().filter_map(|trivia| {
        if matches!(trivia.relationship, Relationship::Trailing) {
            if let TriviaKind::EndOfLineComment(range) = trivia.kind {
                Some(range)
            } else {
                None
            }
        } else {
            None
        }
    }) {
        if std::mem::take(&mut first) {
            write!(f, [line_suffix(&text("  "))])?;
        }
        write!(f, [line_suffix(&literal(range))])?;
    }

    Ok(())
}

fn format_expr(
    f: &mut Formatter<ASTFormatContext<'_>>,
    stmt: &Stmt,
    expr: &Expr,
) -> FormatResult<()> {
    if matches!(stmt.parentheses, Parenthesize::Always) {
        write!(
            f,
            [group(&format_args![
                text("("),
                soft_block_indent(&format_args![expr.format()]),
                text(")"),
            ])]
        )?;
    } else if is_self_closing(expr) {
        write!(f, [group(&format_args![expr.format()])])?;
    } else {
        write!(
            f,
            [group(&format_args![
                if_group_breaks(&text("(")),
                soft_block_indent(&format_args![expr.format()]),
                if_group_breaks(&text(")")),
            ])]
        )?;
    }

    // Format any end-of-line comments.
    let mut first = true;
    for range in stmt.trivia.iter().filter_map(|trivia| {
        if matches!(trivia.relationship, Relationship::Trailing) {
            if let TriviaKind::EndOfLineComment(range) = trivia.kind {
                Some(range)
            } else {
                None
            }
        } else {
            None
        }
    }) {
        if std::mem::take(&mut first) {
            write!(f, [line_suffix(&text("  "))])?;
        }
        write!(f, [line_suffix(&literal(range))])?;
    }

    Ok(())
}

fn format_with_(
    f: &mut Formatter<ASTFormatContext<'_>>,
    stmt: &Stmt,
    items: &[Withitem],
    body: &[Stmt],
    type_comment: Option<&str>,
    async_: bool,
) -> FormatResult<()> {
    if async_ {
        write!(f, [text("async"), space()])?;
    }

    write!(
        f,
        [
            text("with"),
            space(),
            group(&format_args![
                if_group_breaks(&text("(")),
                soft_block_indent(&format_with(|f| {
                    for (i, item) in items.iter().enumerate() {
                        write!(f, [item.format()])?;
                        if i < items.len() - 1 {
                            write!(f, [text(","), soft_line_break_or_space()])?;
                        } else {
                            write!(f, [if_group_breaks(&text(","))])?;
                        }
                    }
                    Ok(())
                })),
                if_group_breaks(&text(")")),
            ]),
            text(":"),
            block_indent(&block(body))
        ]
    )
}

pub struct FormatStmt<'a> {
    item: &'a Stmt,
}

impl Format<ASTFormatContext<'_>> for FormatStmt<'_> {
    fn fmt(&self, f: &mut Formatter<ASTFormatContext<'_>>) -> FormatResult<()> {
        // Any leading comments come on the line before.
        for trivia in &self.item.trivia {
            if matches!(trivia.relationship, Relationship::Leading) {
                match trivia.kind {
                    TriviaKind::EmptyLine => {
                        write!(f, [empty_line()])?;
                    }
                    TriviaKind::OwnLineComment(range) => {
                        write!(f, [literal(range), hard_line_break()])?;
                    }
                    _ => {}
                }
            }
        }

        match &self.item.node {
            StmtKind::Pass => format_pass(f, self.item),
            StmtKind::Break => format_break(f),
            StmtKind::Continue => format_continue(f),
            StmtKind::Global { names } => format_global(f, names),
            StmtKind::Nonlocal { names } => format_nonlocal(f, names),
            StmtKind::FunctionDef {
                name,
                args,
                body,
                decorator_list,
                returns,
                ..
            } => format_func_def(
                f,
                self.item,
                name,
                args,
                returns.as_deref(),
                body,
                decorator_list,
                false,
            ),
            StmtKind::AsyncFunctionDef {
                name,
                args,
                body,
                decorator_list,
                returns,
                ..
            } => format_func_def(
                f,
                self.item,
                name,
                args,
                returns.as_deref(),
                body,
                decorator_list,
                true,
            ),
            StmtKind::ClassDef {
                name,
                bases,
                keywords,
                body,
                decorator_list,
            } => format_class_def(f, name, bases, keywords, body, decorator_list),
            StmtKind::Return { value } => format_return(f, self.item, value.as_ref()),
            StmtKind::Delete { targets } => format_delete(f, targets),
            StmtKind::Assign { targets, value, .. } => format_assign(f, self.item, targets, value),
            // StmtKind::AugAssign { .. } => {}
            StmtKind::AnnAssign {
                target,
                annotation,
                value,
                simple,
            } => format_ann_assign(f, self.item, target, annotation, value.as_deref(), *simple),
            StmtKind::For {
                target,
                iter,
                body,
                orelse,
                type_comment,
            } => format_for(
                f,
                self.item,
                target,
                iter,
                body,
                orelse,
                type_comment.as_deref(),
            ),
            // StmtKind::AsyncFor { .. } => {}
            StmtKind::While { test, body, orelse } => {
                format_while(f, self.item, test, body, orelse)
            }
            StmtKind::If { test, body, orelse } => format_if(f, test, body, orelse),
            StmtKind::With {
                items,
                body,
                type_comment,
            } => format_with_(
                f,
                self.item,
                items,
                body,
                type_comment.as_ref().map(String::as_str),
                false,
            ),
            StmtKind::AsyncWith {
                items,
                body,
                type_comment,
            } => format_with_(
                f,
                self.item,
                items,
                body,
                type_comment.as_ref().map(String::as_str),
                true,
            ),
            // StmtKind::Match { .. } => {}
            StmtKind::Raise { exc, cause } => {
                format_raise(f, self.item, exc.as_deref(), cause.as_deref())
            }
            StmtKind::Try {
                body,
                handlers,
                orelse,
                finalbody,
            } => format_try(f, self.item, body, handlers, orelse, finalbody),
            StmtKind::TryStar {
                body,
                handlers,
                orelse,
                finalbody,
            } => format_try_star(f, self.item, body, handlers, orelse, finalbody),
            StmtKind::Assert { test, msg } => {
                format_assert(f, self.item, test, msg.as_ref().map(|expr| &**expr))
            }
            StmtKind::Import { names } => format_import(f, self.item, names),
            StmtKind::ImportFrom {
                module,
                names,
                level,
            } => format_import_from(
                f,
                self.item,
                module.as_ref().map(String::as_str),
                names,
                level.as_ref(),
            ),
            // StmtKind::Nonlocal { .. } => {}
            StmtKind::Expr { value } => format_expr(f, self.item, value),
            _ => {
                unimplemented!("Implement StmtKind: {:?}", self.item.node)
            }
        }?;

        // Any trailing comments come on the lines after.
        for trivia in &self.item.trivia {
            if matches!(trivia.relationship, Relationship::Trailing) {
                match trivia.kind {
                    TriviaKind::EmptyLine => {
                        write!(f, [empty_line()])?;
                    }
                    TriviaKind::OwnLineComment(range) => {
                        write!(f, [literal(range), hard_line_break()])?;
                    }
                    _ => {}
                }
            }
        }

        Ok(())
    }
}

impl AsFormat<ASTFormatContext<'_>> for Stmt {
    type Format<'a> = FormatStmt<'a>;

    fn format(&self) -> Self::Format<'_> {
        FormatStmt { item: self }
    }
}
