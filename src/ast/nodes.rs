// TODO(Seamooo) replace strings with character iterators

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

impl<'a, T: Located> Located for &'a T {
    #[inline(always)]
    fn start_row(&self) -> usize {
        T::start_row(self)
    }

    #[inline(always)]
    fn start_col(&self) -> usize {
        T::start_col(self)
    }

    #[inline(always)]
    fn end_row(&self) -> usize {
        T::end_row(self)
    }

    #[inline(always)]
    fn end_col(&self) -> usize {
        T::end_col(self)
    }

    #[inline(always)]
    fn start(&self) -> (usize, usize) {
        T::start(self)
    }

    #[inline(always)]
    fn end(&self) -> (usize, usize) {
        T::end(self)
    }
}
pub trait Ident {
    fn val(&self) -> &str;
}

// TODO(Seamooo) these general borrow impls should be macroed

impl<'a, T: Ident> Ident for &'a T {
    #[inline(always)]
    fn val(&self) -> &str {
        T::val(self)
    }
}

pub trait Alias {
    type Ast: Ast;
    fn name(&self) -> <Self::Ast as Ast>::Ident<'_>;
    fn asname(&self) -> Option<<Self::Ast as Ast>::Ident<'_>>;
}

impl<'a, T: Alias> Alias for &'a T {
    type Ast = T::Ast;

    #[inline(always)]
    fn name(&self) -> <Self::Ast as Ast>::Ident<'_> {
        T::name(self)
    }

    #[inline(always)]
    fn asname(&self) -> Option<<Self::Ast as Ast>::Ident<'_>> {
        T::asname(self)
    }
}

pub trait TypeComment {
    fn val(&self) -> &str;
}

impl<'a, T: TypeComment> TypeComment for &'a T {
    #[inline(always)]
    fn val(&self) -> &str {
        T::val(self)
    }
}

pub trait Arg: Located {
    type Ast: Ast;

    fn arg(&self) -> <Self::Ast as Ast>::Ident<'_>;
    fn annotation(&self) -> Option<<Self::Ast as Ast>::Expr<'_>>;
    fn type_comment(&self) -> Option<<Self::Ast as Ast>::TypeComment<'_>>;
}

impl<'a, T: Arg> Arg for &'a T {
    type Ast = T::Ast;

    #[inline(always)]
    fn arg(&self) -> <Self::Ast as Ast>::Ident<'_> {
        T::arg(self)
    }

    #[inline(always)]
    fn annotation(&self) -> Option<<Self::Ast as Ast>::Expr<'_>> {
        T::annotation(self)
    }

    #[inline(always)]
    fn type_comment(&self) -> Option<<Self::Ast as Ast>::TypeComment<'_>> {
        T::type_comment(self)
    }
}

pub trait Arguments {
    type Ast: Ast;
    type PosonlyargsIter<'a>: Iterator<Item = <Self::Ast as Ast>::Arg<'a>>
    where
        Self: 'a;
    type ArgsIter<'a>: Iterator<Item = <Self::Ast as Ast>::Arg<'a>>
    where
        Self: 'a;
    type KwonlyargsIter<'a>: Iterator<Item = <Self::Ast as Ast>::Arg<'a>>
    where
        Self: 'a;
    type KwDefaultsIter<'a>: Iterator<Item = <Self::Ast as Ast>::Expr<'a>>
    where
        Self: 'a;
    type DefaultsIter<'a>: Iterator<Item = <Self::Ast as Ast>::Expr<'a>>
    where
        Self: 'a;
    fn posonlyargs(&self) -> Self::PosonlyargsIter<'_>;
    fn args(&self) -> Self::ArgsIter<'_>;
    fn vararg(&self) -> Option<<Self::Ast as Ast>::Arg<'_>>;
    fn kwonlyargs(&self) -> Self::KwonlyargsIter<'_>;
    fn kw_defaults(&self) -> Self::KwDefaultsIter<'_>;
    fn kwarg(&self) -> Option<<Self::Ast as Ast>::Arg<'_>>;
    fn defaults(&self) -> Self::DefaultsIter<'_>;
}

impl<'a, T: Arguments> Arguments for &'a T {
    type ArgsIter<'b> = T::ArgsIter<'b>
    where
		'a: 'b;
    type Ast = T::Ast;
    type DefaultsIter<'b> = T::DefaultsIter<'b>
    where
		'a: 'b;
    type KwDefaultsIter<'b> = T::KwDefaultsIter<'b>
    where
		'a: 'b;
    type KwonlyargsIter<'b> = T::KwonlyargsIter<'b>
    where
		'a: 'b;
    type PosonlyargsIter<'b> = T::PosonlyargsIter<'b>
    where
		'a: 'b;

    #[inline(always)]
    fn posonlyargs(&self) -> Self::PosonlyargsIter<'_> {
        T::posonlyargs(self)
    }

    #[inline(always)]
    fn args(&self) -> Self::ArgsIter<'_> {
        T::args(self)
    }

    #[inline(always)]
    fn vararg(&self) -> Option<<Self::Ast as Ast>::Arg<'_>> {
        T::vararg(self)
    }

    #[inline(always)]
    fn kwonlyargs(&self) -> Self::KwonlyargsIter<'_> {
        T::kwonlyargs(self)
    }

    #[inline(always)]
    fn kw_defaults(&self) -> Self::KwDefaultsIter<'_> {
        T::kw_defaults(self)
    }

    #[inline(always)]
    fn kwarg(&self) -> Option<<Self::Ast as Ast>::Arg<'_>> {
        T::kwarg(self)
    }

    #[inline(always)]
    fn defaults(&self) -> Self::DefaultsIter<'_> {
        T::defaults(self)
    }
}

pub trait Keyword {
    type Ast: Ast;
    fn arg(&self) -> Option<<Self::Ast as Ast>::Ident<'_>>;
    fn value(&self) -> <Self::Ast as Ast>::Expr<'_>;
}

impl<'a, T: Keyword> Keyword for &'a T {
    type Ast = T::Ast;

    #[inline(always)]
    fn arg(&self) -> Option<<Self::Ast as Ast>::Ident<'_>> {
        T::arg(self)
    }

    #[inline(always)]
    fn value(&self) -> <Self::Ast as Ast>::Expr<'_> {
        T::value(self)
    }
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

// TODO(Seamooo) if there's internal requirements of BigInt an interface should
// be required
pub trait BigInt {}

impl<'a, T: BigInt> BigInt for &'a T {}

pub enum ConstantKind<STR, BYTES, TUPLE, BIGINT> {
    None,
    Bool(bool),
    Str(STR),
    Bytes(BYTES),
    Int(BIGINT),
    Tuple(TUPLE),
    Float(f64),
    Complex { real: f64, imag: f64 },
    Ellipsis,
}

pub trait Constant {
    type BigInt<'a>: BigInt + 'a
    where
        Self: 'a;
    type Constant<'a>: Constant + 'a
    where
        Self: 'a;
    type TupleIter<'a>: Iterator<Item = Self::Constant<'a>>
    where
        Self: 'a;
    type BytesIter<'a>: Iterator<Item = u8> + 'a
    where
        Self: 'a;
    fn value(
        &self,
    ) -> ConstantKind<&str, Self::BytesIter<'_>, Self::TupleIter<'_>, Self::BigInt<'_>>;
}
impl<'a, T: Constant> Constant for &'a T {
    type BigInt<'b> = T::BigInt<'b>
    where
		'a: 'b;
    type BytesIter<'b> = T::BytesIter<'b>
    where
		'a: 'b;
    type Constant<'b> = T::Constant<'b>
    where
        'a: 'b;
    type TupleIter<'b> = T::TupleIter<'b>
    where
		'a: 'b;

    #[inline(always)]
    fn value(
        &self,
    ) -> ConstantKind<&str, Self::BytesIter<'_>, Self::TupleIter<'_>, Self::BigInt<'_>> {
        T::value(self)
    }
}

pub trait Comprehension {
    type Ast: Ast;
    type IfsIter<'a>: Iterator<Item = <Self::Ast as Ast>::Expr<'a>>
    where
        Self: 'a;
    fn target(&self) -> <Self::Ast as Ast>::Expr<'_>;
    fn iter(&self) -> <Self::Ast as Ast>::Expr<'_>;
    fn ifs(&self) -> Self::IfsIter<'_>;
    fn is_async(&self) -> usize;
}

impl<'a, T: Comprehension> Comprehension for &'a T {
    type Ast = T::Ast;
    type IfsIter<'b> = T::IfsIter<'b>
    where
		'a: 'b;

    fn target(&self) -> <Self::Ast as Ast>::Expr<'_> {
        T::target(self)
    }

    #[inline(always)]
    fn iter(&self) -> <Self::Ast as Ast>::Expr<'_> {
        T::iter(self)
    }

    #[inline(always)]
    fn ifs(&self) -> Self::IfsIter<'_> {
        T::ifs(self)
    }

    #[inline(always)]
    fn is_async(&self) -> usize {
        T::is_async(self)
    }
}

pub trait BoolOp {
    type Ast: Ast;
    type ValuesIter<'a>: Iterator<Item = <Self::Ast as Ast>::Expr<'a>>
    where
        Self: 'a;
    fn op(&self) -> Boolop;
    fn values(&self) -> Self::ValuesIter<'_>;
}

impl<'a, T: BoolOp> BoolOp for &'a T {
    type Ast = T::Ast;
    type ValuesIter<'b> = T::ValuesIter<'b>
    where
        'a: 'b;

    #[inline(always)]
    fn op(&self) -> Boolop {
        T::op(self)
    }

    #[inline(always)]
    fn values(&self) -> Self::ValuesIter<'_> {
        T::values(self)
    }
}

