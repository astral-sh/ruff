use std::fmt;
use std::str::FromStr;

use ruff_python_ast::{Expr, Stmt};
use serde::Deserialize;
use thiserror::Error;

/// An AST node selector identifying which nodes a scripted rule should run against.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AstTarget {
    Stmt(StmtKind),
    Expr(ExprKind),
}

impl AstTarget {
    pub const fn kind(&self) -> AstNodeClass {
        match self {
            AstTarget::Stmt(..) => AstNodeClass::Stmt,
            AstTarget::Expr(..) => AstNodeClass::Expr,
        }
    }

    pub const fn name(&self) -> &'static str {
        match self {
            AstTarget::Stmt(kind) => kind.as_str(),
            AstTarget::Expr(kind) => kind.as_str(),
        }
    }
}

impl fmt::Display for AstTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AstTarget::Stmt(kind) => write!(f, "stmt:{}", kind.as_str()),
            AstTarget::Expr(kind) => write!(f, "expr:{}", kind.as_str()),
        }
    }
}

impl FromStr for AstTarget {
    type Err = AstTargetParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        parse_target(s)
    }
}

/// Convenience wrapper that enables parsing `AstTarget` values directly from configuration.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize)]
#[serde(transparent)]
pub struct AstTargetSpec(String);

impl AstTargetSpec {
    pub fn parse(&self) -> Result<AstTarget, AstTargetParseError> {
        self.0.as_str().parse()
    }

    pub fn raw(&self) -> &str {
        &self.0
    }
}

/// Broad AST node classes supported by scripted rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AstNodeClass {
    Stmt,
    Expr,
}

/// Statement kinds supported by scripted rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum StmtKind {
    FunctionDef,
    ClassDef,
    Return,
    Delete,
    TypeAlias,
    Assign,
    AugAssign,
    AnnAssign,
    For,
    While,
    If,
    With,
    Match,
    Raise,
    Try,
    Assert,
    Import,
    ImportFrom,
    Global,
    Nonlocal,
    Expr,
    Pass,
    Break,
    Continue,
    IpyEscapeCommand,
}

impl StmtKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            StmtKind::FunctionDef => "FunctionDef",
            StmtKind::ClassDef => "ClassDef",
            StmtKind::Return => "Return",
            StmtKind::Delete => "Delete",
            StmtKind::TypeAlias => "TypeAlias",
            StmtKind::Assign => "Assign",
            StmtKind::AugAssign => "AugAssign",
            StmtKind::AnnAssign => "AnnAssign",
            StmtKind::For => "For",
            StmtKind::While => "While",
            StmtKind::If => "If",
            StmtKind::With => "With",
            StmtKind::Match => "Match",
            StmtKind::Raise => "Raise",
            StmtKind::Try => "Try",
            StmtKind::Assert => "Assert",
            StmtKind::Import => "Import",
            StmtKind::ImportFrom => "ImportFrom",
            StmtKind::Global => "Global",
            StmtKind::Nonlocal => "Nonlocal",
            StmtKind::Expr => "Expr",
            StmtKind::Pass => "Pass",
            StmtKind::Break => "Break",
            StmtKind::Continue => "Continue",
            StmtKind::IpyEscapeCommand => "IpyEscapeCommand",
        }
    }

    pub fn matches(self, stmt: &Stmt) -> bool {
        matches!(
            (self, stmt),
            (StmtKind::FunctionDef, Stmt::FunctionDef(_))
                | (StmtKind::ClassDef, Stmt::ClassDef(_))
                | (StmtKind::Return, Stmt::Return(_))
                | (StmtKind::Delete, Stmt::Delete(_))
                | (StmtKind::TypeAlias, Stmt::TypeAlias(_))
                | (StmtKind::Assign, Stmt::Assign(_))
                | (StmtKind::AugAssign, Stmt::AugAssign(_))
                | (StmtKind::AnnAssign, Stmt::AnnAssign(_))
                | (StmtKind::For, Stmt::For(_))
                | (StmtKind::While, Stmt::While(_))
                | (StmtKind::If, Stmt::If(_))
                | (StmtKind::With, Stmt::With(_))
                | (StmtKind::Match, Stmt::Match(_))
                | (StmtKind::Raise, Stmt::Raise(_))
                | (StmtKind::Try, Stmt::Try(_))
                | (StmtKind::Assert, Stmt::Assert(_))
                | (StmtKind::Import, Stmt::Import(_))
                | (StmtKind::ImportFrom, Stmt::ImportFrom(_))
                | (StmtKind::Global, Stmt::Global(_))
                | (StmtKind::Nonlocal, Stmt::Nonlocal(_))
                | (StmtKind::Expr, Stmt::Expr(_))
                | (StmtKind::Pass, Stmt::Pass(_))
                | (StmtKind::Break, Stmt::Break(_))
                | (StmtKind::Continue, Stmt::Continue(_))
                | (StmtKind::IpyEscapeCommand, Stmt::IpyEscapeCommand(_))
        )
    }
}

