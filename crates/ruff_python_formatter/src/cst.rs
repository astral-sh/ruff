#![allow(clippy::derive_partial_eq_without_eq)]

use std::iter;

use rustpython_parser::ast::{Constant, Location};
use rustpython_parser::Mode;

use itertools::Itertools;

use crate::core::helpers::{expand_indented_block, find_tok, is_elif};
use crate::core::locator::Locator;
use crate::core::types::Range;
use crate::trivia::{Parenthesize, Trivia};

type Ident = String;

#[derive(Clone, Debug, PartialEq)]
pub struct Located<T> {
    pub location: Location,
    pub end_location: Option<Location>,
    pub node: T,
    pub trivia: Vec<Trivia>,
    pub parentheses: Parenthesize,
}

impl<T> Located<T> {
    pub fn new(location: Location, end_location: Location, node: T) -> Self {
        Self {
            location,
            end_location: Some(end_location),
            node,
            trivia: Vec::new(),
            parentheses: Parenthesize::Never,
        }
    }

    pub fn add_trivia(&mut self, trivia: Trivia) {
        self.trivia.push(trivia);
    }

    pub fn id(&self) -> usize {
        self as *const _ as usize
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

pub type BoolOp = Located<BoolOpKind>;

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

pub type Operator = Located<OperatorKind>;

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

pub type UnaryOp = Located<UnaryOpKind>;

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

pub type CmpOp = Located<CmpOpKind>;

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

pub type Body = Located<Vec<Stmt>>;

impl From<(Vec<rustpython_parser::ast::Stmt>, &Locator<'_>)> for Body {
    fn from((body, locator): (Vec<rustpython_parser::ast::Stmt>, &Locator)) -> Self {
        Body {
            location: body.first().unwrap().location,
            end_location: body.last().unwrap().end_location,
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
        level: Option<usize>,
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

pub type Stmt = Located<StmtKind>;

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
        conversion: usize,
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

pub type Expr = Located<ExprKind>;

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

pub type Excepthandler = Located<ExcepthandlerKind>;

#[derive(Clone, Debug, PartialEq)]
pub enum SliceIndexKind {
    /// The index slot exists, but is empty.
    Empty,
    /// The index slot contains an expression.
    Index { value: Box<Expr> },
}

pub type SliceIndex = Located<SliceIndexKind>;

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

pub type Arg = Located<ArgData>;

#[derive(Clone, Debug, PartialEq)]
pub struct KeywordData {
    pub arg: Option<Ident>,
    pub value: Expr,
}

pub type Keyword = Located<KeywordData>;

#[derive(Clone, Debug, PartialEq)]
pub struct AliasData {
    pub name: Ident,
    pub asname: Option<Ident>,
}

pub type Alias = Located<AliasData>;

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

pub type Pattern = Located<PatternKind>;

impl From<(rustpython_parser::ast::Alias, &Locator<'_>)> for Alias {
    fn from((alias, _locator): (rustpython_parser::ast::Alias, &Locator)) -> Self {
        Alias {
            location: alias.location,
            end_location: alias.end_location,
            node: AliasData {
                name: alias.node.name,
                asname: alias.node.asname,
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
        let rustpython_parser::ast::ExcepthandlerKind::ExceptHandler { type_, name, body } =
            excepthandler.node;

        // Find the start and end of the `body`.
        let body = {
            let (body_location, body_end_location) = expand_indented_block(
                excepthandler.location,
                body.last().unwrap().end_location.unwrap(),
                locator,
            );
            Body {
                location: body_location,
                end_location: Some(body_end_location),
                node: body
                    .into_iter()
                    .map(|node| (node, locator).into())
                    .collect(),
                trivia: vec![],
                parentheses: Parenthesize::Never,
            }
        };

        Excepthandler {
            location: excepthandler.location,
            end_location: body.end_location,
            node: ExcepthandlerKind::ExceptHandler {
                type_: type_.map(|type_| Box::new((*type_, locator).into())),
                name,
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
            location: pattern.location,
            end_location: pattern.end_location,
            node: match pattern.node {
                rustpython_parser::ast::PatternKind::MatchValue { value } => {
                    PatternKind::MatchValue {
                        value: Box::new((*value, locator).into()),
                    }
                }
                rustpython_parser::ast::PatternKind::MatchSingleton { value } => {
                    PatternKind::MatchSingleton { value }
                }
                rustpython_parser::ast::PatternKind::MatchSequence { patterns } => {
                    PatternKind::MatchSequence {
                        patterns: patterns
                            .into_iter()
                            .map(|pattern| (pattern, locator).into())
                            .collect(),
                    }
                }
                rustpython_parser::ast::PatternKind::MatchMapping {
                    keys,
                    patterns,
                    rest,
                } => PatternKind::MatchMapping {
                    keys: keys.into_iter().map(|key| (key, locator).into()).collect(),
                    patterns: patterns
                        .into_iter()
                        .map(|pattern| (pattern, locator).into())
                        .collect(),
                    rest,
                },
                rustpython_parser::ast::PatternKind::MatchClass {
                    cls,
                    patterns,
                    kwd_attrs,
                    kwd_patterns,
                } => PatternKind::MatchClass {
                    cls: Box::new((*cls, locator).into()),
                    patterns: patterns
                        .into_iter()
                        .map(|pattern| (pattern, locator).into())
                        .collect(),
                    kwd_attrs,
                    kwd_patterns: kwd_patterns
                        .into_iter()
                        .map(|pattern| (pattern, locator).into())
                        .collect(),
                },
                rustpython_parser::ast::PatternKind::MatchStar { name } => {
                    PatternKind::MatchStar { name }
                }
                rustpython_parser::ast::PatternKind::MatchAs { pattern, name } => {
                    PatternKind::MatchAs {
                        pattern: pattern.map(|pattern| Box::new((*pattern, locator).into())),
                        name,
                    }
                }
                rustpython_parser::ast::PatternKind::MatchOr { patterns } => PatternKind::MatchOr {
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
            let (body_location, body_end_location) = expand_indented_block(
                match_case.pattern.location,
                match_case.body.last().unwrap().end_location.unwrap(),
                locator,
            );
            Body {
                location: body_location,
                end_location: Some(body_end_location),
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
        match stmt.node {
            rustpython_parser::ast::StmtKind::Expr { value } => Stmt {
                location: stmt.location,
                end_location: stmt.end_location,
                node: StmtKind::Expr {
                    value: Box::new((*value, locator).into()),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::StmtKind::Pass => Stmt {
                location: stmt.location,
                end_location: stmt.end_location,
                node: StmtKind::Pass,
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::StmtKind::Return { value } => Stmt {
                location: stmt.location,
                end_location: stmt.end_location,
                node: StmtKind::Return {
                    value: value.map(|v| (*v, locator).into()),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::StmtKind::Assign {
                targets,
                value,
                type_comment,
            } => Stmt {
                location: stmt.location,
                end_location: stmt.end_location,
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
            rustpython_parser::ast::StmtKind::ClassDef {
                name,
                bases,
                keywords,
                body,
                decorator_list,
            } => {
                // Find the start and end of the `body`.
                let body = {
                    let (body_location, body_end_location) = expand_indented_block(
                        stmt.location,
                        body.last().unwrap().end_location.unwrap(),
                        locator,
                    );
                    Body {
                        location: body_location,
                        end_location: Some(body_end_location),
                        node: body
                            .into_iter()
                            .map(|node| (node, locator).into())
                            .collect(),
                        trivia: vec![],
                        parentheses: Parenthesize::Never,
                    }
                };

                Stmt {
                    location: stmt.location,
                    end_location: body.end_location,
                    node: StmtKind::ClassDef {
                        name,
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
            rustpython_parser::ast::StmtKind::If { test, body, orelse } => {
                // Find the start and end of the `body`.
                let body = {
                    let (body_location, body_end_location) = expand_indented_block(
                        stmt.location,
                        body.last().unwrap().end_location.unwrap(),
                        locator,
                    );
                    Body {
                        location: body_location,
                        end_location: Some(body_end_location),
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
                        location: stmt.location,
                        end_location: body.end_location,
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
                        if let StmtKind::If { is_elif, .. } =
                            &mut elif.node.first_mut().unwrap().node
                        {
                            *is_elif = true;
                        };

                        Stmt {
                            location: stmt.location,
                            end_location: elif.end_location,
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
                        let (orelse_location, orelse_end_location) = expand_indented_block(
                            body.end_location.unwrap(),
                            orelse.last().unwrap().end_location.unwrap(),
                            locator,
                        );
                        let orelse = Body {
                            location: orelse_location,
                            end_location: Some(orelse_end_location),
                            node: orelse
                                .into_iter()
                                .map(|node| (node, locator).into())
                                .collect(),
                            trivia: vec![],
                            parentheses: Parenthesize::Never,
                        };

                        Stmt {
                            location: stmt.location,
                            end_location: orelse.end_location,
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
            rustpython_parser::ast::StmtKind::Assert { test, msg } => Stmt {
                location: stmt.location,
                end_location: stmt.end_location,
                node: StmtKind::Assert {
                    test: Box::new((*test, locator).into()),
                    msg: msg.map(|node| Box::new((*node, locator).into())),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::StmtKind::FunctionDef {
                name,
                args,
                body,
                decorator_list,
                returns,
                type_comment,
            } => {
                // Find the start and end of the `body`.
                let body = {
                    let (body_location, body_end_location) = expand_indented_block(
                        stmt.location,
                        body.last().unwrap().end_location.unwrap(),
                        locator,
                    );
                    Body {
                        location: body_location,
                        end_location: Some(body_end_location),
                        node: body
                            .into_iter()
                            .map(|node| (node, locator).into())
                            .collect(),
                        trivia: vec![],
                        parentheses: Parenthesize::Never,
                    }
                };

                Stmt {
                    location: decorator_list.first().map_or(stmt.location, |d| d.location),
                    end_location: body.end_location,
                    node: StmtKind::FunctionDef {
                        name,
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
            rustpython_parser::ast::StmtKind::AsyncFunctionDef {
                name,
                args,
                body,
                decorator_list,
                returns,
                type_comment,
            } => {
                // Find the start and end of the `body`.
                let body = {
                    let (body_location, body_end_location) = expand_indented_block(
                        stmt.location,
                        body.last().unwrap().end_location.unwrap(),
                        locator,
                    );
                    Body {
                        location: body_location,
                        end_location: Some(body_end_location),
                        node: body
                            .into_iter()
                            .map(|node| (node, locator).into())
                            .collect(),
                        trivia: vec![],
                        parentheses: Parenthesize::Never,
                    }
                };

                Stmt {
                    location: decorator_list
                        .first()
                        .map_or(stmt.location, |expr| expr.location),
                    end_location: body.end_location,
                    node: StmtKind::AsyncFunctionDef {
                        name,
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
            rustpython_parser::ast::StmtKind::Delete { targets } => Stmt {
                location: stmt.location,
                end_location: stmt.end_location,
                node: StmtKind::Delete {
                    targets: targets
                        .into_iter()
                        .map(|node| (node, locator).into())
                        .collect(),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::StmtKind::AugAssign { target, op, value } => Stmt {
                location: stmt.location,
                end_location: stmt.end_location,
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
                        let (op_location, op_end_location) = find_tok(
                            target.end_location.unwrap(),
                            value.location,
                            locator,
                            |tok| tok == target_tok,
                        );
                        Operator::new(op_location, op_end_location, (&op).into())
                    },
                    target: Box::new((*target, locator).into()),
                    value: Box::new((*value, locator).into()),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::StmtKind::AnnAssign {
                target,
                annotation,
                value,
                simple,
            } => Stmt {
                location: stmt.location,
                end_location: stmt.end_location,
                node: StmtKind::AnnAssign {
                    target: Box::new((*target, locator).into()),
                    annotation: Box::new((*annotation, locator).into()),
                    value: value.map(|node| Box::new((*node, locator).into())),
                    simple,
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::StmtKind::For {
                target,
                iter,
                body,
                orelse,
                type_comment,
            } => {
                // Find the start and end of the `body`.
                let body = {
                    let (body_location, body_end_location) = expand_indented_block(
                        stmt.location,
                        body.last().unwrap().end_location.unwrap(),
                        locator,
                    );
                    Body {
                        location: body_location,
                        end_location: Some(body_end_location),
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
                    let (orelse_location, orelse_end_location) = expand_indented_block(
                        body.end_location.unwrap(),
                        orelse.last().unwrap().end_location.unwrap(),
                        locator,
                    );
                    Body {
                        location: orelse_location,
                        end_location: Some(orelse_end_location),
                        node: orelse
                            .into_iter()
                            .map(|node| (node, locator).into())
                            .collect(),
                        trivia: vec![],
                        parentheses: Parenthesize::Never,
                    }
                });

                Stmt {
                    location: stmt.location,
                    end_location: orelse.as_ref().unwrap_or(&body).end_location,
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
            rustpython_parser::ast::StmtKind::AsyncFor {
                target,
                iter,
                body,
                orelse,
                type_comment,
            } => {
                // Find the start and end of the `body`.
                let body = {
                    let (body_location, body_end_location) = expand_indented_block(
                        stmt.location,
                        body.last().unwrap().end_location.unwrap(),
                        locator,
                    );
                    Body {
                        location: body_location,
                        end_location: Some(body_end_location),
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
                    let (orelse_location, orelse_end_location) = expand_indented_block(
                        body.end_location.unwrap(),
                        orelse.last().unwrap().end_location.unwrap(),
                        locator,
                    );
                    Body {
                        location: orelse_location,
                        end_location: Some(orelse_end_location),
                        node: orelse
                            .into_iter()
                            .map(|node| (node, locator).into())
                            .collect(),
                        trivia: vec![],
                        parentheses: Parenthesize::Never,
                    }
                });

                Stmt {
                    location: stmt.location,
                    end_location: orelse.as_ref().unwrap_or(&body).end_location,
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
            rustpython_parser::ast::StmtKind::While { test, body, orelse } => {
                // Find the start and end of the `body`.
                let body = {
                    let (body_location, body_end_location) = expand_indented_block(
                        stmt.location,
                        body.last().unwrap().end_location.unwrap(),
                        locator,
                    );
                    Body {
                        location: body_location,
                        end_location: Some(body_end_location),
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
                    let (orelse_location, orelse_end_location) = expand_indented_block(
                        body.end_location.unwrap(),
                        orelse.last().unwrap().end_location.unwrap(),
                        locator,
                    );
                    Body {
                        location: orelse_location,
                        end_location: Some(orelse_end_location),
                        node: orelse
                            .into_iter()
                            .map(|node| (node, locator).into())
                            .collect(),
                        trivia: vec![],
                        parentheses: Parenthesize::Never,
                    }
                });

                Stmt {
                    location: stmt.location,
                    end_location: orelse.as_ref().unwrap_or(&body).end_location,
                    node: StmtKind::While {
                        test: Box::new((*test, locator).into()),
                        body,
                        orelse,
                    },
                    trivia: vec![],
                    parentheses: Parenthesize::Never,
                }
            }
            rustpython_parser::ast::StmtKind::With {
                items,
                body,
                type_comment,
            } => {
                // Find the start and end of the `body`.
                let body = {
                    let (body_location, body_end_location) = expand_indented_block(
                        stmt.location,
                        body.last().unwrap().end_location.unwrap(),
                        locator,
                    );
                    Body {
                        location: body_location,
                        end_location: Some(body_end_location),
                        node: body
                            .into_iter()
                            .map(|node| (node, locator).into())
                            .collect(),
                        trivia: vec![],
                        parentheses: Parenthesize::Never,
                    }
                };

                Stmt {
                    location: stmt.location,
                    end_location: body.end_location,
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
            rustpython_parser::ast::StmtKind::AsyncWith {
                items,
                body,
                type_comment,
            } => {
                // Find the start and end of the `body`.
                let body = {
                    let (body_location, body_end_location) = expand_indented_block(
                        stmt.location,
                        body.last().unwrap().end_location.unwrap(),
                        locator,
                    );
                    Body {
                        location: body_location,
                        end_location: Some(body_end_location),
                        node: body
                            .into_iter()
                            .map(|node| (node, locator).into())
                            .collect(),
                        trivia: vec![],
                        parentheses: Parenthesize::Never,
                    }
                };

                Stmt {
                    location: stmt.location,
                    end_location: body.end_location,
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
            rustpython_parser::ast::StmtKind::Match { subject, cases } => Stmt {
                location: stmt.location,
                end_location: stmt.end_location,
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
            rustpython_parser::ast::StmtKind::Raise { exc, cause } => Stmt {
                location: stmt.location,
                end_location: stmt.end_location,
                node: StmtKind::Raise {
                    exc: exc.map(|exc| Box::new((*exc, locator).into())),
                    cause: cause.map(|cause| Box::new((*cause, locator).into())),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::StmtKind::Try {
                body,
                handlers,
                orelse,
                finalbody,
            } => {
                // Find the start and end of the `body`.
                let body = {
                    let (body_location, body_end_location) = expand_indented_block(
                        stmt.location,
                        body.last().unwrap().end_location.unwrap(),
                        locator,
                    );
                    Body {
                        location: body_location,
                        end_location: Some(body_end_location),
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
                    let (orelse_location, orelse_end_location) = expand_indented_block(
                        handlers
                            .last()
                            .map_or(body.end_location.unwrap(), |handler| {
                                handler.end_location.unwrap()
                            }),
                        orelse.last().unwrap().end_location.unwrap(),
                        locator,
                    );
                    Body {
                        location: orelse_location,
                        end_location: Some(orelse_end_location),
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
                    let (finalbody_location, finalbody_end_location) = expand_indented_block(
                        orelse.as_ref().map_or(
                            handlers
                                .last()
                                .map_or(body.end_location.unwrap(), |handler| {
                                    handler.end_location.unwrap()
                                }),
                            |orelse| orelse.end_location.unwrap(),
                        ),
                        finalbody.last().unwrap().end_location.unwrap(),
                        locator,
                    );
                    Body {
                        location: finalbody_location,
                        end_location: Some(finalbody_end_location),
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
                        handlers
                            .last()
                            .map_or(body.end_location.unwrap(), |handler| {
                                handler.end_location.unwrap()
                            }),
                        |orelse| orelse.end_location.unwrap(),
                    ),
                    |finalbody| finalbody.end_location.unwrap(),
                );

                Stmt {
                    location: stmt.location,
                    end_location: Some(end_location),
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
            rustpython_parser::ast::StmtKind::TryStar {
                body,
                handlers,
                orelse,
                finalbody,
            } => {
                // Find the start and end of the `body`.
                let body = {
                    let (body_location, body_end_location) = expand_indented_block(
                        stmt.location,
                        body.last().unwrap().end_location.unwrap(),
                        locator,
                    );
                    Body {
                        location: body_location,
                        end_location: Some(body_end_location),
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
                    let (orelse_location, orelse_end_location) = expand_indented_block(
                        handlers
                            .last()
                            .map_or(body.end_location.unwrap(), |handler| {
                                handler.end_location.unwrap()
                            }),
                        orelse.last().unwrap().end_location.unwrap(),
                        locator,
                    );
                    Body {
                        location: orelse_location,
                        end_location: Some(orelse_end_location),
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
                    let (finalbody_location, finalbody_end_location) = expand_indented_block(
                        orelse.as_ref().map_or(
                            handlers
                                .last()
                                .map_or(body.end_location.unwrap(), |handler| {
                                    handler.end_location.unwrap()
                                }),
                            |orelse| orelse.end_location.unwrap(),
                        ),
                        finalbody.last().unwrap().end_location.unwrap(),
                        locator,
                    );
                    Body {
                        location: finalbody_location,
                        end_location: Some(finalbody_end_location),
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
                        handlers
                            .last()
                            .map_or(body.end_location.unwrap(), |handler| {
                                handler.end_location.unwrap()
                            }),
                        |orelse| orelse.end_location.unwrap(),
                    ),
                    |finalbody| finalbody.end_location.unwrap(),
                );

                Stmt {
                    location: stmt.location,
                    end_location: Some(end_location),
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
            rustpython_parser::ast::StmtKind::Import { names } => Stmt {
                location: stmt.location,
                end_location: stmt.end_location,
                node: StmtKind::Import {
                    names: names
                        .into_iter()
                        .map(|node| (node, locator).into())
                        .collect(),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::StmtKind::ImportFrom {
                module,
                names,
                level,
            } => Stmt {
                location: stmt.location,
                end_location: stmt.end_location,
                node: StmtKind::ImportFrom {
                    module,
                    names: names
                        .into_iter()
                        .map(|node| (node, locator).into())
                        .collect(),
                    level,
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::StmtKind::Global { names } => Stmt {
                location: stmt.location,
                end_location: stmt.end_location,
                node: StmtKind::Global { names },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::StmtKind::Nonlocal { names } => Stmt {
                location: stmt.location,
                end_location: stmt.end_location,
                node: StmtKind::Nonlocal { names },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::StmtKind::Break => Stmt {
                location: stmt.location,
                end_location: stmt.end_location,
                node: StmtKind::Break,
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::StmtKind::Continue => Stmt {
                location: stmt.location,
                end_location: stmt.end_location,
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
            location: keyword.location,
            end_location: keyword.end_location,
            node: KeywordData {
                arg: keyword.node.arg,
                value: (keyword.node.value, locator).into(),
            },
            trivia: vec![],
            parentheses: Parenthesize::Never,
        }
    }
}

impl From<(rustpython_parser::ast::Arg, &Locator<'_>)> for Arg {
    fn from((arg, locator): (rustpython_parser::ast::Arg, &Locator)) -> Self {
        Arg {
            location: arg.location,
            end_location: arg.end_location,
            node: ArgData {
                arg: arg.node.arg,
                annotation: arg
                    .node
                    .annotation
                    .map(|node| Box::new((*node, locator).into())),
                type_comment: arg.node.type_comment,
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
            is_async: comprehension.is_async,
        }
    }
}

impl From<(rustpython_parser::ast::Expr, &Locator<'_>)> for Expr {
    fn from((expr, locator): (rustpython_parser::ast::Expr, &Locator)) -> Self {
        match expr.node {
            rustpython_parser::ast::ExprKind::Name { id, ctx } => Expr {
                location: expr.location,
                end_location: expr.end_location,
                node: ExprKind::Name {
                    id,
                    ctx: ctx.into(),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::ExprKind::BoolOp { op, values } => Expr {
                location: expr.location,
                end_location: expr.end_location,
                node: ExprKind::BoolOp {
                    ops: values
                        .iter()
                        .tuple_windows()
                        .map(|(left, right)| {
                            let target_tok = match &op {
                                rustpython_parser::ast::Boolop::And => rustpython_parser::Tok::And,
                                rustpython_parser::ast::Boolop::Or => rustpython_parser::Tok::Or,
                            };
                            let (op_location, op_end_location) = find_tok(
                                left.end_location.unwrap(),
                                right.location,
                                locator,
                                |tok| tok == target_tok,
                            );
                            BoolOp::new(op_location, op_end_location, (&op).into())
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
            rustpython_parser::ast::ExprKind::NamedExpr { target, value } => Expr {
                location: expr.location,
                end_location: expr.end_location,
                node: ExprKind::NamedExpr {
                    target: Box::new((*target, locator).into()),
                    value: Box::new((*value, locator).into()),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::ExprKind::BinOp { left, op, right } => Expr {
                location: expr.location,
                end_location: expr.end_location,
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
                        let (op_location, op_end_location) =
                            find_tok(left.end_location.unwrap(), right.location, locator, |tok| {
                                tok == target_tok
                            });
                        Operator::new(op_location, op_end_location, (&op).into())
                    },
                    left: Box::new((*left, locator).into()),
                    right: Box::new((*right, locator).into()),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::ExprKind::UnaryOp { op, operand } => Expr {
                location: expr.location,
                end_location: expr.end_location,
                node: ExprKind::UnaryOp {
                    op: {
                        let target_tok = match &op {
                            rustpython_parser::ast::Unaryop::Invert => {
                                rustpython_parser::Tok::Tilde
                            }
                            rustpython_parser::ast::Unaryop::Not => rustpython_parser::Tok::Not,
                            rustpython_parser::ast::Unaryop::UAdd => rustpython_parser::Tok::Plus,
                            rustpython_parser::ast::Unaryop::USub => rustpython_parser::Tok::Minus,
                        };
                        let (op_location, op_end_location) =
                            find_tok(expr.location, operand.location, locator, |tok| {
                                tok == target_tok
                            });
                        UnaryOp::new(op_location, op_end_location, (&op).into())
                    },
                    operand: Box::new((*operand, locator).into()),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::ExprKind::Lambda { args, body } => Expr {
                location: expr.location,
                end_location: expr.end_location,
                node: ExprKind::Lambda {
                    args: Box::new((*args, locator).into()),
                    body: Box::new((*body, locator).into()),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::ExprKind::IfExp { test, body, orelse } => Expr {
                location: expr.location,
                end_location: expr.end_location,
                node: ExprKind::IfExp {
                    test: Box::new((*test, locator).into()),
                    body: Box::new((*body, locator).into()),
                    orelse: Box::new((*orelse, locator).into()),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::ExprKind::Dict { keys, values } => Expr {
                location: expr.location,
                end_location: expr.end_location,
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
            rustpython_parser::ast::ExprKind::Set { elts } => Expr {
                location: expr.location,
                end_location: expr.end_location,
                node: ExprKind::Set {
                    elts: elts
                        .into_iter()
                        .map(|node| (node, locator).into())
                        .collect(),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::ExprKind::ListComp { elt, generators } => Expr {
                location: expr.location,
                end_location: expr.end_location,
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
            rustpython_parser::ast::ExprKind::SetComp { elt, generators } => Expr {
                location: expr.location,
                end_location: expr.end_location,
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
            rustpython_parser::ast::ExprKind::DictComp {
                key,
                value,
                generators,
            } => Expr {
                location: expr.location,
                end_location: expr.end_location,
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
            rustpython_parser::ast::ExprKind::GeneratorExp { elt, generators } => Expr {
                location: expr.location,
                end_location: expr.end_location,
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
            rustpython_parser::ast::ExprKind::Await { value } => Expr {
                location: expr.location,
                end_location: expr.end_location,
                node: ExprKind::Await {
                    value: Box::new((*value, locator).into()),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::ExprKind::Yield { value } => Expr {
                location: expr.location,
                end_location: expr.end_location,
                node: ExprKind::Yield {
                    value: value.map(|v| Box::new((*v, locator).into())),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::ExprKind::YieldFrom { value } => Expr {
                location: expr.location,
                end_location: expr.end_location,
                node: ExprKind::YieldFrom {
                    value: Box::new((*value, locator).into()),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::ExprKind::Compare {
                left,
                ops,
                comparators,
            } => Expr {
                location: expr.location,
                end_location: expr.end_location,
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
                            let (op_location, op_end_location) = find_tok(
                                left.end_location.unwrap(),
                                right.location,
                                locator,
                                |tok| tok == target_tok,
                            );
                            CmpOp::new(op_location, op_end_location, (&op).into())
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
            rustpython_parser::ast::ExprKind::Call {
                func,
                args,
                keywords,
            } => Expr {
                location: expr.location,
                end_location: expr.end_location,
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
            rustpython_parser::ast::ExprKind::FormattedValue {
                value,
                conversion,
                format_spec,
            } => Expr {
                location: expr.location,
                end_location: expr.end_location,
                node: ExprKind::FormattedValue {
                    value: Box::new((*value, locator).into()),
                    conversion,
                    format_spec: format_spec.map(|f| Box::new((*f, locator).into())),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::ExprKind::JoinedStr { values } => Expr {
                location: expr.location,
                end_location: expr.end_location,
                node: ExprKind::JoinedStr {
                    values: values
                        .into_iter()
                        .map(|node| (node, locator).into())
                        .collect(),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::ExprKind::Constant { value, kind } => Expr {
                location: expr.location,
                end_location: expr.end_location,
                node: ExprKind::Constant { value, kind },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::ExprKind::Attribute { value, attr, ctx } => Expr {
                location: expr.location,
                end_location: expr.end_location,
                node: ExprKind::Attribute {
                    value: Box::new((*value, locator).into()),
                    attr,
                    ctx: ctx.into(),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::ExprKind::Subscript { value, slice, ctx } => Expr {
                location: expr.location,
                end_location: expr.end_location,
                node: ExprKind::Subscript {
                    value: Box::new((*value, locator).into()),
                    slice: Box::new((*slice, locator).into()),
                    ctx: ctx.into(),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::ExprKind::Starred { value, ctx } => Expr {
                location: expr.location,
                end_location: expr.end_location,
                node: ExprKind::Starred {
                    value: Box::new((*value, locator).into()),
                    ctx: ctx.into(),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::ExprKind::List { elts, ctx } => Expr {
                location: expr.location,
                end_location: expr.end_location,
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
            rustpython_parser::ast::ExprKind::Tuple { elts, ctx } => Expr {
                location: expr.location,
                end_location: expr.end_location,
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
            rustpython_parser::ast::ExprKind::Slice { lower, upper, step } => {
                // Locate the colon tokens, which indicate the number of index segments.
                let (source, start, end) =
                    locator.slice(Range::new(expr.location, expr.end_location.unwrap()));
                let tokens = rustpython_parser::lexer::lex_located(
                    &source[start..end],
                    Mode::Module,
                    expr.location,
                );

                // Find the first and (if it exists) second colon in the slice, avoiding any
                // semicolons within nested slices, and any lambda expressions.
                let mut first_colon = None;
                let mut second_colon = None;
                let mut lambda = 0;
                let mut nesting = 0;
                for (start, tok, ..) in tokens.flatten() {
                    match tok {
                        rustpython_parser::Tok::Lambda if nesting == 0 => lambda += 1,
                        rustpython_parser::Tok::Colon if nesting == 0 => {
                            if lambda > 0 {
                                lambda -= 1;
                            } else {
                                if first_colon.is_none() {
                                    first_colon = Some(start);
                                } else {
                                    second_colon = Some(start);
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
                    expr.location,
                    first_colon.unwrap(),
                    lower.map_or(SliceIndexKind::Empty, |node| SliceIndexKind::Index {
                        value: Box::new((*node, locator).into()),
                    }),
                );
                let upper = SliceIndex::new(
                    first_colon.unwrap(),
                    second_colon.unwrap_or(expr.end_location.unwrap()),
                    upper.map_or(SliceIndexKind::Empty, |node| SliceIndexKind::Index {
                        value: Box::new((*node, locator).into()),
                    }),
                );
                let step = second_colon.map(|second_colon| {
                    SliceIndex::new(
                        second_colon,
                        expr.end_location.unwrap(),
                        step.map_or(SliceIndexKind::Empty, |node| SliceIndexKind::Index {
                            value: Box::new((*node, locator).into()),
                        }),
                    )
                });

                Expr {
                    location: expr.location,
                    end_location: expr.end_location,
                    node: ExprKind::Slice { lower, upper, step },
                    trivia: vec![],
                    parentheses: Parenthesize::Never,
                }
            }
        }
    }
}