pub trait NamedExpr {
    type Ast: Ast;
    fn target(&self) -> <Self::Ast as Ast>::Expr<'_>;
    fn value(&self) -> <Self::Ast as Ast>::Expr<'_>;
}

impl<'a, T: NamedExpr> NamedExpr for &'a T {
    type Ast = T::Ast;

    #[inline(always)]
    fn target(&self) -> <Self::Ast as Ast>::Expr<'_> {
        T::target(self)
    }

    #[inline(always)]
    fn value(&self) -> <Self::Ast as Ast>::Expr<'_> {
        T::value(self)
    }
}

pub trait BinOp {
    type Ast: Ast;
    fn left(&self) -> <Self::Ast as Ast>::Expr<'_>;
    fn op(&self) -> Operator;
    fn right(&self) -> <Self::Ast as Ast>::Expr<'_>;
}

impl<'a, T: BinOp> BinOp for &'a T {
    type Ast = T::Ast;

    #[inline(always)]
    fn left(&self) -> <Self::Ast as Ast>::Expr<'_> {
        T::left(self)
    }

    #[inline(always)]
    fn op(&self) -> Operator {
        T::op(self)
    }

    #[inline(always)]
    fn right(&self) -> <Self::Ast as Ast>::Expr<'_> {
        T::right(self)
    }
}

pub trait UnaryOp {
    type Ast: Ast;
    fn op(&self) -> Unaryop;
    fn operand(&self) -> <Self::Ast as Ast>::Expr<'_>;
}

impl<'a, T: UnaryOp> UnaryOp for &'a T {
    type Ast = T::Ast;

    #[inline(always)]
    fn op(&self) -> Unaryop {
        T::op(self)
    }

    #[inline(always)]
    fn operand(&self) -> <Self::Ast as Ast>::Expr<'_> {
        T::operand(self)
    }
}

pub trait Lambda {
    type Ast: Ast;
    fn args(&self) -> <Self::Ast as Ast>::Arguments<'_>;
    fn body(&self) -> <Self::Ast as Ast>::Expr<'_>;
}

impl<'a, T: Lambda> Lambda for &'a T {
    type Ast = T::Ast;

    #[inline(always)]
    fn args(&self) -> <Self::Ast as Ast>::Arguments<'_> {
        T::args(self)
    }

    #[inline(always)]
    fn body(&self) -> <Self::Ast as Ast>::Expr<'_> {
        T::body(self)
    }
}

pub trait IfExp {
    type Ast: Ast;
    fn test(&self) -> <Self::Ast as Ast>::Expr<'_>;
    fn body(&self) -> <Self::Ast as Ast>::Expr<'_>;
    fn orelse(&self) -> <Self::Ast as Ast>::Expr<'_>;
}

impl<'a, T: IfExp> IfExp for &'a T {
    type Ast = T::Ast;

    #[inline(always)]
    fn test(&self) -> <Self::Ast as Ast>::Expr<'_> {
        T::test(self)
    }

    #[inline(always)]
    fn body(&self) -> <Self::Ast as Ast>::Expr<'_> {
        T::body(self)
    }

    #[inline(always)]
    fn orelse(&self) -> <Self::Ast as Ast>::Expr<'_> {
        T::orelse(self)
    }
}

pub trait Dict {
    type Ast: Ast;
    type KeysIter<'a>: Iterator<Item = <Self::Ast as Ast>::Expr<'a>>
    where
        Self: 'a;
    type ValuesIter<'a>: Iterator<Item = <Self::Ast as Ast>::Expr<'a>>
    where
        Self: 'a;
    fn keys(&self) -> Self::KeysIter<'_>;
    fn values(&self) -> Self::ValuesIter<'_>;
}

impl<'a, T: Dict> Dict for &'a T {
    type Ast = T::Ast;
    type KeysIter<'b> = T::KeysIter<'b>
    where
		'a: 'b;
    type ValuesIter<'b> = T::ValuesIter<'b>
    where
		'a: 'b;

    #[inline(always)]
    fn keys(&self) -> Self::KeysIter<'_> {
        T::keys(self)
    }

    #[inline(always)]
    fn values(&self) -> Self::ValuesIter<'_> {
        T::values(self)
    }
}

pub trait Set {
    type Ast: Ast;
    type EltsIter<'a>: Iterator<Item = <Self::Ast as Ast>::Expr<'a>>
    where
        Self: 'a;
    fn elts(&self) -> Self::EltsIter<'_>;
}

impl<'a, T: Set> Set for &'a T {
    type Ast = T::Ast;
    type EltsIter<'b> = T::EltsIter<'b>
    where
		'a: 'b;

    #[inline(always)]
    fn elts(&self) -> Self::EltsIter<'_> {
        T::elts(self)
    }
}

pub trait ListComp {
    type Ast: Ast;
    type GeneratorsIter<'a>: Iterator<Item = <Self::Ast as Ast>::Comprehension<'a>>
    where
        Self: 'a;
    fn elt(&self) -> <Self::Ast as Ast>::Expr<'_>;
    fn generators(&self) -> Self::GeneratorsIter<'_>;
}

impl<'a, T: ListComp> ListComp for &'a T {
    type Ast = T::Ast;
    type GeneratorsIter<'b> = T::GeneratorsIter<'b>
    where
		'a: 'b;

    #[inline(always)]
    fn elt(&self) -> <Self::Ast as Ast>::Expr<'_> {
        T::elt(self)
    }

    #[inline(always)]
    fn generators(&self) -> Self::GeneratorsIter<'_> {
        T::generators(self)
    }
}

pub trait SetComp {
    type Ast: Ast;
    type GeneratorsIter<'a>: Iterator<Item = <Self::Ast as Ast>::Comprehension<'a>>
    where
        Self: 'a;
    fn elt(&self) -> <Self::Ast as Ast>::Expr<'_>;
    fn generators(&self) -> Self::GeneratorsIter<'_>;
}

impl<'a, T: SetComp> SetComp for &'a T {
    type Ast = T::Ast;
    type GeneratorsIter<'b> = T::GeneratorsIter<'b>
    where
		'a: 'b;

    #[inline(always)]
    fn elt(&self) -> <Self::Ast as Ast>::Expr<'_> {
        T::elt(self)
    }

    #[inline(always)]
    fn generators(&self) -> Self::GeneratorsIter<'_> {
        T::generators(self)
    }
}

pub trait DictComp {
    type Ast: Ast;
    type GeneratorsIter<'a>: Iterator<Item = <Self::Ast as Ast>::Comprehension<'a>>
    where
        Self: 'a;
    fn key(&self) -> <Self::Ast as Ast>::Expr<'_>;
    fn value(&self) -> <Self::Ast as Ast>::Expr<'_>;
    fn generators(&self) -> Self::GeneratorsIter<'_>;
}

impl<'a, T: DictComp> DictComp for &'a T {
    type Ast = T::Ast;
    type GeneratorsIter<'b> = T::GeneratorsIter<'b>
    where
		'a: 'b;

    #[inline(always)]
    fn key(&self) -> <Self::Ast as Ast>::Expr<'_> {
        T::key(self)
    }

    #[inline(always)]
    fn value(&self) -> <Self::Ast as Ast>::Expr<'_> {
        T::value(self)
    }

    #[inline(always)]
    fn generators(&self) -> Self::GeneratorsIter<'_> {
        T::generators(self)
    }
}

pub trait GeneratorExp {
    type Ast: Ast;
    type GeneratorsIter<'a>: Iterator<Item = <Self::Ast as Ast>::Comprehension<'a>>
    where
        Self: 'a;
    fn elt(&self) -> <Self::Ast as Ast>::Expr<'_>;
    fn generators(&self) -> Self::GeneratorsIter<'_>;
}

impl<'a, T: GeneratorExp> GeneratorExp for &'a T {
    type Ast = T::Ast;
    type GeneratorsIter<'b> = T::GeneratorsIter<'b>
    where
		'a: 'b;

    #[inline(always)]
    fn elt(&self) -> <Self::Ast as Ast>::Expr<'_> {
        T::elt(self)
    }

    #[inline(always)]
    fn generators(&self) -> Self::GeneratorsIter<'_> {
        T::generators(self)
    }
}

pub trait Await {
    type Ast: Ast;
    fn value(&self) -> <Self::Ast as Ast>::Expr<'_>;
}

impl<'a, T: Await> Await for &'a T {
    type Ast = T::Ast;

    #[inline(always)]
    fn value(&self) -> <Self::Ast as Ast>::Expr<'_> {
        T::value(self)
    }
}

pub trait Yield {
    type Ast: Ast;
    fn value(&self) -> Option<<Self::Ast as Ast>::Expr<'_>>;
}

impl<'a, T: Yield> Yield for &'a T {
    type Ast = T::Ast;

    #[inline(always)]
    fn value(&self) -> Option<<Self::Ast as Ast>::Expr<'_>> {
        T::value(self)
    }
}

pub trait YieldFrom {
    type Ast: Ast;
    fn value(&self) -> <Self::Ast as Ast>::Expr<'_>;
}

impl<'a, T: YieldFrom> YieldFrom for &'a T {
    type Ast = T::Ast;

    #[inline(always)]
    fn value(&self) -> <Self::Ast as Ast>::Expr<'_> {
        T::value(self)
    }
}

pub trait Compare {
    type Ast: Ast;
    type CmpopIter<'a>: Iterator<Item = Cmpop> + 'a
    where
        Self: 'a;
    fn left(&self) -> <Self::Ast as Ast>::Expr<'_>;
    fn ops(&self) -> Self::CmpopIter<'_>;
}