impl fmt::Display for StmtKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl From<&Stmt> for StmtKind {
    fn from(value: &Stmt) -> Self {
        match value {
            Stmt::FunctionDef(_) => StmtKind::FunctionDef,
            Stmt::ClassDef(_) => StmtKind::ClassDef,
            Stmt::Return(_) => StmtKind::Return,
            Stmt::Delete(_) => StmtKind::Delete,
            Stmt::TypeAlias(_) => StmtKind::TypeAlias,
            Stmt::Assign(_) => StmtKind::Assign,
            Stmt::AugAssign(_) => StmtKind::AugAssign,
            Stmt::AnnAssign(_) => StmtKind::AnnAssign,
            Stmt::For(_) => StmtKind::For,
            Stmt::While(_) => StmtKind::While,
            Stmt::If(_) => StmtKind::If,
            Stmt::With(_) => StmtKind::With,
            Stmt::Match(_) => StmtKind::Match,
            Stmt::Raise(_) => StmtKind::Raise,
            Stmt::Try(_) => StmtKind::Try,
            Stmt::Assert(_) => StmtKind::Assert,
            Stmt::Import(_) => StmtKind::Import,
            Stmt::ImportFrom(_) => StmtKind::ImportFrom,
            Stmt::Global(_) => StmtKind::Global,
            Stmt::Nonlocal(_) => StmtKind::Nonlocal,
            Stmt::Expr(_) => StmtKind::Expr,
            Stmt::Pass(_) => StmtKind::Pass,
            Stmt::Break(_) => StmtKind::Break,
            Stmt::Continue(_) => StmtKind::Continue,
            Stmt::IpyEscapeCommand(_) => StmtKind::IpyEscapeCommand,
        }
    }
}

/// Expression kinds supported by scripted rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ExprKind {
    Attribute,
    Await,
    BinOp,
    BoolOp,
    BooleanLiteral,
    BytesLiteral,
    Call,
    Compare,
    Dict,
    DictComp,
    EllipsisLiteral,
    FString,
    Generator,
    If,
    IpyEscapeCommand,
    Lambda,
    List,
    ListComp,
    Name,
    Named,
    NoneLiteral,
    NumberLiteral,
    Set,
    SetComp,
    Slice,
    Starred,
    StringLiteral,
    Subscript,
    Tuple,
    UnaryOp,
    Yield,
    YieldFrom,
}

