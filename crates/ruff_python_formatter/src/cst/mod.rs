#![allow(clippy::derive_partial_eq_without_eq)]

use std::iter;
use std::ops::Deref;

use itertools::Itertools;
use ruff_text_size::{TextRange, TextSize};
use rustpython_parser::ast::{Constant, ConversionFlag, Ranged};
use rustpython_parser::{ast, Mode};

use ruff_python_ast::source_code::Locator;

use crate::cst::helpers::{expand_indented_block, find_tok, is_elif};
use crate::trivia::{Parenthesize, Trivia};

pub(crate) mod helpers;
pub(crate) mod visitor;

type Ident = String;

#[derive(Clone, Debug, PartialEq)]
pub struct Attributed<T> {
    pub range: TextRange,
    pub node: T,
    pub trivia: Vec<Trivia>,
    pub parentheses: Parenthesize,
}

impl<T> Attributed<T> {
    pub fn new(range: TextRange, node: T) -> Self {
        Self {
            range,
            node,
            trivia: Vec::new(),
            parentheses: Parenthesize::Never,
        }
    }

    pub const fn range(&self) -> TextRange {
        self.range
    }

    pub const fn start(&self) -> TextSize {
        self.range.start()
    }

    pub const fn end(&self) -> TextSize {
        self.range.end()
    }

    pub fn add_trivia(&mut self, trivia: Trivia) {
        self.trivia.push(trivia);
    }

    pub fn id(&self) -> usize {
        std::ptr::addr_of!(self.node) as usize
    }
}

impl<T> Deref for Attributed<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.node
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ExprContext {
    Load,
    Store,
    Del,
}