impl<'a, T: Compare> Compare for &'a T {
    type Ast = T::Ast;
    type CmpopIter<'b> = T::CmpopIter<'b>
    where
        'a: 'b;

    #[inline(always)]
    fn left(&self) -> <Self::Ast as Ast>::Expr<'_> {
        T::left(self)
    }

    #[inline(always)]
    fn ops(&self) -> Self::CmpopIter<'_> {
        T::ops(self)
    }
}

pub trait Call {
    type Ast: Ast;
    type ArgsIter<'a>: Iterator<Item = <Self::Ast as Ast>::Expr<'a>>
    where
        Self: 'a;
    type KeywordsIter<'a>: Iterator<Item = <Self::Ast as Ast>::Keyword<'a>>
    where
        Self: 'a;
    fn func(&self) -> <Self::Ast as Ast>::Expr<'_>;
    fn args(&self) -> Self::ArgsIter<'_>;
    fn keywords(&self) -> Self::KeywordsIter<'_>;
}

impl<'a, T: Call> Call for &'a T {
    type ArgsIter<'b> = T::ArgsIter<'b>
    where
		'a: 'b;
    type Ast = T::Ast;
    type KeywordsIter<'b> = T::KeywordsIter<'b>
    where
		'a: 'b;

    #[inline(always)]
    fn func(&self) -> <Self::Ast as Ast>::Expr<'_> {
        T::func(self)
    }

    #[inline(always)]
    fn args(&self) -> Self::ArgsIter<'_> {
        T::args(self)
    }

    #[inline(always)]
    fn keywords(&self) -> Self::KeywordsIter<'_> {
        T::keywords(self)
    }
}

pub trait FormattedValue {
    type Ast: Ast;
    fn value(&self) -> <Self::Ast as Ast>::Expr<'_>;
    fn conversion(&self) -> usize;
    fn format_spec(&self) -> Option<<Self::Ast as Ast>::Expr<'_>>;
}

impl<'a, T: FormattedValue> FormattedValue for &'a T {
    type Ast = T::Ast;

    #[inline(always)]
    fn value(&self) -> <Self::Ast as Ast>::Expr<'_> {
        T::value(self)
    }

    #[inline(always)]
    fn conversion(&self) -> usize {
        T::conversion(self)
    }

    #[inline(always)]
    fn format_spec(&self) -> Option<<Self::Ast as Ast>::Expr<'_>> {
        T::format_spec(self)
    }
}

pub trait JoinedStr {
    type Ast: Ast;
    type ValuesIter<'a>: Iterator<Item = <Self::Ast as Ast>::Expr<'a>>
    where
        Self: 'a;
    fn values(&self) -> Self::ValuesIter<'_>;
}

impl<'a, T: JoinedStr> JoinedStr for &'a T {
    type Ast = T::Ast;
    type ValuesIter<'b> = T::ValuesIter<'b>
    where
		'a: 'b;

    #[inline(always)]
    fn values(&self) -> Self::ValuesIter<'_> {
        T::values(self)
    }
}

// TODO(Seamooo) represent kind as an iterator

pub trait ConstantExpr {
    type Ast: Ast;
    fn value(&self) -> <Self::Ast as Ast>::Constant<'_>;
    fn kind(&self) -> Option<&str>;
}

impl<'a, T: ConstantExpr> ConstantExpr for &'a T {
    type Ast = T::Ast;

    #[inline(always)]
    fn value(&self) -> <Self::Ast as Ast>::Constant<'_> {
        T::value(self)
    }

    #[inline(always)]
    fn kind(&self) -> Option<&str> {
        T::kind(self)
    }
}

pub trait Attribute {
    type Ast: Ast;
    fn value(&self) -> <Self::Ast as Ast>::Expr<'_>;
    fn attr(&self) -> <Self::Ast as Ast>::Ident<'_>;
    fn ctx(&self) -> ExprContext;
}

impl<'a, T: Attribute> Attribute for &'a T {
    type Ast = T::Ast;

    #[inline(always)]
    fn value(&self) -> <Self::Ast as Ast>::Expr<'_> {
        T::value(self)
    }

    #[inline(always)]
    fn attr(&self) -> <Self::Ast as Ast>::Ident<'_> {
        T::attr(self)
    }

    #[inline(always)]
    fn ctx(&self) -> ExprContext {
        T::ctx(self)
    }
}

pub trait Subscript {
    type Ast: Ast;
    fn value(&self) -> <Self::Ast as Ast>::Expr<'_>;
    fn slice(&self) -> <Self::Ast as Ast>::Expr<'_>;
    fn ctx(&self) -> ExprContext;
}

impl<'a, T: Subscript> Subscript for &'a T {
    type Ast = T::Ast;

    #[inline(always)]
    fn value(&self) -> <Self::Ast as Ast>::Expr<'_> {
        T::value(self)
    }

    #[inline(always)]
    fn slice(&self) -> <Self::Ast as Ast>::Expr<'_> {
        T::slice(self)
    }

    #[inline(always)]
    fn ctx(&self) -> ExprContext {
        T::ctx(self)
    }
}

pub trait Starred {
    type Ast: Ast;
    fn value(&self) -> <Self::Ast as Ast>::Expr<'_>;
    fn ctx(&self) -> ExprContext;
}

impl<'a, T: Starred> Starred for &'a T {
    type Ast = T::Ast;

    #[inline(always)]
    fn value(&self) -> <Self::Ast as Ast>::Expr<'_> {
        T::value(self)
    }

    #[inline(always)]
    fn ctx(&self) -> ExprContext {
        T::ctx(self)
    }
}

pub trait Name {
    type Ast: Ast;
    fn id(&self) -> <Self::Ast as Ast>::Ident<'_>;
    fn ctx(&self) -> ExprContext;
}

impl<'a, T: Name> Name for &'a T {
    type Ast = T::Ast;

    #[inline(always)]
    fn id(&self) -> <Self::Ast as Ast>::Ident<'_> {
        T::id(self)
    }

    #[inline(always)]
    fn ctx(&self) -> ExprContext {
        T::ctx(self)
    }
}

pub trait List {
    type Ast: Ast;
    type EltsIter<'a>: Iterator<Item = <Self::Ast as Ast>::Expr<'a>>
    where
        Self: 'a;
    fn elts(&self) -> Self::EltsIter<'_>;
    fn ctx(&self) -> ExprContext;
}

impl<'a, T: List> List for &'a T {
    type Ast = T::Ast;
    type EltsIter<'b> = T::EltsIter<'b>
    where
        'a: 'b;

    #[inline(always)]
    fn elts(&self) -> Self::EltsIter<'_> {
        T::elts(self)
    }

    #[inline(always)]
    fn ctx(&self) -> ExprContext {
        T::ctx(self)
    }
}

pub trait Tuple {
    type Ast: Ast;
    type EltsIter<'a>: Iterator<Item = <Self::Ast as Ast>::Expr<'a>>
    where
        Self: 'a;
    fn elts(&self) -> Self::EltsIter<'_>;
    fn ctx(&self) -> ExprContext;
}

impl<'a, T: Tuple> Tuple for &'a T {
    type Ast = T::Ast;
    type EltsIter<'b> = T::EltsIter<'b>
    where
        'a: 'b;

    #[inline(always)]
    fn elts(&self) -> Self::EltsIter<'_> {
        T::elts(self)
    }

    #[inline(always)]
    fn ctx(&self) -> ExprContext {
        T::ctx(self)
    }
}

pub trait Slice {
    type Ast: Ast;
    fn lower(&self) -> Option<<Self::Ast as Ast>::Expr<'_>>;
    fn upper(&self) -> Option<<Self::Ast as Ast>::Expr<'_>>;
    fn step(&self) -> Option<<Self::Ast as Ast>::Expr<'_>>;
}

impl<'a, T: Slice> Slice for &'a T {
    type Ast = T::Ast;

    #[inline(always)]
    fn lower(&self) -> Option<<Self::Ast as Ast>::Expr<'_>> {
        T::lower(self)
    }

    #[inline(always)]
    fn upper(&self) -> Option<<Self::Ast as Ast>::Expr<'_>> {
        T::upper(self)
    }

    #[inline(always)]
    fn step(&self) -> Option<<Self::Ast as Ast>::Expr<'_>> {
        T::step(self)
    }
}

pub enum ExprKind<
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
    BoolOp(BOOLOP),
    NamedExpr(NAMEDEXPR),
    BinOp(BINOP),
    UnaryOp(UNARYOP),
    Lambda(LAMBDA),
    IfExp(IFEXP),
    Dict(DICT),
    Set(SET),
    ListComp(LISTCOMP),
    SetComp(SETCOMP),
    DictComp(DICTCOMP),
    GeneratorExp(GENERATOREXP),
    Await(AWAIT),
    Yield(YIELD),
    YieldFrom(YIELDFROM),
    Compare(COMPARE),
    Call(CALL),
    FormattedValue(FORMATTEDVALUE),
    JoinedStr(JOINEDSTR),
    ConstantExpr(CONSTANTEXPR),
    Attribute(ATTRIBUTE),
    Subscript(SUBSCRIPT),
    Starred(STARRED),
    Name(NAME),
    List(LIST),
    Tuple(TUPLE),
    Slice(SLICE),
}

