use rustc_hash::FxHashMap;
use rustpython_parser::ast::Location;
use rustpython_parser::lexer::LexResult;
use rustpython_parser::Tok;

use crate::core::types::Range;
use crate::cst::{
    Alias, Excepthandler, ExcepthandlerKind, Expr, ExprKind, SliceIndex, SliceIndexKind, Stmt,
    StmtKind,
};

#[derive(Clone, Debug)]
pub enum Node<'a> {
    Mod(&'a [Stmt]),
    Stmt(&'a Stmt),
    Expr(&'a Expr),
    Alias(&'a Alias),
    Excepthandler(&'a Excepthandler),
    SliceIndex(&'a SliceIndex),
}

impl Node<'_> {
    pub fn id(&self) -> usize {
        match self {
            Node::Mod(nodes) => nodes as *const _ as usize,
            Node::Stmt(node) => node.id(),
            Node::Expr(node) => node.id(),
            Node::Alias(node) => node.id(),
            Node::Excepthandler(node) => node.id(),
            Node::SliceIndex(node) => node.id(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TriviaTokenKind {
    OwnLineComment,
    EndOfLineComment,
    MagicTrailingComma,
    EmptyLine,
    Parentheses,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TriviaToken {
    pub start: Location,
    pub end: Location,
    pub kind: TriviaTokenKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, is_macro::Is)]
pub enum TriviaKind {
    /// A Comment that is separated by at least one line break from the
    /// preceding token.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// a = 1
    /// # This is an own-line comment.
    /// b = 2
    /// ```
    OwnLineComment(Range),
    /// A comment that is on the same line as the preceding token.
    ///
    /// # Examples
    ///
    /// ## End of line
    ///
    /// ```ignore
    /// a = 1  # This is an end-of-line comment.
    /// b = 2
    /// ```
    EndOfLineComment(Range),
    MagicTrailingComma,
    EmptyLine,
    Parentheses,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, is_macro::Is)]
pub enum Relationship {
    Leading,
    Trailing,
    Dangling,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Parenthesize {
    /// Always parenthesize the statement or expression.
    Always,
    /// Never parenthesize the statement or expression.
    Never,
    /// Parenthesize the statement or expression if it expands.
    IfExpanded,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Trivia {
    pub kind: TriviaKind,
    pub relationship: Relationship,
}

impl Trivia {
    pub fn from_token(token: &TriviaToken, relationship: Relationship) -> Self {
        match token.kind {
            TriviaTokenKind::MagicTrailingComma => Self {
                kind: TriviaKind::MagicTrailingComma,
                relationship,
            },
            TriviaTokenKind::EmptyLine => Self {
                kind: TriviaKind::EmptyLine,
                relationship,
            },
            TriviaTokenKind::OwnLineComment => Self {
                kind: TriviaKind::OwnLineComment(Range::new(token.start, token.end)),
                relationship,
            },
            TriviaTokenKind::EndOfLineComment => Self {
                kind: TriviaKind::EndOfLineComment(Range::new(token.start, token.end)),
                relationship,
            },
            TriviaTokenKind::Parentheses => Self {
                kind: TriviaKind::Parentheses,
                relationship,
            },
        }
    }
}

pub fn extract_trivia_tokens(lxr: &[LexResult]) -> Vec<TriviaToken> {
    let mut tokens = vec![];
    let mut prev_tok: Option<(&Location, &Tok, &Location)> = None;
    let mut prev_non_newline_tok: Option<(&Location, &Tok, &Location)> = None;
    let mut prev_semantic_tok: Option<(&Location, &Tok, &Location)> = None;
    let mut parens = vec![];
    for (start, tok, end) in lxr.iter().flatten() {
        // Add empty lines.
        if let Some((.., prev)) = prev_non_newline_tok {
            for row in prev.row() + 1..start.row() {
                tokens.push(TriviaToken {
                    start: Location::new(row, 0),
                    end: Location::new(row + 1, 0),
                    kind: TriviaTokenKind::EmptyLine,
                });
            }
        }

        // Add comments.
        if let Tok::Comment(_) = tok {
            tokens.push(TriviaToken {
                start: *start,
                end: *end,
                kind: if prev_non_newline_tok.map_or(true, |(prev, ..)| prev.row() < start.row()) {
                    TriviaTokenKind::OwnLineComment
                } else {
                    TriviaTokenKind::EndOfLineComment
                },
            });
        }

        // Add magic trailing commas.
        if matches!(
            tok,
            Tok::Rpar | Tok::Rsqb | Tok::Rbrace | Tok::Equal | Tok::Newline
        ) {
            if let Some((prev_start, prev_tok, prev_end)) = prev_semantic_tok {
                if prev_tok == &Tok::Comma {
                    tokens.push(TriviaToken {
                        start: *prev_start,
                        end: *prev_end,
                        kind: TriviaTokenKind::MagicTrailingComma,
                    });
                }
            }
        }

        if matches!(tok, Tok::Lpar) {
            if prev_tok.map_or(true, |(_, prev_tok, _)| {
                !matches!(
                    prev_tok,
                    Tok::Name { .. }
                        | Tok::Int { .. }
                        | Tok::Float { .. }
                        | Tok::Complex { .. }
                        | Tok::String { .. }
                )
            }) {
                parens.push((start, true));
            } else {
                parens.push((start, false));
            }
        } else if matches!(tok, Tok::Rpar) {
            let (start, explicit) = parens.pop().unwrap();
            if explicit {
                tokens.push(TriviaToken {
                    start: *start,
                    end: *end,
                    kind: TriviaTokenKind::Parentheses,
                });
            }
        }

        prev_tok = Some((start, tok, end));

        // Track the most recent non-whitespace token.
        if !matches!(tok, Tok::Newline | Tok::NonLogicalNewline) {
            prev_non_newline_tok = Some((start, tok, end));
        }

        // Track the most recent semantic token.
        if !matches!(
            tok,
            Tok::Newline | Tok::NonLogicalNewline | Tok::Comment(..)
        ) {
            prev_semantic_tok = Some((start, tok, end));
        }
    }
    tokens
}

fn sorted_child_nodes_inner<'a>(node: &Node<'a>, result: &mut Vec<Node<'a>>) {
    match node {
        Node::Mod(nodes) => {
            for stmt in nodes.iter() {
                result.push(Node::Stmt(stmt));
            }
        }
        Node::Stmt(stmt) => match &stmt.node {
            StmtKind::Return { value } => {
                if let Some(value) = value {
                    result.push(Node::Expr(value));
                }
            }
            StmtKind::Expr { value } => {
                result.push(Node::Expr(value));
            }
            StmtKind::Pass => {}
            StmtKind::Assign { targets, value, .. } => {
                for target in targets {
                    result.push(Node::Expr(target));
                }
                result.push(Node::Expr(value));
            }
            StmtKind::FunctionDef {
                args,
                body,
                decorator_list,
                returns,
                ..
            }
            | StmtKind::AsyncFunctionDef {
                args,
                body,
                decorator_list,
                returns,
                ..
            } => {
                for decorator in decorator_list {
                    result.push(Node::Expr(decorator));
                }
                for arg in &args.posonlyargs {
                    if let Some(expr) = &arg.node.annotation {
                        result.push(Node::Expr(expr));
                    }
                }
                for arg in &args.args {
                    if let Some(expr) = &arg.node.annotation {
                        result.push(Node::Expr(expr));
                    }
                }
                if let Some(arg) = &args.vararg {
                    if let Some(expr) = &arg.node.annotation {
                        result.push(Node::Expr(expr));
                    }
                }
                for arg in &args.kwonlyargs {
                    if let Some(expr) = &arg.node.annotation {
                        result.push(Node::Expr(expr));
                    }
                }
                if let Some(arg) = &args.kwarg {
                    if let Some(expr) = &arg.node.annotation {
                        result.push(Node::Expr(expr));
                    }
                }
                for expr in &args.defaults {
                    result.push(Node::Expr(expr));
                }
                for expr in &args.kw_defaults {
                    result.push(Node::Expr(expr));
                }
                if let Some(returns) = returns {
                    result.push(Node::Expr(returns));
                }
                for stmt in body {
                    result.push(Node::Stmt(stmt));
                }
            }
            StmtKind::ClassDef {
                bases,
                keywords,
                body,
                decorator_list,
                ..
            } => {
                for decorator in decorator_list {
                    result.push(Node::Expr(decorator));
                }
                for base in bases {
                    result.push(Node::Expr(base));
                }
                for keyword in keywords {
                    result.push(Node::Expr(&keyword.node.value));
                }
                for stmt in body {
                    result.push(Node::Stmt(stmt));
                }
            }
            StmtKind::Delete { targets } => {
                for target in targets {
                    result.push(Node::Expr(target));
                }
            }
            StmtKind::AugAssign { target, value, .. } => {
                result.push(Node::Expr(target));
                result.push(Node::Expr(value));
            }
            StmtKind::AnnAssign {
                target,
                annotation,
                value,
                ..
            } => {
                result.push(Node::Expr(target));
                result.push(Node::Expr(annotation));
                if let Some(value) = value {
                    result.push(Node::Expr(value));
                }
            }
            StmtKind::For {
                target,
                iter,
                body,
                orelse,
                ..
            }
            | StmtKind::AsyncFor {
                target,
                iter,
                body,
                orelse,
                ..
            } => {
                result.push(Node::Expr(target));
                result.push(Node::Expr(iter));
                for stmt in body {
                    result.push(Node::Stmt(stmt));
                }
                for stmt in orelse {
                    result.push(Node::Stmt(stmt));
                }
            }
            StmtKind::While { test, body, orelse } => {
                result.push(Node::Expr(test));
                for stmt in body {
                    result.push(Node::Stmt(stmt));
                }
                for stmt in orelse {
                    result.push(Node::Stmt(stmt));
                }
            }
            StmtKind::If { test, body, orelse } => {
                result.push(Node::Expr(test));
                for stmt in body {
                    result.push(Node::Stmt(stmt));
                }
                for stmt in orelse {
                    result.push(Node::Stmt(stmt));
                }
            }
            StmtKind::With { items, body, .. } | StmtKind::AsyncWith { items, body, .. } => {
                for item in items {
                    result.push(Node::Expr(&item.context_expr));
                    if let Some(expr) = &item.optional_vars {
                        result.push(Node::Expr(expr));
                    }
                }
                for stmt in body {
                    result.push(Node::Stmt(stmt));
                }
            }
            StmtKind::Match { .. } => {
                todo!("Support match statements");
            }
            StmtKind::Raise { exc, cause } => {
                if let Some(exc) = exc {
                    result.push(Node::Expr(exc));
                }
                if let Some(cause) = cause {
                    result.push(Node::Expr(cause));
                }
            }
            StmtKind::Assert { test, msg } => {
                result.push(Node::Expr(test));
                if let Some(msg) = msg {
                    result.push(Node::Expr(msg));
                }
            }
            StmtKind::Break => {}
            StmtKind::Continue => {}
            StmtKind::Try {
                body,
                handlers,
                orelse,
                finalbody,
            }
            | StmtKind::TryStar {
                body,
                handlers,
                orelse,
                finalbody,
            } => {
                for stmt in body {
                    result.push(Node::Stmt(stmt));
                }
                for handler in handlers {
                    result.push(Node::Excepthandler(handler));
                }
                for stmt in orelse {
                    result.push(Node::Stmt(stmt));
                }
                for stmt in finalbody {
                    result.push(Node::Stmt(stmt));
                }
            }
            StmtKind::Import { names } => {
                for name in names {
                    result.push(Node::Alias(name));
                }
            }
            StmtKind::ImportFrom { names, .. } => {
                for name in names {
                    result.push(Node::Alias(name));
                }
            }
            StmtKind::Global { .. } => {}
            StmtKind::Nonlocal { .. } => {}
        },
        // TODO(charlie): Actual logic, this doesn't do anything.
        Node::Expr(expr) => match &expr.node {
            ExprKind::BoolOp { values, .. } => {
                for value in values {
                    result.push(Node::Expr(value));
                }
            }
            ExprKind::NamedExpr { target, value } => {
                result.push(Node::Expr(target));
                result.push(Node::Expr(value));
            }
            ExprKind::BinOp { left, right, .. } => {
                result.push(Node::Expr(left));
                result.push(Node::Expr(right));
            }
            ExprKind::UnaryOp { operand, .. } => {
                result.push(Node::Expr(operand));
            }
            ExprKind::Lambda { body, args, .. } => {
                // TODO(charlie): Arguments.
                for expr in &args.defaults {
                    result.push(Node::Expr(expr));
                }
                for expr in &args.kw_defaults {
                    result.push(Node::Expr(expr));
                }
                result.push(Node::Expr(body));
            }
            ExprKind::IfExp { test, body, orelse } => {
                result.push(Node::Expr(body));
                result.push(Node::Expr(test));
                result.push(Node::Expr(orelse));
            }
            ExprKind::Dict { keys, values } => {
                for key in keys.iter().flatten() {
                    result.push(Node::Expr(key));
                }
                for value in values {
                    result.push(Node::Expr(value));
                }
            }
            ExprKind::Set { elts } => {
                for elt in elts {
                    result.push(Node::Expr(elt));
                }
            }
            ExprKind::ListComp { elt, generators } => {
                result.push(Node::Expr(elt));
                for generator in generators {
                    result.push(Node::Expr(&generator.target));
                    result.push(Node::Expr(&generator.iter));
                    for expr in &generator.ifs {
                        result.push(Node::Expr(expr));
                    }
                }
            }
            ExprKind::SetComp { elt, generators } => {
                result.push(Node::Expr(elt));
                for generator in generators {
                    result.push(Node::Expr(&generator.target));
                    result.push(Node::Expr(&generator.iter));
                    for expr in &generator.ifs {
                        result.push(Node::Expr(expr));
                    }
                }
            }
            ExprKind::DictComp {
                key,
                value,
                generators,
            } => {
                result.push(Node::Expr(key));
                result.push(Node::Expr(value));
                for generator in generators {
                    result.push(Node::Expr(&generator.target));
                    result.push(Node::Expr(&generator.iter));
                    for expr in &generator.ifs {
                        result.push(Node::Expr(expr));
                    }
                }
            }
            ExprKind::GeneratorExp { elt, generators } => {
                result.push(Node::Expr(elt));
                for generator in generators {
                    result.push(Node::Expr(&generator.target));
                    result.push(Node::Expr(&generator.iter));
                    for expr in &generator.ifs {
                        result.push(Node::Expr(expr));
                    }
                }
            }
            ExprKind::Await { value } => {
                result.push(Node::Expr(value));
            }
            ExprKind::Yield { value } => {
                if let Some(value) = value {
                    result.push(Node::Expr(value));
                }
            }
            ExprKind::YieldFrom { value } => {
                result.push(Node::Expr(value));
            }
            ExprKind::Compare {
                left, comparators, ..
            } => {
                result.push(Node::Expr(left));
                for comparator in comparators {
                    result.push(Node::Expr(comparator));
                }
            }
            ExprKind::Call {
                func,
                args,
                keywords,
            } => {
                result.push(Node::Expr(func));
                for arg in args {
                    result.push(Node::Expr(arg));
                }
                for keyword in keywords {
                    result.push(Node::Expr(&keyword.node.value));
                }
            }
            ExprKind::FormattedValue {
                value, format_spec, ..
            } => {
                result.push(Node::Expr(value));
                if let Some(format_spec) = format_spec {
                    result.push(Node::Expr(format_spec));
                }
            }
            ExprKind::JoinedStr { values } => {
                for value in values {
                    result.push(Node::Expr(value));
                }
            }
            ExprKind::Constant { .. } => {}
            ExprKind::Attribute { value, .. } => {
                result.push(Node::Expr(value));
            }
            ExprKind::Subscript { value, slice, .. } => {
                result.push(Node::Expr(value));
                result.push(Node::Expr(slice));
            }
            ExprKind::Starred { value, .. } => {
                result.push(Node::Expr(value));
            }

            ExprKind::Name { .. } => {}
            ExprKind::List { elts, .. } => {
                for elt in elts {
                    result.push(Node::Expr(elt));
                }
            }
            ExprKind::Tuple { elts, .. } => {
                for elt in elts {
                    result.push(Node::Expr(elt));
                }
            }
            ExprKind::Slice { lower, upper, step } => {
                result.push(Node::SliceIndex(lower));
                result.push(Node::SliceIndex(upper));
                if let Some(step) = step {
                    result.push(Node::SliceIndex(step));
                }
            }
        },
        Node::Alias(..) => {}
        Node::Excepthandler(excepthandler) => {
            // TODO(charlie): Ident.
            let ExcepthandlerKind::ExceptHandler { type_, body, .. } = &excepthandler.node;
            if let Some(type_) = type_ {
                result.push(Node::Expr(type_));
            }
            for stmt in body {
                result.push(Node::Stmt(stmt));
            }
        }
        Node::SliceIndex(slice_index) => {
            if let SliceIndexKind::Index { value } = &slice_index.node {
                result.push(Node::Expr(value));
            }
        }
    }
}

pub fn sorted_child_nodes<'a>(node: &Node<'a>) -> Vec<Node<'a>> {
    let mut result = Vec::new();
    sorted_child_nodes_inner(node, &mut result);
    result
}

pub fn decorate_token<'a>(
    token: &TriviaToken,
    node: &Node<'a>,
    enclosing_node: Option<Node<'a>>,
    enclosed_node: Option<Node<'a>>,
    cache: &mut FxHashMap<usize, Vec<Node<'a>>>,
) -> (
    Option<Node<'a>>,
    Option<Node<'a>>,
    Option<Node<'a>>,
    Option<Node<'a>>,
) {
    let child_nodes = cache
        .entry(node.id())
        .or_insert_with(|| sorted_child_nodes(node));

    let mut preceding_node = None;
    let mut following_node = None;
    let mut enclosed_node = enclosed_node;

    let mut left = 0;
    let mut right = child_nodes.len();

    while left < right {
        let middle = (left + right) / 2;
        let child = &child_nodes[middle];
        let start = match &child {
            Node::Stmt(node) => node.location,
            Node::Expr(node) => node.location,
            Node::Alias(node) => node.location,
            Node::Excepthandler(node) => node.location,
            Node::SliceIndex(node) => node.location,
            Node::Mod(..) => unreachable!("Node::Mod cannot be a child node"),
        };
        let end = match &child {
            Node::Stmt(node) => node.end_location.unwrap(),
            Node::Expr(node) => node.end_location.unwrap(),
            Node::Alias(node) => node.end_location.unwrap(),
            Node::Excepthandler(node) => node.end_location.unwrap(),
            Node::SliceIndex(node) => node.end_location.unwrap(),
            Node::Mod(..) => unreachable!("Node::Mod cannot be a child node"),
        };

        if let Some(existing) = &enclosed_node {
            // Special-case: if we're dealing with a statement that's a single expression,
            // we want to treat the expression as the enclosed node.
            let existing_start = match &existing {
                Node::Stmt(node) => node.location,
                Node::Expr(node) => node.location,
                Node::Alias(node) => node.location,
                Node::Excepthandler(node) => node.location,
                Node::SliceIndex(node) => node.location,
                Node::Mod(..) => unreachable!("Node::Mod cannot be a child node"),
            };
            let existing_end = match &existing {
                Node::Stmt(node) => node.end_location.unwrap(),
                Node::Expr(node) => node.end_location.unwrap(),
                Node::Alias(node) => node.end_location.unwrap(),
                Node::Excepthandler(node) => node.end_location.unwrap(),
                Node::SliceIndex(node) => node.end_location.unwrap(),
                Node::Mod(..) => unreachable!("Node::Mod cannot be a child node"),
            };
            if start == existing_start && end == existing_end {
                enclosed_node = Some(child.clone());
            }
        } else {
            if token.start <= start && token.end >= end {
                enclosed_node = Some(child.clone());
            }
        }

        // The comment is completely contained by this child node.
        if token.start >= start && token.end <= end {
            return decorate_token(
                token,
                &child.clone(),
                Some(child.clone()),
                enclosed_node,
                cache,
            );
        }

        if end <= token.start {
            // This child node falls completely before the comment.
            // Because we will never consider this node or any nodes
            // before it again, this node must be the closest preceding
            // node we have encountered so far.
            preceding_node = Some(child.clone());
            left = middle + 1;
            continue;
        }

        if token.end <= start {
            // This child node falls completely after the comment.
            // Because we will never consider this node or any nodes after
            // it again, this node must be the closest following node we
            // have encountered so far.
            following_node = Some(child.clone());
            right = middle;
            continue;
        }

        return (None, None, None, enclosed_node);
    }

    (
        preceding_node,
        following_node,
        enclosing_node,
        enclosed_node,
    )
}

#[derive(Debug, Default)]
pub struct TriviaIndex {
    pub stmt: FxHashMap<usize, Vec<Trivia>>,
    pub expr: FxHashMap<usize, Vec<Trivia>>,
    pub alias: FxHashMap<usize, Vec<Trivia>>,
    pub excepthandler: FxHashMap<usize, Vec<Trivia>>,
    pub slice_index: FxHashMap<usize, Vec<Trivia>>,
}

fn add_comment(comment: Trivia, node: &Node, trivia: &mut TriviaIndex) {
    match node {
        Node::Mod(_) => {}
        Node::Stmt(node) => {
            trivia
                .stmt
                .entry(node.id())
                .or_insert_with(Vec::new)
                .push(comment);
        }
        Node::Expr(node) => {
            trivia
                .expr
                .entry(node.id())
                .or_insert_with(Vec::new)
                .push(comment);
        }
        Node::Alias(node) => {
            trivia
                .alias
                .entry(node.id())
                .or_insert_with(Vec::new)
                .push(comment);
        }
        Node::Excepthandler(node) => {
            trivia
                .excepthandler
                .entry(node.id())
                .or_insert_with(Vec::new)
                .push(comment);
        }
        Node::SliceIndex(node) => {
            trivia
                .slice_index
                .entry(node.id())
                .or_insert_with(Vec::new)
                .push(comment);
        }
    }
}

pub fn decorate_trivia(tokens: Vec<TriviaToken>, python_ast: &[Stmt]) -> TriviaIndex {
    let mut stack = vec![];
    let mut cache = FxHashMap::default();
    for token in &tokens {
        let (preceding_node, following_node, enclosing_node, enclosed_node) =
            decorate_token(token, &Node::Mod(python_ast), None, None, &mut cache);

        stack.push((
            preceding_node,
            following_node,
            enclosing_node,
            enclosed_node,
        ));
    }

    let mut trivia_index = TriviaIndex::default();

    for (index, token) in tokens.into_iter().enumerate() {
        let (preceding_node, following_node, enclosing_node, enclosed_node) = &stack[index];
        match token.kind {
            TriviaTokenKind::EmptyLine | TriviaTokenKind::OwnLineComment => {
                if let Some(following_node) = following_node {
                    // Always a leading comment.
                    add_comment(
                        Trivia::from_token(&token, Relationship::Leading),
                        following_node,
                        &mut trivia_index,
                    );
                } else if let Some(enclosing_node) = enclosing_node {
                    // TODO(charlie): Prettier puts this `else if` after `preceding_note`.
                    add_comment(
                        Trivia::from_token(&token, Relationship::Dangling),
                        enclosing_node,
                        &mut trivia_index,
                    );
                } else if let Some(preceding_node) = preceding_node {
                    add_comment(
                        Trivia::from_token(&token, Relationship::Trailing),
                        preceding_node,
                        &mut trivia_index,
                    );
                } else {
                    unreachable!("Attach token to the ast: {:?}", token);
                }
            }
            TriviaTokenKind::EndOfLineComment => {
                if let Some(preceding_node) = preceding_node {
                    // There is content before this comment on the same line, but
                    // none after it, so prefer a trailing comment of the previous node.
                    add_comment(
                        Trivia::from_token(&token, Relationship::Trailing),
                        preceding_node,
                        &mut trivia_index,
                    );
                } else if let Some(enclosing_node) = enclosing_node {
                    // TODO(charlie): Prettier puts this later, and uses `Relationship::Dangling`.
                    add_comment(
                        Trivia::from_token(&token, Relationship::Trailing),
                        enclosing_node,
                        &mut trivia_index,
                    );
                } else if let Some(following_node) = following_node {
                    add_comment(
                        Trivia::from_token(&token, Relationship::Leading),
                        following_node,
                        &mut trivia_index,
                    );
                } else {
                    unreachable!("Attach token to the ast: {:?}", token);
                }
            }
            TriviaTokenKind::MagicTrailingComma => {
                if let Some(enclosing_node) = enclosing_node {
                    add_comment(
                        Trivia::from_token(&token, Relationship::Trailing),
                        enclosing_node,
                        &mut trivia_index,
                    );
                } else {
                    unreachable!("Attach token to the ast: {:?}", token);
                }
            }
            TriviaTokenKind::Parentheses => {
                if let Some(enclosed_node) = enclosed_node {
                    add_comment(
                        Trivia::from_token(&token, Relationship::Leading),
                        enclosed_node,
                        &mut trivia_index,
                    );
                } else {
                    unreachable!("Attach token to the ast: {:?}", token);
                }
            }
        }
    }

    trivia_index
}
