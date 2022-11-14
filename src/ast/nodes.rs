pub trait Located {
    fn start_row(&self) -> usize;
    fn start_col(&self) -> usize;
    fn end_row(&self) -> usize;
    fn end_col(&self) -> usize;
    #[inline]
    fn start(&self) -> (usize, usize) {
        (self.start_row(), self.start_col())
    }
    #[inline]
    fn end(&self) -> (usize, usize) {
        (self.end_row(), self.end_col())
    }
}

pub trait Ident {
    fn val(&self) -> &str;
}

pub trait Alias {
    fn name(&self) -> &dyn Ident;
    fn asname(&self) -> Option<&dyn Ident>;
}

pub trait Arg<'a>: Located {
    type Expr: Expr<'a>;
    fn arg(&self) -> &dyn Ident;
    fn annotation(&self) -> Option<&Self::Expr>;
    fn type_comment(&self) -> Option<&str>;
}

pub trait Arguments<'a> {
    type Arg: Arg<'a>;
    type Expr: Expr<'a>;
    fn posonlyargs(&self) -> &[Self::Arg];
    fn args(&self) -> &[Self::Arg];
    fn vararg(&self) -> Option<&Self::Arg>;
    fn kwonlyargs(&self) -> &[Self::Arg];
    fn kw_defaults(&self) -> &[Self::Expr];
    fn kwarg(&self) -> Option<&Self::Arg>;
    fn defaults(&self) -> &[Self::Expr];
}

pub trait Keyword<'a> {
    type Expr: Expr<'a>;
    fn arg(&self) -> Option<&dyn Ident>;
    fn value(&self) -> &Self::Expr;
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Boolop {
    And,
    Or,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
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

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Unaryop {
    Invert,
    Not,
    UAdd,
    USub,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
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

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ExprContext {
    Load,
    Store,
    Del,
}

// TODO if there's internal requirements of BigInt an interface should be required
pub trait BigInt {}

pub enum ConstantKind<'a, CONSTANT, BIGINT> {
    None,
    Bool(&'a bool),
    Str(&'a String),
    Bytes(&'a [u8]),
    Int(&'a BIGINT),
    Tuple(&'a [CONSTANT]),
    Float(&'a f64),
    Complex { real: &'a f64, imag: &'a f64 },
    Ellipsis,
}

pub trait Constant<'a> {
    type Constant: Sized + Constant<'a>;
    type BigInt: BigInt;
    fn value(&'a self) -> ConstantKind<'a, Self::Constant, Self::BigInt>;
}

pub trait Comprehension<'a> {
    type Expr: Expr<'a>;
    fn target(&self) -> &Self::Expr;
    fn iter(&self) -> &Self::Expr;
    fn ifs(&self) -> &[Self::Expr];
    fn is_async(&self) -> usize;
}

pub trait BoolOp<'a> {
    type Expr: Expr<'a>;
    fn op(&self) -> Boolop;
    fn values(&self) -> &[Self::Expr];
}

pub trait NamedExpr<'a> {
    type Expr: Expr<'a>;
    fn target(&self) -> &Self::Expr;
    fn value(&self) -> &Self::Expr;
}

pub trait BinOp<'a> {
    type Expr: Expr<'a>;
    fn left(&self) -> &Self::Expr;
    fn op(&self) -> Operator;
    fn right(&self) -> &Self::Expr;
}

pub trait UnaryOp<'a> {
    type Expr: Expr<'a>;
    fn op(&self) -> Unaryop;
    fn operand(&self) -> &Self::Expr;
}

pub trait Lambda<'a> {
    type Arguments: Arguments<'a>;
    type Expr: Expr<'a>;
    fn args(&self) -> &Self::Arguments;
    fn body(&self) -> &Self::Expr;
}

pub trait IfExp<'a> {
    type Expr: Expr<'a>;
    fn test(&self) -> &Self::Expr;
    fn body(&self) -> &Self::Expr;
    fn orelse(&self) -> &Self::Expr;
}

pub trait Dict<'a> {
    type Expr: Expr<'a>;
    fn keys(&self) -> &[Self::Expr];
    fn values(&self) -> &[Self::Expr];
}

pub trait Set<'a> {
    type Expr: Expr<'a>;
    fn elts(&self) -> &[Self::Expr];
}

pub trait ListComp<'a> {
    type Expr: Expr<'a>;
    type Comprehension: Comprehension<'a>;
    fn elt(&self) -> &Self::Expr;
    fn generators(&self) -> &[Self::Comprehension];
}

pub trait SetComp<'a> {
    type Expr: Expr<'a>;
    type Comprehension: Comprehension<'a>;
    fn elt(&self) -> &Self::Expr;
    fn generators(&self) -> &[Self::Comprehension];
}

pub trait DictComp<'a> {
    type Expr: Expr<'a>;
    type Comprehension: Comprehension<'a>;
    fn key(&self) -> &Self::Expr;
    fn value(&self) -> &Self::Expr;
    fn generators(&self) -> &[Self::Comprehension];
}

pub trait GeneratorExp<'a> {
    type Expr: Expr<'a>;
    type Comprehension: Comprehension<'a>;
    fn elt(&self) -> &Self::Expr;
    fn generators(&self) -> &[Self::Comprehension];
}

pub trait Await<'a> {
    type Expr: Expr<'a>;
    fn value(&self) -> &Self::Expr;
}

pub trait Yield<'a> {
    type Expr: Expr<'a>;
    fn value(&self) -> Option<&Self::Expr>;
}

pub trait YieldFrom<'a> {
    type Expr: Expr<'a>;
    fn value(&self) -> &Self::Expr;
}

pub trait Compare<'a> {
    type Expr: Expr<'a>;
    fn left(&self) -> &Self::Expr;
    fn ops(&self) -> Vec<Cmpop>;
}

pub trait Call<'a> {
    type Expr: Expr<'a>;
    type Keyword: Keyword<'a>;
    fn func(&self) -> &Self::Expr;
    fn args(&self) -> &[Self::Expr];
    fn keywords(&self) -> &[Self::Keyword];
}

pub trait FormattedValue<'a> {
    type Expr: Expr<'a>;
    fn value(&self) -> &Self::Expr;
    fn conversion(&self) -> usize;
    fn format_spec(&self) -> Option<&Self::Expr>;
}

pub trait JoinedStr<'a> {
    type Expr: Expr<'a>;
    fn values(&self) -> &[Self::Expr];
}

pub trait ConstantExpr<'a> {
    type Constant: Constant<'a>;
    fn value(&self) -> &Self::Constant;
    fn kind(&self) -> Option<&str>;
}

pub trait Attribute<'a> {
    type Expr: Expr<'a>;
    fn value(&self) -> &Self::Expr;
    fn attr(&self) -> &dyn Ident;
    fn ctx(&self) -> ExprContext;
}

pub trait Subscript<'a> {
    type Expr: Expr<'a>;
    fn value(&self) -> &Self::Expr;
    fn slice(&self) -> &Self::Expr;
    fn ctx(&self) -> ExprContext;
}

pub trait Starred<'a> {
    type Expr: Expr<'a>;
    fn value(&self) -> &Self::Expr;
    fn ctx(&self) -> ExprContext;
}

pub trait Name {
    fn id(&self) -> &dyn Ident;
    fn ctx(&self) -> ExprContext;
}

pub trait List<'a> {
    type Expr: Expr<'a>;
    fn elts(&self) -> &[Self::Expr];
    fn ctx(&self) -> ExprContext;
}

pub trait Tuple<'a> {
    type Expr: Expr<'a>;
    fn elts(&self) -> &[Self::Expr];
    fn ctx(&self) -> ExprContext;
}

pub trait Slice<'a> {
    type Expr: Expr<'a>;
    fn lower(&self) -> Option<&Self::Expr>;
    fn upper(&self) -> Option<&Self::Expr>;
    fn step(&self) -> Option<&Self::Expr>;
}

pub enum ExprKind<
    'a,
    BOOLOP,
    NAMEDEXPR,
    BINOP,
    UNARYOP,
    LAMBDA,
    IFEXP,
    DICT,
    SET,
    LISTCOMP,
    SETCOMP,
    DICTCOMP,
    GENERATOREXP,
    AWAIT,
    YIELD,
    YIELDFROM,
    COMPARE,
    CALL,
    FORMATTEDVALUE,
    JOINEDSTR,
    CONSTANTEXPR,
    ATTRIBUTE,
    SUBSCRIPT,
    STARRED,
    NAME,
    LIST,
    TUPLE,
    SLICE,