pub trait Expr: Located {
    type Ast: Ast;
    fn expr(
        &self,
    ) -> ExprKind<
        <Self::Ast as Ast>::BoolOp<'_>,
        <Self::Ast as Ast>::NamedExpr<'_>,
        <Self::Ast as Ast>::BinOp<'_>,
        <Self::Ast as Ast>::UnaryOp<'_>,
        <Self::Ast as Ast>::Lambda<'_>,
        <Self::Ast as Ast>::IfExp<'_>,
        <Self::Ast as Ast>::Dict<'_>,
        <Self::Ast as Ast>::Set<'_>,
        <Self::Ast as Ast>::ListComp<'_>,
        <Self::Ast as Ast>::SetComp<'_>,
        <Self::Ast as Ast>::DictComp<'_>,
        <Self::Ast as Ast>::GeneratorExp<'_>,
        <Self::Ast as Ast>::Await<'_>,
        <Self::Ast as Ast>::Yield<'_>,
        <Self::Ast as Ast>::YieldFrom<'_>,
        <Self::Ast as Ast>::Compare<'_>,
        <Self::Ast as Ast>::Call<'_>,
        <Self::Ast as Ast>::FormattedValue<'_>,
        <Self::Ast as Ast>::JoinedStr<'_>,
        <Self::Ast as Ast>::ConstantExpr<'_>,
        <Self::Ast as Ast>::Attribute<'_>,
        <Self::Ast as Ast>::Subscript<'_>,
        <Self::Ast as Ast>::Starred<'_>,
        <Self::Ast as Ast>::Name<'_>,
        <Self::Ast as Ast>::List<'_>,
        <Self::Ast as Ast>::Tuple<'_>,
        <Self::Ast as Ast>::Slice<'_>,
    >;
}

impl<'a, T: Expr> Expr for &'a T {
    type Ast = T::Ast;

    #[inline(always)]
    fn expr(
        &self,
    ) -> ExprKind<
        <Self::Ast as Ast>::BoolOp<'_>,
        <Self::Ast as Ast>::NamedExpr<'_>,
        <Self::Ast as Ast>::BinOp<'_>,
        <Self::Ast as Ast>::UnaryOp<'_>,
        <Self::Ast as Ast>::Lambda<'_>,
        <Self::Ast as Ast>::IfExp<'_>,
        <Self::Ast as Ast>::Dict<'_>,
        <Self::Ast as Ast>::Set<'_>,
        <Self::Ast as Ast>::ListComp<'_>,
        <Self::Ast as Ast>::SetComp<'_>,
        <Self::Ast as Ast>::DictComp<'_>,
        <Self::Ast as Ast>::GeneratorExp<'_>,
        <Self::Ast as Ast>::Await<'_>,
        <Self::Ast as Ast>::Yield<'_>,
        <Self::Ast as Ast>::YieldFrom<'_>,
        <Self::Ast as Ast>::Compare<'_>,
        <Self::Ast as Ast>::Call<'_>,
        <Self::Ast as Ast>::FormattedValue<'_>,
        <Self::Ast as Ast>::JoinedStr<'_>,
        <Self::Ast as Ast>::ConstantExpr<'_>,
        <Self::Ast as Ast>::Attribute<'_>,
        <Self::Ast as Ast>::Subscript<'_>,
        <Self::Ast as Ast>::Starred<'_>,
        <Self::Ast as Ast>::Name<'_>,
        <Self::Ast as Ast>::List<'_>,
        <Self::Ast as Ast>::Tuple<'_>,
        <Self::Ast as Ast>::Slice<'_>,
    > {
        T::expr(self)
    }
}

pub trait ExceptHandler: Located {
    type Ast: Ast;
    type BodyIter<'a>: Iterator<Item = <Self::Ast as Ast>::Stmt<'a>>
    where
        Self: 'a;
    fn type_(&self) -> Option<<Self::Ast as Ast>::Expr<'_>>;
    fn name(&self) -> Option<<Self::Ast as Ast>::Ident<'_>>;
    fn body(&self) -> Self::BodyIter<'_>;
}

impl<'a, T: ExceptHandler> ExceptHandler for &'a T {
    type Ast = T::Ast;
    type BodyIter<'b> = T::BodyIter<'b>
    where
        'a: 'b;

    #[inline(always)]
    fn type_(&self) -> Option<<Self::Ast as Ast>::Expr<'_>> {
        T::type_(self)
    }

    #[inline(always)]
    fn name(&self) -> Option<<Self::Ast as Ast>::Ident<'_>> {
        T::name(self)
    }

    #[inline(always)]
    fn body(&self) -> Self::BodyIter<'_> {
        T::body(self)
    }
}

pub trait MatchValue {
    type Ast: Ast;
    fn value(&self) -> <Self::Ast as Ast>::Expr<'_>;
}

impl<'a, T: MatchValue> MatchValue for &'a T {
    type Ast = T::Ast;

    #[inline(always)]
    fn value(&self) -> <Self::Ast as Ast>::Expr<'_> {
        T::value(self)
    }
}

pub trait MatchSingleton {
    type Ast: Ast;
    fn value(&self) -> <Self::Ast as Ast>::Constant<'_>;
}

impl<'a, T: MatchSingleton> MatchSingleton for &'a T {
    type Ast = T::Ast;

    #[inline(always)]
    fn value(&self) -> <Self::Ast as Ast>::Constant<'_> {
        T::value(self)
    }
}

pub trait MatchSequence {
    type Ast: Ast;
    type PatternsIter<'a>: Iterator<Item = <Self::Ast as Ast>::Pattern<'a>>
    where
        Self: 'a;
    fn patterns(&self) -> Self::PatternsIter<'_>;
}

impl<'a, T: MatchSequence> MatchSequence for &'a T {
    type Ast = T::Ast;
    type PatternsIter<'b> = T::PatternsIter<'b>
    where
        'a: 'b;

    #[inline(always)]
    fn patterns(&self) -> Self::PatternsIter<'_> {
        T::patterns(self)
    }
}

pub trait MatchMapping {
    type Ast: Ast;
    type KeysIter<'a>: Iterator<Item = <Self::Ast as Ast>::Expr<'a>>
    where
        Self: 'a;
    type PatternsIter<'a>: Iterator<Item = <Self::Ast as Ast>::Pattern<'a>>
    where
        Self: 'a;
    fn keys(&self) -> Self::KeysIter<'_>;
    fn patterns(&self) -> Self::PatternsIter<'_>;
    fn rest(&self) -> Option<<Self::Ast as Ast>::Ident<'_>>;
}

impl<'a, T: MatchMapping> MatchMapping for &'a T {
    type Ast = T::Ast;
    type KeysIter<'b> = T::KeysIter<'b>
    where
        'a: 'b;
    type PatternsIter<'b> = T::PatternsIter<'b>
    where
        'a: 'b;

    #[inline(always)]
    fn keys(&self) -> Self::KeysIter<'_> {
        T::keys(self)
    }

    #[inline(always)]
    fn patterns(&self) -> Self::PatternsIter<'_> {
        T::patterns(self)
    }

    #[inline(always)]
    fn rest(&self) -> Option<<Self::Ast as Ast>::Ident<'_>> {
        T::rest(self)
    }
}

pub trait MatchClass {
    type Ast: Ast;
    type PatternsIter<'a>: Iterator<Item = <Self::Ast as Ast>::Pattern<'a>>
    where
        Self: 'a;
    type KwdAttrsIter<'a>: Iterator<Item = <Self::Ast as Ast>::Ident<'a>>
    where
        Self: 'a;
    type KwdPatternsIter<'a>: Iterator<Item = <Self::Ast as Ast>::Pattern<'a>>
    where
        Self: 'a;
    fn cls(&self) -> <Self::Ast as Ast>::Expr<'_>;
    fn patterns(&self) -> Self::PatternsIter<'_>;
    fn kwd_attrs(&self) -> Self::KwdAttrsIter<'_>;
    fn kwd_patterns(&self) -> Self::KwdPatternsIter<'_>;
}

impl<'a, T: MatchClass> MatchClass for &'a T {
    type Ast = T::Ast;
    type KwdAttrsIter<'b> = T::KwdAttrsIter<'b>
    where
        'a: 'b;
    type KwdPatternsIter<'b> = T::KwdPatternsIter<'b>
    where
        'a: 'b;
    type PatternsIter<'b> = T::PatternsIter<'b>
    where
        'a: 'b;

    #[inline(always)]
    fn cls(&self) -> <Self::Ast as Ast>::Expr<'_> {
        T::cls(self)
    }

    #[inline(always)]
    fn patterns(&self) -> Self::PatternsIter<'_> {
        T::patterns(self)
    }

    #[inline(always)]
    fn kwd_attrs(&self) -> Self::KwdAttrsIter<'_> {
        T::kwd_attrs(self)
    }

    #[inline(always)]
    fn kwd_patterns(&self) -> Self::KwdPatternsIter<'_> {
        T::kwd_patterns(self)
    }
}

pub trait MatchStar {
    type Ast: Ast;
    fn name(&self) -> Option<<Self::Ast as Ast>::Ident<'_>>;
}

impl<'a, T: MatchStar> MatchStar for &'a T {
    type Ast = T::Ast;

    #[inline(always)]
    fn name(&self) -> Option<<Self::Ast as Ast>::Ident<'_>> {
        T::name(self)
    }
}

pub trait MatchAs {
    type Ast: Ast;
    fn pattern(&self) -> Option<<Self::Ast as Ast>::Pattern<'_>>;
    fn name(&self) -> Option<<Self::Ast as Ast>::Ident<'_>>;
}

impl<'a, T: MatchAs> MatchAs for &'a T {
    type Ast = T::Ast;

    #[inline(always)]
    fn pattern(&self) -> Option<<Self::Ast as Ast>::Pattern<'_>> {
        T::pattern(self)
    }

    #[inline(always)]
    fn name(&self) -> Option<<Self::Ast as Ast>::Ident<'_>> {
        T::name(self)
    }
}