impl From<rustpython_parser::ast::ExprContext> for ExprContext {
    fn from(context: rustpython_parser::ast::ExprContext) -> Self {
        match context {
            rustpython_parser::ast::ExprContext::Load => Self::Load,
            rustpython_parser::ast::ExprContext::Store => Self::Store,
            rustpython_parser::ast::ExprContext::Del => Self::Del,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum BoolOpKind {
    And,
    Or,
}

impl From<&rustpython_parser::ast::Boolop> for BoolOpKind {
    fn from(op: &rustpython_parser::ast::Boolop) -> Self {
        match op {
            rustpython_parser::ast::Boolop::And => Self::And,
            rustpython_parser::ast::Boolop::Or => Self::Or,
        }
    }
}

pub(crate) type BoolOp = Attributed<BoolOpKind>;

#[derive(Clone, Debug, PartialEq)]
pub enum OperatorKind {
    Add,
    Sub,
    Mult,
    MatMult,
    Div,
    Mod,
    Pow,
    LShift,
    RShift,
    BitOr,
    BitXor,
    BitAnd,
    FloorDiv,
}

pub(crate) type Operator = Attributed<OperatorKind>;

impl From<&rustpython_parser::ast::Operator> for OperatorKind {
    fn from(op: &rustpython_parser::ast::Operator) -> Self {
        match op {
            rustpython_parser::ast::Operator::Add => Self::Add,
            rustpython_parser::ast::Operator::Sub => Self::Sub,
            rustpython_parser::ast::Operator::Mult => Self::Mult,
            rustpython_parser::ast::Operator::MatMult => Self::MatMult,
            rustpython_parser::ast::Operator::Div => Self::Div,
            rustpython_parser::ast::Operator::Mod => Self::Mod,
            rustpython_parser::ast::Operator::Pow => Self::Pow,
            rustpython_parser::ast::Operator::LShift => Self::LShift,
            rustpython_parser::ast::Operator::RShift => Self::RShift,
            rustpython_parser::ast::Operator::BitOr => Self::BitOr,
            rustpython_parser::ast::Operator::BitXor => Self::BitXor,
            rustpython_parser::ast::Operator::BitAnd => Self::BitAnd,
            rustpython_parser::ast::Operator::FloorDiv => Self::FloorDiv,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum UnaryOpKind {
    Invert,
    Not,
    UAdd,
    USub,
}

pub(crate) type UnaryOp = Attributed<UnaryOpKind>;

impl From<&rustpython_parser::ast::Unaryop> for UnaryOpKind {
    fn from(op: &rustpython_parser::ast::Unaryop) -> Self {
        match op {
            rustpython_parser::ast::Unaryop::Invert => Self::Invert,
            rustpython_parser::ast::Unaryop::Not => Self::Not,
            rustpython_parser::ast::Unaryop::UAdd => Self::UAdd,
            rustpython_parser::ast::Unaryop::USub => Self::USub,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum CmpOpKind {
    Eq,
    NotEq,
    Lt,
    LtE,
    Gt,
    GtE,
    Is,
    IsNot,
    In,
    NotIn,
}

pub(crate) type CmpOp = Attributed<CmpOpKind>;

impl From<&rustpython_parser::ast::Cmpop> for CmpOpKind {
    fn from(op: &rustpython_parser::ast::Cmpop) -> Self {
        match op {
            rustpython_parser::ast::Cmpop::Eq => Self::Eq,
            rustpython_parser::ast::Cmpop::NotEq => Self::NotEq,
            rustpython_parser::ast::Cmpop::Lt => Self::Lt,
            rustpython_parser::ast::Cmpop::LtE => Self::LtE,
            rustpython_parser::ast::Cmpop::Gt => Self::Gt,
            rustpython_parser::ast::Cmpop::GtE => Self::GtE,
            rustpython_parser::ast::Cmpop::Is => Self::Is,
            rustpython_parser::ast::Cmpop::IsNot => Self::IsNot,
            rustpython_parser::ast::Cmpop::In => Self::In,
            rustpython_parser::ast::Cmpop::NotIn => Self::NotIn,
        }
    }
}

pub(crate) type Body = Attributed<Vec<Stmt>>;

impl From<(Vec<rustpython_parser::ast::Stmt>, &Locator<'_>)> for Body {
    fn from((body, locator): (Vec<rustpython_parser::ast::Stmt>, &Locator)) -> Self {
        Body {
            range: body.first().unwrap().range(),
            node: body
                .into_iter()
                .map(|node| (node, locator).into())
                .collect(),
            trivia: vec![],
            parentheses: Parenthesize::Never,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum StmtKind {
    FunctionDef {
        name: Ident,
        args: Box<Arguments>,
        body: Body,
        decorator_list: Vec<Expr>,
        returns: Option<Box<Expr>>,
        type_comment: Option<String>,
    },
    AsyncFunctionDef {
        name: Ident,
        args: Box<Arguments>,
        body: Body,
        decorator_list: Vec<Expr>,
        returns: Option<Box<Expr>>,
        type_comment: Option<String>,
    },
    ClassDef {
        name: Ident,
        bases: Vec<Expr>,
        keywords: Vec<Keyword>,
        body: Body,
        decorator_list: Vec<Expr>,
    },
    Return {
        value: Option<Expr>,
    },
    Delete {
        targets: Vec<Expr>,
    },
    Assign {
        targets: Vec<Expr>,
        value: Box<Expr>,
        type_comment: Option<String>,
    },
    AugAssign {
        target: Box<Expr>,
        op: Operator,
        value: Box<Expr>,
    },
    AnnAssign {
        target: Box<Expr>,
        annotation: Box<Expr>,
        value: Option<Box<Expr>>,
        simple: usize,
    },
    For {
        target: Box<Expr>,
        iter: Box<Expr>,
        body: Body,
        orelse: Option<Body>,
        type_comment: Option<String>,
    },
    AsyncFor {
        target: Box<Expr>,
        iter: Box<Expr>,
        body: Body,
        orelse: Option<Body>,
        type_comment: Option<String>,
    },
    While {
        test: Box<Expr>,
        body: Body,
        orelse: Option<Body>,
    },
    If {
        test: Box<Expr>,
        body: Body,
        orelse: Option<Body>,
        is_elif: bool,
    },
    With {
        items: Vec<Withitem>,
        body: Body,
        type_comment: Option<String>,
    },
    AsyncWith {
        items: Vec<Withitem>,
        body: Body,
        type_comment: Option<String>,
    },
    Match {
        subject: Box<Expr>,
        cases: Vec<MatchCase>,
    },
    Raise {
        exc: Option<Box<Expr>>,
        cause: Option<Box<Expr>>,
    },
    Try {
        body: Body,
        handlers: Vec<Excepthandler>,
        orelse: Option<Body>,
        finalbody: Option<Body>,
    },
    TryStar {
        body: Body,
        handlers: Vec<Excepthandler>,
        orelse: Option<Body>,
        finalbody: Option<Body>,
    },
    Assert {
        test: Box<Expr>,
        msg: Option<Box<Expr>>,
    },
    Import {
        names: Vec<Alias>,
    },
    ImportFrom {
        module: Option<Ident>,
        names: Vec<Alias>,
        level: Option<u32>,
    },
    Global {
        names: Vec<Ident>,
    },
    Nonlocal {
        names: Vec<Ident>,
    },
    Expr {
        value: Box<Expr>,
    },
    Pass,
    Break,
    Continue,
}

pub(crate) type Stmt = Attributed<StmtKind>;

#[derive(Clone, Debug, PartialEq)]
pub enum ExprKind {
    BoolOp {
        ops: Vec<BoolOp>,
        values: Vec<Expr>,
    },
    NamedExpr {
        target: Box<Expr>,
        value: Box<Expr>,
    },
    BinOp {
        left: Box<Expr>,
        op: Operator,
        right: Box<Expr>,
    },
    UnaryOp {
        op: UnaryOp,
        operand: Box<Expr>,
    },
    Lambda {
        args: Box<Arguments>,
        body: Box<Expr>,
    },
    IfExp {
        test: Box<Expr>,
        body: Box<Expr>,
        orelse: Box<Expr>,
    },
    Dict {
        keys: Vec<Option<Expr>>,
        values: Vec<Expr>,
    },
    Set {
        elts: Vec<Expr>,
    },
    ListComp {
        elt: Box<Expr>,
        generators: Vec<Comprehension>,
    },
    SetComp {
        elt: Box<Expr>,
        generators: Vec<Comprehension>,
    },
    DictComp {
        key: Box<Expr>,
        value: Box<Expr>,
        generators: Vec<Comprehension>,
    },
    GeneratorExp {
        elt: Box<Expr>,
        generators: Vec<Comprehension>,
    },
    Await {
        value: Box<Expr>,
    },
    Yield {
        value: Option<Box<Expr>>,
    },
    YieldFrom {
        value: Box<Expr>,
    },
    Compare {
        left: Box<Expr>,
        ops: Vec<CmpOp>,
        comparators: Vec<Expr>,
    },
    Call {
        func: Box<Expr>,
        args: Vec<Expr>,
        keywords: Vec<Keyword>,
    },
    FormattedValue {
        value: Box<Expr>,
        conversion: ConversionFlag,
        format_spec: Option<Box<Expr>>,
    },
    JoinedStr {
        values: Vec<Expr>,
    },
    Constant {
        value: Constant,
        kind: Option<String>,
    },
    Attribute {
        value: Box<Expr>,
        attr: Ident,
        ctx: ExprContext,
    },
    Subscript {
        value: Box<Expr>,
        slice: Box<Expr>,
        ctx: ExprContext,
    },
    Starred {
        value: Box<Expr>,
        ctx: ExprContext,
    },
    Name {
        id: String,
        ctx: ExprContext,
    },
    List {
        elts: Vec<Expr>,
        ctx: ExprContext,
    },
    Tuple {
        elts: Vec<Expr>,
        ctx: ExprContext,
    },
    Slice {
        lower: SliceIndex,
        upper: SliceIndex,
        step: Option<SliceIndex>,
    },
}

pub(crate) type Expr = Attributed<ExprKind>;

#[derive(Clone, Debug, PartialEq)]
pub struct Comprehension {
    pub target: Expr,
    pub iter: Expr,
    pub ifs: Vec<Expr>,
    pub is_async: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ExcepthandlerKind {
    ExceptHandler {
        type_: Option<Box<Expr>>,
        name: Option<Ident>,
        body: Body,
    },
}

pub(crate) type Excepthandler = Attributed<ExcepthandlerKind>;

#[derive(Clone, Debug, PartialEq)]
pub enum SliceIndexKind {
    /// The index slot exists, but is empty.
    Empty,
    /// The index slot contains an expression.
    Index { value: Box<Expr> },
}

pub(crate) type SliceIndex = Attributed<SliceIndexKind>;

#[derive(Clone, Debug, PartialEq)]
pub struct Arguments {
    pub posonlyargs: Vec<Arg>,
    pub args: Vec<Arg>,
    pub vararg: Option<Box<Arg>>,
    pub kwonlyargs: Vec<Arg>,
    pub kw_defaults: Vec<Expr>,
    pub kwarg: Option<Box<Arg>>,
    pub defaults: Vec<Expr>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ArgData {
    pub arg: Ident,
    pub annotation: Option<Box<Expr>>,
    pub type_comment: Option<String>,
}

pub(crate) type Arg = Attributed<ArgData>;

#[derive(Clone, Debug, PartialEq)]
pub struct KeywordData {
    pub arg: Option<Ident>,
    pub value: Expr,
}

pub(crate) type Keyword = Attributed<KeywordData>;

#[derive(Clone, Debug, PartialEq)]
pub struct AliasData {
    pub name: Ident,
    pub asname: Option<Ident>,
}

pub(crate) type Alias = Attributed<AliasData>;

#[derive(Clone, Debug, PartialEq)]
pub struct Withitem {
    pub context_expr: Expr,
    pub optional_vars: Option<Box<Expr>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MatchCase {
    pub pattern: Pattern,
    pub guard: Option<Box<Expr>>,
    pub body: Body,
}

#[allow(clippy::enum_variant_names)]
#[derive(Clone, Debug, PartialEq)]
pub enum PatternKind {
    MatchValue {
        value: Box<Expr>,
    },
    MatchSingleton {
        value: Constant,
    },
    MatchSequence {
        patterns: Vec<Pattern>,
    },
    MatchMapping {
        keys: Vec<Expr>,
        patterns: Vec<Pattern>,
        rest: Option<Ident>,
    },
    MatchClass {
        cls: Box<Expr>,
        patterns: Vec<Pattern>,
        kwd_attrs: Vec<Ident>,
        kwd_patterns: Vec<Pattern>,
    },
    MatchStar {
        name: Option<Ident>,
    },
    MatchAs {
        pattern: Option<Box<Pattern>>,
        name: Option<Ident>,
    },
    MatchOr {
        patterns: Vec<Pattern>,
    },
}

pub(crate) type Pattern = Attributed<PatternKind>;

impl From<(rustpython_parser::ast::Alias, &Locator<'_>)> for Alias {
    fn from((alias, _locator): (rustpython_parser::ast::Alias, &Locator)) -> Self {
        Alias {
            range: alias.range(),
            node: AliasData {
                name: alias.name.to_string(),
                asname: alias
                    .asname
                    .as_ref()
                    .map(rustpython_parser::ast::Identifier::to_string),
            },
            trivia: vec![],
            parentheses: Parenthesize::Never,
        }
    }
}

impl From<(rustpython_parser::ast::Withitem, &Locator<'_>)> for Withitem {
    fn from((withitem, locator): (rustpython_parser::ast::Withitem, &Locator)) -> Self {
        Withitem {
            context_expr: (withitem.context_expr, locator).into(),
            optional_vars: withitem
                .optional_vars
                .map(|v| Box::new((*v, locator).into())),
        }
    }
}

impl From<(rustpython_parser::ast::Excepthandler, &Locator<'_>)> for Excepthandler {
    fn from((excepthandler, locator): (rustpython_parser::ast::Excepthandler, &Locator)) -> Self {
        let rustpython_parser::ast::Excepthandler::ExceptHandler(ast::ExcepthandlerExceptHandler {
            type_,
            name,
            body,
            range,
        }) = excepthandler;

        // Find the start and end of the `body`.
        let body = {
            let body_range =
                expand_indented_block(range.start(), body.last().unwrap().end(), locator);
            Body {
                range: body_range,
                node: body
                    .into_iter()
                    .map(|node| (node, locator).into())
                    .collect(),
                trivia: vec![],
                parentheses: Parenthesize::Never,
            }
        };

        Excepthandler {
            range: TextRange::new(range.start(), body.end()),
            node: ExcepthandlerKind::ExceptHandler {
                type_: type_.map(|type_| Box::new((*type_, locator).into())),
                name: name.map(Into::into),
                body,
            },
            trivia: vec![],
            parentheses: Parenthesize::Never,
        }
    }
}

impl From<(rustpython_parser::ast::Pattern, &Locator<'_>)> for Pattern {
    fn from((pattern, locator): (rustpython_parser::ast::Pattern, &Locator)) -> Self {
        Pattern {
            range: pattern.range(),
            node: match pattern {
                rustpython_parser::ast::Pattern::MatchValue(
                    rustpython_parser::ast::PatternMatchValue { value, range: _ },
                ) => PatternKind::MatchValue {
                    value: Box::new((*value, locator).into()),
                },
                rustpython_parser::ast::Pattern::MatchSingleton(
                    rustpython_parser::ast::PatternMatchSingleton { value, range: _ },
                ) => PatternKind::MatchSingleton { value },
                rustpython_parser::ast::Pattern::MatchSequence(
                    rustpython_parser::ast::PatternMatchSequence { patterns, range: _ },
                ) => PatternKind::MatchSequence {
                    patterns: patterns
                        .into_iter()
                        .map(|pattern| (pattern, locator).into())
                        .collect(),
                },
                rustpython_parser::ast::Pattern::MatchMapping(
                    rustpython_parser::ast::PatternMatchMapping {
                        keys,
                        patterns,
                        rest,
                        range: _,
                    },
                ) => PatternKind::MatchMapping {
                    keys: keys.into_iter().map(|key| (key, locator).into()).collect(),
                    patterns: patterns
                        .into_iter()
                        .map(|pattern| (pattern, locator).into())
                        .collect(),
                    rest: rest.map(Into::into),
                },
                rustpython_parser::ast::Pattern::MatchClass(
                    rustpython_parser::ast::PatternMatchClass {
                        cls,
                        patterns,
                        kwd_attrs,
                        kwd_patterns,
                        range: _,
                    },
                ) => PatternKind::MatchClass {
                    cls: Box::new((*cls, locator).into()),
                    patterns: patterns
                        .into_iter()
                        .map(|pattern| (pattern, locator).into())
                        .collect(),
                    kwd_attrs: kwd_attrs.into_iter().map(Into::into).collect(),
                    kwd_patterns: kwd_patterns
                        .into_iter()
                        .map(|pattern| (pattern, locator).into())
                        .collect(),
                },
                rustpython_parser::ast::Pattern::MatchStar(
                    rustpython_parser::ast::PatternMatchStar { name, range: _ },
                ) => PatternKind::MatchStar {
                    name: name.map(Into::into),
                },
                rustpython_parser::ast::Pattern::MatchAs(
                    rustpython_parser::ast::PatternMatchAs {
                        pattern,
                        name,
                        range: _,
                    },
                ) => PatternKind::MatchAs {
                    pattern: pattern.map(|pattern| Box::new((*pattern, locator).into())),
                    name: name.map(Into::into),
                },
                rustpython_parser::ast::Pattern::MatchOr(
                    rustpython_parser::ast::PatternMatchOr { patterns, range: _ },
                ) => PatternKind::MatchOr {
                    patterns: patterns
                        .into_iter()
                        .map(|pattern| (pattern, locator).into())
                        .collect(),
                },
            },
            trivia: vec![],
            parentheses: Parenthesize::Never,
        }
    }
}

impl From<(rustpython_parser::ast::MatchCase, &Locator<'_>)> for MatchCase {
    fn from((match_case, locator): (rustpython_parser::ast::MatchCase, &Locator)) -> Self {
        // Find the start and end of the `body`.
        let body = {
            let body_range = expand_indented_block(
                match_case.pattern.start(),
                match_case.body.last().unwrap().end(),
                locator,
            );
            Body {
                range: body_range,
                node: match_case
                    .body
                    .into_iter()
                    .map(|node| (node, locator).into())
                    .collect(),
                trivia: vec![],
                parentheses: Parenthesize::Never,
            }
        };

        MatchCase {
            pattern: (match_case.pattern, locator).into(),
            guard: match_case
                .guard
                .map(|guard| Box::new((*guard, locator).into())),
            body,
        }
    }
}

impl From<(rustpython_parser::ast::Stmt, &Locator<'_>)> for Stmt {
    fn from((stmt, locator): (rustpython_parser::ast::Stmt, &Locator)) -> Self {
        match stmt {
            rustpython_parser::ast::Stmt::Expr(ast::StmtExpr { value, range }) => Stmt {
                range,
                node: StmtKind::Expr {
                    value: Box::new((*value, locator).into()),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::Stmt::Pass(ast::StmtPass { range }) => Stmt {
                range,
                node: StmtKind::Pass,
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::Stmt::Return(ast::StmtReturn { value, range }) => Stmt {
                range,
                node: StmtKind::Return {
                    value: value.map(|v| (*v, locator).into()),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::Stmt::Assign(ast::StmtAssign {
                targets,
                value,
                type_comment,
                range,
            }) => Stmt {
                range,
                node: StmtKind::Assign {
                    targets: targets
                        .into_iter()
                        .map(|node| (node, locator).into())
                        .collect(),
                    value: Box::new((*value, locator).into()),
                    type_comment,
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::Stmt::ClassDef(ast::StmtClassDef {
                name,
                bases,
                keywords,
                body,
                decorator_list,
                range,
            }) => {
                // Find the start and end of the `body`.
                let body = {
                    let body_range =
                        expand_indented_block(range.start(), body.last().unwrap().end(), locator);
                    Body {
                        range: body_range,
                        node: body
                            .into_iter()
                            .map(|node| (node, locator).into())
                            .collect(),
                        trivia: vec![],
                        parentheses: Parenthesize::Never,
                    }
                };

                Stmt {
                    range: TextRange::new(range.start(), body.end()),
                    node: StmtKind::ClassDef {
                        name: name.into(),
                        bases: bases
                            .into_iter()
                            .map(|node| (node, locator).into())
                            .collect(),
                        keywords: keywords
                            .into_iter()
                            .map(|node| (node, locator).into())
                            .collect(),
                        body,
                        decorator_list: decorator_list
                            .into_iter()
                            .map(|node| (node, locator).into())
                            .collect(),
                    },
                    trivia: vec![],
                    parentheses: Parenthesize::Never,
                }
            }
            rustpython_parser::ast::Stmt::If(ast::StmtIf {
                test,
                body,
                orelse,
                range,
            }) => {
                // Find the start and end of the `body`.
                let body = {
                    let body_range =
                        expand_indented_block(range.start(), body.last().unwrap().end(), locator);
                    Body {
                        range: body_range,
                        node: body
                            .into_iter()
                            .map(|node| (node, locator).into())
                            .collect(),
                        trivia: vec![],
                        parentheses: Parenthesize::Never,
                    }
                };

                if orelse.is_empty() {
                    // No `else` block.
                    Stmt {
                        range: TextRange::new(range.start(), body.end()),
                        node: StmtKind::If {
                            test: Box::new((*test, locator).into()),
                            body,
                            orelse: None,
                            is_elif: false,
                        },
                        trivia: vec![],
                        parentheses: Parenthesize::Never,
                    }
                } else {
                    if is_elif(&orelse, locator) {
                        // Find the start and end of the `elif`.
                        let mut elif: Body = (orelse, locator).into();
                        if let Attributed {
                            node: StmtKind::If { is_elif, .. },
                            ..
                        } = elif.node.first_mut().unwrap()
                        {
                            *is_elif = true;
                        };

                        Stmt {
                            range: TextRange::new(range.start(), elif.end()),
                            node: StmtKind::If {
                                test: Box::new((*test, locator).into()),
                                body,
                                orelse: Some(elif),
                                is_elif: false,
                            },
                            trivia: vec![],
                            parentheses: Parenthesize::Never,
                        }
                    } else {
                        // Find the start and end of the `else`.
                        let orelse_range = expand_indented_block(
                            body.end(),
                            orelse.last().unwrap().end(),
                            locator,
                        );
                        let orelse = Body {
                            range: orelse_range,
                            node: orelse
                                .into_iter()
                                .map(|node| (node, locator).into())
                                .collect(),
                            trivia: vec![],
                            parentheses: Parenthesize::Never,
                        };

                        Stmt {
                            range: TextRange::new(range.start(), orelse.end()),
                            node: StmtKind::If {
                                test: Box::new((*test, locator).into()),
                                body,
                                orelse: Some(orelse),
                                is_elif: false,
                            },
                            trivia: vec![],
                            parentheses: Parenthesize::Never,
                        }
                    }
                }
            }
            rustpython_parser::ast::Stmt::Assert(ast::StmtAssert { test, msg, range }) => Stmt {
                range,
                node: StmtKind::Assert {
                    test: Box::new((*test, locator).into()),
                    msg: msg.map(|node| Box::new((*node, locator).into())),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::Stmt::FunctionDef(ast::StmtFunctionDef {
                name,
                args,
                body,
                decorator_list,
                returns,
                type_comment,
                range,
            }) => {
                // Find the start and end of the `body`.
                let body = {
                    let body_range =
                        expand_indented_block(range.start(), body.last().unwrap().end(), locator);
                    Body {
                        range: body_range,
                        node: body
                            .into_iter()
                            .map(|node| (node, locator).into())
                            .collect(),
                        trivia: vec![],
                        parentheses: Parenthesize::Never,
                    }
                };

                Stmt {
                    range: TextRange::new(
                        decorator_list
                            .first()
                            .map_or(range.start(), ast::Ranged::start),
                        body.end(),
                    ),
                    node: StmtKind::FunctionDef {
                        name: name.into(),
                        args: Box::new((*args, locator).into()),
                        body,
                        decorator_list: decorator_list
                            .into_iter()
                            .map(|node| (node, locator).into())
                            .collect(),
                        returns: returns.map(|r| Box::new((*r, locator).into())),
                        type_comment,
                    },
                    trivia: vec![],
                    parentheses: Parenthesize::Never,
                }
            }
            rustpython_parser::ast::Stmt::AsyncFunctionDef(ast::StmtAsyncFunctionDef {
                name,
                args,
                body,
                decorator_list,
                returns,
                type_comment,
                range,
            }) => {
                // Find the start and end of the `body`.
                let body = {
                    let body_range =
                        expand_indented_block(range.start(), body.last().unwrap().end(), locator);
                    Body {
                        range: body_range,
                        node: body
                            .into_iter()
                            .map(|node| (node, locator).into())
                            .collect(),
                        trivia: vec![],
                        parentheses: Parenthesize::Never,
                    }
                };

                Stmt {
                    range: TextRange::new(
                        decorator_list
                            .first()
                            .map_or(range.start(), |expr| expr.range().start()),
                        body.end(),
                    ),
                    node: StmtKind::AsyncFunctionDef {
                        name: name.into(),
                        args: Box::new((*args, locator).into()),
                        body,
                        decorator_list: decorator_list
                            .into_iter()
                            .map(|node| (node, locator).into())
                            .collect(),
                        returns: returns.map(|r| Box::new((*r, locator).into())),
                        type_comment,
                    },
                    trivia: vec![],
                    parentheses: Parenthesize::Never,
                }
            }
            rustpython_parser::ast::Stmt::Delete(ast::StmtDelete { targets, range }) => Stmt {
                range,
                node: StmtKind::Delete {
                    targets: targets
                        .into_iter()
                        .map(|node| (node, locator).into())
                        .collect(),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::Stmt::AugAssign(ast::StmtAugAssign {
                target,
                op,
                value,
                range,
            }) => Stmt {
                range,
                node: StmtKind::AugAssign {
                    op: {
                        let target_tok = match &op {
                            rustpython_parser::ast::Operator::Add => {
                                rustpython_parser::Tok::PlusEqual
                            }
                            rustpython_parser::ast::Operator::Sub => {
                                rustpython_parser::Tok::MinusEqual
                            }
                            rustpython_parser::ast::Operator::Mult => {
                                rustpython_parser::Tok::StarEqual
                            }
                            rustpython_parser::ast::Operator::MatMult => {
                                rustpython_parser::Tok::AtEqual
                            }
                            rustpython_parser::ast::Operator::Div => {
                                rustpython_parser::Tok::SlashEqual
                            }
                            rustpython_parser::ast::Operator::Mod => {
                                rustpython_parser::Tok::PercentEqual
                            }
                            rustpython_parser::ast::Operator::Pow => {
                                rustpython_parser::Tok::DoubleStarEqual
                            }
                            rustpython_parser::ast::Operator::LShift => {
                                rustpython_parser::Tok::LeftShiftEqual
                            }
                            rustpython_parser::ast::Operator::RShift => {
                                rustpython_parser::Tok::RightShiftEqual
                            }
                            rustpython_parser::ast::Operator::BitOr => {
                                rustpython_parser::Tok::VbarEqual
                            }
                            rustpython_parser::ast::Operator::BitXor => {
                                rustpython_parser::Tok::CircumflexEqual
                            }
                            rustpython_parser::ast::Operator::BitAnd => {
                                rustpython_parser::Tok::AmperEqual
                            }
                            rustpython_parser::ast::Operator::FloorDiv => {
                                rustpython_parser::Tok::DoubleSlashEqual
                            }
                        };
                        let op_range =
                            find_tok(TextRange::new(target.end(), value.end()), locator, |tok| {
                                tok == target_tok
                            });
                        Operator::new(op_range, (&op).into())
                    },
                    target: Box::new((*target, locator).into()),
                    value: Box::new((*value, locator).into()),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::Stmt::AnnAssign(ast::StmtAnnAssign {
                target,
                annotation,
                value,
                simple,
                range,
            }) => Stmt {
                range,
                node: StmtKind::AnnAssign {
                    target: Box::new((*target, locator).into()),
                    annotation: Box::new((*annotation, locator).into()),
                    value: value.map(|node| Box::new((*node, locator).into())),
                    simple: usize::from(simple),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::Stmt::For(ast::StmtFor {
                target,
                iter,
                body,
                orelse,
                type_comment,
                range,
            }) => {
                // Find the start and end of the `body`.
                let body = {
                    let body_range =
                        expand_indented_block(range.start(), body.last().unwrap().end(), locator);
                    Body {
                        range: body_range,
                        node: body
                            .into_iter()
                            .map(|node| (node, locator).into())
                            .collect(),
                        trivia: vec![],
                        parentheses: Parenthesize::Never,
                    }
                };

                // Find the start and end of the `orelse`.
                let orelse = (!orelse.is_empty()).then(|| {
                    let orelse_range =
                        expand_indented_block(body.end(), orelse.last().unwrap().end(), locator);
                    Body {
                        range: orelse_range,
                        node: orelse
                            .into_iter()
                            .map(|node| (node, locator).into())
                            .collect(),
                        trivia: vec![],
                        parentheses: Parenthesize::Never,
                    }
                });

                Stmt {
                    range: TextRange::new(range.start(), orelse.as_ref().unwrap_or(&body).end()),
                    node: StmtKind::For {
                        target: Box::new((*target, locator).into()),
                        iter: Box::new((*iter, locator).into()),
                        body,
                        orelse,
                        type_comment,
                    },
                    trivia: vec![],
                    parentheses: Parenthesize::Never,
                }
            }
            rustpython_parser::ast::Stmt::AsyncFor(ast::StmtAsyncFor {
                target,
                iter,
                body,
                orelse,
                type_comment,
                range,
            }) => {
                // Find the start and end of the `body`.
                let body = {
                    let body_range =
                        expand_indented_block(range.start(), body.last().unwrap().end(), locator);
                    Body {
                        range: body_range,
                        node: body
                            .into_iter()
                            .map(|node| (node, locator).into())
                            .collect(),
                        trivia: vec![],
                        parentheses: Parenthesize::Never,
                    }
                };

                // Find the start and end of the `orelse`.
                let orelse = (!orelse.is_empty()).then(|| {
                    let orelse_range =
                        expand_indented_block(body.end(), orelse.last().unwrap().end(), locator);
                    Body {
                        range: orelse_range,
                        node: orelse
                            .into_iter()
                            .map(|node| (node, locator).into())
                            .collect(),
                        trivia: vec![],
                        parentheses: Parenthesize::Never,
                    }
                });

                Stmt {
                    range: TextRange::new(range.start(), orelse.as_ref().unwrap_or(&body).end()),
                    node: StmtKind::AsyncFor {
                        target: Box::new((*target, locator).into()),
                        iter: Box::new((*iter, locator).into()),
                        body,
                        orelse,
                        type_comment,
                    },
                    trivia: vec![],
                    parentheses: Parenthesize::Never,
                }
            }
            rustpython_parser::ast::Stmt::While(ast::StmtWhile {
                test,
                body,
                orelse,
                range,
            }) => {
                // Find the start and end of the `body`.
                let body = {
                    let body_range =
                        expand_indented_block(range.start(), body.last().unwrap().end(), locator);
                    Body {
                        range: body_range,
                        node: body
                            .into_iter()
                            .map(|node| (node, locator).into())
                            .collect(),
                        trivia: vec![],
                        parentheses: Parenthesize::Never,
                    }
                };

                // Find the start and end of the `orelse`.
                let orelse = (!orelse.is_empty()).then(|| {
                    let orelse_range =
                        expand_indented_block(body.end(), orelse.last().unwrap().end(), locator);
                    Body {
                        range: orelse_range,
                        node: orelse
                            .into_iter()
                            .map(|node| (node, locator).into())
                            .collect(),
                        trivia: vec![],
                        parentheses: Parenthesize::Never,
                    }
                });

                Stmt {
                    range: TextRange::new(range.start(), orelse.as_ref().unwrap_or(&body).end()),
                    node: StmtKind::While {
                        test: Box::new((*test, locator).into()),
                        body,
                        orelse,
                    },
                    trivia: vec![],
                    parentheses: Parenthesize::Never,
                }
            }
            rustpython_parser::ast::Stmt::With(ast::StmtWith {
                items,
                body,
                type_comment,
                range,
            }) => {
                // Find the start and end of the `body`.
                let body = {
                    let body_range =
                        expand_indented_block(range.start(), body.last().unwrap().end(), locator);
                    Body {
                        range: body_range,
                        node: body
                            .into_iter()
                            .map(|node| (node, locator).into())
                            .collect(),
                        trivia: vec![],
                        parentheses: Parenthesize::Never,
                    }
                };

                Stmt {
                    range: TextRange::new(range.start(), body.end()),
                    node: StmtKind::With {
                        items: items
                            .into_iter()
                            .map(|node| (node, locator).into())
                            .collect(),
                        body,
                        type_comment,
                    },
                    trivia: vec![],
                    parentheses: Parenthesize::Never,
                }
            }
            rustpython_parser::ast::Stmt::AsyncWith(ast::StmtAsyncWith {
                items,
                body,
                type_comment,
                range,
            }) => {
                // Find the start and end of the `body`.
                let body = {
                    let body_range =
                        expand_indented_block(range.start(), body.last().unwrap().end(), locator);
                    Body {
                        range: body_range,
                        node: body
                            .into_iter()
                            .map(|node| (node, locator).into())
                            .collect(),
                        trivia: vec![],
                        parentheses: Parenthesize::Never,
                    }
                };

                Stmt {
                    range: TextRange::new(range.start(), body.end()),
                    node: StmtKind::AsyncWith {
                        items: items
                            .into_iter()
                            .map(|node| (node, locator).into())
                            .collect(),
                        body,
                        type_comment,
                    },
                    trivia: vec![],
                    parentheses: Parenthesize::Never,
                }
            }
            rustpython_parser::ast::Stmt::Match(ast::StmtMatch {
                subject,
                cases,
                range,
            }) => Stmt {
                range,
                node: StmtKind::Match {
                    subject: Box::new((*subject, locator).into()),
                    cases: cases
                        .into_iter()
                        .map(|node| (node, locator).into())
                        .collect(),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::Stmt::Raise(ast::StmtRaise { exc, cause, range }) => Stmt {
                range,
                node: StmtKind::Raise {
                    exc: exc.map(|exc| Box::new((*exc, locator).into())),
                    cause: cause.map(|cause| Box::new((*cause, locator).into())),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::Stmt::Try(ast::StmtTry {
                body,
                handlers,
                orelse,
                finalbody,
                range,
            }) => {
                // Find the start and end of the `body`.
                let body = {
                    let body_range =
                        expand_indented_block(range.start(), body.last().unwrap().end(), locator);
                    Body {
                        range: body_range,
                        node: body
                            .into_iter()
                            .map(|node| (node, locator).into())
                            .collect(),
                        trivia: vec![],
                        parentheses: Parenthesize::Never,
                    }
                };

                let handlers: Vec<Excepthandler> = handlers
                    .into_iter()
                    .map(|node| (node, locator).into())
                    .collect();

                // Find the start and end of the `orelse`.
                let orelse = (!orelse.is_empty()).then(|| {
                    let orelse_range = expand_indented_block(
                        handlers.last().map_or(body.end(), Attributed::end),
                        orelse.last().unwrap().end(),
                        locator,
                    );
                    Body {
                        range: orelse_range,
                        node: orelse
                            .into_iter()
                            .map(|node| (node, locator).into())
                            .collect(),
                        trivia: vec![],
                        parentheses: Parenthesize::Never,
                    }
                });

                // Find the start and end of the `finalbody`.
                let finalbody = (!finalbody.is_empty()).then(|| {
                    let finalbody_range = expand_indented_block(
                        orelse.as_ref().map_or(
                            handlers.last().map_or(body.end(), Attributed::end),
                            Attributed::end,
                        ),
                        finalbody.last().unwrap().end(),
                        locator,
                    );
                    Body {
                        range: finalbody_range,
                        node: finalbody
                            .into_iter()
                            .map(|node| (node, locator).into())
                            .collect(),
                        trivia: vec![],
                        parentheses: Parenthesize::Never,
                    }
                });

                let end_location = finalbody.as_ref().map_or(
                    orelse.as_ref().map_or(
                        handlers.last().map_or(body.end(), Attributed::end),
                        Attributed::end,
                    ),
                    Attributed::end,
                );

                Stmt {
                    range: TextRange::new(range.start(), end_location),
                    node: StmtKind::Try {
                        body,
                        handlers,
                        orelse,
                        finalbody,
                    },
                    trivia: vec![],
                    parentheses: Parenthesize::Never,
                }
            }
            rustpython_parser::ast::Stmt::TryStar(ast::StmtTryStar {
                body,
                handlers,
                orelse,
                finalbody,
                range,
            }) => {
                // Find the start and end of the `body`.
                let body = {
                    let body_range =
                        expand_indented_block(range.start(), body.last().unwrap().end(), locator);
                    Body {
                        range: body_range,
                        node: body
                            .into_iter()
                            .map(|node| (node, locator).into())
                            .collect(),
                        trivia: vec![],
                        parentheses: Parenthesize::Never,
                    }
                };

                let handlers: Vec<Excepthandler> = handlers
                    .into_iter()
                    .map(|node| (node, locator).into())
                    .collect();

                // Find the start and end of the `orelse`.
                let orelse = (!orelse.is_empty()).then(|| {
                    let orelse_range = expand_indented_block(
                        handlers.last().map_or(body.end(), Attributed::end),
                        orelse.last().unwrap().end(),
                        locator,
                    );
                    Body {
                        range: orelse_range,
                        node: orelse
                            .into_iter()
                            .map(|node| (node, locator).into())
                            .collect(),
                        trivia: vec![],
                        parentheses: Parenthesize::Never,
                    }
                });

                // Find the start and end of the `finalbody`.
                let finalbody = (!finalbody.is_empty()).then(|| {
                    let finalbody_range = expand_indented_block(
                        orelse.as_ref().map_or(
                            handlers.last().map_or(body.end(), Attributed::end),
                            Attributed::end,
                        ),
                        finalbody.last().unwrap().end(),
                        locator,
                    );
                    Body {
                        range: finalbody_range,
                        node: finalbody
                            .into_iter()
                            .map(|node| (node, locator).into())
                            .collect(),
                        trivia: vec![],
                        parentheses: Parenthesize::Never,
                    }
                });

                let end_location = finalbody.as_ref().map_or(
                    orelse.as_ref().map_or(
                        handlers.last().map_or(body.end(), Attributed::end),
                        Attributed::end,
                    ),
                    Attributed::end,
                );

                Stmt {
                    range: TextRange::new(range.start(), end_location),
                    node: StmtKind::TryStar {
                        body,
                        handlers,
                        orelse,
                        finalbody,
                    },
                    trivia: vec![],
                    parentheses: Parenthesize::Never,
                }
            }
            rustpython_parser::ast::Stmt::Import(ast::StmtImport { names, range }) => Stmt {
                range,
                node: StmtKind::Import {
                    names: names
                        .into_iter()
                        .map(|node| (node, locator).into())
                        .collect(),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::Stmt::ImportFrom(ast::StmtImportFrom {
                module,
                names,
                level,
                range,
            }) => Stmt {
                range,
                node: StmtKind::ImportFrom {
                    module: module.map(Into::into),
                    names: names
                        .into_iter()
                        .map(|node| (node, locator).into())
                        .collect(),
                    level: level.map(|level| level.to_u32()),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::Stmt::Global(ast::StmtGlobal { names, range }) => Stmt {
                range,
                node: StmtKind::Global {
                    names: names.into_iter().map(Into::into).collect(),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::Stmt::Nonlocal(ast::StmtNonlocal { names, range }) => Stmt {
                range,
                node: StmtKind::Nonlocal {
                    names: names.into_iter().map(Into::into).collect(),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::Stmt::Break(ast::StmtBreak { range }) => Stmt {
                range,
                node: StmtKind::Break,
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::Stmt::Continue(ast::StmtContinue { range }) => Stmt {
                range,
                node: StmtKind::Continue,
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
        }
    }
}

impl From<(rustpython_parser::ast::Keyword, &Locator<'_>)> for Keyword {
    fn from((keyword, locator): (rustpython_parser::ast::Keyword, &Locator)) -> Self {
        Keyword {
            range: keyword.range(),
            node: KeywordData {
                arg: keyword.arg.map(Into::into),
                value: (keyword.value, locator).into(),
            },
            trivia: vec![],
            parentheses: Parenthesize::Never,
        }
    }
}

impl From<(rustpython_parser::ast::Arg, &Locator<'_>)> for Arg {
    fn from((arg, locator): (rustpython_parser::ast::Arg, &Locator)) -> Self {
        Arg {
            range: arg.range(),
            node: ArgData {
                arg: arg.arg.into(),
                annotation: arg.annotation.map(|node| Box::new((*node, locator).into())),
                type_comment: arg.type_comment,
            },
            trivia: vec![],
            parentheses: Parenthesize::Never,
        }
    }
}

impl From<(rustpython_parser::ast::Arguments, &Locator<'_>)> for Arguments {
    fn from((arguments, locator): (rustpython_parser::ast::Arguments, &Locator)) -> Self {
        Arguments {
            posonlyargs: arguments
                .posonlyargs
                .into_iter()
                .map(|node| (node, locator).into())
                .collect(),
            args: arguments
                .args
                .into_iter()
                .map(|node| (node, locator).into())
                .collect(),
            vararg: arguments
                .vararg
                .map(|node| Box::new((*node, locator).into())),
            kwonlyargs: arguments
                .kwonlyargs
                .into_iter()
                .map(|node| (node, locator).into())
                .collect(),
            kw_defaults: arguments
                .kw_defaults
                .into_iter()
                .map(|node| (node, locator).into())
                .collect(),
            kwarg: arguments
                .kwarg
                .map(|node| Box::new((*node, locator).into())),
            defaults: arguments
                .defaults
                .into_iter()
                .map(|node| (node, locator).into())
                .collect(),
        }
    }
}

impl From<(rustpython_parser::ast::Comprehension, &Locator<'_>)> for Comprehension {
    fn from((comprehension, locator): (rustpython_parser::ast::Comprehension, &Locator)) -> Self {
        Comprehension {
            target: (comprehension.target, locator).into(),
            iter: (comprehension.iter, locator).into(),
            ifs: comprehension
                .ifs
                .into_iter()
                .map(|node| (node, locator).into())
                .collect(),
            is_async: usize::from(comprehension.is_async),
        }
    }
}

impl From<(rustpython_parser::ast::Expr, &Locator<'_>)> for Expr {
    fn from((expr, locator): (rustpython_parser::ast::Expr, &Locator)) -> Self {
        match expr {
            rustpython_parser::ast::Expr::Name(ast::ExprName { id, ctx, range }) => Expr {
                range,
                node: ExprKind::Name {
                    id: id.into(),
                    ctx: ctx.into(),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::Expr::BoolOp(ast::ExprBoolOp { op, values, range }) => Expr {
                range,
                node: ExprKind::BoolOp {
                    ops: values
                        .iter()
                        .tuple_windows()
                        .map(|(left, right)| {
                            let target_tok = match &op {
                                rustpython_parser::ast::Boolop::And => rustpython_parser::Tok::And,
                                rustpython_parser::ast::Boolop::Or => rustpython_parser::Tok::Or,
                            };
                            let op_range = find_tok(
                                TextRange::new(left.end(), right.start()),
                                locator,
                                |tok| tok == target_tok,
                            );
                            BoolOp::new(op_range, (&op).into())
                        })
                        .collect(),
                    values: values
                        .into_iter()
                        .map(|node| (node, locator).into())
                        .collect(),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::Expr::NamedExpr(ast::ExprNamedExpr {
                target,
                value,
                range,
            }) => Expr {
                range,
                node: ExprKind::NamedExpr {
                    target: Box::new((*target, locator).into()),
                    value: Box::new((*value, locator).into()),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::Expr::BinOp(ast::ExprBinOp {
                left,
                op,
                right,
                range,
            }) => Expr {
                range,
                node: ExprKind::BinOp {
                    op: {
                        let target_tok = match &op {
                            rustpython_parser::ast::Operator::Add => rustpython_parser::Tok::Plus,
                            rustpython_parser::ast::Operator::Sub => rustpython_parser::Tok::Minus,
                            rustpython_parser::ast::Operator::Mult => rustpython_parser::Tok::Star,
                            rustpython_parser::ast::Operator::MatMult => rustpython_parser::Tok::At,
                            rustpython_parser::ast::Operator::Div => rustpython_parser::Tok::Slash,
                            rustpython_parser::ast::Operator::Mod => {
                                rustpython_parser::Tok::Percent
                            }
                            rustpython_parser::ast::Operator::Pow => {
                                rustpython_parser::Tok::DoubleStar
                            }
                            rustpython_parser::ast::Operator::LShift => {
                                rustpython_parser::Tok::LeftShift
                            }
                            rustpython_parser::ast::Operator::RShift => {
                                rustpython_parser::Tok::RightShift
                            }
                            rustpython_parser::ast::Operator::BitOr => rustpython_parser::Tok::Vbar,
                            rustpython_parser::ast::Operator::BitXor => {
                                rustpython_parser::Tok::CircumFlex
                            }
                            rustpython_parser::ast::Operator::BitAnd => {
                                rustpython_parser::Tok::Amper
                            }
                            rustpython_parser::ast::Operator::FloorDiv => {
                                rustpython_parser::Tok::DoubleSlash
                            }
                        };
                        let op_range =
                            find_tok(TextRange::new(left.end(), right.start()), locator, |tok| {
                                tok == target_tok
                            });
                        Operator::new(op_range, (&op).into())
                    },
                    left: Box::new((*left, locator).into()),
                    right: Box::new((*right, locator).into()),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::Expr::UnaryOp(ast::ExprUnaryOp { op, operand, range }) => {
                Expr {
                    range,
                    node: ExprKind::UnaryOp {
                        op: {
                            let target_tok = match &op {
                                rustpython_parser::ast::Unaryop::Invert => {
                                    rustpython_parser::Tok::Tilde
                                }
                                rustpython_parser::ast::Unaryop::Not => rustpython_parser::Tok::Not,
                                rustpython_parser::ast::Unaryop::UAdd => {
                                    rustpython_parser::Tok::Plus
                                }
                                rustpython_parser::ast::Unaryop::USub => {
                                    rustpython_parser::Tok::Minus
                                }
                            };
                            let op_range = find_tok(
                                TextRange::new(range.start(), operand.start()),
                                locator,
                                |tok| tok == target_tok,
                            );
                            UnaryOp::new(op_range, (&op).into())
                        },
                        operand: Box::new((*operand, locator).into()),
                    },
                    trivia: vec![],
                    parentheses: Parenthesize::Never,
                }
            }
            rustpython_parser::ast::Expr::Lambda(ast::ExprLambda { args, body, range }) => Expr {
                range,
                node: ExprKind::Lambda {
                    args: Box::new((*args, locator).into()),
                    body: Box::new((*body, locator).into()),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::Expr::IfExp(ast::ExprIfExp {
                test,
                body,
                orelse,
                range,
            }) => Expr {
                range,
                node: ExprKind::IfExp {
                    test: Box::new((*test, locator).into()),
                    body: Box::new((*body, locator).into()),
                    orelse: Box::new((*orelse, locator).into()),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::Expr::Dict(ast::ExprDict {
                keys,
                values,
                range,
            }) => Expr {
                range,
                node: ExprKind::Dict {
                    keys: keys
                        .into_iter()
                        .map(|key| key.map(|node| (node, locator).into()))
                        .collect(),
                    values: values
                        .into_iter()
                        .map(|node| (node, locator).into())
                        .collect(),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::Expr::Set(ast::ExprSet { elts, range }) => Expr {
                range,
                node: ExprKind::Set {
                    elts: elts
                        .into_iter()
                        .map(|node| (node, locator).into())
                        .collect(),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::Expr::ListComp(ast::ExprListComp {
                elt,
                generators,
                range,
            }) => Expr {
                range,
                node: ExprKind::ListComp {
                    elt: Box::new((*elt, locator).into()),
                    generators: generators
                        .into_iter()
                        .map(|node| (node, locator).into())
                        .collect(),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::Expr::SetComp(ast::ExprSetComp {
                elt,
                generators,
                range,
            }) => Expr {
                range,
                node: ExprKind::SetComp {
                    elt: Box::new((*elt, locator).into()),
                    generators: generators
                        .into_iter()
                        .map(|node| (node, locator).into())
                        .collect(),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::Expr::DictComp(ast::ExprDictComp {
                key,
                value,
                generators,
                range,
            }) => Expr {
                range,
                node: ExprKind::DictComp {
                    key: Box::new((*key, locator).into()),
                    value: Box::new((*value, locator).into()),
                    generators: generators
                        .into_iter()
                        .map(|node| (node, locator).into())
                        .collect(),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::Expr::GeneratorExp(ast::ExprGeneratorExp {
                elt,
                generators,
                range,
            }) => Expr {
                range,
                node: ExprKind::GeneratorExp {
                    elt: Box::new((*elt, locator).into()),
                    generators: generators
                        .into_iter()
                        .map(|node| (node, locator).into())
                        .collect(),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::Expr::Await(ast::ExprAwait { value, range }) => Expr {
                range,
                node: ExprKind::Await {
                    value: Box::new((*value, locator).into()),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::Expr::Yield(ast::ExprYield { value, range }) => Expr {
                range,
                node: ExprKind::Yield {
                    value: value.map(|v| Box::new((*v, locator).into())),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::Expr::YieldFrom(ast::ExprYieldFrom { value, range }) => Expr {
                range,
                node: ExprKind::YieldFrom {
                    value: Box::new((*value, locator).into()),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::Expr::Compare(ast::ExprCompare {
                left,
                ops,
                comparators,
                range,
            }) => Expr {
                range,
                node: ExprKind::Compare {
                    ops: iter::once(left.as_ref())
                        .chain(comparators.iter())
                        .tuple_windows()
                        .zip(ops.into_iter())
                        .map(|((left, right), op)| {
                            let target_tok = match &op {
                                rustpython_parser::ast::Cmpop::Eq => {
                                    rustpython_parser::Tok::EqEqual
                                }
                                rustpython_parser::ast::Cmpop::NotEq => {
                                    rustpython_parser::Tok::NotEqual
                                }
                                rustpython_parser::ast::Cmpop::Lt => rustpython_parser::Tok::Less,
                                rustpython_parser::ast::Cmpop::LtE => {
                                    rustpython_parser::Tok::LessEqual
                                }
                                rustpython_parser::ast::Cmpop::Gt => {
                                    rustpython_parser::Tok::Greater
                                }
                                rustpython_parser::ast::Cmpop::GtE => {
                                    rustpython_parser::Tok::GreaterEqual
                                }
                                rustpython_parser::ast::Cmpop::Is => rustpython_parser::Tok::Is,
                                // TODO(charlie): Break this into two tokens.
                                rustpython_parser::ast::Cmpop::IsNot => rustpython_parser::Tok::Is,
                                rustpython_parser::ast::Cmpop::In => rustpython_parser::Tok::In,
                                // TODO(charlie): Break this into two tokens.
                                rustpython_parser::ast::Cmpop::NotIn => rustpython_parser::Tok::In,
                            };
                            let op_range = find_tok(
                                TextRange::new(left.end(), right.start()),
                                locator,
                                |tok| tok == target_tok,
                            );
                            CmpOp::new(op_range, (&op).into())
                        })
                        .collect(),
                    left: Box::new((*left, locator).into()),
                    comparators: comparators
                        .into_iter()
                        .map(|node| (node, locator).into())
                        .collect(),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::Expr::Call(ast::ExprCall {
                func,
                args,
                keywords,
                range,
            }) => Expr {
                range,
                node: ExprKind::Call {
                    func: Box::new((*func, locator).into()),
                    args: args
                        .into_iter()
                        .map(|node| (node, locator).into())
                        .collect(),
                    keywords: keywords
                        .into_iter()
                        .map(|node| (node, locator).into())
                        .collect(),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::Expr::FormattedValue(ast::ExprFormattedValue {
                value,
                conversion,
                format_spec,
                range,
            }) => Expr {
                range,
                node: ExprKind::FormattedValue {
                    value: Box::new((*value, locator).into()),
                    conversion,
                    format_spec: format_spec.map(|f| Box::new((*f, locator).into())),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::Expr::JoinedStr(ast::ExprJoinedStr { values, range }) => Expr {
                range,
                node: ExprKind::JoinedStr {
                    values: values
                        .into_iter()
                        .map(|node| (node, locator).into())
                        .collect(),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::Expr::Constant(ast::ExprConstant { value, kind, range }) => {
                Expr {
                    range,
                    node: ExprKind::Constant { value, kind },
                    trivia: vec![],
                    parentheses: Parenthesize::Never,
                }
            }
            rustpython_parser::ast::Expr::Attribute(ast::ExprAttribute {
                value,
                attr,
                ctx,
                range,
            }) => Expr {
                range,
                node: ExprKind::Attribute {
                    value: Box::new((*value, locator).into()),
                    attr: attr.into(),
                    ctx: ctx.into(),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::Expr::Subscript(ast::ExprSubscript {
                value,
                slice,
                ctx,
                range,
            }) => Expr {
                range,
                node: ExprKind::Subscript {
                    value: Box::new((*value, locator).into()),
                    slice: Box::new((*slice, locator).into()),
                    ctx: ctx.into(),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::Expr::Starred(ast::ExprStarred { value, ctx, range }) => Expr {
                range,
                node: ExprKind::Starred {
                    value: Box::new((*value, locator).into()),
                    ctx: ctx.into(),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::Expr::List(ast::ExprList { elts, ctx, range }) => Expr {
                range,
                node: ExprKind::List {
                    elts: elts
                        .into_iter()
                        .map(|node| (node, locator).into())
                        .collect(),
                    ctx: ctx.into(),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::Expr::Tuple(ast::ExprTuple { elts, ctx, range }) => Expr {
                range,
                node: ExprKind::Tuple {
                    elts: elts
                        .into_iter()
                        .map(|node| (node, locator).into())
                        .collect(),
                    ctx: ctx.into(),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::Expr::Slice(ast::ExprSlice {
                lower,
                upper,
                step,
                range: expr_range,
            }) => {
                // Locate the colon tokens, which indicate the number of index segments.
                let tokens = rustpython_parser::lexer::lex_starts_at(
                    &locator.contents()[expr_range],
                    Mode::Module,
                    expr_range.start(),
                );

                // Find the first and (if it exists) second colon in the slice, avoiding any
                // semicolons within nested slices, and any lambda expressions.
                let mut first_colon = None;
                let mut second_colon = None;
                let mut lambda = 0;
                let mut nesting = 0;
                for (tok, range) in tokens.flatten() {
                    match tok {
                        rustpython_parser::Tok::Lambda if nesting == 0 => lambda += 1,
                        rustpython_parser::Tok::Colon if nesting == 0 => {
                            if lambda > 0 {
                                lambda -= 1;
                            } else {
                                if first_colon.is_none() {
                                    first_colon = Some(range.start());
                                } else {
                                    second_colon = Some(range.start());
                                    break;
                                }
                            }
                        }
                        rustpython_parser::Tok::Lpar
                        | rustpython_parser::Tok::Lsqb
                        | rustpython_parser::Tok::Lbrace => nesting += 1,
                        rustpython_parser::Tok::Rpar
                        | rustpython_parser::Tok::Rsqb
                        | rustpython_parser::Tok::Rbrace => nesting -= 1,
                        _ => {}
                    }
                }

                let lower = SliceIndex::new(
                    TextRange::new(expr_range.start(), first_colon.unwrap()),
                    lower.map_or(SliceIndexKind::Empty, |node| SliceIndexKind::Index {
                        value: Box::new((*node, locator).into()),
                    }),
                );
                let upper = SliceIndex::new(
                    TextRange::new(
                        first_colon.unwrap(),
                        second_colon.unwrap_or(expr_range.end()),
                    ),
                    upper.map_or(SliceIndexKind::Empty, |node| SliceIndexKind::Index {
                        value: Box::new((*node, locator).into()),
                    }),
                );
                let step = second_colon.map(|second_colon| {
                    SliceIndex::new(
                        TextRange::new(second_colon, expr_range.end()),
                        step.map_or(SliceIndexKind::Empty, |node| SliceIndexKind::Index {
                            value: Box::new((*node, locator).into()),
                        }),
                    )
                });

                Expr {
                    range: expr_range,
                    node: ExprKind::Slice { lower, upper, step },
                    trivia: vec![],
                    parentheses: Parenthesize::Never,
                }
            }
        }
    }
}