impl ExprKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            ExprKind::Attribute => "Attribute",
            ExprKind::Await => "Await",
            ExprKind::BinOp => "BinOp",
            ExprKind::BoolOp => "BoolOp",
            ExprKind::BooleanLiteral => "BooleanLiteral",
            ExprKind::BytesLiteral => "BytesLiteral",
            ExprKind::Call => "Call",
            ExprKind::Compare => "Compare",
            ExprKind::Dict => "Dict",
            ExprKind::DictComp => "DictComp",
            ExprKind::EllipsisLiteral => "EllipsisLiteral",
            ExprKind::FString => "FString",
            ExprKind::Generator => "Generator",
            ExprKind::If => "If",
            ExprKind::IpyEscapeCommand => "IpyEscapeCommand",
            ExprKind::Lambda => "Lambda",
            ExprKind::List => "List",
            ExprKind::ListComp => "ListComp",
            ExprKind::Name => "Name",
            ExprKind::Named => "Named",
            ExprKind::NoneLiteral => "NoneLiteral",
            ExprKind::NumberLiteral => "NumberLiteral",
            ExprKind::Set => "Set",
            ExprKind::SetComp => "SetComp",
            ExprKind::Slice => "Slice",
            ExprKind::Starred => "Starred",
            ExprKind::StringLiteral => "StringLiteral",
            ExprKind::Subscript => "Subscript",
            ExprKind::Tuple => "Tuple",
            ExprKind::UnaryOp => "UnaryOp",
            ExprKind::Yield => "Yield",
            ExprKind::YieldFrom => "YieldFrom",
        }
    }

    pub fn matches(self, expr: &Expr) -> bool {
        match self {
            ExprKind::Attribute => matches!(expr, Expr::Attribute(_)),
            ExprKind::Await => matches!(expr, Expr::Await(_)),
            ExprKind::BinOp => matches!(expr, Expr::BinOp(_)),
            ExprKind::BoolOp => matches!(expr, Expr::BoolOp(_)),
            ExprKind::BooleanLiteral => matches!(expr, Expr::BooleanLiteral(_)),
            ExprKind::BytesLiteral => matches!(expr, Expr::BytesLiteral(_)),
            ExprKind::Call => matches!(expr, Expr::Call(_)),
            ExprKind::Compare => matches!(expr, Expr::Compare(_)),
            ExprKind::Dict => matches!(expr, Expr::Dict(_)),
            ExprKind::DictComp => matches!(expr, Expr::DictComp(_)),
            ExprKind::EllipsisLiteral => matches!(expr, Expr::EllipsisLiteral(_)),
            ExprKind::FString => matches!(expr, Expr::FString(_) | Expr::TString(_)),
            ExprKind::Generator => matches!(expr, Expr::Generator(_)),
            ExprKind::If => matches!(expr, Expr::If(_)),
            ExprKind::IpyEscapeCommand => matches!(expr, Expr::IpyEscapeCommand(_)),
            ExprKind::Lambda => matches!(expr, Expr::Lambda(_)),
            ExprKind::List => matches!(expr, Expr::List(_)),
            ExprKind::ListComp => matches!(expr, Expr::ListComp(_)),
            ExprKind::Name => matches!(expr, Expr::Name(_)),
            ExprKind::Named => matches!(expr, Expr::Named(_)),
            ExprKind::NoneLiteral => matches!(expr, Expr::NoneLiteral(_)),
            ExprKind::NumberLiteral => matches!(expr, Expr::NumberLiteral(_)),
            ExprKind::Set => matches!(expr, Expr::Set(_)),
            ExprKind::SetComp => matches!(expr, Expr::SetComp(_)),
            ExprKind::Slice => matches!(expr, Expr::Slice(_)),
            ExprKind::Starred => matches!(expr, Expr::Starred(_)),
            ExprKind::StringLiteral => matches!(expr, Expr::StringLiteral(_)),
            ExprKind::Subscript => matches!(expr, Expr::Subscript(_)),
            ExprKind::Tuple => matches!(expr, Expr::Tuple(_)),
            ExprKind::UnaryOp => matches!(expr, Expr::UnaryOp(_)),
            ExprKind::Yield => matches!(expr, Expr::Yield(_)),
            ExprKind::YieldFrom => matches!(expr, Expr::YieldFrom(_)),
        }
    }
}

impl fmt::Display for ExprKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl From<&Expr> for ExprKind {
    fn from(value: &Expr) -> Self {
        match value {
            Expr::Attribute(_) => ExprKind::Attribute,
            Expr::Await(_) => ExprKind::Await,
            Expr::BinOp(_) => ExprKind::BinOp,
            Expr::BoolOp(_) => ExprKind::BoolOp,
            Expr::BooleanLiteral(_) => ExprKind::BooleanLiteral,
            Expr::BytesLiteral(_) => ExprKind::BytesLiteral,
            Expr::Call(_) => ExprKind::Call,
            Expr::Compare(_) => ExprKind::Compare,
            Expr::Dict(_) => ExprKind::Dict,
            Expr::DictComp(_) => ExprKind::DictComp,
            Expr::EllipsisLiteral(_) => ExprKind::EllipsisLiteral,
            Expr::FString(_) => ExprKind::FString,
            Expr::TString(_) => ExprKind::FString,
            Expr::Generator(_) => ExprKind::Generator,
            Expr::If(_) => ExprKind::If,
            Expr::IpyEscapeCommand(_) => ExprKind::IpyEscapeCommand,
            Expr::Lambda(_) => ExprKind::Lambda,
            Expr::List(_) => ExprKind::List,
            Expr::ListComp(_) => ExprKind::ListComp,
            Expr::Name(_) => ExprKind::Name,
            Expr::Named(_) => ExprKind::Named,
            Expr::NoneLiteral(_) => ExprKind::NoneLiteral,
            Expr::NumberLiteral(_) => ExprKind::NumberLiteral,
            Expr::Set(_) => ExprKind::Set,
            Expr::SetComp(_) => ExprKind::SetComp,
            Expr::Slice(_) => ExprKind::Slice,
            Expr::Starred(_) => ExprKind::Starred,
            Expr::StringLiteral(_) => ExprKind::StringLiteral,
            Expr::Subscript(_) => ExprKind::Subscript,
            Expr::Tuple(_) => ExprKind::Tuple,
            Expr::UnaryOp(_) => ExprKind::UnaryOp,
            Expr::Yield(_) => ExprKind::Yield,
            Expr::YieldFrom(_) => ExprKind::YieldFrom,
        }
    }
}