pub trait MatchOr {
    type Ast: Ast;
    type PatternsIter<'a>: Iterator<Item = <Self::Ast as Ast>::Pattern<'a>>
    where
        Self: 'a;
    fn patterns(&self) -> Self::PatternsIter<'_>;
}

impl<'a, T: MatchOr> MatchOr for &'a T {
    type Ast = T::Ast;
    type PatternsIter<'b> = T::PatternsIter<'b>
    where
        'a: 'b;

    fn patterns(&self) -> Self::PatternsIter<'_> {
        T::patterns(self)
    }
}

pub enum PatternKind<
    MATCHVALUE,
    MATCHSINGLETON,
    MATCHSEQUENCE,
    MATCHMAPPING,
    MATCHCLASS,
    MATCHSTAR,
    MATCHAS,
    MATCHOR,
> {
    MatchValue(MATCHVALUE),
    MatchSingleton(MATCHSINGLETON),
    MatchSequence(MATCHSEQUENCE),
    MatchMapping(MATCHMAPPING),
    MatchClass(MATCHCLASS),
    MatchStar(MATCHSTAR),
    MatchAs(MATCHAS),
    MatchOr(MATCHOR),
}

pub trait Pattern {
    type Ast: Ast;
    fn pattern(
        &self,
    ) -> PatternKind<
        <Self::Ast as Ast>::MatchValue<'_>,
        <Self::Ast as Ast>::MatchSingleton<'_>,
        <Self::Ast as Ast>::MatchSequence<'_>,
        <Self::Ast as Ast>::MatchMapping<'_>,
        <Self::Ast as Ast>::MatchClass<'_>,
        <Self::Ast as Ast>::MatchStar<'_>,
        <Self::Ast as Ast>::MatchAs<'_>,
        <Self::Ast as Ast>::MatchOr<'_>,
    >;
}

impl<'a, T: Pattern> Pattern for &'a T {
    type Ast = T::Ast;

    #[inline(always)]
    fn pattern(
        &self,
    ) -> PatternKind<
        <Self::Ast as Ast>::MatchValue<'_>,
        <Self::Ast as Ast>::MatchSingleton<'_>,
        <Self::Ast as Ast>::MatchSequence<'_>,
        <Self::Ast as Ast>::MatchMapping<'_>,
        <Self::Ast as Ast>::MatchClass<'_>,
        <Self::Ast as Ast>::MatchStar<'_>,
        <Self::Ast as Ast>::MatchAs<'_>,
        <Self::Ast as Ast>::MatchOr<'_>,
    > {
        T::pattern(self)
    }
}

pub trait Withitem {
    type Ast: Ast;
    fn context_expr(&self) -> <Self::Ast as Ast>::Expr<'_>;
    fn optional_vars(&self) -> Option<<Self::Ast as Ast>::Expr<'_>>;
}

impl<'a, T: Withitem> Withitem for &'a T {
    type Ast = T::Ast;

    #[inline(always)]
    fn context_expr(&self) -> <Self::Ast as Ast>::Expr<'_> {
        T::context_expr(self)
    }

    #[inline(always)]
    fn optional_vars(&self) -> Option<<Self::Ast as Ast>::Expr<'_>> {
        T::optional_vars(self)
    }
}

pub trait MatchCase {
    type Ast: Ast;
    type BodyIter<'a>: Iterator<Item = <Self::Ast as Ast>::Stmt<'a>>
    where
        Self: 'a;
    fn pattern(&self) -> <Self::Ast as Ast>::Pattern<'_>;
    fn guard(&self) -> Option<<Self::Ast as Ast>::Expr<'_>>;
    fn body(&self) -> Self::BodyIter<'_>;
}

impl<'a, T: MatchCase> MatchCase for &'a T {
    type Ast = T::Ast;
    type BodyIter<'b> = T::BodyIter<'b>
    where
        'a: 'b;

    #[inline(always)]
    fn pattern(&self) -> <Self::Ast as Ast>::Pattern<'_> {
        T::pattern(self)
    }

    #[inline(always)]
    fn guard(&self) -> Option<<Self::Ast as Ast>::Expr<'_>> {
        T::guard(self)
    }

    #[inline(always)]
    fn body(&self) -> Self::BodyIter<'_> {
        T::body(self)
    }
}

// TODO add type comment associated type

pub trait FunctionDef {
    type Ast: Ast;
    type BodyIter<'a>: Iterator<Item = <Self::Ast as Ast>::Stmt<'a>>
    where
        Self: 'a;
    type DecoratorListIter<'a>: Iterator<Item = <Self::Ast as Ast>::Expr<'a>>
    where
        Self: 'a;
    fn name(&self) -> <Self::Ast as Ast>::Ident<'_>;
    fn args(&self) -> <Self::Ast as Ast>::Arguments<'_>;
    fn body(&self) -> Self::BodyIter<'_>;
    fn decorator_list(&self) -> Self::DecoratorListIter<'_>;
    fn returns(&self) -> Option<<Self::Ast as Ast>::Expr<'_>>;
    fn type_comment(&self) -> Option<&str>;
}

impl<'a, T: FunctionDef> FunctionDef for &'a T {
    type Ast = T::Ast;
    type BodyIter<'b> = T::BodyIter<'b>
    where
        'a: 'b;
    type DecoratorListIter<'b> = T::DecoratorListIter<'b>
    where
        'a: 'b;

    #[inline(always)]
    fn name(&self) -> <Self::Ast as Ast>::Ident<'_> {
        T::name(self)
    }

    #[inline(always)]
    fn args(&self) -> <Self::Ast as Ast>::Arguments<'_> {
        T::args(self)
    }

    #[inline(always)]
    fn body(&self) -> Self::BodyIter<'_> {
        T::body(self)
    }

    #[inline(always)]
    fn decorator_list(&self) -> Self::DecoratorListIter<'_> {
        T::decorator_list(self)
    }

    #[inline(always)]
    fn returns(&self) -> Option<<Self::Ast as Ast>::Expr<'_>> {
        T::returns(self)
    }

    #[inline(always)]
    fn type_comment(&self) -> Option<&str> {
        T::type_comment(self)
    }
}

pub trait AsyncFunctionDef {
    type Ast: Ast;
    type BodyIter<'a>: Iterator<Item = <Self::Ast as Ast>::Stmt<'a>>
    where
        Self: 'a;
    type DecoratorListIter<'a>: Iterator<Item = <Self::Ast as Ast>::Expr<'a>>
    where
        Self: 'a;
    fn name(&self) -> <Self::Ast as Ast>::Ident<'_>;
    fn args(&self) -> <Self::Ast as Ast>::Arguments<'_>;
    fn body(&self) -> Self::BodyIter<'_>;
    fn decorator_list(&self) -> Self::DecoratorListIter<'_>;
    fn returns(&self) -> Option<<Self::Ast as Ast>::Expr<'_>>;
    fn type_comment(&self) -> Option<&str>;
}

impl<'a, T: AsyncFunctionDef> AsyncFunctionDef for &'a T {
    type Ast = T::Ast;
    type BodyIter<'b> = T::BodyIter<'b>
    where
        'a: 'b;
    type DecoratorListIter<'b> = T::DecoratorListIter<'b>
    where
        'a: 'b;

    #[inline(always)]
    fn name(&self) -> <Self::Ast as Ast>::Ident<'_> {
        T::name(self)
    }

    #[inline(always)]
    fn args(&self) -> <Self::Ast as Ast>::Arguments<'_> {
        T::args(self)
    }

    #[inline(always)]
    fn body(&self) -> Self::BodyIter<'_> {
        T::body(self)
    }

    #[inline(always)]
    fn decorator_list(&self) -> Self::DecoratorListIter<'_> {
        T::decorator_list(self)
    }

    #[inline(always)]
    fn returns(&self) -> Option<<Self::Ast as Ast>::Expr<'_>> {
        T::returns(self)
    }

    #[inline(always)]
    fn type_comment(&self) -> Option<&str> {
        T::type_comment(self)
    }
}

pub trait ClassDef {
    type Ast: Ast;
    type BasesIter<'a>: Iterator<Item = <Self::Ast as Ast>::Expr<'a>>
    where
        Self: 'a;
    type KeywordsIter<'a>: Iterator<Item = <Self::Ast as Ast>::Keyword<'a>>
    where
        Self: 'a;
    type BodyIter<'a>: Iterator<Item = <Self::Ast as Ast>::Stmt<'a>>
    where
        Self: 'a;
    type DecoratorListIter<'a>: Iterator<Item = <Self::Ast as Ast>::Expr<'a>>
    where
        Self: 'a;
    fn name(&self) -> <Self::Ast as Ast>::Ident<'_>;
    fn bases(&self) -> Self::BasesIter<'_>;
    fn keywords(&self) -> Self::KeywordsIter<'_>;
    fn body(&self) -> Self::BodyIter<'_>;
    fn decorator_list(&self) -> Self::DecoratorListIter<'_>;
}

impl<'a, T: ClassDef> ClassDef for &'a T {
    type Ast = T::Ast;
    type BasesIter<'b> = T::BasesIter<'b>
    where
        'a: 'b;
    type BodyIter<'b> = T::BodyIter<'b>
    where
        'a: 'b;
    type DecoratorListIter<'b> = T::DecoratorListIter<'b>
    where
        'a: 'b;
    type KeywordsIter<'b> = T::KeywordsIter<'b>
    where
        'a: 'b;

    #[inline(always)]
    fn name(&self) -> <Self::Ast as Ast>::Ident<'_> {
        T::name(self)
    }

    #[inline(always)]
    fn bases(&self) -> Self::BasesIter<'_> {
        T::bases(self)
    }

    #[inline(always)]
    fn keywords(&self) -> Self::KeywordsIter<'_> {
        T::keywords(self)
    }

    #[inline(always)]
    fn body(&self) -> Self::BodyIter<'_> {
        T::body(self)
    }

    #[inline(always)]
    fn decorator_list(&self) -> Self::DecoratorListIter<'_> {
        T::decorator_list(self)
    }
}

