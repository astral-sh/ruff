use ruff_text_size::{TextRange, TextSize};
use rustc_hash::FxHashMap;
use rustpython_parser::lexer::LexResult;
use rustpython_parser::Tok;

use crate::cst::{
    Alias, Arg, Body, BoolOp, CmpOp, Excepthandler, ExcepthandlerKind, Expr, ExprKind, Keyword,
    Operator, Pattern, PatternKind, SliceIndex, SliceIndexKind, Stmt, StmtKind, UnaryOp,
};

#[derive(Clone, Copy, Debug)]
pub(crate) enum Node<'a> {
    Alias(&'a Alias),
    Arg(&'a Arg),
    Body(&'a Body),
    BoolOp(&'a BoolOp),
    CmpOp(&'a CmpOp),
    Excepthandler(&'a Excepthandler),
    Expr(&'a Expr),
    Keyword(&'a Keyword),
    Mod(&'a [Stmt]),
    Operator(&'a Operator),
    Pattern(&'a Pattern),
    SliceIndex(&'a SliceIndex),
    Stmt(&'a Stmt),
    UnaryOp(&'a UnaryOp),
}

impl Node<'_> {
    pub(crate) fn id(&self) -> usize {
        match self {
            Node::Alias(node) => node.id(),
            Node::Arg(node) => node.id(),
            Node::Body(node) => node.id(),
            Node::BoolOp(node) => node.id(),
            Node::CmpOp(node) => node.id(),
            Node::Excepthandler(node) => node.id(),
            Node::Expr(node) => node.id(),
            Node::Keyword(node) => node.id(),
            Node::Mod(nodes) => nodes as *const _ as usize,
            Node::Operator(node) => node.id(),
            Node::Pattern(node) => node.id(),
            Node::SliceIndex(node) => node.id(),
            Node::Stmt(node) => node.id(),
            Node::UnaryOp(node) => node.id(),
        }
    }

    pub(crate) fn start(&self) -> TextSize {
        match self {
            Node::Alias(node) => node.start(),
            Node::Arg(node) => node.start(),
            Node::Body(node) => node.start(),
            Node::BoolOp(node) => node.start(),
            Node::CmpOp(node) => node.start(),
            Node::Excepthandler(node) => node.start(),
            Node::Expr(node) => node.start(),
            Node::Keyword(node) => node.start(),
            Node::Mod(..) => unreachable!("Node::Mod cannot be a child node"),
            Node::Operator(node) => node.start(),
            Node::Pattern(node) => node.start(),
            Node::SliceIndex(node) => node.start(),
            Node::Stmt(node) => node.start(),
            Node::UnaryOp(node) => node.start(),
        }
    }

    pub(crate) fn end(&self) -> TextSize {
        match self {
            Node::Alias(node) => node.end(),
            Node::Arg(node) => node.end(),
            Node::Body(node) => node.end(),
            Node::BoolOp(node) => node.end(),
            Node::CmpOp(node) => node.end(),
            Node::Excepthandler(node) => node.end(),
            Node::Expr(node) => node.end(),
            Node::Keyword(node) => node.end(),
            Node::Mod(..) => unreachable!("Node::Mod cannot be a child node"),
            Node::Operator(node) => node.end(),
            Node::Pattern(node) => node.end(),
            Node::SliceIndex(node) => node.end(),
            Node::Stmt(node) => node.end(),
            Node::UnaryOp(node) => node.end(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TriviaTokenKind {
    OwnLineComment,
    EndOfLineComment,
    MagicTrailingComma,
    EmptyLine,
    Parentheses,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TriviaToken {
    pub(crate) range: TextRange,
    pub(crate) kind: TriviaTokenKind,
}

impl TriviaToken {
    pub(crate) const fn start(&self) -> TextSize {
        self.range.start()
    }

    pub(crate) const fn end(&self) -> TextSize {
        self.range.end()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, is_macro::Is)]
pub(crate) enum TriviaKind {
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
    OwnLineComment(TextRange),
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
    EndOfLineComment(TextRange),
    MagicTrailingComma,
    EmptyLine,
    Parentheses,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, is_macro::Is)]
pub(crate) enum Relationship {
    Leading,
    Trailing,
    Dangling,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, is_macro::Is)]
pub(crate) enum Parenthesize {
    /// Always parenthesize the statement or expression.
    Always,
    /// Never parenthesize the statement or expression.
    Never,
    /// Parenthesize the statement or expression if it expands.
    IfExpanded,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Trivia {
    pub(crate) kind: TriviaKind,
    pub(crate) relationship: Relationship,
}

impl Trivia {
    pub(crate) fn from_token(token: &TriviaToken, relationship: Relationship) -> Self {
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
                kind: TriviaKind::OwnLineComment(token.range),
                relationship,
            },
            TriviaTokenKind::EndOfLineComment => Self {
                kind: TriviaKind::EndOfLineComment(token.range),
                relationship,
            },
            TriviaTokenKind::Parentheses => Self {
                kind: TriviaKind::Parentheses,
                relationship,
            },
        }
    }
}

pub(crate) fn extract_trivia_tokens(lxr: &[LexResult]) -> Vec<TriviaToken> {
    let mut tokens = vec![];
    let mut prev_tok: Option<(&Tok, TextRange)> = None;
    let mut prev_semantic_tok: Option<(&Tok, TextRange)> = None;
    let mut parens = vec![];

    for (tok, range) in lxr.iter().flatten() {
        let after_new_line = matches!(prev_tok, Some((Tok::Newline | Tok::NonLogicalNewline, _)));

        // Add empty lines.
        if after_new_line && matches!(tok, Tok::NonLogicalNewline) {
            tokens.push(TriviaToken {
                range: *range,
                kind: TriviaTokenKind::EmptyLine,
            });
        }

        // Add comments.
        if matches!(tok, Tok::Comment(..)) {
            tokens.push(TriviaToken {
                range: *range,
                // Used to use prev_non-newline_tok
                kind: if after_new_line || prev_tok.is_none() {
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
            if let Some((prev_tok, prev_range)) = prev_semantic_tok {
                if prev_tok == &Tok::Comma {
                    tokens.push(TriviaToken {
                        range: prev_range,
                        kind: TriviaTokenKind::MagicTrailingComma,
                    });
                }
            }
        }

        if matches!(tok, Tok::Lpar) {
            if prev_tok.map_or(true, |(prev_tok, _)| {
                !matches!(
                    prev_tok,
                    Tok::Name { .. }
                        | Tok::Int { .. }
                        | Tok::Float { .. }
                        | Tok::Complex { .. }
                        | Tok::String { .. }
                )
            }) {
                parens.push((range.start(), true));
            } else {
                parens.push((range.start(), false));
            }
        } else if matches!(tok, Tok::Rpar) {
            let (start, explicit) = parens.pop().unwrap();
            if explicit {
                tokens.push(TriviaToken {
                    range: TextRange::new(start, range.end()),
                    kind: TriviaTokenKind::Parentheses,
                });
            }
        }

        prev_tok = Some((tok, *range));

        // Track the most recent semantic token.
        if !matches!(
            tok,
            Tok::Newline | Tok::NonLogicalNewline | Tok::Comment(..)
        ) {
            prev_semantic_tok = Some((tok, *range));
        }
    }
    tokens
}

fn sorted_child_nodes_inner<'a>(node: Node<'a>, result: &mut Vec<Node<'a>>) {
    match node {
        Node::Mod(nodes) => {
            for stmt in nodes.iter() {
                result.push(Node::Stmt(stmt));
            }
        }
        Node::Body(body) => {
            result.extend(body.iter().map(Node::Stmt));
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
                    result.push(Node::Arg(arg));
                }
                for arg in &args.args {
                    result.push(Node::Arg(arg));
                }
                if let Some(arg) = &args.vararg {
                    result.push(Node::Arg(arg));
                }
                for arg in &args.kwonlyargs {
                    result.push(Node::Arg(arg));
                }
                if let Some(arg) = &args.kwarg {
                    result.push(Node::Arg(arg));
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
                result.push(Node::Body(body));
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
                    result.push(Node::Keyword(keyword));
                }
                result.push(Node::Body(body));
            }
            StmtKind::Delete { targets } => {
                for target in targets {
                    result.push(Node::Expr(target));
                }
            }
            StmtKind::AugAssign { target, op, value } => {
                result.push(Node::Expr(target));
                result.push(Node::Operator(op));
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
                result.push(Node::Body(body));
                if let Some(orelse) = orelse {
                    result.push(Node::Body(orelse));
                }
            }
            StmtKind::While { test, body, orelse } => {
                result.push(Node::Expr(test));
                result.push(Node::Body(body));
                if let Some(orelse) = orelse {
                    result.push(Node::Body(orelse));
                }
            }
            StmtKind::If {
                test, body, orelse, ..
            } => {
                result.push(Node::Expr(test));
                result.push(Node::Body(body));
                if let Some(orelse) = orelse {
                    result.push(Node::Body(orelse));
                }
            }
            StmtKind::With { items, body, .. } | StmtKind::AsyncWith { items, body, .. } => {
                for item in items {
                    result.push(Node::Expr(&item.context_expr));
                    if let Some(expr) = &item.optional_vars {
                        result.push(Node::Expr(expr));
                    }
                }
                result.push(Node::Body(body));
            }
            StmtKind::Match { subject, cases } => {
                result.push(Node::Expr(subject));
                for case in cases {
                    result.push(Node::Pattern(&case.pattern));
                    if let Some(expr) = &case.guard {
                        result.push(Node::Expr(expr));
                    }
                    result.push(Node::Body(&case.body));
                }
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
                result.push(Node::Body(body));
                for handler in handlers {
                    result.push(Node::Excepthandler(handler));
                }
                if let Some(orelse) = orelse {
                    result.push(Node::Body(orelse));
                }
                if let Some(finalbody) = finalbody {
                    result.push(Node::Body(finalbody));
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
        Node::Arg(arg) => {
            if let Some(annotation) = &arg.node.annotation {
                result.push(Node::Expr(annotation));
            }
        }
        Node::Expr(expr) => match &expr.node {
            ExprKind::BoolOp { ops, values } => {
                result.push(Node::Expr(&values[0]));
                for (op, value) in ops.iter().zip(&values[1..]) {
                    result.push(Node::BoolOp(op));
                    result.push(Node::Expr(value));
                }
            }
            ExprKind::NamedExpr { target, value } => {
                result.push(Node::Expr(target));
                result.push(Node::Expr(value));
            }
            ExprKind::BinOp { left, op, right } => {
                result.push(Node::Expr(left));
                result.push(Node::Operator(op));
                result.push(Node::Expr(right));
            }
            ExprKind::UnaryOp { op, operand } => {
                result.push(Node::UnaryOp(op));
                result.push(Node::Expr(operand));
            }
            ExprKind::Lambda { body, args, .. } => {
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
                left,
                ops,
                comparators,
            } => {
                result.push(Node::Expr(left));
                for (op, comparator) in ops.iter().zip(comparators) {
                    result.push(Node::CmpOp(op));
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
                    result.push(Node::Keyword(keyword));
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
        Node::Keyword(keyword) => {
            result.push(Node::Expr(&keyword.node.value));
        }
        Node::Alias(..) => {}
        Node::Excepthandler(excepthandler) => {
            let ExcepthandlerKind::ExceptHandler { type_, body, .. } = &excepthandler.node;
            if let Some(type_) = type_ {
                result.push(Node::Expr(type_));
            }
            result.push(Node::Body(body));
        }
        Node::SliceIndex(slice_index) => {
            if let SliceIndexKind::Index { value } = &slice_index.node {
                result.push(Node::Expr(value));
            }
        }
        Node::Pattern(pattern) => match &pattern.node {
            PatternKind::MatchValue { value } => {
                result.push(Node::Expr(value));
            }
            PatternKind::MatchSingleton { .. } => {}
            PatternKind::MatchSequence { patterns } => {
                for pattern in patterns {
                    result.push(Node::Pattern(pattern));
                }
            }
            PatternKind::MatchMapping { keys, patterns, .. } => {
                for (key, pattern) in keys.iter().zip(patterns.iter()) {
                    result.push(Node::Expr(key));
                    result.push(Node::Pattern(pattern));
                }
            }
            PatternKind::MatchClass {
                cls,
                patterns,
                kwd_patterns,
                ..
            } => {
                result.push(Node::Expr(cls));
                for pattern in patterns {
                    result.push(Node::Pattern(pattern));
                }
                for pattern in kwd_patterns {
                    result.push(Node::Pattern(pattern));
                }
            }
            PatternKind::MatchStar { .. } => {}
            PatternKind::MatchAs { pattern, .. } => {
                if let Some(pattern) = pattern {
                    result.push(Node::Pattern(pattern));
                }
            }
            PatternKind::MatchOr { patterns } => {
                for pattern in patterns {
                    result.push(Node::Pattern(pattern));
                }
            }
        },
        Node::BoolOp(..) => {}
        Node::UnaryOp(..) => {}
        Node::Operator(..) => {}
        Node::CmpOp(..) => {}
    }
}

pub(crate) fn sorted_child_nodes(node: Node) -> Vec<Node> {
    let mut result = Vec::new();
    sorted_child_nodes_inner(node, &mut result);

    result
}

pub(crate) fn decorate_token<'a>(
    token: &TriviaToken,
    node: Node<'a>,
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
        let child = child_nodes[middle];
        let start = child.start();
        let end = child.end();

        if let Some(existing) = &enclosed_node {
            // Special-case: if we're dealing with a statement that's a single expression,
            // we want to treat the expression as the enclosed node.
            let existing_start = existing.start();
            let existing_end = existing.end();
            if start == existing_start && end == existing_end {
                enclosed_node = Some(child);
            }
        } else {
            if token.start() <= start && token.end() >= end {
                enclosed_node = Some(child);
            }
        }

        // The comment is completely contained by this child node.
        if token.start() >= start && token.end() <= end {
            return decorate_token(token, child, Some(child), enclosed_node, cache);
        }

        if end <= token.start() {
            // This child node falls completely before the comment.
            // Because we will never consider this node or any nodes
            // before it again, this node must be the closest preceding
            // node we have encountered so far.
            preceding_node = Some(child);
            left = middle + 1;
            continue;
        }

        if token.end() <= start {
            // This child node falls completely after the comment.
            // Because we will never consider this node or any nodes after
            // it again, this node must be the closest following node we
            // have encountered so far.
            following_node = Some(child);
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
pub(crate) struct TriviaIndex {
    pub(crate) alias: FxHashMap<usize, Vec<Trivia>>,
    pub(crate) arg: FxHashMap<usize, Vec<Trivia>>,
    pub(crate) body: FxHashMap<usize, Vec<Trivia>>,
    pub(crate) bool_op: FxHashMap<usize, Vec<Trivia>>,
    pub(crate) cmp_op: FxHashMap<usize, Vec<Trivia>>,
    pub(crate) excepthandler: FxHashMap<usize, Vec<Trivia>>,
    pub(crate) expr: FxHashMap<usize, Vec<Trivia>>,
    pub(crate) keyword: FxHashMap<usize, Vec<Trivia>>,
    pub(crate) operator: FxHashMap<usize, Vec<Trivia>>,
    pub(crate) pattern: FxHashMap<usize, Vec<Trivia>>,
    pub(crate) slice_index: FxHashMap<usize, Vec<Trivia>>,
    pub(crate) stmt: FxHashMap<usize, Vec<Trivia>>,
    pub(crate) unary_op: FxHashMap<usize, Vec<Trivia>>,
}

fn add_comment(comment: Trivia, node: &Node, trivia: &mut TriviaIndex) {
    match node {
        Node::Alias(node) => {
            trivia
                .alias
                .entry(node.id())
                .or_insert_with(Vec::new)
                .push(comment);
        }
        Node::Arg(node) => {
            trivia
                .arg
                .entry(node.id())
                .or_insert_with(Vec::new)
                .push(comment);
        }
        Node::Body(node) => {
            trivia
                .body
                .entry(node.id())
                .or_insert_with(Vec::new)
                .push(comment);
        }
        Node::BoolOp(node) => {
            trivia
                .bool_op
                .entry(node.id())
                .or_insert_with(Vec::new)
                .push(comment);
        }
        Node::CmpOp(node) => {
            trivia
                .cmp_op
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
        Node::Expr(node) => {
            trivia
                .expr
                .entry(node.id())
                .or_insert_with(Vec::new)
                .push(comment);
        }
        Node::Keyword(node) => {
            trivia
                .keyword
                .entry(node.id())
                .or_insert_with(Vec::new)
                .push(comment);
        }
        Node::Operator(node) => {
            trivia
                .operator
                .entry(node.id())
                .or_insert_with(Vec::new)
                .push(comment);
        }
        Node::Pattern(node) => {
            trivia
                .pattern
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
        Node::Stmt(node) => {
            trivia
                .stmt
                .entry(node.id())
                .or_insert_with(Vec::new)
                .push(comment);
        }
        Node::UnaryOp(node) => {
            trivia
                .unary_op
                .entry(node.id())
                .or_insert_with(Vec::new)
                .push(comment);
        }
        Node::Mod(_) => {}
    }
}

pub(crate) fn decorate_trivia(tokens: Vec<TriviaToken>, python_ast: &[Stmt]) -> TriviaIndex {
    let mut stack = vec![];
    let mut cache = FxHashMap::default();
    for token in &tokens {
        let (preceding_node, following_node, enclosing_node, enclosed_node) =
            decorate_token(token, Node::Mod(python_ast), None, None, &mut cache);

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