#[derive(Debug, Error)]
pub enum AstTargetParseError {
    #[error("expected `stmt:<kind>` or `expr:<kind>` target selector")]
    MissingPrefix,
    #[error("unknown statement selector `{0}`")]
    UnknownStmtKind(String),
    #[error("unknown expression selector `{0}`")]
    UnknownExprKind(String),
}

fn parse_target(raw: &str) -> Result<AstTarget, AstTargetParseError> {
    let (prefix, name) = raw
        .split_once(':')
        .ok_or(AstTargetParseError::MissingPrefix)?;
    match prefix {
        "stmt" => Ok(AstTarget::Stmt(parse_stmt_kind(name)?)),
        "expr" => Ok(AstTarget::Expr(parse_expr_kind(name)?)),
        _ => Err(AstTargetParseError::MissingPrefix),
    }
}

fn parse_stmt_kind(name: &str) -> Result<StmtKind, AstTargetParseError> {
    match name {
        "FunctionDef" => Ok(StmtKind::FunctionDef),
        "ClassDef" => Ok(StmtKind::ClassDef),
        "Return" => Ok(StmtKind::Return),
        "Delete" => Ok(StmtKind::Delete),
        "TypeAlias" => Ok(StmtKind::TypeAlias),
        "Assign" => Ok(StmtKind::Assign),
        "AugAssign" => Ok(StmtKind::AugAssign),
        "AnnAssign" => Ok(StmtKind::AnnAssign),
        "For" => Ok(StmtKind::For),
        "While" => Ok(StmtKind::While),
        "If" => Ok(StmtKind::If),
        "With" => Ok(StmtKind::With),
        "Match" => Ok(StmtKind::Match),
        "Raise" => Ok(StmtKind::Raise),
        "Try" => Ok(StmtKind::Try),
        "Assert" => Ok(StmtKind::Assert),
        "Import" => Ok(StmtKind::Import),
        "ImportFrom" => Ok(StmtKind::ImportFrom),
        "Global" => Ok(StmtKind::Global),
        "Nonlocal" => Ok(StmtKind::Nonlocal),
        "Expr" => Ok(StmtKind::Expr),
        "Pass" => Ok(StmtKind::Pass),
        "Break" => Ok(StmtKind::Break),
        "Continue" => Ok(StmtKind::Continue),
        "IpyEscapeCommand" => Ok(StmtKind::IpyEscapeCommand),
        other => Err(AstTargetParseError::UnknownStmtKind(other.to_string())),
    }
}

fn parse_expr_kind(name: &str) -> Result<ExprKind, AstTargetParseError> {
    match name {
        "Attribute" => Ok(ExprKind::Attribute),
        "Await" => Ok(ExprKind::Await),
        "BinOp" => Ok(ExprKind::BinOp),
        "BoolOp" => Ok(ExprKind::BoolOp),
        "BooleanLiteral" => Ok(ExprKind::BooleanLiteral),
        "BytesLiteral" => Ok(ExprKind::BytesLiteral),
        "Call" => Ok(ExprKind::Call),
        "Compare" => Ok(ExprKind::Compare),
        "Dict" => Ok(ExprKind::Dict),
        "DictComp" => Ok(ExprKind::DictComp),
        "EllipsisLiteral" => Ok(ExprKind::EllipsisLiteral),
        "FString" => Ok(ExprKind::FString),
        "Generator" => Ok(ExprKind::Generator),
        "If" => Ok(ExprKind::If),
        "IpyEscapeCommand" => Ok(ExprKind::IpyEscapeCommand),
        "Lambda" => Ok(ExprKind::Lambda),
        "List" => Ok(ExprKind::List),
        "ListComp" => Ok(ExprKind::ListComp),
        "Name" => Ok(ExprKind::Name),
        "Named" => Ok(ExprKind::Named),
        "NoneLiteral" => Ok(ExprKind::NoneLiteral),
        "NumberLiteral" => Ok(ExprKind::NumberLiteral),
        "Set" => Ok(ExprKind::Set),
        "SetComp" => Ok(ExprKind::SetComp),
        "Slice" => Ok(ExprKind::Slice),
        "Starred" => Ok(ExprKind::Starred),
        "StringLiteral" => Ok(ExprKind::StringLiteral),
        "Subscript" => Ok(ExprKind::Subscript),
        "Tuple" => Ok(ExprKind::Tuple),
        "TString" => Ok(ExprKind::FString),
        "UnaryOp" => Ok(ExprKind::UnaryOp),
        "Yield" => Ok(ExprKind::Yield),
        "YieldFrom" => Ok(ExprKind::YieldFrom),
        other => Err(AstTargetParseError::UnknownExprKind(other.to_string())),
    }
}