pub trait Return {
    type Ast: Ast;
    fn value(&self) -> Option<<Self::Ast as Ast>::Expr<'_>>;
}

impl<'a, T: Return> Return for &'a T {
    type Ast = T::Ast;

    #[inline(always)]
    fn value(&self) -> Option<<Self::Ast as Ast>::Expr<'_>> {
        T::value(self)
    }
}

pub trait Delete {
    type Ast: Ast;
    type TargetsIter<'a>: Iterator<Item = <Self::Ast as Ast>::Expr<'a>>
    where
        Self: 'a;
    fn targets(&self) -> Self::TargetsIter<'_>;
}

impl<'a, T: Delete> Delete for &'a T {
    type Ast = T::Ast;
    type TargetsIter<'b> = T::TargetsIter<'b>
    where
        'a: 'b;

    #[inline(always)]
    fn targets(&self) -> Self::TargetsIter<'_> {
        T::targets(self)
    }
}

pub trait Assign {
    type Ast: Ast;
    type TargetsIter<'a>: Iterator<Item = <Self::Ast as Ast>::Expr<'a>>
    where
        Self: 'a;
    fn targets(&self) -> Self::TargetsIter<'_>;
    fn value(&self) -> <Self::Ast as Ast>::Expr<'_>;
    fn type_comment(&self) -> Option<&str>;
}

impl<'a, T: Assign> Assign for &'a T {
    type Ast = T::Ast;
    type TargetsIter<'b> = T::TargetsIter<'b>
    where
        'a: 'b;

    #[inline(always)]
    fn targets(&self) -> Self::TargetsIter<'_> {
        T::targets(self)
    }

    #[inline(always)]
    fn value(&self) -> <Self::Ast as Ast>::Expr<'_> {
        T::value(self)
    }

    #[inline(always)]
    fn type_comment(&self) -> Option<&str> {
        T::type_comment(self)
    }
}

pub trait AugAssign {
    type Ast: Ast;
    fn target(&self) -> <Self::Ast as Ast>::Expr<'_>;
    fn op(&self) -> Operator;
    fn value(&self) -> <Self::Ast as Ast>::Expr<'_>;
}

impl<'a, T: AugAssign> AugAssign for &'a T {
    type Ast = T::Ast;

    #[inline(always)]
    fn target(&self) -> <Self::Ast as Ast>::Expr<'_> {
        T::target(self)
    }

    #[inline(always)]
    fn op(&self) -> Operator {
        T::op(self)
    }

    #[inline(always)]
    fn value(&self) -> <Self::Ast as Ast>::Expr<'_> {
        T::value(self)
    }
}

pub trait AnnAssign {
    type Ast: Ast;
    fn target(&self) -> <Self::Ast as Ast>::Expr<'_>;
    fn annotation(&self) -> <Self::Ast as Ast>::Expr<'_>;
    fn value(&self) -> Option<<Self::Ast as Ast>::Expr<'_>>;
    fn simple(&self) -> usize;
}

impl<'a, T: AnnAssign> AnnAssign for &'a T {
    type Ast = T::Ast;

    #[inline(always)]
    fn target(&self) -> <Self::Ast as Ast>::Expr<'_> {
        T::target(self)
    }

    #[inline(always)]
    fn annotation(&self) -> <Self::Ast as Ast>::Expr<'_> {
        T::annotation(self)
    }

    #[inline(always)]
    fn value(&self) -> Option<<Self::Ast as Ast>::Expr<'_>> {
        T::value(self)
    }

    #[inline(always)]
    fn simple(&self) -> usize {
        T::simple(self)
    }
}

// TODO add type_comment associated type

pub trait For {
    type Ast: Ast;
    type BodyIter<'a>: Iterator<Item = <Self::Ast as Ast>::Stmt<'a>>
    where
        Self: 'a;
    type OrelseIter<'a>: Iterator<Item = <Self::Ast as Ast>::Stmt<'a>>
    where
        Self: 'a;
    fn target(&self) -> <Self::Ast as Ast>::Expr<'_>;
    fn iter(&self) -> <Self::Ast as Ast>::Expr<'_>;
    fn body(&self) -> Self::BodyIter<'_>;
    fn orelse(&self) -> Self::OrelseIter<'_>;
    fn type_comment(&self) -> Option<&str>;
}

impl<'a, T: For> For for &'a T {
    type Ast = T::Ast;
    type BodyIter<'b> = T::BodyIter<'b>
    where
        'a: 'b;
    type OrelseIter<'b> = T::OrelseIter<'b>
    where
        'a: 'b;

    #[inline(always)]
    fn target(&self) -> <Self::Ast as Ast>::Expr<'_> {
        T::target(self)
    }

    #[inline(always)]
    fn iter(&self) -> <Self::Ast as Ast>::Expr<'_> {
        T::iter(self)
    }

    #[inline(always)]
    fn body(&self) -> Self::BodyIter<'_> {
        T::body(self)
    }

    #[inline(always)]
    fn orelse(&self) -> Self::OrelseIter<'_> {
        T::orelse(self)
    }

    #[inline(always)]
    fn type_comment(&self) -> Option<&str> {
        T::type_comment(self)
    }
}

pub trait AsyncFor {
    type Ast: Ast;
    type BodyIter<'a>: Iterator<Item = <Self::Ast as Ast>::Stmt<'a>>
    where
        Self: 'a;
    type OrelseIter<'a>: Iterator<Item = <Self::Ast as Ast>::Stmt<'a>>
    where
        Self: 'a;
    fn target(&self) -> <Self::Ast as Ast>::Expr<'_>;
    fn iter(&self) -> <Self::Ast as Ast>::Expr<'_>;
    fn body(&self) -> Self::BodyIter<'_>;
    fn orelse(&self) -> Self::OrelseIter<'_>;
    fn type_comment(&self) -> Option<&str>;
}

impl<'a, T: AsyncFor> AsyncFor for &'a T {
    type Ast = T::Ast;
    type BodyIter<'b> = T::BodyIter<'b>
    where
        'a: 'b;
    type OrelseIter<'b> = T::OrelseIter<'b>
    where
        'a: 'b;

    #[inline(always)]
    fn target(&self) -> <Self::Ast as Ast>::Expr<'_> {
        T::target(self)
    }

    #[inline(always)]
    fn iter(&self) -> <Self::Ast as Ast>::Expr<'_> {
        T::iter(self)
    }

    #[inline(always)]
    fn body(&self) -> Self::BodyIter<'_> {
        T::body(self)
    }

    #[inline(always)]
    fn orelse(&self) -> Self::OrelseIter<'_> {
        T::orelse(self)
    }

    #[inline(always)]
    fn type_comment(&self) -> Option<&str> {
        T::type_comment(self)
    }
}

pub trait While {
    type Ast: Ast;
    type BodyIter<'a>: Iterator<Item = <Self::Ast as Ast>::Stmt<'a>>
    where
        Self: 'a;
    type OrelseIter<'a>: Iterator<Item = <Self::Ast as Ast>::Stmt<'a>>
    where
        Self: 'a;
    fn test(&self) -> <Self::Ast as Ast>::Expr<'_>;
    fn body(&self) -> Self::BodyIter<'_>;
    fn orelse(&self) -> Self::OrelseIter<'_>;
}

impl<'a, T: While> While for &'a T {
    type Ast = T::Ast;
    type BodyIter<'b> = T::BodyIter<'b>
    where
        'a: 'b;
    type OrelseIter<'b> = T::OrelseIter<'b>
    where
        'a: 'b;

    #[inline(always)]
    fn test(&self) -> <Self::Ast as Ast>::Expr<'_> {
        T::test(self)
    }

    #[inline(always)]
    fn body(&self) -> Self::BodyIter<'_> {
        T::body(self)
    }

    #[inline(always)]
    fn orelse(&self) -> Self::OrelseIter<'_> {
        T::orelse(self)
    }
}

pub trait If {
    type Ast: Ast;
    type BodyIter<'a>: Iterator<Item = <Self::Ast as Ast>::Stmt<'a>>
    where
        Self: 'a;
    type OrelseIter<'a>: Iterator<Item = <Self::Ast as Ast>::Stmt<'a>>
    where
        Self: 'a;
    fn test(&self) -> <Self::Ast as Ast>::Expr<'_>;
    fn body(&self) -> Self::BodyIter<'_>;
    fn orelse(&self) -> Self::OrelseIter<'_>;
}

impl<'a, T: If> If for &'a T {
    type Ast = T::Ast;
    type BodyIter<'b> = T::BodyIter<'b>
    where
        'a: 'b;
    type OrelseIter<'b> = T::OrelseIter<'b>
    where
        'a: 'b;

    #[inline(always)]
    fn test(&self) -> <Self::Ast as Ast>::Expr<'_> {
        T::test(self)
    }

    #[inline(always)]
    fn body(&self) -> Self::BodyIter<'_> {
        T::body(self)
    }

    #[inline(always)]
    fn orelse(&self) -> Self::OrelseIter<'_> {
        T::orelse(self)
    }
}

pub trait With {
    type Ast: Ast;
    type ItemsIter<'a>: Iterator<Item = <Self::Ast as Ast>::Withitem<'a>>
    where
        Self: 'a;
    type BodyIter<'a>: Iterator<Item = <Self::Ast as Ast>::Stmt<'a>>
    where
        Self: 'a;
    fn items(&self) -> Self::ItemsIter<'_>;
    fn body(&self) -> Self::BodyIter<'_>;
    fn type_comment(&self) -> Option<&str>;
}