> {
    BoolOp(&'a BOOLOP),
    NamedExpr(&'a NAMEDEXPR),
    BinOp(&'a BINOP),
    UnaryOp(&'a UNARYOP),
    Lambda(&'a LAMBDA),
    IfExp(&'a IFEXP),
    Dict(&'a DICT),
    Set(&'a SET),
    ListComp(&'a LISTCOMP),
    SetComp(&'a SETCOMP),
    DictComp(&'a DICTCOMP),
    GeneratorExp(&'a GENERATOREXP),
    Await(&'a AWAIT),
    Yield(&'a YIELD),
    YieldFrom(&'a YIELDFROM),
    Compare(&'a COMPARE),
    Call(&'a CALL),
    FormattedValue(&'a FORMATTEDVALUE),
    JoinedStr(&'a JOINEDSTR),
    ConstantExpr(&'a CONSTANTEXPR),
    Attribute(&'a ATTRIBUTE),
    Subscript(&'a SUBSCRIPT),
    Starred(&'a STARRED),
    Name(&'a NAME),
    List(&'a LIST),
    Tuple(&'a TUPLE),
    Slice(&'a SLICE),
}

#[allow(clippy::type_complexity)]
pub trait Expr<'a>: Located {
    type BoolOp: BoolOp<'a>;
    type NamedExpr: NamedExpr<'a>;
    type BinOp: BinOp<'a>;
    type UnaryOp: UnaryOp<'a>;
    type Lambda: Lambda<'a>;
    type IfExp: IfExp<'a>;
    type Dict: Dict<'a>;
    type Set: Set<'a>;
    type ListComp: ListComp<'a>;
    type SetComp: SetComp<'a>;
    type DictComp: DictComp<'a>;
    type GeneratorExp: GeneratorExp<'a>;
    type Await: Await<'a>;
    type Yield: Yield<'a>;
    type YieldFrom: YieldFrom<'a>;
    type Compare: Compare<'a>;
    type Call: Call<'a>;
    type FormattedValue: FormattedValue<'a>;
    type JoinedStr: JoinedStr<'a>;
    type ConstantExpr: ConstantExpr<'a>;
    type Attribute: Attribute<'a>;
    type Subscript: Subscript<'a>;
    type Starred: Starred<'a>;
    type Name: Name;
    type List: List<'a>;
    type Tuple: Tuple<'a>;
    type Slice: Slice<'a>;
    fn expr(
        &'a self,
    ) -> ExprKind<
        'a,
        Self::BoolOp,
        Self::NamedExpr,
        Self::BinOp,
        Self::UnaryOp,
        Self::Lambda,
        Self::IfExp,
        Self::Dict,
        Self::Set,
        Self::ListComp,
        Self::SetComp,
        Self::DictComp,
        Self::GeneratorExp,
        Self::Await,
        Self::Yield,
        Self::YieldFrom,
        Self::Compare,
        Self::Call,
        Self::FormattedValue,
        Self::JoinedStr,
        Self::ConstantExpr,
        Self::Attribute,
        Self::Subscript,
        Self::Starred,
        Self::Name,
        Self::List,
        Self::Tuple,
        Self::Slice,
    >;
}

pub trait ExceptHandler<'a>: Located {
    type Expr: Expr<'a>;
    type Stmt: Stmt<'a>;
    fn type_(&self) -> Option<&Self::Expr>;
    fn name(&self) -> Option<&dyn Ident>;
    fn body(&self) -> &[Self::Stmt];
}

pub trait MatchValue<'a> {
    type Expr: Expr<'a>;
    fn value(&self) -> &Self::Expr;
}

pub trait MatchSingleton<'a> {
    type Constant: Constant<'a>;
    fn value(&self) -> &Self::Constant;
}

pub trait MatchSequence<'a> {
    type Pattern: Pattern<'a>;
    fn patterns(&self) -> &[Self::Pattern];
}

pub trait MatchMapping<'a> {
    type Expr: Expr<'a>;
    type Pattern: Pattern<'a>;
    fn keys(&self) -> &[Self::Expr];
    fn patterns(&self) -> &[Self::Pattern];
    fn rest(&self) -> Option<&dyn Ident>;
}

pub trait MatchClass<'a> {
    type Expr: Expr<'a>;
    type Pattern: Pattern<'a>;
    type Ident: Ident;
    fn cls(&self) -> &Self::Expr;
    fn patterns(&self) -> &[Self::Pattern];
    fn kwd_attrs(&self) -> &[Self::Ident];
    fn kwd_patterns(&self) -> &[Self::Pattern];
}

pub trait MatchStar {
    fn name(&self) -> Option<&dyn Ident>;
}

pub trait MatchAs<'a> {
    type Pattern: Pattern<'a>;
    fn pattern(&self) -> Option<&Self::Pattern>;
    fn name(&self) -> Option<&dyn Ident>;
}

pub trait MatchOr<'a> {
    type Pattern: Pattern<'a>;
    fn patterns(&self) -> &[Self::Pattern];
}

// Type complexity required due to need to support circular
// associated types.
// Enum variant names correspond to python grammar
#[allow(clippy::type_complexity)]
#[allow(clippy::enum_variant_names)]
pub enum PatternKind<
    'a,
    MATCHVALUE,
    MATCHSINGLETON,
    MATCHSEQUENCE,
    MATCHMAPPING,
    MATCHCLASS,
    MATCHSTAR,
    MATCHAS,
    MATCHOR,
> {
    MatchValue(&'a MATCHVALUE),
    MatchSingleton(&'a MATCHSINGLETON),
    MatchSequence(&'a MATCHSEQUENCE),
    MatchMapping(&'a MATCHMAPPING),
    MatchClass(&'a MATCHCLASS),
    MatchStar(&'a MATCHSTAR),
    MatchAs(&'a MATCHAS),
    MatchOr(&'a MATCHOR),
}

// Type complexity required due to need to support circular
// associated types.
#[allow(clippy::type_complexity)]
pub trait Pattern<'a>: Located {
    type MatchValue: MatchValue<'a>;
    type MatchSingleton: MatchSingleton<'a>;
    type MatchSequence: MatchSequence<'a>;
    type MatchMapping: MatchMapping<'a>;
    type MatchClass: MatchClass<'a>;
    type MatchStar: MatchStar;
    type MatchAs: MatchAs<'a>;
    type MatchOr: MatchOr<'a>;
    fn pattern(
        &'a self,
    ) -> PatternKind<
        'a,
        Self::MatchValue,
        Self::MatchSingleton,
        Self::MatchSequence,
        Self::MatchMapping,
        Self::MatchClass,
        Self::MatchStar,
        Self::MatchAs,
        Self::MatchOr,
    >;
}

pub trait Withitem<'a> {
    type Expr: Expr<'a>;
    fn context_expr(&self) -> &Self::Expr;
    fn optional_vars(&self) -> Option<&Self::Expr>;
}

pub trait MatchCase<'a> {
    type Expr: Expr<'a>;
    type Pattern: Pattern<'a>;
    type Stmt: Stmt<'a>;
    fn pattern(&self) -> &Self::Pattern;
    fn guard(&self) -> Option<&Self::Expr>;
    fn body(&self) -> &[Self::Stmt];
}

pub trait FunctionDef<'a> {
    type Arguments: Arguments<'a>;
    type Expr: Expr<'a>;
    type Stmt: Stmt<'a>;
    fn name(&self) -> &dyn Ident;
    fn args(&self) -> &Self::Arguments;
    fn body(&self) -> &[Self::Stmt];
    fn decorator_list(&self) -> &[Self::Expr];
    fn returns(&self) -> Option<&Self::Expr>;
    fn type_comment(&self) -> Option<&str>;
}

pub trait AsyncFunctionDef<'a> {
    type Arguments: Arguments<'a>;
    type Stmt: Stmt<'a>;
    type Expr: Expr<'a>;
    fn name(&self) -> &dyn Ident;
    fn args(&self) -> &Self::Arguments;
    fn body(&self) -> &[Self::Stmt];
    fn decorator_list(&self) -> &[Self::Expr];
    fn returns(&self) -> Option<&Self::Expr>;
    fn type_comment(&self) -> Option<&str>;
}

pub trait ClassDef<'a> {
    type Stmt: Stmt<'a>;
    type Expr: Expr<'a>;
    type Keyword: Keyword<'a>;
    fn name(&self) -> &dyn Ident;
    fn bases(&self) -> &[Self::Expr];
    fn keywords(&self) -> &[Self::Keyword];
    fn body(&self) -> &[Self::Stmt];
    fn decorator_list(&self) -> &[Self::Expr];
}

pub trait Return<'a> {
    type Expr: Expr<'a>;
    fn value(&self) -> Option<&Self::Expr>;
}

pub trait Delete<'a> {
    type Expr: Expr<'a>;
    fn targets(&self) -> &[Self::Expr];
}

pub trait Assign<'a> {
    type Expr: Expr<'a>;
    fn targets(&self) -> &[Self::Expr];
    fn value(&self) -> &Self::Expr;
    fn type_comment(&self) -> Option<&str>;
}

pub trait AugAssign<'a> {
    type Expr: Expr<'a>;
    fn target(&self) -> &Self::Expr;
    fn op(&self) -> Operator;
    fn value(&self) -> &Self::Expr;
}

pub trait AnnAssign<'a> {
    type Expr: Expr<'a>;
    fn target(&self) -> &Self::Expr;
    fn annotation(&self) -> &Self::Expr;
    fn value(&self) -> Option<&Self::Expr>;
    fn simple(&self) -> usize;
}

pub trait For<'a> {
    type Expr: Expr<'a>;
    type Stmt: Stmt<'a>;
    fn target(&self) -> &Self::Expr;
    fn iter(&self) -> &Self::Expr;
    fn body(&self) -> &[Self::Stmt];
    fn orelse(&self) -> &[Self::Stmt];
    fn type_comment(&self) -> Option<&str>;
}

pub trait AsyncFor<'a> {
    type Expr: Expr<'a>;
    type Stmt: Stmt<'a>;
    fn target(&self) -> &Self::Expr;
    fn iter(&self) -> &Self::Expr;
    fn body(&self) -> &[Self::Stmt];
    fn orelse(&self) -> &[Self::Stmt];
    fn type_comment(&self) -> Option<&str>;
}

pub trait While<'a> {
    type Expr: Expr<'a>;
    type Stmt: Stmt<'a>;
    fn test(&self) -> &Self::Expr;
    fn body(&self) -> &[Self::Stmt];
    fn orelse(&self) -> &[Self::Stmt];
}

pub trait If<'a> {
    type Expr: Expr<'a>;
    type Stmt: Stmt<'a>;
    fn test(&self) -> &Self::Expr;
    fn body(&self) -> &[Self::Stmt];
    fn orelse(&self) -> &[Self::Stmt];
}

pub trait With<'a> {
    type Withitem: Withitem<'a>;
    type Stmt: Stmt<'a>;
    fn items(&self) -> &[Self::Withitem];
    fn body(&self) -> &[Self::Stmt];
    fn type_comment(&self) -> Option<&str>;
}

pub trait AsyncWith<'a> {
    type Withitem: Withitem<'a>;
    type Stmt: Stmt<'a>;
    fn items(&self) -> &[Self::Withitem];
    fn body(&self) -> &[Self::Stmt];
    fn type_comment(&self) -> Option<&str>;
}

pub trait Match<'a> {
    type Expr: Expr<'a>;
    type MatchCase: MatchCase<'a>;
    fn subject(&self) -> &Self::Expr;
    fn cases(&self) -> &[Self::MatchCase];
}

pub trait Raise<'a> {
    type Expr: Expr<'a>;
    fn exc(&self) -> Option<&Self::Expr>;
    fn cause(&self) -> Option<&Self::Expr>;
}

pub trait Try<'a> {
    type Stmt: Stmt<'a>;
    type ExceptHandler: ExceptHandler<'a>;
    fn body(&self) -> &[Self::Stmt];
    fn handlers(&self) -> &[Self::ExceptHandler];
    fn orelse(&self) -> &[Self::Stmt];
    fn finalbody(&self) -> &[Self::Stmt];
}

pub trait Assert<'a> {
    type Expr: Expr<'a>;
    fn test(&self) -> &Self::Expr;
    fn msg(&self) -> Option<&Self::Expr>;
}

pub trait Import {
    type Alias: Alias;
    fn names(&self) -> &[Self::Alias];
}

pub trait ImportFrom {
    type Alias: Alias;
    fn module(&self) -> Option<&dyn Ident>;
    fn names(&self) -> &[Self::Alias];
    fn level(&self) -> Option<usize>;
}

pub trait Global {
    type Ident: Ident;
    fn names(&self) -> &[Self::Ident];
}

pub trait Nonlocal {
    type Ident: Ident;
    fn names(&self) -> &[Self::Ident];
}

pub enum StmtKind<
    'a,
    FUNCTIONDEF,
    ASYNCFUNCTIONDEF,
    CLASSDEF,
    RETURN,
    DELETE,
    ASSIGN,
    AUGASSIGN,
    ANNASSIGN,
    FOR,
    ASYNCFOR,
    WHILE,
    IF,
    WITH,
    ASYNCWITH,
    MATCH,
    RAISE,
    TRY,
    ASSERT,
    IMPORT,
    IMPORTFROM,
    GLOBAL,
    NONLOCAL,
    EXPR,
> {
    FunctionDef(&'a FUNCTIONDEF),
    AsyncFunctionDef(&'a ASYNCFUNCTIONDEF),
    ClassDef(&'a CLASSDEF),
    Return(&'a RETURN),
    Delete(&'a DELETE),
    Assign(&'a ASSIGN),
    AugAssign(&'a AUGASSIGN),
    AnnAssign(&'a ANNASSIGN),
    For(&'a FOR),
    AsyncFor(&'a ASYNCFOR),
    While(&'a WHILE),
    If(&'a IF),
    With(&'a WITH),
    AsyncWith(&'a ASYNCWITH),
    Match(&'a MATCH),
    Raise(&'a RAISE),
    Try(&'a TRY),
    Assert(&'a ASSERT),
    Import(&'a IMPORT),
    ImportFrom(&'a IMPORTFROM),
    Global(&'a GLOBAL),
    Nonlocal(&'a NONLOCAL),
    Expr(&'a EXPR),
    Pass,
    Break,
    Continue,
}

// Type complexity required due to need to support circular
// associated types.
#[allow(clippy::type_complexity)]
pub trait Stmt<'a>: Located {
    type FunctionDef: FunctionDef<'a>;
    type AsyncFunctionDef: AsyncFunctionDef<'a>;
    type ClassDef: ClassDef<'a>;
    type Return: Return<'a>;
    type Delete: Delete<'a>;
    type Assign: Assign<'a>;
    type AugAssign: AugAssign<'a>;
    type AnnAssign: AnnAssign<'a>;
    type For: For<'a>;
    type AsyncFor: AsyncFor<'a>;
    type While: While<'a>;
    type If: If<'a>;
    type With: With<'a>;
    type AsyncWith: AsyncWith<'a>;
    type Match: Match<'a>;
    type Raise: Raise<'a>;
    type Try: Try<'a>;
    type Assert: Assert<'a>;
    type Import: Import;
    type ImportFrom: ImportFrom;
    type Global: Global;
    type Nonlocal: Nonlocal;
    type Expr: Expr<'a>;
    fn stmt(
        &'a self,
    ) -> StmtKind<
        'a,
        Self::FunctionDef,
        Self::AsyncFunctionDef,
        Self::ClassDef,
        Self::Return,
        Self::Delete,
        Self::Assign,
        Self::AugAssign,
        Self::AnnAssign,
        Self::For,
        Self::AsyncFor,
        Self::While,
        Self::If,
        Self::With,
        Self::AsyncWith,
        Self::Match,
        Self::Raise,
        Self::Try,
        Self::Assert,
        Self::Import,
        Self::ImportFrom,
        Self::Global,
        Self::Nonlocal,
        Self::Expr,
    >;
}

pub trait Ast<'a> {
    type Ident: Ident;
    type Alias: Alias;
    type Arg: Arg<'a, Expr = Self::Expr>;
    type Arguments: Arguments<'a, Expr = Self::Expr>;
    type Keyword: Keyword<'a, Expr = Self::Expr>;
    type BigInt: BigInt;
    type Constant: Constant<'a, Constant = Self::Constant, BigInt = Self::BigInt>;
    type Comprehension: Comprehension<'a, Expr = Self::Expr>;
    type BoolOp: BoolOp<'a, Expr = Self::Expr>;
    type NamedExpr: NamedExpr<'a, Expr = Self::NamedExpr>;
    type BinOp: BinOp<'a, Expr = Self::Expr>;
    type UnaryOp: UnaryOp<'a, Expr = Self::Expr>;
    type Lambda: Lambda<'a, Arguments = Self::Arguments, Expr = Self::Expr>;
    type IfExp: IfExp<'a, Expr = Self::Expr>;
    type Dict: Dict<'a, Expr = Self::Expr>;
    type Set: Set<'a, Expr = Self::Expr>;
    type ListComp: ListComp<'a, Expr = Self::Expr, Comprehension = Self::Comprehension>;
    type SetComp: SetComp<'a, Expr = Self::Expr, Comprehension = Self::Comprehension>;
    type DictComp: DictComp<'a, Expr = Self::Expr, Comprehension = Self::Comprehension>;
    type GeneratorExp: GeneratorExp<'a, Expr = Self::Expr, Comprehension = Self::Comprehension>;
    type Await: Await<'a, Expr = Self::Expr>;
    type Yield: Yield<'a, Expr = Self::Expr>;
    type YieldFrom: YieldFrom<'a, Expr = Self::Expr>;
    type Compare: Compare<'a, Expr = Self::Expr>;
    type Call: Call<'a, Expr = Self::Expr, Keyword = Self::Keyword>;
    type FormattedValue: FormattedValue<'a, Expr = Self::Expr>;
    type JoinedStr: JoinedStr<'a, Expr = Self::Expr>;
    type ConstantExpr: ConstantExpr<'a, Constant = Self::Constant>;
    type Attribute: Attribute<'a, Expr = Self::Expr>;
    type Subscript: Subscript<'a, Expr = Self::Expr>;
    type Starred: Starred<'a, Expr = Self::Expr>;
    type Name: Name;
    type List: List<'a, Expr = Self::Expr>;
    type Tuple: Tuple<'a, Expr = Self::Expr>;
    type Slice: Slice<'a, Expr = Self::Expr>;
    type Expr: Expr<
        'a,
        BoolOp = Self::BoolOp,
        NamedExpr = Self::NamedExpr,
        BinOp = Self::BinOp,
        UnaryOp = Self::UnaryOp,
        Lambda = Self::Lambda,
        IfExp = Self::IfExp,
        Dict = Self::Dict,
        Set = Self::Set,
        ListComp = Self::ListComp,
        SetComp = Self::SetComp,
        DictComp = Self::DictComp,
        GeneratorExp = Self::GeneratorExp,
        Await = Self::Await,
        Yield = Self::Yield,
        YieldFrom = Self::YieldFrom,
        Compare = Self::Compare,
        Call = Self::Call,
        FormattedValue = Self::FormattedValue,
        JoinedStr = Self::JoinedStr,
        ConstantExpr = Self::ConstantExpr,
        Attribute = Self::Attribute,
        Subscript = Self::Subscript,
        Starred = Self::Starred,
        Name = Self::Name,
        List = Self::List,
        Tuple = Self::Tuple,
        Slice = Self::Slice,
    >;
    type ExceptHandler: ExceptHandler<'a, Expr = Self::Expr, Stmt = Self::Stmt>;
    type MatchValue: MatchValue<'a, Expr = Self::Expr>;
    type MatchSingleton: MatchSingleton<'a, Constant = Self::Constant>;
    type MatchSequence: MatchSequence<'a, Pattern = Self::Pattern>;
    type MatchMapping: MatchMapping<'a, Expr = Self::Expr, Pattern = Self::Pattern>;
    type MatchClass: MatchClass<'a, Expr = Self::Expr, Pattern = Self::Pattern, Ident = Self::Ident>;
    type MatchStar: MatchStar;
    type MatchAs: MatchAs<'a, Pattern = Self::Pattern>;
    type MatchOr: MatchOr<'a, Pattern = Self::Pattern>;
    type Pattern: Pattern<
        'a,
        MatchValue = Self::MatchValue,
        MatchSingleton = Self::MatchSingleton,
        MatchSequence = Self::MatchSequence,
        MatchMapping = Self::MatchMapping,
        MatchClass = Self::MatchClass,
        MatchStar = Self::MatchStar,
        MatchAs = Self::MatchAs,
        MatchOr = Self::MatchOr,
    >;
    type Withitem: Withitem<'a, Expr = Self::Expr>;
    type MatchCase: MatchCase<'a, Expr = Self::Expr, Pattern = Self::Pattern, Stmt = Self::Stmt>;
    type FunctionDef: FunctionDef<
        'a,
        Arguments = Self::Arguments,
        Expr = Self::Expr,
        Stmt = Self::Stmt,
    >;
    type AsyncFunctionDef: AsyncFunctionDef<
        'a,
        Arguments = Self::Arguments,
        Stmt = Self::Stmt,
        Expr = Self::Expr,
    >;
    type ClassDef: ClassDef<'a, Stmt = Self::Stmt, Expr = Self::Expr, Keyword = Self::Keyword>;
    type Return: Return<'a, Expr = Self::Expr>;
    type Delete: Delete<'a, Expr = Self::Expr>;
    type Assign: Assign<'a, Expr = Self::Expr>;
    type AugAssign: AugAssign<'a, Expr = Self::Expr>;
    type AnnAssign: AnnAssign<'a, Expr = Self::Expr>;
    type For: For<'a, Expr = Self::Expr, Stmt = Self::Stmt>;
    type AsyncFor: AsyncFor<'a, Expr = Self::Expr, Stmt = Self::Stmt>;
    type While: While<'a, Expr = Self::Expr, Stmt = Self::Stmt>;
    type If: If<'a, Expr = Self::Expr, Stmt = Self::Stmt>;
    type With: With<'a, Withitem = Self::Withitem, Stmt = Self::Stmt>;
    type AsyncWith: AsyncWith<'a, Withitem = Self::Withitem, Stmt = Self::Stmt>;
    type Match: Match<'a, Expr = Self::Expr, MatchCase = Self::MatchCase>;
    type Raise: Raise<'a, Expr = Self::Expr>;
    type Try: Try<'a, Stmt = Self::Stmt, ExceptHandler = Self::ExceptHandler>;
    type Assert: Assert<'a, Expr = Self::Expr>;
    type Import: Import<Alias = Self::Alias>;
    type ImportFrom: ImportFrom<Alias = Self::Alias>;
    type Global: Global<Ident = Self::Ident>;
    type Nonlocal: Nonlocal<Ident = Self::Ident>;
    type Stmt: Stmt<
        'a,
        FunctionDef = Self::FunctionDef,
        AsyncFunctionDef = Self::AsyncFunctionDef,
        ClassDef = Self::ClassDef,
        Return = Self::Return,
        Delete = Self::Delete,
        Assign = Self::Assign,
        AugAssign = Self::AugAssign,
        AnnAssign = Self::AnnAssign,
        For = Self::For,
        AsyncFor = Self::AsyncFor,
        While = Self::While,
        If = Self::If,
        With = Self::With,
        AsyncWith = Self::AsyncWith,
        Match = Self::Match,
        Raise = Self::Raise,
        Try = Self::Try,
        Assert = Self::Assert,
        Import = Self::Import,
        ImportFrom = Self::ImportFrom,
        Global = Self::Global,
        Nonlocal = Self::Nonlocal,
        Expr = Self::Expr,
    >;
    fn stmts(&self) -> &[Self::Stmt];
}

// RustPython ast impls
// TODO(Seamooo) make below a compilation feature
mod rs_python_impls {
    use super::*;
    use num_bigint::BigInt as RspyBigInt;
    use rustpython_parser::ast as rspy_ast;

    macro_rules! rspy_types {
        ($generic_name:ident, $($ty_name:ident),*) => {
            $(
                type $ty_name = ::rustpython_parser::ast::$ty_name<$generic_name>;
            )*
        };
    }

    impl<T, U> Located for rspy_ast::Located<T, U> {
        #[inline]
        fn start_row(&self) -> usize {
            self.location.row()
        }
        #[inline]
        fn start_col(&self) -> usize {
            self.location.column()
        }
        #[inline]
        fn end_row(&self) -> usize {
            self.end_location.as_ref().unwrap().row()
        }
        #[inline]
        fn end_col(&self) -> usize {
            self.end_location.as_ref().unwrap().column()
        }
    }

    impl Ident for String {
        #[inline]
        fn val(&self) -> &str {
            self.as_str()
        }
    }

    impl<U> Alias for rspy_ast::Alias<U> {
        #[inline]
        fn name(&self) -> &dyn Ident {
            &self.node.name
        }
        #[inline]
        fn asname(&self) -> Option<&dyn Ident> {
            self.node.asname.as_ref().map(|x| x as &dyn Ident)
        }
    }

    impl<'a, U> Arg<'a> for rspy_ast::Arg<U> {
        type Expr = rspy_ast::Expr<U>;
        #[inline]
        fn arg(&self) -> &dyn Ident {
            &self.node.arg
        }
        #[inline]
        fn annotation(&self) -> Option<&Self::Expr> {
            self.node.annotation.as_deref()
        }
        #[inline]
        fn type_comment(&self) -> Option<&str> {
            self.node.type_comment.as_deref()
        }
    }

    impl<'a, U> Arguments<'a> for rspy_ast::Arguments<U> {
        rspy_types!(U, Arg, Expr);

        #[inline]
        fn posonlyargs(&self) -> &[Self::Arg] {
            &self.posonlyargs
        }
        #[inline]
        fn args(&self) -> &[Self::Arg] {
            &self.args
        }
        #[inline]
        fn vararg(&self) -> Option<&Self::Arg> {
            self.vararg.as_deref()
        }
        #[inline]
        fn kwonlyargs(&self) -> &[Self::Arg] {
            &self.kwonlyargs
        }
        #[inline]
        fn kw_defaults(&self) -> &[Self::Expr] {
            &self.kw_defaults
        }
        #[inline]
        fn kwarg(&self) -> Option<&Self::Arg> {
            self.kwarg.as_deref()
        }
        #[inline]
        fn defaults(&self) -> &[Self::Expr] {
            &self.defaults
        }
    }

    impl<'a, U> Keyword<'a> for rspy_ast::Keyword<U> {
        rspy_types!(U, Expr);

        #[inline]
        fn arg(&self) -> Option<&dyn Ident> {
            self.node.arg.as_ref().map(|x| x as &dyn Ident)
        }
        #[inline]
        fn value(&self) -> &Self::Expr {
            &self.node.value
        }
    }

    impl BigInt for RspyBigInt {}

    impl<'a> Constant<'a> for rspy_ast::Constant {
        type Constant = Self;
        type BigInt = RspyBigInt;

        fn value(&'a self) -> ConstantKind<'a, Self, Self::BigInt> {
            match self {
                Self::None => ConstantKind::None,
                Self::Bool(x) => ConstantKind::Bool(x),
                Self::Str(x) => ConstantKind::Str(x),
                Self::Bytes(x) => ConstantKind::Bytes(x),
                Self::Int(x) => ConstantKind::Int(x),
                Self::Tuple(x) => ConstantKind::Tuple(x),
                Self::Float(x) => ConstantKind::Float(x),
                Self::Complex { real, imag } => ConstantKind::Complex { real, imag },
                Self::Ellipsis => ConstantKind::Ellipsis,
            }
        }
    }

    impl<'a, U> Comprehension<'a> for rspy_ast::Comprehension<U> {
        rspy_types!(U, Expr);

        #[inline]
        fn target(&self) -> &Self::Expr {
            &self.target
        }
        #[inline]
        fn iter(&self) -> &Self::Expr {
            &self.iter
        }
        #[inline]
        fn ifs(&self) -> &[Self::Expr] {
            &self.ifs
        }
        #[inline]
        fn is_async(&self) -> usize {
            self.is_async
        }
    }

    impl From<rspy_ast::Boolop> for Boolop {
        fn from(val: rspy_ast::Boolop) -> Self {
            match val {
                rspy_ast::Boolop::And => Self::And,
                rspy_ast::Boolop::Or => Self::Or,
            }
        }
    }

    impl<'a, U> BoolOp<'a> for rspy_ast::ExprKind<U> {
        rspy_types!(U, Expr);

        #[inline]
        fn op(&self) -> Boolop {
            match self {
                Self::BoolOp { op, .. } => op.clone().into(),
                _ => unreachable!(),
            }
        }
        #[inline]
        fn values(&self) -> &[Self::Expr] {
            match self {
                Self::BoolOp { values, .. } => values,
                _ => unreachable!(),
            }
        }
    }

    impl<'a, U> NamedExpr<'a> for rspy_ast::ExprKind<U> {
        type Expr = rspy_ast::Expr<U>;
        #[inline]
        fn target(&self) -> &Self::Expr {
            match self {
                Self::NamedExpr { target, .. } => target,
                _ => unreachable!(),
            }
        }
        #[inline]
        fn value(&self) -> &Self::Expr {
            match self {
                Self::NamedExpr { value, .. } => value,
                _ => unreachable!(),
            }
        }
    }

    impl From<rspy_ast::Operator> for Operator {
        fn from(val: rspy_ast::Operator) -> Self {
            match val {
                rspy_ast::Operator::Add => Self::Add,
                rspy_ast::Operator::Sub => Self::Sub,
                rspy_ast::Operator::Mult => Self::Mult,
                rspy_ast::Operator::MatMult => Self::MatMult,
                rspy_ast::Operator::Div => Self::Div,
                rspy_ast::Operator::Mod => Self::Mod,
                rspy_ast::Operator::Pow => Self::Pow,
                rspy_ast::Operator::LShift => Self::LShift,
                rspy_ast::Operator::RShift => Self::RShift,
                rspy_ast::Operator::BitOr => Self::BitOr,
                rspy_ast::Operator::BitXor => Self::BitXor,
                rspy_ast::Operator::BitAnd => Self::BitAnd,
                rspy_ast::Operator::FloorDiv => Self::FloorDiv,
            }
        }
    }

    impl<'a, U> BinOp<'a> for rspy_ast::ExprKind<U> {
        type Expr = rspy_ast::Expr<U>;
        #[inline]
        fn left(&self) -> &Self::Expr {
            match self {
                Self::BinOp { left, .. } => left,
                _ => unreachable!(),
            }
        }
        #[inline]
        fn op(&self) -> Operator {
            match self {
                Self::BinOp { op, .. } => op.clone().into(),
                _ => unreachable!(),
            }
        }
        #[inline]
        fn right(&self) -> &Self::Expr {
            match self {
                Self::BinOp { right, .. } => right,
                _ => unreachable!(),
            }
        }
    }

    impl From<rspy_ast::Unaryop> for Unaryop {
        fn from(val: rspy_ast::Unaryop) -> Self {
            match val {
                rspy_ast::Unaryop::Invert => Self::Invert,
                rspy_ast::Unaryop::Not => Self::Not,
                rspy_ast::Unaryop::UAdd => Self::UAdd,
                rspy_ast::Unaryop::USub => Self::USub,
            }
        }
    }

    impl<'a, U> UnaryOp<'a> for rspy_ast::ExprKind<U> {
        type Expr = rspy_ast::Expr<U>;
        #[inline]
        fn op(&self) -> Unaryop {
            match self {
                Self::UnaryOp { op, .. } => op.clone().into(),
                _ => unreachable!(),
            }
        }
        #[inline]
        fn operand(&self) -> &Self::Expr {
            match self {
                Self::UnaryOp { operand, .. } => operand,
                _ => unreachable!(),
            }
        }
    }
    impl<'a, U> Lambda<'a> for rspy_ast::ExprKind<U> {
        rspy_types!(U, Arguments, Expr);

        #[inline]
        fn args(&self) -> &Self::Arguments {
            match self {
                Self::Lambda { args, .. } => args,
                _ => unreachable!(),
            }
        }
        #[inline]
        fn body(&self) -> &Self::Expr {
            match self {
                Self::Lambda { body, .. } => body,
                _ => unreachable!(),
            }
        }
    }
    impl<'a, U> IfExp<'a> for rspy_ast::ExprKind<U> {
        rspy_types!(U, Expr);

        #[inline]
        fn test(&self) -> &Self::Expr {
            match self {
                Self::IfExp { test, .. } => test,
                _ => unreachable!(),
            }
        }
        #[inline]
        fn body(&self) -> &Self::Expr {
            match self {
                Self::IfExp { body, .. } => body,
                _ => unreachable!(),
            }
        }
        #[inline]
        fn orelse(&self) -> &Self::Expr {
            match self {
                Self::IfExp { orelse, .. } => orelse,
                _ => unreachable!(),
            }
        }
    }
    impl<'a, U> Dict<'a> for rspy_ast::ExprKind<U> {
        rspy_types!(U, Expr);

        #[inline]
        fn keys(&self) -> &[Self::Expr] {
            match self {
                Self::Dict { keys, .. } => keys,
                _ => unreachable!(),
            }
        }
        #[inline]
        fn values(&self) -> &[Self::Expr] {
            match self {
                Self::Dict { values, .. } => values,
                _ => unreachable!(),
            }
        }
    }
    impl<'a, U> Set<'a> for rspy_ast::ExprKind<U> {
        rspy_types!(U, Expr);

        #[inline]
        fn elts(&self) -> &[Self::Expr] {
            match self {
                Self::Set { elts } => elts,
                _ => unreachable!(),
            }
        }
    }
    impl<'a, U> ListComp<'a> for rspy_ast::ExprKind<U> {
        rspy_types!(U, Expr, Comprehension);

        #[inline]
        fn elt(&self) -> &Self::Expr {
            match self {
                Self::ListComp { elt, .. } => elt,
                _ => unreachable!(),
            }
        }
        #[inline]
        fn generators(&self) -> &[Self::Comprehension] {
            match self {
                Self::ListComp { generators, .. } => generators,
                _ => unreachable!(),
            }
        }
    }
    impl<'a, U> SetComp<'a> for rspy_ast::ExprKind<U> {
        rspy_types!(U, Expr, Comprehension);

        #[inline]
        fn elt(&self) -> &Self::Expr {
            match self {
                Self::SetComp { elt, .. } => elt,
                _ => unreachable!(),
            }
        }
        #[inline]
        fn generators(&self) -> &[Self::Comprehension] {
            match self {
                Self::SetComp { generators, .. } => generators,
                _ => unreachable!(),
            }
        }
    }
    impl<'a, U> DictComp<'a> for rspy_ast::ExprKind<U> {
        rspy_types!(U, Expr, Comprehension);

        #[inline]
        fn key(&self) -> &Self::Expr {
            match self {
                Self::DictComp { key, .. } => key,
                _ => unreachable!(),
            }
        }
        #[inline]
        fn value(&self) -> &Self::Expr {
            match self {
                Self::DictComp { value, .. } => value,
                _ => unreachable!(),
            }
        }
        #[inline]
        fn generators(&self) -> &[Self::Comprehension] {
            match self {
                Self::DictComp { generators, .. } => generators,
                _ => unreachable!(),
            }
        }
    }
    impl<'a, U> GeneratorExp<'a> for rspy_ast::ExprKind<U> {
        rspy_types!(U, Expr, Comprehension);

        #[inline]
        fn elt(&self) -> &Self::Expr {
            match self {
                Self::GeneratorExp { elt, .. } => elt,
                _ => unreachable!(),
            }
        }
        #[inline]
        fn generators(&self) -> &[Self::Comprehension] {
            match self {
                Self::GeneratorExp { generators, .. } => generators,
                _ => unreachable!(),
            }
        }
    }
    impl<'a, U> Await<'a> for rspy_ast::ExprKind<U> {
        rspy_types!(U, Expr);

        #[inline]
        fn value(&self) -> &Self::Expr {
            match self {
                Self::Await { value } => value,
                _ => unreachable!(),
            }
        }
    }
    impl<'a, U> Yield<'a> for rspy_ast::ExprKind<U> {
        rspy_types!(U, Expr);

        #[inline]
        fn value(&self) -> Option<&Self::Expr> {
            match self {
                Self::Yield { value } => value.as_deref(),
                _ => unreachable!(),
            }
        }
    }
    impl<'a, U> YieldFrom<'a> for rspy_ast::ExprKind<U> {
        rspy_types!(U, Expr);

        #[inline]
        fn value(&self) -> &Self::Expr {
            match self {
                Self::YieldFrom { value } => value,
                _ => unreachable!(),
            }
        }
    }
    impl From<rspy_ast::Cmpop> for Cmpop {
        fn from(val: rspy_ast::Cmpop) -> Self {
            match val {
                rspy_ast::Cmpop::Eq => Self::Eq,
                rspy_ast::Cmpop::NotEq => Self::NotEq,
                rspy_ast::Cmpop::Lt => Self::Lt,
                rspy_ast::Cmpop::LtE => Self::LtE,
                rspy_ast::Cmpop::Gt => Self::Gt,
                rspy_ast::Cmpop::GtE => Self::GtE,
                rspy_ast::Cmpop::Is => Self::Is,
                rspy_ast::Cmpop::IsNot => Self::IsNot,
                rspy_ast::Cmpop::In => Self::In,
                rspy_ast::Cmpop::NotIn => Self::NotIn,
            }
        }
    }
    impl<'a, U> Compare<'a> for rspy_ast::ExprKind<U> {
        rspy_types!(U, Expr);

        #[inline]
        fn left(&self) -> &Self::Expr {
            match self {
                Self::Compare { left, .. } => left,
                _ => unreachable!(),
            }
        }
        #[inline]
        fn ops(&self) -> Vec<Cmpop> {
            match self {
                Self::Compare { ops, .. } => ops.iter().cloned().map(Cmpop::from).collect(),
                _ => unreachable!(),
            }
        }
    }
    impl<'a, U> Call<'a> for rspy_ast::ExprKind<U> {
        rspy_types!(U, Expr, Keyword);

        #[inline]
        fn func(&self) -> &Self::Expr {
            match self {
                Self::Call { func, .. } => func,
                _ => unreachable!(),
            }
        }
        #[inline]
        fn args(&self) -> &[Self::Expr] {
            match self {
                Self::Call { args, .. } => args,
                _ => unreachable!(),
            }
        }
        #[inline]
        fn keywords(&self) -> &[Self::Keyword] {
            match self {
                Self::Call { keywords, .. } => keywords,
                _ => unreachable!(),
            }
        }
    }
    impl<'a, U> FormattedValue<'a> for rspy_ast::ExprKind<U> {
        rspy_types!(U, Expr);

        #[inline]
        fn value(&self) -> &Self::Expr {
            match self {
                Self::FormattedValue { value, .. } => value,
                _ => unreachable!(),
            }
        }
        #[inline]
        fn conversion(&self) -> usize {
            match self {
                Self::FormattedValue { conversion, .. } => *conversion,
                _ => unreachable!(),
            }
        }
        #[inline]
        fn format_spec(&self) -> Option<&Self::Expr> {
            match self {
                Self::FormattedValue { format_spec, .. } => format_spec.as_deref(),
                _ => unreachable!(),
            }
        }
    }
    impl<'a, U> JoinedStr<'a> for rspy_ast::ExprKind<U> {
        rspy_types!(U, Expr);

        #[inline]
        fn values(&self) -> &[Self::Expr] {
            match self {
                Self::JoinedStr { values } => values,
                _ => unreachable!(),
            }
        }
    }
    impl<'a, U> ConstantExpr<'a> for rspy_ast::ExprKind<U> {
        type Constant = rspy_ast::Constant;

        #[inline]
        fn value(&self) -> &<Self as ConstantExpr>::Constant {
            match self {
                Self::Constant { value, .. } => value,
                _ => unreachable!(),
            }
        }
        #[inline]
        fn kind(&self) -> Option<&str> {
            match self {
                Self::Constant { kind, .. } => kind.as_deref(),
                _ => unreachable!(),
            }
        }
    }
    impl From<rspy_ast::ExprContext> for ExprContext {
        fn from(val: rspy_ast::ExprContext) -> Self {
            match val {
                rspy_ast::ExprContext::Load => Self::Load,
                rspy_ast::ExprContext::Store => Self::Store,
                rspy_ast::ExprContext::Del => Self::Del,
            }
        }
    }
    impl<'a, U> Attribute<'a> for rspy_ast::ExprKind<U> {
        rspy_types!(U, Expr);

        #[inline]
        fn value(&self) -> &Self::Expr {
            match self {
                Self::Attribute { value, .. } => value,
                _ => unreachable!(),
            }
        }
        #[inline]
        fn attr(&self) -> &dyn Ident {
            match self {
                Self::Attribute { attr, .. } => attr,
                _ => unreachable!(),
            }
        }
        #[inline]
        fn ctx(&self) -> ExprContext {
            match self {
                Self::Attribute { ctx, .. } => ctx.clone().into(),
                _ => unreachable!(),
            }
        }
    }
    impl<'a, U> Subscript<'a> for rspy_ast::ExprKind<U> {
        rspy_types!(U, Expr);

        #[inline]
        fn value(&self) -> &Self::Expr {
            match self {
                Self::Subscript { value, .. } => value,
                _ => unreachable!(),
            }
        }
        #[inline]
        fn slice(&self) -> &Self::Expr {
            match self {
                Self::Subscript { slice, .. } => slice,
                _ => unreachable!(),
            }
        }
        #[inline]
        fn ctx(&self) -> ExprContext {
            match self {
                Self::Subscript { ctx, .. } => ctx.clone().into(),
                _ => unreachable!(),
            }
        }
    }
    impl<'a, U> Starred<'a> for rspy_ast::ExprKind<U> {
        rspy_types!(U, Expr);

        #[inline]
        fn value(&self) -> &Self::Expr {
            match self {
                Self::Starred { value, .. } => value,
                _ => unreachable!(),
            }
        }
        #[inline]
        fn ctx(&self) -> ExprContext {
            match self {
                Self::Starred { ctx, .. } => ctx.clone().into(),
                _ => unreachable!(),
            }
        }
    }
    impl<U> Name for rspy_ast::ExprKind<U> {
        #[inline]
        fn id(&self) -> &dyn Ident {
            match self {
                Self::Name { id, .. } => id,
                _ => unreachable!(),
            }
        }
        #[inline]
        fn ctx(&self) -> ExprContext {
            match self {
                Self::Name { ctx, .. } => ctx.clone().into(),
                _ => unreachable!(),
            }
        }
    }
    impl<'a, U> List<'a> for rspy_ast::ExprKind<U> {
        rspy_types!(U, Expr);

        #[inline]
        fn elts(&self) -> &[Self::Expr] {
            match self {
                Self::List { elts, .. } => elts,
                _ => unreachable!(),
            }
        }
        #[inline]
        fn ctx(&self) -> ExprContext {
            match self {
                Self::List { ctx, .. } => ctx.clone().into(),
                _ => unreachable!(),
            }
        }
    }
    impl<'a, U> Tuple<'a> for rspy_ast::ExprKind<U> {
        rspy_types!(U, Expr);

        #[inline]
        fn elts(&self) -> &[Self::Expr] {
            match self {
                Self::Tuple { elts, .. } => elts,
                _ => unreachable!(),
            }
        }
        #[inline]
        fn ctx(&self) -> ExprContext {
            match self {
                Self::Tuple { ctx, .. } => ctx.clone().into(),
                _ => unreachable!(),
            }
        }
    }
    impl<'a, U> Slice<'a> for rspy_ast::ExprKind<U> {
        rspy_types!(U, Expr);

        #[inline]
        fn lower(&self) -> Option<&Self::Expr> {
            match self {
                Self::Slice { lower, .. } => lower.as_deref(),
                _ => unreachable!(),
            }
        }
        #[inline]
        fn upper(&self) -> Option<&Self::Expr> {
            match self {
                Self::Slice { upper, .. } => upper.as_deref(),
                _ => unreachable!(),
            }
        }
        #[inline]
        fn step(&self) -> Option<&Self::Expr> {
            match self {
                Self::Slice { step, .. } => step.as_deref(),
                _ => unreachable!(),
            }
        }
    }
    impl<'a, U> Expr<'a> for rspy_ast::Expr<U> {
        type BoolOp = rspy_ast::ExprKind<U>;
        type NamedExpr = rspy_ast::ExprKind<U>;
        type BinOp = rspy_ast::ExprKind<U>;
        type UnaryOp = rspy_ast::ExprKind<U>;
        type Lambda = rspy_ast::ExprKind<U>;
        type IfExp = rspy_ast::ExprKind<U>;
        type Dict = rspy_ast::ExprKind<U>;
        type Set = rspy_ast::ExprKind<U>;
        type ListComp = rspy_ast::ExprKind<U>;
        type SetComp = rspy_ast::ExprKind<U>;
        type DictComp = rspy_ast::ExprKind<U>;
        type GeneratorExp = rspy_ast::ExprKind<U>;
        type Await = rspy_ast::ExprKind<U>;
        type Yield = rspy_ast::ExprKind<U>;
        type YieldFrom = rspy_ast::ExprKind<U>;
        type Compare = rspy_ast::ExprKind<U>;
        type Call = rspy_ast::ExprKind<U>;
        type FormattedValue = rspy_ast::ExprKind<U>;
        type JoinedStr = rspy_ast::ExprKind<U>;
        type ConstantExpr = rspy_ast::ExprKind<U>;
        type Attribute = rspy_ast::ExprKind<U>;
        type Subscript = rspy_ast::ExprKind<U>;
        type Starred = rspy_ast::ExprKind<U>;
        type Name = rspy_ast::ExprKind<U>;
        type List = rspy_ast::ExprKind<U>;
        type Tuple = rspy_ast::ExprKind<U>;
        type Slice = rspy_ast::ExprKind<U>;
        #[inline]
        fn expr(
            &'a self,
        ) -> ExprKind<
            'a,
            Self::BoolOp,
            Self::NamedExpr,
            Self::BinOp,
            Self::UnaryOp,
            Self::Lambda,
            Self::IfExp,
            Self::Dict,
            Self::Set,
            Self::ListComp,
            Self::SetComp,
            Self::DictComp,
            Self::GeneratorExp,
            Self::Await,
            Self::Yield,
            Self::YieldFrom,
            Self::Compare,
            Self::Call,
            Self::FormattedValue,
            Self::JoinedStr,
            Self::ConstantExpr,
            Self::Attribute,
            Self::Subscript,
            Self::Starred,
            Self::Name,
            Self::List,
            Self::Tuple,
            Self::Slice,
        > {
            match &self.node {
                rspy_ast::ExprKind::BoolOp { .. } => ExprKind::BoolOp(&self.node),
                rspy_ast::ExprKind::NamedExpr { .. } => ExprKind::NamedExpr(&self.node),
                rspy_ast::ExprKind::BinOp { .. } => ExprKind::BinOp(&self.node),
                rspy_ast::ExprKind::UnaryOp { .. } => ExprKind::UnaryOp(&self.node),
                rspy_ast::ExprKind::Lambda { .. } => ExprKind::Lambda(&self.node),
                rspy_ast::ExprKind::IfExp { .. } => ExprKind::IfExp(&self.node),
                rspy_ast::ExprKind::Dict { .. } => ExprKind::Dict(&self.node),
                rspy_ast::ExprKind::Set { .. } => ExprKind::Set(&self.node),
                rspy_ast::ExprKind::ListComp { .. } => ExprKind::ListComp(&self.node),
                rspy_ast::ExprKind::SetComp { .. } => ExprKind::SetComp(&self.node),
                rspy_ast::ExprKind::DictComp { .. } => ExprKind::DictComp(&self.node),
                rspy_ast::ExprKind::GeneratorExp { .. } => ExprKind::GeneratorExp(&self.node),
                rspy_ast::ExprKind::Await { .. } => ExprKind::Await(&self.node),
                rspy_ast::ExprKind::Yield { .. } => ExprKind::Yield(&self.node),
                rspy_ast::ExprKind::YieldFrom { .. } => ExprKind::YieldFrom(&self.node),
                rspy_ast::ExprKind::Compare { .. } => ExprKind::Compare(&self.node),
                rspy_ast::ExprKind::Call { .. } => ExprKind::Call(&self.node),
                rspy_ast::ExprKind::FormattedValue { .. } => ExprKind::FormattedValue(&self.node),
                rspy_ast::ExprKind::JoinedStr { .. } => ExprKind::JoinedStr(&self.node),
                rspy_ast::ExprKind::Constant { .. } => ExprKind::ConstantExpr(&self.node),
                rspy_ast::ExprKind::Attribute { .. } => ExprKind::Attribute(&self.node),
                rspy_ast::ExprKind::Subscript { .. } => ExprKind::Subscript(&self.node),
                rspy_ast::ExprKind::Starred { .. } => ExprKind::Starred(&self.node),
                rspy_ast::ExprKind::Name { .. } => ExprKind::Name(&self.node),
                rspy_ast::ExprKind::List { .. } => ExprKind::List(&self.node),
                rspy_ast::ExprKind::Tuple { .. } => ExprKind::Tuple(&self.node),
                rspy_ast::ExprKind::Slice { .. } => ExprKind::Slice(&self.node),
            }
        }
    }

    impl<'a, U> ExceptHandler<'a> for rspy_ast::Excepthandler<U> {
        rspy_types!(U, Expr, Stmt);

        #[inline]
        fn type_(&self) -> Option<&Self::Expr> {
            match &self.node {
                rspy_ast::ExcepthandlerKind::ExceptHandler { type_, .. } => type_.as_deref(),
            }
        }
        #[inline]
        fn name(&self) -> Option<&dyn Ident> {
            match &self.node {
                rspy_ast::ExcepthandlerKind::ExceptHandler { name, .. } => {
                    name.as_ref().map(|x| x as &dyn Ident)
                }
            }
        }
        #[inline]
        fn body(&self) -> &[Self::Stmt] {
            match &self.node {
                rspy_ast::ExcepthandlerKind::ExceptHandler { body, .. } => body,
            }
        }
    }
    impl<'a, U> MatchValue<'a> for rspy_ast::PatternKind<U> {
        rspy_types!(U, Expr);

        #[inline]
        fn value(&self) -> &Self::Expr {
            match self {
                Self::MatchValue { value } => value,
                _ => unreachable!(),
            }
        }
    }
    impl<'a, U> MatchSingleton<'a> for rspy_ast::PatternKind<U> {
        type Constant = rspy_ast::Constant;

        #[inline]
        fn value(&self) -> &Self::Constant {
            match self {
                Self::MatchSingleton { value } => value,
                _ => unreachable!(),
            }
        }
    }
    impl<'a, U> MatchSequence<'a> for rspy_ast::PatternKind<U> {
        rspy_types!(U, Pattern);

        #[inline]
        fn patterns(&self) -> &[Self::Pattern] {
            match self {
                Self::MatchSequence { patterns } => patterns,
                _ => unreachable!(),
            }
        }
    }
    impl<'a, U> MatchMapping<'a> for rspy_ast::PatternKind<U> {
        rspy_types!(U, Expr, Pattern);

        #[inline]
        fn keys(&self) -> &[Self::Expr] {
            match self {
                Self::MatchMapping { keys, .. } => keys,
                _ => unreachable!(),
            }
        }
        #[inline]
        fn patterns(&self) -> &[Self::Pattern] {
            match self {
                Self::MatchMapping { patterns, .. } => patterns,
                _ => unreachable!(),
            }
        }
        #[inline]
        fn rest(&self) -> Option<&dyn Ident> {
            match self {
                Self::MatchMapping { rest, .. } => rest.as_ref().map(|x| x as &dyn Ident),
                _ => unreachable!(),
            }
        }
    }
    impl<'a, U> MatchClass<'a> for rspy_ast::PatternKind<U> {
        rspy_types!(U, Expr, Pattern);
        type Ident = String;

        #[inline]
        fn cls(&self) -> &Self::Expr {
            match self {
                Self::MatchClass { cls, .. } => cls,
                _ => unreachable!(),
            }
        }
        #[inline]
        fn patterns(&self) -> &[Self::Pattern] {
            match self {
                Self::MatchClass { patterns, .. } => patterns,
                _ => unreachable!(),
            }
        }
        #[inline]
        fn kwd_attrs(&self) -> &[Self::Ident] {
            match self {
                Self::MatchClass { kwd_attrs, .. } => kwd_attrs,
                _ => unreachable!(),
            }
        }
        #[inline]
        fn kwd_patterns(&self) -> &[Self::Pattern] {
            match self {
                Self::MatchClass { kwd_patterns, .. } => kwd_patterns,
                _ => unreachable!(),
            }
        }
    }
    impl<U> MatchStar for rspy_ast::PatternKind<U> {
        #[inline]
        fn name(&self) -> Option<&dyn Ident> {
            match self {
                Self::MatchStar { name } => name.as_ref().map(|x| x as &dyn Ident),
                _ => unreachable!(),
            }
        }
    }
    impl<'a, U> MatchAs<'a> for rspy_ast::PatternKind<U> {
        rspy_types!(U, Pattern);

        #[inline]
        fn pattern(&self) -> Option<&Self::Pattern> {
            match self {
                Self::MatchAs { pattern, .. } => pattern.as_deref(),
                _ => unreachable!(),
            }
        }
        #[inline]
        fn name(&self) -> Option<&dyn Ident> {
            match self {
                Self::MatchAs { name, .. } => name.as_ref().map(|x| x as &dyn Ident),
                _ => unreachable!(),
            }
        }
    }
    impl<'a, U> MatchOr<'a> for rspy_ast::PatternKind<U> {
        rspy_types!(U, Pattern);

        #[inline]
        fn patterns(&self) -> &[Self::Pattern] {
            match self {
                Self::MatchOr { patterns } => patterns,
                _ => unreachable!(),
            }
        }
    }
    impl<'a, U> Pattern<'a> for rspy_ast::Pattern<U> {
        type MatchValue = rspy_ast::PatternKind<U>;
        type MatchSingleton = rspy_ast::PatternKind<U>;
        type MatchSequence = rspy_ast::PatternKind<U>;
        type MatchMapping = rspy_ast::PatternKind<U>;
        type MatchClass = rspy_ast::PatternKind<U>;
        type MatchStar = rspy_ast::PatternKind<U>;
        type MatchAs = rspy_ast::PatternKind<U>;
        type MatchOr = rspy_ast::PatternKind<U>;
        #[inline]
        fn pattern(
            &'a self,
        ) -> PatternKind<
            'a,
            Self::MatchValue,
            Self::MatchSingleton,
            Self::MatchSequence,
            Self::MatchMapping,
            Self::MatchClass,
            Self::MatchStar,
            Self::MatchAs,
            Self::MatchOr,
        > {
            match &self.node {
                rspy_ast::PatternKind::MatchValue { .. } => PatternKind::MatchValue(&self.node),
                rspy_ast::PatternKind::MatchSingleton { .. } => {
                    PatternKind::MatchSingleton(&self.node)
                }
                rspy_ast::PatternKind::MatchSequence { .. } => {
                    PatternKind::MatchSequence(&self.node)
                }
                rspy_ast::PatternKind::MatchMapping { .. } => PatternKind::MatchMapping(&self.node),
                rspy_ast::PatternKind::MatchClass { .. } => PatternKind::MatchClass(&self.node),
                rspy_ast::PatternKind::MatchStar { .. } => PatternKind::MatchStar(&self.node),
                rspy_ast::PatternKind::MatchAs { .. } => PatternKind::MatchAs(&self.node),
                rspy_ast::PatternKind::MatchOr { .. } => PatternKind::MatchOr(&self.node),
            }
        }
    }
    impl<'a, U> Withitem<'a> for rspy_ast::Withitem<U> {
        type Expr = rspy_ast::Expr<U>;
        #[inline]
        fn context_expr(&self) -> &Self::Expr {
            &self.context_expr
        }
        #[inline]
        fn optional_vars(&self) -> Option<&Self::Expr> {
            self.optional_vars.as_deref()
        }
    }
    impl<'a, U> MatchCase<'a> for rspy_ast::MatchCase<U> {
        rspy_types!(U, Pattern, Expr, Stmt);

        #[inline]
        fn pattern(&self) -> &Self::Pattern {
            &self.pattern
        }
        #[inline]
        fn guard(&self) -> Option<&Self::Expr> {
            self.guard.as_deref()
        }
        #[inline]
        fn body(&self) -> &[Self::Stmt] {
            &self.body
        }
    }
    impl<'a, U> FunctionDef<'a> for rspy_ast::StmtKind<U> {
        rspy_types!(U, Arguments, Expr, Stmt);

        #[inline]
        fn name(&self) -> &dyn Ident {
            match self {
                Self::FunctionDef { name, .. } => name,
                _ => unreachable!(),
            }
        }
        #[inline]
        fn args(&self) -> &Self::Arguments {
            match self {
                Self::FunctionDef { args, .. } => args,
                _ => unreachable!(),
            }
        }
        #[inline]
        fn body(&self) -> &[Self::Stmt] {
            match self {
                Self::FunctionDef { body, .. } => body,
                _ => unreachable!(),
            }
        }
        #[inline]
        fn decorator_list(&self) -> &[<Self as FunctionDef>::Expr] {
            match self {
                Self::FunctionDef { decorator_list, .. } => decorator_list,
                _ => unreachable!(),
            }
        }
        #[inline]
        fn returns(&self) -> Option<&<Self as FunctionDef>::Expr> {
            match self {
                Self::FunctionDef { returns, .. } => returns.as_deref(),
                _ => unreachable!(),
            }
        }
        #[inline]
        fn type_comment(&self) -> Option<&str> {
            match self {
                Self::FunctionDef { type_comment, .. } => type_comment.as_deref(),
                _ => unreachable!(),
            }
        }
    }
    impl<'a, U> AsyncFunctionDef<'a> for rspy_ast::StmtKind<U> {
        rspy_types!(U, Arguments, Expr, Stmt);

        #[inline]
        fn name(&self) -> &dyn Ident {
            match self {
                Self::AsyncFunctionDef { name, .. } => name,
                _ => unreachable!(),
            }
        }
        #[inline]
        fn args(&self) -> &Self::Arguments {
            match self {
                Self::AsyncFunctionDef { args, .. } => args,
                _ => unreachable!(),
            }
        }
        #[inline]
        fn body(&self) -> &[Self::Stmt] {
            match self {
                Self::AsyncFunctionDef { body, .. } => body,
                _ => unreachable!(),
            }
        }
        #[inline]
        fn decorator_list(&self) -> &[<Self as AsyncFunctionDef>::Expr] {
            match self {
                Self::AsyncFunctionDef { decorator_list, .. } => decorator_list,
                _ => unreachable!(),
            }
        }
        #[inline]
        fn returns(&self) -> Option<&<Self as AsyncFunctionDef>::Expr> {
            match self {
                Self::AsyncFunctionDef { returns, .. } => returns.as_deref(),
                _ => unreachable!(),
            }
        }
        #[inline]
        fn type_comment(&self) -> Option<&str> {
            match self {
                Self::AsyncFunctionDef { type_comment, .. } => type_comment.as_deref(),
                _ => unreachable!(),
            }
        }
    }
    impl<'a, U> ClassDef<'a> for rspy_ast::StmtKind<U> {
        rspy_types!(U, Keyword, Expr, Stmt);

        #[inline]
        fn name(&self) -> &dyn Ident {
            match self {
                Self::ClassDef { name, .. } => name,
                _ => unreachable!(),
            }
        }
        #[inline]
        fn bases(&self) -> &[<Self as ClassDef>::Expr] {
            match self {
                Self::ClassDef { bases, .. } => bases,
                _ => unreachable!(),
            }
        }
        #[inline]
        fn keywords(&self) -> &[Self::Keyword] {
            match self {
                Self::ClassDef { keywords, .. } => keywords,
                _ => unreachable!(),
            }
        }
        #[inline]
        fn body(&self) -> &[Self::Stmt] {
            match self {
                Self::ClassDef { body, .. } => body,
                _ => unreachable!(),
            }
        }
        #[inline]
        fn decorator_list(&self) -> &[<Self as ClassDef>::Expr] {
            match self {
                Self::ClassDef { decorator_list, .. } => decorator_list,
                _ => unreachable!(),
            }
        }
    }
    impl<'a, U> Return<'a> for rspy_ast::StmtKind<U> {
        rspy_types!(U, Expr);

        #[inline]
        fn value(&self) -> Option<&<Self as Return>::Expr> {
            match self {
                Self::Return { value } => value.as_deref(),
                _ => unreachable!(),
            }
        }
    }
    impl<'a, U> Delete<'a> for rspy_ast::StmtKind<U> {
        rspy_types!(U, Expr);

        #[inline]
        fn targets(&self) -> &[<Self as Delete>::Expr] {
            match self {
                Self::Delete { targets } => targets,
                _ => unreachable!(),
            }
        }
    }
    impl<'a, U> Assign<'a> for rspy_ast::StmtKind<U> {
        rspy_types!(U, Expr);

        #[inline]
        fn targets(&self) -> &[<Self as Assign>::Expr] {
            match self {
                Self::Assign { targets, .. } => targets,
                _ => unreachable!(),
            }
        }
        #[inline]
        fn value(&self) -> &<Self as Assign>::Expr {
            match self {
                Self::Assign { value, .. } => value,
                _ => unreachable!(),
            }
        }
        #[inline]
        fn type_comment(&self) -> Option<&str> {
            match self {
                Self::Assign { type_comment, .. } => type_comment.as_deref(),
                _ => unreachable!(),
            }
        }
    }
    impl<'a, U> AugAssign<'a> for rspy_ast::StmtKind<U> {
        rspy_types!(U, Expr);

        #[inline]
        fn target(&self) -> &<Self as AugAssign>::Expr {
            match self {
                Self::AugAssign { target, .. } => target,
                _ => unreachable!(),
            }
        }
        #[inline]
        fn op(&self) -> Operator {
            match self {
                Self::AugAssign { op, .. } => op.clone().into(),
                _ => unreachable!(),
            }
        }
        #[inline]
        fn value(&self) -> &<Self as AugAssign>::Expr {
            match self {
                Self::AugAssign { value, .. } => value,
                _ => unreachable!(),
            }
        }
    }
    impl<'a, U> AnnAssign<'a> for rspy_ast::StmtKind<U> {
        rspy_types!(U, Expr);
        #[inline]
        fn target(&self) -> &<Self as AnnAssign>::Expr {
            match self {
                Self::AnnAssign { target, .. } => target,
                _ => unreachable!(),
            }
        }
        #[inline]
        fn annotation(&self) -> &<Self as AnnAssign>::Expr {
            match self {
                Self::AnnAssign { annotation, .. } => annotation,
                _ => unreachable!(),
            }
        }
        #[inline]
        fn value(&self) -> Option<&<Self as AnnAssign>::Expr> {
            match self {
                Self::AnnAssign { value, .. } => value.as_deref(),
                _ => unreachable!(),
            }
        }
        #[inline]
        fn simple(&self) -> usize {
            match self {
                Self::AnnAssign { simple, .. } => *simple,
                _ => unreachable!(),
            }
        }
    }
    impl<'a, U> For<'a> for rspy_ast::StmtKind<U> {
        rspy_types!(U, Expr, Stmt);

        #[inline]
        fn target(&self) -> &<Self as For>::Expr {
            match self {
                Self::For { target, .. } => target,
                _ => unreachable!(),
            }
        }
        #[inline]
        fn iter(&self) -> &<Self as For>::Expr {
            match self {
                Self::For { iter, .. } => iter,
                _ => unreachable!(),
            }
        }
        #[inline]
        fn body(&self) -> &[Self::Stmt] {
            match self {
                Self::For { body, .. } => body,
                _ => unreachable!(),
            }
        }
        #[inline]
        fn orelse(&self) -> &[Self::Stmt] {
            match self {
                Self::For { orelse, .. } => orelse,
                _ => unreachable!(),
            }
        }
        #[inline]
        fn type_comment(&self) -> Option<&str> {
            match self {
                Self::For { type_comment, .. } => type_comment.as_deref(),
                _ => unreachable!(),
            }
        }
    }
    impl<'a, U> AsyncFor<'a> for rspy_ast::StmtKind<U> {
        rspy_types!(U, Expr, Stmt);

        #[inline]
        fn target(&self) -> &<Self as AsyncFor>::Expr {
            match self {
                Self::AsyncFor { target, .. } => target,
                _ => unreachable!(),
            }
        }
        #[inline]
        fn iter(&self) -> &<Self as AsyncFor>::Expr {
            match self {
                Self::AsyncFor { iter, .. } => iter,
                _ => unreachable!(),
            }
        }
        #[inline]
        fn body(&self) -> &[Self::Stmt] {
            match self {
                Self::AsyncFor { body, .. } => body,
                _ => unreachable!(),
            }
        }
        #[inline]
        fn orelse(&self) -> &[Self::Stmt] {
            match self {
                Self::AsyncFor { orelse, .. } => orelse,
                _ => unreachable!(),
            }
        }
        #[inline]
        fn type_comment(&self) -> Option<&str> {
            match self {
                Self::AsyncFor { type_comment, .. } => type_comment.as_deref(),
                _ => unreachable!(),
            }
        }
    }
    impl<'a, U> While<'a> for rspy_ast::StmtKind<U> {
        rspy_types!(U, Stmt, Expr);

        #[inline]
        fn test(&self) -> &<Self as While>::Expr {
            match self {
                Self::While { test, .. } => test,
                _ => unreachable!(),
            }
        }
        #[inline]
        fn body(&self) -> &[Self::Stmt] {
            match self {
                Self::While { body, .. } => body,
                _ => unreachable!(),
            }
        }
        #[inline]
        fn orelse(&self) -> &[Self::Stmt] {
            match self {
                Self::While { orelse, .. } => orelse,
                _ => unreachable!(),
            }
        }
    }
    impl<'a, U> If<'a> for rspy_ast::StmtKind<U> {
        rspy_types!(U, Stmt, Expr);

        #[inline]
        fn test(&self) -> &<Self as If>::Expr {
            match self {
                Self::If { test, .. } => test,
                _ => unreachable!(),
            }
        }
        #[inline]
        fn body(&self) -> &[Self::Stmt] {
            match self {
                Self::If { body, .. } => body,
                _ => unreachable!(),
            }
        }
        #[inline]
        fn orelse(&self) -> &[Self::Stmt] {
            match self {
                Self::If { orelse, .. } => orelse,
                _ => unreachable!(),
            }
        }
    }
    impl<'a, U> With<'a> for rspy_ast::StmtKind<U> {
        rspy_types!(U, Withitem, Stmt);

        #[inline]
        fn items(&self) -> &[Self::Withitem] {
            match self {
                Self::With { items, .. } => items,
                _ => unreachable!(),
            }
        }
        #[inline]
        fn body(&self) -> &[Self::Stmt] {
            match self {
                Self::With { body, .. } => body,
                _ => unreachable!(),
            }
        }
        #[inline]
        fn type_comment(&self) -> Option<&str> {
            match self {
                Self::With { type_comment, .. } => type_comment.as_deref(),
                _ => unreachable!(),
            }
        }
    }
    impl<'a, U> AsyncWith<'a> for rspy_ast::StmtKind<U> {
        rspy_types!(U, Withitem, Stmt);

        #[inline]
        fn items(&self) -> &[Self::Withitem] {
            match self {
                Self::AsyncWith { items, .. } => items,
                _ => unreachable!(),
            }
        }
        #[inline]
        fn body(&self) -> &[Self::Stmt] {
            match self {
                Self::AsyncWith { body, .. } => body,
                _ => unreachable!(),
            }
        }
        #[inline]
        fn type_comment(&self) -> Option<&str> {
            match self {
                Self::AsyncWith { type_comment, .. } => type_comment.as_deref(),
                _ => unreachable!(),
            }
        }
    }
    impl<'a, U> Match<'a> for rspy_ast::StmtKind<U> {
        rspy_types!(U, MatchCase, Expr);

        #[inline]
        fn subject(&self) -> &<Self as Match>::Expr {
            match self {
                Self::Match { subject, .. } => subject,
                _ => unreachable!(),
            }
        }
        #[inline]
        fn cases(&self) -> &[Self::MatchCase] {
            match self {
                Self::Match { cases, .. } => cases,
                _ => unreachable!(),
            }
        }
    }
    impl<'a, U> Raise<'a> for rspy_ast::StmtKind<U> {
        rspy_types!(U, Expr);

        #[inline]
        fn exc(&self) -> Option<&<Self as Raise>::Expr> {
            match self {
                Self::Raise { exc, .. } => exc.as_deref(),
                _ => unreachable!(),
            }
        }
        #[inline]
        fn cause(&self) -> Option<&<Self as Raise>::Expr> {
            match self {
                Self::Raise { cause, .. } => cause.as_deref(),
                _ => unreachable!(),
            }
        }
    }
    impl<'a, U> Try<'a> for rspy_ast::StmtKind<U> {
        rspy_types!(U, Stmt);
        type ExceptHandler = rspy_ast::Excepthandler<U>;

        #[inline]
        fn body(&self) -> &[Self::Stmt] {
            match self {
                Self::Try { body, .. } => body,
                _ => unreachable!(),
            }
        }
        #[inline]
        fn handlers(&self) -> &[Self::ExceptHandler] {
            match self {
                Self::Try { handlers, .. } => handlers,
                _ => unreachable!(),
            }
        }
        #[inline]
        fn orelse(&self) -> &[Self::Stmt] {
            match self {
                Self::Try { orelse, .. } => orelse,
                _ => unreachable!(),
            }
        }
        #[inline]
        fn finalbody(&self) -> &[Self::Stmt] {
            match self {
                Self::Try { finalbody, .. } => finalbody,
                _ => unreachable!(),
            }
        }
    }
    impl<'a, U> Assert<'a> for rspy_ast::StmtKind<U> {
        rspy_types!(U, Expr);

        #[inline]
        fn test(&self) -> &<Self as Assert>::Expr {
            match self {
                Self::Assert { test, .. } => test,
                _ => unreachable!(),
            }
        }
        #[inline]
        fn msg(&self) -> Option<&<Self as Assert>::Expr> {
            match self {
                Self::Assert { msg, .. } => msg.as_deref(),
                _ => unreachable!(),
            }
        }
    }
    impl<U> Import for rspy_ast::StmtKind<U> {
        rspy_types!(U, Alias);

        #[inline]
        fn names(&self) -> &[Self::Alias] {
            match self {
                Self::Import { names } => names,
                _ => unreachable!(),
            }
        }
    }
    impl<U> ImportFrom for rspy_ast::StmtKind<U> {
        rspy_types!(U, Alias);

        #[inline]
        fn module(&self) -> Option<&dyn Ident> {
            match self {
                Self::ImportFrom { module, .. } => module.as_ref().map(|x| x as &dyn Ident),
                _ => unreachable!(),
            }
        }
        #[inline]
        fn names(&self) -> &[Self::Alias] {
            match self {
                Self::ImportFrom { names, .. } => names,
                _ => unreachable!(),
            }
        }
        #[inline]
        fn level(&self) -> Option<usize> {
            match self {
                Self::ImportFrom { level, .. } => *level,
                _ => unreachable!(),
            }
        }
    }
    impl<U> Global for rspy_ast::StmtKind<U> {
        type Ident = String;

        #[inline]
        fn names(&self) -> &[Self::Ident] {
            match self {
                Self::Global { names } => names,
                _ => unreachable!(),
            }
        }
    }
    impl<U> Nonlocal for rspy_ast::StmtKind<U> {
        type Ident = String;

        #[inline]
        fn names(&self) -> &[Self::Ident] {
            match self {
                Self::Nonlocal { names } => names,
                _ => unreachable!(),
            }
        }
    }
    impl<'a, U> Stmt<'a> for rspy_ast::Stmt<U> {
        type FunctionDef = rspy_ast::StmtKind<U>;
        type AsyncFunctionDef = rspy_ast::StmtKind<U>;
        type ClassDef = rspy_ast::StmtKind<U>;
        type Return = rspy_ast::StmtKind<U>;
        type Delete = rspy_ast::StmtKind<U>;
        type Assign = rspy_ast::StmtKind<U>;
        type AugAssign = rspy_ast::StmtKind<U>;
        type AnnAssign = rspy_ast::StmtKind<U>;
        type For = rspy_ast::StmtKind<U>;
        type AsyncFor = rspy_ast::StmtKind<U>;
        type While = rspy_ast::StmtKind<U>;
        type If = rspy_ast::StmtKind<U>;
        type With = rspy_ast::StmtKind<U>;
        type AsyncWith = rspy_ast::StmtKind<U>;
        type Match = rspy_ast::StmtKind<U>;
        type Raise = rspy_ast::StmtKind<U>;
        type Try = rspy_ast::StmtKind<U>;
        type Assert = rspy_ast::StmtKind<U>;
        type Import = rspy_ast::StmtKind<U>;
        type ImportFrom = rspy_ast::StmtKind<U>;
        type Global = rspy_ast::StmtKind<U>;
        type Nonlocal = rspy_ast::StmtKind<U>;
        type Expr = rspy_ast::Expr<U>;

        fn stmt(
            &'a self,
        ) -> StmtKind<
            'a,
            Self::FunctionDef,
            Self::AsyncFunctionDef,
            Self::ClassDef,
            Self::Return,
            Self::Delete,
            Self::Assign,
            Self::AugAssign,
            Self::AnnAssign,
            Self::For,
            Self::AsyncFor,
            Self::While,
            Self::If,
            Self::With,
            Self::AsyncWith,
            Self::Match,
            Self::Raise,
            Self::Try,
            Self::Assert,
            Self::Import,
            Self::ImportFrom,
            Self::Global,
            Self::Nonlocal,
            Self::Expr,
        > {
            match &self.node {
                rspy_ast::StmtKind::FunctionDef { .. } => StmtKind::FunctionDef(&self.node),
                rspy_ast::StmtKind::AsyncFunctionDef { .. } => {
                    StmtKind::AsyncFunctionDef(&self.node)
                }
                rspy_ast::StmtKind::ClassDef { .. } => StmtKind::ClassDef(&self.node),
                rspy_ast::StmtKind::Return { .. } => StmtKind::Return(&self.node),
                rspy_ast::StmtKind::Delete { .. } => StmtKind::Delete(&self.node),
                rspy_ast::StmtKind::Assign { .. } => StmtKind::Assign(&self.node),
                rspy_ast::StmtKind::AugAssign { .. } => StmtKind::AugAssign(&self.node),
                rspy_ast::StmtKind::AnnAssign { .. } => StmtKind::AnnAssign(&self.node),
                rspy_ast::StmtKind::For { .. } => StmtKind::For(&self.node),
                rspy_ast::StmtKind::AsyncFor { .. } => StmtKind::AsyncFor(&self.node),
                rspy_ast::StmtKind::While { .. } => StmtKind::While(&self.node),
                rspy_ast::StmtKind::If { .. } => StmtKind::If(&self.node),
                rspy_ast::StmtKind::With { .. } => StmtKind::With(&self.node),
                rspy_ast::StmtKind::AsyncWith { .. } => StmtKind::AsyncWith(&self.node),
                rspy_ast::StmtKind::Match { .. } => StmtKind::Match(&self.node),
                rspy_ast::StmtKind::Raise { .. } => StmtKind::Raise(&self.node),
                rspy_ast::StmtKind::Try { .. } => StmtKind::Try(&self.node),
                rspy_ast::StmtKind::Assert { .. } => StmtKind::Assert(&self.node),
                rspy_ast::StmtKind::Import { .. } => StmtKind::Import(&self.node),
                rspy_ast::StmtKind::ImportFrom { .. } => StmtKind::ImportFrom(&self.node),
                rspy_ast::StmtKind::Global { .. } => StmtKind::Global(&self.node),
                rspy_ast::StmtKind::Nonlocal { .. } => StmtKind::Nonlocal(&self.node),
                rspy_ast::StmtKind::Expr { value } => StmtKind::Expr(value),
                rspy_ast::StmtKind::Pass => StmtKind::Pass,
                rspy_ast::StmtKind::Break => StmtKind::Break,
                rspy_ast::StmtKind::Continue => StmtKind::Continue,
            }
        }
    }

    impl<'a, U> Ast<'a> for rspy_ast::Suite<U> {
        type Ident = String;
        type Alias = rspy_ast::Alias<U>;
        type Arg = rspy_ast::Arg<U>;
        type Arguments = rspy_ast::Arguments<U>;
        type Keyword = rspy_ast::Keyword<U>;
        type BigInt = RspyBigInt;
        type Constant = rspy_ast::Constant;
        type Comprehension = rspy_ast::Comprehension<U>;
        type BoolOp = rspy_ast::ExprKind<U>;
        type NamedExpr = rspy_ast::ExprKind<U>;
        type BinOp = rspy_ast::ExprKind<U>;
        type UnaryOp = rspy_ast::ExprKind<U>;
        type Lambda = rspy_ast::ExprKind<U>;
        type IfExp = rspy_ast::ExprKind<U>;
        type Dict = rspy_ast::ExprKind<U>;
        type Set = rspy_ast::ExprKind<U>;
        type ListComp = rspy_ast::ExprKind<U>;
        type SetComp = rspy_ast::ExprKind<U>;
        type DictComp = rspy_ast::ExprKind<U>;
        type GeneratorExp = rspy_ast::ExprKind<U>;
        type Await = rspy_ast::ExprKind<U>;
        type Yield = rspy_ast::ExprKind<U>;
        type YieldFrom = rspy_ast::ExprKind<U>;
        type Compare = rspy_ast::ExprKind<U>;
        type Call = rspy_ast::ExprKind<U>;
        type FormattedValue = rspy_ast::ExprKind<U>;
        type JoinedStr = rspy_ast::ExprKind<U>;
        type ConstantExpr = rspy_ast::ExprKind<U>;
        type Attribute = rspy_ast::ExprKind<U>;
        type Subscript = rspy_ast::ExprKind<U>;
        type Starred = rspy_ast::ExprKind<U>;
        type Name = rspy_ast::ExprKind<U>;
        type List = rspy_ast::ExprKind<U>;
        type Tuple = rspy_ast::ExprKind<U>;
        type Slice = rspy_ast::ExprKind<U>;
        type Expr = rspy_ast::Expr<U>;
        type ExceptHandler = rspy_ast::Excepthandler<U>;
        type MatchValue = rspy_ast::PatternKind<U>;
        type MatchSingleton = rspy_ast::PatternKind<U>;
        type MatchSequence = rspy_ast::PatternKind<U>;
        type MatchMapping = rspy_ast::PatternKind<U>;
        type MatchClass = rspy_ast::PatternKind<U>;
        type MatchStar = rspy_ast::PatternKind<U>;
        type MatchAs = rspy_ast::PatternKind<U>;
        type MatchOr = rspy_ast::PatternKind<U>;
        type Pattern = rspy_ast::Pattern<U>;
        type Withitem = rspy_ast::Withitem<U>;
        type MatchCase = rspy_ast::MatchCase<U>;
        type FunctionDef = rspy_ast::StmtKind<U>;
        type AsyncFunctionDef = rspy_ast::StmtKind<U>;
        type ClassDef = rspy_ast::StmtKind<U>;
        type Return = rspy_ast::StmtKind<U>;
        type Delete = rspy_ast::StmtKind<U>;
        type Assign = rspy_ast::StmtKind<U>;
        type AugAssign = rspy_ast::StmtKind<U>;
        type AnnAssign = rspy_ast::StmtKind<U>;
        type For = rspy_ast::StmtKind<U>;
        type AsyncFor = rspy_ast::StmtKind<U>;
        type While = rspy_ast::StmtKind<U>;
        type If = rspy_ast::StmtKind<U>;
        type With = rspy_ast::StmtKind<U>;
        type AsyncWith = rspy_ast::StmtKind<U>;
        type Match = rspy_ast::StmtKind<U>;
        type Raise = rspy_ast::StmtKind<U>;
        type Try = rspy_ast::StmtKind<U>;
        type Assert = rspy_ast::StmtKind<U>;
        type Import = rspy_ast::StmtKind<U>;
        type ImportFrom = rspy_ast::StmtKind<U>;
        type Global = rspy_ast::StmtKind<U>;
        type Nonlocal = rspy_ast::StmtKind<U>;
        type Stmt = rspy_ast::Stmt<U>;

        #[inline]
        fn stmts(&self) -> &[Self::Stmt] {
            self
        }
    }
}
