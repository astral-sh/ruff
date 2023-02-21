#![allow(clippy::derive_partial_eq_without_eq)]

use rustpython_parser::ast::{Constant, Location};

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
pub enum Boolop {
    And,
    Or,
}

impl From<rustpython_parser::ast::Boolop> for Boolop {
    fn from(op: rustpython_parser::ast::Boolop) -> Self {
        match op {
            rustpython_parser::ast::Boolop::And => Self::And,
            rustpython_parser::ast::Boolop::Or => Self::Or,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Operator {
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

impl From<rustpython_parser::ast::Operator> for Operator {
    fn from(op: rustpython_parser::ast::Operator) -> Self {
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
pub enum Unaryop {
    Invert,
    Not,
    UAdd,
    USub,
}

impl From<rustpython_parser::ast::Unaryop> for Unaryop {
    fn from(op: rustpython_parser::ast::Unaryop) -> Self {
        match op {
            rustpython_parser::ast::Unaryop::Invert => Self::Invert,
            rustpython_parser::ast::Unaryop::Not => Self::Not,
            rustpython_parser::ast::Unaryop::UAdd => Self::UAdd,
            rustpython_parser::ast::Unaryop::USub => Self::USub,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Cmpop {
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

impl From<rustpython_parser::ast::Cmpop> for Cmpop {
    fn from(op: rustpython_parser::ast::Cmpop) -> Self {
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

#[derive(Clone, Debug, PartialEq)]
pub enum StmtKind {
    FunctionDef {
        name: Ident,
        args: Box<Arguments>,
        body: Vec<Stmt>,
        decorator_list: Vec<Expr>,
        returns: Option<Box<Expr>>,
        type_comment: Option<String>,
    },
    AsyncFunctionDef {
        name: Ident,
        args: Box<Arguments>,
        body: Vec<Stmt>,
        decorator_list: Vec<Expr>,
        returns: Option<Box<Expr>>,
        type_comment: Option<String>,
    },
    ClassDef {
        name: Ident,
        bases: Vec<Expr>,
        keywords: Vec<Keyword>,
        body: Vec<Stmt>,
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
        body: Vec<Stmt>,
        orelse: Vec<Stmt>,
        type_comment: Option<String>,
    },
    AsyncFor {
        target: Box<Expr>,
        iter: Box<Expr>,
        body: Vec<Stmt>,
        orelse: Vec<Stmt>,
        type_comment: Option<String>,
    },
    While {
        test: Box<Expr>,
        body: Vec<Stmt>,
        orelse: Vec<Stmt>,
    },
    If {
        test: Box<Expr>,
        body: Vec<Stmt>,
        orelse: Vec<Stmt>,
    },
    With {
        items: Vec<Withitem>,
        body: Vec<Stmt>,
        type_comment: Option<String>,
    },
    AsyncWith {
        items: Vec<Withitem>,
        body: Vec<Stmt>,
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
        body: Vec<Stmt>,
        handlers: Vec<Excepthandler>,
        orelse: Vec<Stmt>,
        finalbody: Vec<Stmt>,
    },
    TryStar {
        body: Vec<Stmt>,
        handlers: Vec<Excepthandler>,
        orelse: Vec<Stmt>,
        finalbody: Vec<Stmt>,
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
        op: Boolop,
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
        op: Unaryop,
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
        ops: Vec<Cmpop>,
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
        lower: Option<Box<Expr>>,
        upper: Option<Box<Expr>>,
        step: Option<Box<Expr>>,
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
        body: Vec<Stmt>,
    },
}

pub type Excepthandler = Located<ExcepthandlerKind>;

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
    pub body: Vec<Stmt>,
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

impl From<rustpython_parser::ast::Alias> for Alias {
    fn from(alias: rustpython_parser::ast::Alias) -> Self {
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

impl From<rustpython_parser::ast::Withitem> for Withitem {
    fn from(withitem: rustpython_parser::ast::Withitem) -> Self {
        Withitem {
            context_expr: withitem.context_expr.into(),
            optional_vars: withitem.optional_vars.map(|v| Box::new((*v).into())),
        }
    }
}

impl From<rustpython_parser::ast::Excepthandler> for Excepthandler {
    fn from(excepthandler: rustpython_parser::ast::Excepthandler) -> Self {
        let rustpython_parser::ast::ExcepthandlerKind::ExceptHandler { type_, name, body } =
            excepthandler.node;
        Excepthandler {
            location: excepthandler.location,
            end_location: excepthandler.end_location,
            node: ExcepthandlerKind::ExceptHandler {
                type_: type_.map(|type_| Box::new((*type_).into())),
                name,
                body: body.into_iter().map(Into::into).collect(),
            },
            trivia: vec![],
            parentheses: Parenthesize::Never,
        }
    }
}

impl From<rustpython_parser::ast::Stmt> for Stmt {
    fn from(stmt: rustpython_parser::ast::Stmt) -> Self {
        match stmt.node {
            rustpython_parser::ast::StmtKind::Expr { value } => Stmt {
                location: stmt.location,
                end_location: stmt.end_location,
                node: StmtKind::Expr {
                    value: Box::new((*value).into()),
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
                    value: value.map(|v| (*v).into()),
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
                    targets: targets.into_iter().map(Into::into).collect(),
                    value: Box::new((*value).into()),
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
            } => Stmt {
                location: stmt.location,
                end_location: stmt.end_location,
                node: StmtKind::ClassDef {
                    name,
                    bases: bases.into_iter().map(Into::into).collect(),
                    keywords: keywords.into_iter().map(Into::into).collect(),
                    body: body.into_iter().map(Into::into).collect(),
                    decorator_list: decorator_list.into_iter().map(Into::into).collect(),
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
            } => Stmt {
                location: decorator_list
                    .first()
                    .map_or(stmt.location, |expr| expr.location),
                end_location: stmt.end_location,
                node: StmtKind::FunctionDef {
                    name,
                    args: Box::new((*args).into()),
                    body: body.into_iter().map(Into::into).collect(),
                    decorator_list: decorator_list.into_iter().map(Into::into).collect(),
                    returns: returns.map(|r| Box::new((*r).into())),
                    type_comment,
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::StmtKind::If { test, body, orelse } => Stmt {
                location: stmt.location,
                end_location: stmt.end_location,
                node: StmtKind::If {
                    test: Box::new((*test).into()),
                    body: body.into_iter().map(Into::into).collect(),
                    orelse: orelse.into_iter().map(Into::into).collect(),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::StmtKind::Assert { test, msg } => Stmt {
                location: stmt.location,
                end_location: stmt.end_location,
                node: StmtKind::Assert {
                    test: Box::new((*test).into()),
                    msg: msg.map(|msg| Box::new((*msg).into())),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::StmtKind::AsyncFunctionDef {
                name,
                args,
                body,
                decorator_list,
                returns,
                type_comment,
            } => Stmt {
                location: stmt.location,
                end_location: stmt.end_location,
                node: StmtKind::AsyncFunctionDef {
                    name,
                    args: Box::new((*args).into()),
                    body: body.into_iter().map(Into::into).collect(),
                    decorator_list: decorator_list.into_iter().map(Into::into).collect(),
                    returns: returns.map(|r| Box::new((*r).into())),
                    type_comment,
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::StmtKind::Delete { targets } => Stmt {
                location: stmt.location,
                end_location: stmt.end_location,
                node: StmtKind::Delete {
                    targets: targets.into_iter().map(Into::into).collect(),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::StmtKind::AugAssign { target, op, value } => Stmt {
                location: stmt.location,
                end_location: stmt.end_location,
                node: StmtKind::AugAssign {
                    target: Box::new((*target).into()),
                    op: op.into(),
                    value: Box::new((*value).into()),
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
                    target: Box::new((*target).into()),
                    annotation: Box::new((*annotation).into()),
                    value: value.map(|v| Box::new((*v).into())),
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
            } => Stmt {
                location: stmt.location,
                end_location: stmt.end_location,
                node: StmtKind::For {
                    target: Box::new((*target).into()),
                    iter: Box::new((*iter).into()),
                    body: body.into_iter().map(Into::into).collect(),
                    orelse: orelse.into_iter().map(Into::into).collect(),
                    type_comment,
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::StmtKind::AsyncFor {
                target,
                iter,
                body,
                orelse,
                type_comment,
            } => Stmt {
                location: stmt.location,
                end_location: stmt.end_location,
                node: StmtKind::AsyncFor {
                    target: Box::new((*target).into()),
                    iter: Box::new((*iter).into()),
                    body: body.into_iter().map(Into::into).collect(),
                    orelse: orelse.into_iter().map(Into::into).collect(),
                    type_comment,
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::StmtKind::While { test, body, orelse } => Stmt {
                location: stmt.location,
                end_location: stmt.end_location,
                node: StmtKind::While {
                    test: Box::new((*test).into()),
                    body: body.into_iter().map(Into::into).collect(),
                    orelse: orelse.into_iter().map(Into::into).collect(),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::StmtKind::With {
                items,
                body,
                type_comment,
            } => Stmt {
                location: stmt.location,
                end_location: stmt.end_location,
                node: StmtKind::With {
                    items: items.into_iter().map(Into::into).collect(),
                    body: body.into_iter().map(Into::into).collect(),
                    type_comment,
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::StmtKind::AsyncWith {
                items,
                body,
                type_comment,
            } => Stmt {
                location: stmt.location,
                end_location: stmt.end_location,
                node: StmtKind::AsyncWith {
                    items: items.into_iter().map(Into::into).collect(),
                    body: body.into_iter().map(Into::into).collect(),
                    type_comment,
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::StmtKind::Match { .. } => {
                todo!("match statement");
            }
            rustpython_parser::ast::StmtKind::Raise { exc, cause } => Stmt {
                location: stmt.location,
                end_location: stmt.end_location,
                node: StmtKind::Raise {
                    exc: exc.map(|exc| Box::new((*exc).into())),
                    cause: cause.map(|cause| Box::new((*cause).into())),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::StmtKind::Try {
                body,
                handlers,
                orelse,
                finalbody,
            } => Stmt {
                location: stmt.location,
                end_location: stmt.end_location,
                node: StmtKind::Try {
                    body: body.into_iter().map(Into::into).collect(),
                    handlers: handlers.into_iter().map(Into::into).collect(),
                    orelse: orelse.into_iter().map(Into::into).collect(),
                    finalbody: finalbody.into_iter().map(Into::into).collect(),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::StmtKind::TryStar {
                body,
                handlers,
                orelse,
                finalbody,
            } => Stmt {
                location: stmt.location,
                end_location: stmt.end_location,
                node: StmtKind::TryStar {
                    body: body.into_iter().map(Into::into).collect(),
                    handlers: handlers.into_iter().map(Into::into).collect(),
                    orelse: orelse.into_iter().map(Into::into).collect(),
                    finalbody: finalbody.into_iter().map(Into::into).collect(),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::StmtKind::Import { names } => Stmt {
                location: stmt.location,
                end_location: stmt.end_location,
                node: StmtKind::Import {
                    names: names.into_iter().map(Into::into).collect(),
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
                    names: names.into_iter().map(Into::into).collect(),
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

impl From<rustpython_parser::ast::Keyword> for Keyword {
    fn from(keyword: rustpython_parser::ast::Keyword) -> Self {
        Keyword {
            location: keyword.location,
            end_location: keyword.end_location,
            node: KeywordData {
                arg: keyword.node.arg,
                value: keyword.node.value.into(),
            },
            trivia: vec![],
            parentheses: Parenthesize::Never,
        }
    }
}

impl From<rustpython_parser::ast::Arg> for Arg {
    fn from(arg: rustpython_parser::ast::Arg) -> Self {
        Arg {
            location: arg.location,
            end_location: arg.end_location,
            node: ArgData {
                arg: arg.node.arg,
                annotation: arg.node.annotation.map(|a| Box::new((*a).into())),
                type_comment: arg.node.type_comment,
            },
            trivia: vec![],
            parentheses: Parenthesize::Never,
        }
    }
}

impl From<rustpython_parser::ast::Arguments> for Arguments {
    fn from(arguments: rustpython_parser::ast::Arguments) -> Self {
        Arguments {
            posonlyargs: arguments.posonlyargs.into_iter().map(Into::into).collect(),
            args: arguments.args.into_iter().map(Into::into).collect(),
            vararg: arguments.vararg.map(|v| Box::new((*v).into())),
            kwonlyargs: arguments.kwonlyargs.into_iter().map(Into::into).collect(),
            kw_defaults: arguments.kw_defaults.into_iter().map(Into::into).collect(),
            kwarg: arguments.kwarg.map(|k| Box::new((*k).into())),
            defaults: arguments.defaults.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<rustpython_parser::ast::Comprehension> for Comprehension {
    fn from(comprehension: rustpython_parser::ast::Comprehension) -> Self {
        Comprehension {
            target: comprehension.target.into(),
            iter: comprehension.iter.into(),
            ifs: comprehension.ifs.into_iter().map(Into::into).collect(),
            is_async: comprehension.is_async,
        }
    }
}

impl From<rustpython_parser::ast::Expr> for Expr {
    fn from(expr: rustpython_parser::ast::Expr) -> Self {
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
                    op: op.into(),
                    values: values.into_iter().map(Into::into).collect(),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::ExprKind::NamedExpr { target, value } => Expr {
                location: expr.location,
                end_location: expr.end_location,
                node: ExprKind::NamedExpr {
                    target: Box::new((*target).into()),
                    value: Box::new((*value).into()),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::ExprKind::BinOp { left, op, right } => Expr {
                location: expr.location,
                end_location: expr.end_location,
                node: ExprKind::BinOp {
                    left: Box::new((*left).into()),
                    op: op.into(),
                    right: Box::new((*right).into()),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::ExprKind::UnaryOp { op, operand } => Expr {
                location: expr.location,
                end_location: expr.end_location,
                node: ExprKind::UnaryOp {
                    op: op.into(),
                    operand: Box::new((*operand).into()),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::ExprKind::Lambda { args, body } => Expr {
                location: expr.location,
                end_location: expr.end_location,
                node: ExprKind::Lambda {
                    args: Box::new((*args).into()),
                    body: Box::new((*body).into()),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::ExprKind::IfExp { test, body, orelse } => Expr {
                location: expr.location,
                end_location: expr.end_location,
                node: ExprKind::IfExp {
                    test: Box::new((*test).into()),
                    body: Box::new((*body).into()),
                    orelse: Box::new((*orelse).into()),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::ExprKind::Dict { keys, values } => Expr {
                location: expr.location,
                end_location: expr.end_location,
                node: ExprKind::Dict {
                    keys: keys.into_iter().map(|key| key.map(Into::into)).collect(),
                    values: values.into_iter().map(Into::into).collect(),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::ExprKind::Set { elts } => Expr {
                location: expr.location,
                end_location: expr.end_location,
                node: ExprKind::Set {
                    elts: elts.into_iter().map(Into::into).collect(),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::ExprKind::ListComp { elt, generators } => Expr {
                location: expr.location,
                end_location: expr.end_location,
                node: ExprKind::ListComp {
                    elt: Box::new((*elt).into()),
                    generators: generators.into_iter().map(Into::into).collect(),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::ExprKind::SetComp { elt, generators } => Expr {
                location: expr.location,
                end_location: expr.end_location,
                node: ExprKind::SetComp {
                    elt: Box::new((*elt).into()),
                    generators: generators.into_iter().map(Into::into).collect(),
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
                    key: Box::new((*key).into()),
                    value: Box::new((*value).into()),
                    generators: generators.into_iter().map(Into::into).collect(),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::ExprKind::GeneratorExp { elt, generators } => Expr {
                location: expr.location,
                end_location: expr.end_location,
                node: ExprKind::GeneratorExp {
                    elt: Box::new((*elt).into()),
                    generators: generators.into_iter().map(Into::into).collect(),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::ExprKind::Await { value } => Expr {
                location: expr.location,
                end_location: expr.end_location,
                node: ExprKind::Await {
                    value: Box::new((*value).into()),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::ExprKind::Yield { value } => Expr {
                location: expr.location,
                end_location: expr.end_location,
                node: ExprKind::Yield {
                    value: value.map(|v| Box::new((*v).into())),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::ExprKind::YieldFrom { value } => Expr {
                location: expr.location,
                end_location: expr.end_location,
                node: ExprKind::YieldFrom {
                    value: Box::new((*value).into()),
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
                    left: Box::new((*left).into()),
                    ops: ops.into_iter().map(Into::into).collect(),
                    comparators: comparators.into_iter().map(Into::into).collect(),
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
                    func: Box::new((*func).into()),
                    args: args.into_iter().map(Into::into).collect(),
                    keywords: keywords.into_iter().map(Into::into).collect(),
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
                    value: Box::new((*value).into()),
                    conversion,
                    format_spec: format_spec.map(|f| Box::new((*f).into())),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::ExprKind::JoinedStr { values } => Expr {
                location: expr.location,
                end_location: expr.end_location,
                node: ExprKind::JoinedStr {
                    values: values.into_iter().map(Into::into).collect(),
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
                    value: Box::new((*value).into()),
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
                    value: Box::new((*value).into()),
                    slice: Box::new((*slice).into()),
                    ctx: ctx.into(),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::ExprKind::Starred { value, ctx } => Expr {
                location: expr.location,
                end_location: expr.end_location,
                node: ExprKind::Starred {
                    value: Box::new((*value).into()),
                    ctx: ctx.into(),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::ExprKind::List { elts, ctx } => Expr {
                location: expr.location,
                end_location: expr.end_location,
                node: ExprKind::List {
                    elts: elts.into_iter().map(Into::into).collect(),
                    ctx: ctx.into(),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::ExprKind::Tuple { elts, ctx } => Expr {
                location: expr.location,
                end_location: expr.end_location,
                node: ExprKind::Tuple {
                    elts: elts.into_iter().map(Into::into).collect(),
                    ctx: ctx.into(),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
            rustpython_parser::ast::ExprKind::Slice { lower, upper, step } => Expr {
                location: expr.location,
                end_location: expr.end_location,
                node: ExprKind::Slice {
                    lower: lower.map(|l| Box::new((*l).into())),
                    upper: upper.map(|u| Box::new((*u).into())),
                    step: step.map(|s| Box::new((*s).into())),
                },
                trivia: vec![],
                parentheses: Parenthesize::Never,
            },
        }
    }
}