impl<'a, T: With> With for &'a T {
    type Ast = T::Ast;
    type BodyIter<'b> = T::BodyIter<'b>
    where
        'a: 'b;
    type ItemsIter<'b> = T::ItemsIter<'b>
    where
        'a: 'b;

    #[inline(always)]
    fn items(&self) -> Self::ItemsIter<'_> {
        T::items(self)
    }

    #[inline(always)]
    fn body(&self) -> Self::BodyIter<'_> {
        T::body(self)
    }

    #[inline(always)]
    fn type_comment(&self) -> Option<&str> {
        T::type_comment(self)
    }
}

pub trait AsyncWith {
    type Ast: Ast;
    type ItemsIter<'a>: Iterator<Item = <Self::Ast as Ast>::Withitem<'a>>
    where
        Self: 'a;
    type BodyIter<'a>: Iterator<Item = <Self::Ast as Ast>::Stmt<'a>>
    where
        Self: 'a;
    fn items(&self) -> Self::ItemsIter<'_>;
    fn body(&self) -> Self::BodyIter<'_>;
    fn type_comment(&self) -> Option<&str>;
}

impl<'a, T: AsyncWith> AsyncWith for &'a T {
    type Ast = T::Ast;
    type BodyIter<'b> = T::BodyIter<'b>
    where
        'a: 'b;
    type ItemsIter<'b> = T::ItemsIter<'b>
    where
        'a: 'b;

    #[inline(always)]
    fn items(&self) -> Self::ItemsIter<'_> {
        T::items(self)
    }

    #[inline(always)]
    fn body(&self) -> Self::BodyIter<'_> {
        T::body(self)
    }

    #[inline(always)]
    fn type_comment(&self) -> Option<&str> {
        T::type_comment(self)
    }
}

pub trait Match {
    type Ast: Ast;
    type CasesIter<'a>: Iterator<Item = <Self::Ast as Ast>::MatchCase<'a>>
    where
        Self: 'a;
    fn subject(&self) -> <Self::Ast as Ast>::Expr<'_>;
    fn cases(&self) -> Self::CasesIter<'_>;
}

impl<'a, T: Match> Match for &'a T {
    type Ast = T::Ast;
    type CasesIter<'b> = T::CasesIter<'b>
    where
        'a: 'b;

    #[inline(always)]
    fn subject(&self) -> <Self::Ast as Ast>::Expr<'_> {
        T::subject(self)
    }

    #[inline(always)]
    fn cases(&self) -> Self::CasesIter<'_> {
        T::cases(self)
    }
}

pub trait Raise {
    type Ast: Ast;
    fn exc(&self) -> Option<<Self::Ast as Ast>::Expr<'_>>;
    fn cause(&self) -> Option<<Self::Ast as Ast>::Expr<'_>>;
}

impl<'a, T: Raise> Raise for &'a T {
    type Ast = T::Ast;

    #[inline(always)]
    fn exc(&self) -> Option<<Self::Ast as Ast>::Expr<'_>> {
        T::exc(self)
    }

    #[inline(always)]
    fn cause(&self) -> Option<<Self::Ast as Ast>::Expr<'_>> {
        T::cause(self)
    }
}

pub trait Try {
    type Ast: Ast;
    type BodyIter<'a>: Iterator<Item = <Self::Ast as Ast>::Stmt<'a>>
    where
        Self: 'a;
    type HandlersIter<'a>: Iterator<Item = <Self::Ast as Ast>::ExceptHandler<'a>>
    where
        Self: 'a;
    type OrelseIter<'a>: Iterator<Item = <Self::Ast as Ast>::Stmt<'a>>
    where
        Self: 'a;
    type FinalbodyIter<'a>: Iterator<Item = <Self::Ast as Ast>::Stmt<'a>>
    where
        Self: 'a;
    fn body(&self) -> Self::BodyIter<'_>;
    fn handlers(&self) -> Self::HandlersIter<'_>;
    fn orelse(&self) -> Self::OrelseIter<'_>;
    fn finalbody(&self) -> Self::FinalbodyIter<'_>;
}

impl<'a, T: Try> Try for &'a T {
    type Ast = T::Ast;
    type BodyIter<'b> = T::BodyIter<'b>
    where
        'a: 'b;
    type FinalbodyIter<'b> = T::FinalbodyIter<'b>
    where
        'a: 'b;
    type HandlersIter<'b> = T::HandlersIter<'b>
    where
        'a: 'b;
    type OrelseIter<'b> = T::OrelseIter<'b>
    where
        'a: 'b;

    #[inline(always)]
    fn body(&self) -> Self::BodyIter<'_> {
        T::body(self)
    }

    #[inline(always)]
    fn handlers(&self) -> Self::HandlersIter<'_> {
        T::handlers(self)
    }

    #[inline(always)]
    fn orelse(&self) -> Self::OrelseIter<'_> {
        T::orelse(self)
    }

    #[inline(always)]
    fn finalbody(&self) -> Self::FinalbodyIter<'_> {
        T::finalbody(self)
    }
}

pub trait Assert {
    type Ast: Ast;
    fn test(&self) -> <Self::Ast as Ast>::Expr<'_>;
    fn msg(&self) -> Option<<Self::Ast as Ast>::Expr<'_>>;
}

impl<'a, T: Assert> Assert for &'a T {
    type Ast = T::Ast;

    #[inline(always)]
    fn test(&self) -> <Self::Ast as Ast>::Expr<'_> {
        T::test(self)
    }

    #[inline(always)]
    fn msg(&self) -> Option<<Self::Ast as Ast>::Expr<'_>> {
        T::msg(self)
    }
}

pub trait Import {
    type Ast: Ast;
    type NamesIter<'a>: Iterator<Item = <Self::Ast as Ast>::Alias<'a>>
    where
        Self: 'a;
    fn names(&self) -> Self::NamesIter<'_>;
}

impl<'a, T: Import> Import for &'a T {
    type Ast = T::Ast;
    type NamesIter<'b> = T::NamesIter<'b>
    where
        'a: 'b;

    #[inline(always)]
    fn names(&self) -> Self::NamesIter<'_> {
        T::names(self)
    }
}

pub trait ImportFrom {
    type Ast: Ast;
    type NamesIter<'a>: Iterator<Item = <Self::Ast as Ast>::Alias<'a>>
    where
        Self: 'a;
    fn module(&self) -> Option<<Self::Ast as Ast>::Ident<'_>>;
    fn names(&self) -> Self::NamesIter<'_>;
    fn level(&self) -> Option<usize>;
}

impl<'a, T: ImportFrom> ImportFrom for &'a T {
    type Ast = T::Ast;
    type NamesIter<'b> = T::NamesIter<'b>
    where
        'a: 'b;

    #[inline(always)]
    fn module(&self) -> Option<<Self::Ast as Ast>::Ident<'_>> {
        T::module(self)
    }

    #[inline(always)]
    fn names(&self) -> Self::NamesIter<'_> {
        T::names(self)
    }

    #[inline(always)]
    fn level(&self) -> Option<usize> {
        T::level(self)
    }
}

pub trait Global {
    type Ast: Ast;
    type NamesIter<'a>: Iterator<Item = <Self::Ast as Ast>::Ident<'a>>
    where
        Self: 'a;
    fn names(&self) -> Self::NamesIter<'_>;
}

impl<'a, T: Global> Global for &'a T {
    type Ast = T::Ast;
    type NamesIter<'b> = T::NamesIter<'b>
    where
        'a: 'b;

    #[inline(always)]
    fn names(&self) -> Self::NamesIter<'_> {
        T::names(self)
    }
}

pub trait Nonlocal {
    type Ast: Ast;
    type NamesIter<'a>: Iterator<Item = <Self::Ast as Ast>::Ident<'a>>
    where
        Self: 'a;
    fn names(&self) -> Self::NamesIter<'_>;
}

impl<'a, T: Nonlocal> Nonlocal for &'a T {
    type Ast = T::Ast;
    type NamesIter<'b> = T::NamesIter<'b>
    where
        'a: 'b;

    #[inline(always)]
    fn names(&self) -> Self::NamesIter<'_> {
        T::names(self)
    }
}

pub enum StmtKind<
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
    FunctionDef(FUNCTIONDEF),
    AsyncFunctionDef(ASYNCFUNCTIONDEF),
    ClassDef(CLASSDEF),
    Return(RETURN),
    Delete(DELETE),
    Assign(ASSIGN),
    AugAssign(AUGASSIGN),
    AnnAssign(ANNASSIGN),
    For(FOR),
    AsyncFor(ASYNCFOR),
    While(WHILE),
    If(IF),
    With(WITH),
    AsyncWith(ASYNCWITH),
    Match(MATCH),
    Raise(RAISE),
    Try(TRY),
    Assert(ASSERT),
    Import(IMPORT),
    ImportFrom(IMPORTFROM),
    Global(GLOBAL),
    Nonlocal(NONLOCAL),
    Expr(EXPR),
    Pass,
    Break,
    Continue,
}

pub trait Stmt: Located {
    type Ast: Ast;
    fn stmt(
        &self,
    ) -> StmtKind<
        <Self::Ast as Ast>::FunctionDef<'_>,
        <Self::Ast as Ast>::AsyncFunctionDef<'_>,
        <Self::Ast as Ast>::ClassDef<'_>,
        <Self::Ast as Ast>::Return<'_>,
        <Self::Ast as Ast>::Delete<'_>,
        <Self::Ast as Ast>::Assign<'_>,
        <Self::Ast as Ast>::AugAssign<'_>,
        <Self::Ast as Ast>::AnnAssign<'_>,
        <Self::Ast as Ast>::For<'_>,
        <Self::Ast as Ast>::AsyncFor<'_>,
        <Self::Ast as Ast>::While<'_>,
        <Self::Ast as Ast>::If<'_>,
        <Self::Ast as Ast>::With<'_>,
        <Self::Ast as Ast>::AsyncWith<'_>,
        <Self::Ast as Ast>::Match<'_>,
        <Self::Ast as Ast>::Raise<'_>,
        <Self::Ast as Ast>::Try<'_>,
        <Self::Ast as Ast>::Assert<'_>,
        <Self::Ast as Ast>::Import<'_>,
        <Self::Ast as Ast>::ImportFrom<'_>,
        <Self::Ast as Ast>::Global<'_>,
        <Self::Ast as Ast>::Nonlocal<'_>,
        <Self::Ast as Ast>::Expr<'_>,
    >;
}

impl<'a, T: Stmt> Stmt for &'a T {
    type Ast = T::Ast;

    #[inline(always)]
    fn stmt(
        &self,
    ) -> StmtKind<
        <Self::Ast as Ast>::FunctionDef<'_>,
        <Self::Ast as Ast>::AsyncFunctionDef<'_>,
        <Self::Ast as Ast>::ClassDef<'_>,
        <Self::Ast as Ast>::Return<'_>,
        <Self::Ast as Ast>::Delete<'_>,
        <Self::Ast as Ast>::Assign<'_>,
        <Self::Ast as Ast>::AugAssign<'_>,
        <Self::Ast as Ast>::AnnAssign<'_>,
        <Self::Ast as Ast>::For<'_>,
        <Self::Ast as Ast>::AsyncFor<'_>,
        <Self::Ast as Ast>::While<'_>,
        <Self::Ast as Ast>::If<'_>,
        <Self::Ast as Ast>::With<'_>,
        <Self::Ast as Ast>::AsyncWith<'_>,
        <Self::Ast as Ast>::Match<'_>,
        <Self::Ast as Ast>::Raise<'_>,
        <Self::Ast as Ast>::Try<'_>,
        <Self::Ast as Ast>::Assert<'_>,
        <Self::Ast as Ast>::Import<'_>,
        <Self::Ast as Ast>::ImportFrom<'_>,
        <Self::Ast as Ast>::Global<'_>,
        <Self::Ast as Ast>::Nonlocal<'_>,
        <Self::Ast as Ast>::Expr<'_>,
    > {
        T::stmt(self)
    }
}

pub trait Ast {
    type Ident<'a>: Ident + 'a
    where
        Self: 'a;
    type TypeComment<'a>: TypeComment + 'a
    where
        Self: 'a;
    type Alias<'a>: Alias<Ast = Self> + 'a
    where
        Self: 'a;
    type Arg<'a>: Arg<Ast = Self> + 'a
    where
        Self: 'a;
    type Arguments<'a>: Arguments<Ast = Self> + 'a
    where
        Self: 'a;
    type Keyword<'a>: Keyword<Ast = Self> + 'a
    where
        Self: 'a;
    type BigInt<'a>: BigInt + 'a
    where
        Self: 'a;
    type Constant<'a>: Constant + 'a
    where
        Self: 'a;
    type Comprehension<'a>: Comprehension<Ast = Self> + 'a
    where
        Self: 'a;
    type BoolOp<'a>: BoolOp<Ast = Self> + 'a
    where
        Self: 'a;
    type NamedExpr<'a>: NamedExpr<Ast = Self> + 'a
    where
        Self: 'a;
    type BinOp<'a>: BinOp<Ast = Self> + 'a
    where
        Self: 'a;
    type UnaryOp<'a>: UnaryOp<Ast = Self> + 'a
    where
        Self: 'a;
    type Lambda<'a>: Lambda<Ast = Self> + 'a
    where
        Self: 'a;
    type IfExp<'a>: IfExp<Ast = Self> + 'a
    where
        Self: 'a;
    type Dict<'a>: Dict<Ast = Self> + 'a
    where
        Self: 'a;
    type Set<'a>: Set<Ast = Self> + 'a
    where
        Self: 'a;
    type ListComp<'a>: ListComp<Ast = Self> + 'a
    where
        Self: 'a;
    type SetComp<'a>: SetComp<Ast = Self> + 'a
    where
        Self: 'a;
    type DictComp<'a>: DictComp<Ast = Self> + 'a
    where
        Self: 'a;
    type GeneratorExp<'a>: GeneratorExp<Ast = Self> + 'a
    where
        Self: 'a;
    type Await<'a>: Await<Ast = Self> + 'a
    where
        Self: 'a;
    type Yield<'a>: Yield<Ast = Self> + 'a
    where
        Self: 'a;
    type YieldFrom<'a>: YieldFrom<Ast = Self> + 'a
    where
        Self: 'a;
    type Compare<'a>: Compare<Ast = Self> + 'a
    where
        Self: 'a;
    type Call<'a>: Call<Ast = Self> + 'a
    where
        Self: 'a;
    type FormattedValue<'a>: FormattedValue<Ast = Self> + 'a
    where
        Self: 'a;
    type JoinedStr<'a>: JoinedStr<Ast = Self> + 'a
    where
        Self: 'a;
    type ConstantExpr<'a>: ConstantExpr<Ast = Self> + 'a
    where
        Self: 'a;
    type Attribute<'a>: Attribute<Ast = Self> + 'a
    where
        Self: 'a;
    type Subscript<'a>: Subscript<Ast = Self> + 'a
    where
        Self: 'a;
    type Starred<'a>: Starred<Ast = Self> + 'a
    where
        Self: 'a;
    type Name<'a>: Name<Ast = Self> + 'a
    where
        Self: 'a;
    type List<'a>: List<Ast = Self> + 'a
    where
        Self: 'a;
    type Tuple<'a>: Tuple<Ast = Self> + 'a
    where
        Self: 'a;
    type Slice<'a>: Slice<Ast = Self> + 'a
    where
        Self: 'a;
    type Expr<'a>: Expr<Ast = Self> + 'a
    where
        Self: 'a;
    type ExceptHandler<'a>: ExceptHandler<Ast = Self> + 'a
    where
        Self: 'a;
    type MatchValue<'a>: MatchValue<Ast = Self> + 'a
    where
        Self: 'a;
    type MatchSingleton<'a>: MatchSingleton<Ast = Self> + 'a
    where
        Self: 'a;
    type MatchSequence<'a>: MatchSequence<Ast = Self> + 'a
    where
        Self: 'a;
    type MatchMapping<'a>: MatchMapping<Ast = Self> + 'a
    where
        Self: 'a;
    type MatchClass<'a>: MatchClass<Ast = Self> + 'a
    where
        Self: 'a;
    type MatchStar<'a>: MatchStar<Ast = Self> + 'a
    where
        Self: 'a;
    type MatchAs<'a>: MatchAs<Ast = Self> + 'a
    where
        Self: 'a;
    type MatchOr<'a>: MatchOr<Ast = Self> + 'a
    where
        Self: 'a;
    type Pattern<'a>: Pattern<Ast = Self> + 'a
    where
        Self: 'a;
    type Withitem<'a>: Withitem<Ast = Self> + 'a
    where
        Self: 'a;
    type MatchCase<'a>: MatchCase<Ast = Self> + 'a
    where
        Self: 'a;
    type FunctionDef<'a>: FunctionDef<Ast = Self> + 'a
    where
        Self: 'a;
    type AsyncFunctionDef<'a>: AsyncFunctionDef<Ast = Self> + 'a
    where
        Self: 'a;
    type ClassDef<'a>: ClassDef<Ast = Self> + 'a
    where
        Self: 'a;
    type Return<'a>: Return<Ast = Self> + 'a
    where
        Self: 'a;
    type Delete<'a>: Delete<Ast = Self> + 'a
    where
        Self: 'a;
    type Assign<'a>: Assign<Ast = Self> + 'a
    where
        Self: 'a;
    type AugAssign<'a>: AugAssign<Ast = Self> + 'a
    where
        Self: 'a;
    type AnnAssign<'a>: AnnAssign<Ast = Self> + 'a
    where
        Self: 'a;
    type For<'a>: For<Ast = Self> + 'a
    where
        Self: 'a;
    type AsyncFor<'a>: AsyncFor<Ast = Self> + 'a
    where
        Self: 'a;
    type While<'a>: While<Ast = Self> + 'a
    where
        Self: 'a;
    type If<'a>: If<Ast = Self> + 'a
    where
        Self: 'a;
    type With<'a>: With<Ast = Self> + 'a
    where
        Self: 'a;
    type AsyncWith<'a>: AsyncWith<Ast = Self> + 'a
    where
        Self: 'a;
    type Match<'a>: Match<Ast = Self> + 'a
    where
        Self: 'a;
    type Raise<'a>: Raise<Ast = Self> + 'a
    where
        Self: 'a;
    type Try<'a>: Try<Ast = Self> + 'a
    where
        Self: 'a;
    type Assert<'a>: Assert<Ast = Self> + 'a
    where
        Self: 'a;
    type Import<'a>: Import<Ast = Self> + 'a
    where
        Self: 'a;
    type ImportFrom<'a>: ImportFrom<Ast = Self> + 'a
    where
        Self: 'a;
    type Global<'a>: Global<Ast = Self> + 'a
    where
        Self: 'a;
    type Nonlocal<'a>: Nonlocal<Ast = Self> + 'a
    where
        Self: 'a;
    type Stmt<'a>: Stmt<Ast = Self> + 'a
    where
        Self: 'a;
}

pub mod rustpython_impl;
pub use rustpython_impl::RspyAst;
