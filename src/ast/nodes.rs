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
    type ValIter<'a>: Iterator<Item = char> + 'a
    where
        Self: 'a;
    fn val(&self) -> Self::ValIter<'_>;
}

// TODO(Seamooo) these general borrow impls should be macroed

impl<'a, T: Ident> Ident for &'a T {
    type ValIter<'b> = T::ValIter<'b>
    where 'a: 'b;

    #[inline(always)]
    fn val(&self) -> Self::ValIter<'_> {
        T::val(self)
    }
}

pub trait Alias {
    type Ident<'a>: Ident + 'a
    where
        Self: 'a;
    fn name(&self) -> Self::Ident<'_>;
    fn asname(&self) -> Option<Self::Ident<'_>>;
}

impl<'a, T: Alias> Alias for &'a T {
    type Ident<'b> = T::Ident<'b>
    where
        'a: 'b;

    #[inline(always)]
    fn name(&self) -> Self::Ident<'_> {
        T::name(self)
    }

    #[inline(always)]
    fn asname(&self) -> Option<Self::Ident<'_>> {
        T::asname(self)
    }
}

pub trait TypeComment {
    type ValIter<'a>: Iterator<Item = char> + 'a
    where
        Self: 'a;
    fn val(&self) -> Self::ValIter<'_>;
}

impl<'a, T: TypeComment> TypeComment for &'a T {
    type ValIter<'b> = T::ValIter<'b>
	where 'a: 'b;

    #[inline(always)]
    fn val(&self) -> Self::ValIter<'_> {
        T::val(self)
    }
}

pub trait Arg: Located {
    type Ident<'a>: Ident + 'a
    where
        Self: 'a;
    type Expr<'a>: Expr + 'a
    where
        Self: 'a;
    type TypeComment<'a>: TypeComment + 'a
    where
        Self: 'a;
    fn arg(&self) -> Self::Ident<'_>;
    fn annotation(&self) -> Option<Self::Expr<'_>>;
    fn type_comment(&self) -> Option<Self::TypeComment<'_>>;
}

impl<'a, T: Arg> Arg for &'a T {
    type Expr<'b> = T::Expr<'b>
    where
		'a: 'b;
    type Ident<'b> = T::Ident<'b>
    where
		'a: 'b;
    type TypeComment<'b> = T::TypeComment<'b>
    where
		'a: 'b;

    #[inline(always)]
    fn arg(&self) -> Self::Ident<'_> {
        T::arg(self)
    }

    #[inline(always)]
    fn annotation(&self) -> Option<Self::Expr<'_>> {
        T::annotation(self)
    }

    #[inline(always)]
    fn type_comment(&self) -> Option<Self::TypeComment<'_>> {
        T::type_comment(self)
    }
}

pub trait Arguments {
    type Arg<'a>: Arg + 'a
    where
        Self: 'a;
    type Expr<'a>: Expr + 'a
    where
        Self: 'a;
    type PosonlyargsIter<'a>: Iterator<Item = Self::Arg<'a>>
    where
        Self: 'a;
    type ArgsIter<'a>: Iterator<Item = Self::Arg<'a>>
    where
        Self: 'a;
    type KwonlyargsIter<'a>: Iterator<Item = Self::Arg<'a>>
    where
        Self: 'a;
    type KwDefaultsIter<'a>: Iterator<Item = Self::Expr<'a>>
    where
        Self: 'a;
    type DefaultsIter<'a>: Iterator<Item = Self::Expr<'a>>
    where
        Self: 'a;
    fn posonlyargs(&self) -> Self::PosonlyargsIter<'_>;
    fn args(&self) -> Self::ArgsIter<'_>;
    fn vararg(&self) -> Option<Self::Arg<'_>>;
    fn kwonlyargs(&self) -> Self::KwonlyargsIter<'_>;
    fn kw_defaults(&self) -> Self::KwDefaultsIter<'_>;
    fn kwarg(&self) -> Option<Self::Arg<'_>>;
    fn defaults(&self) -> Self::DefaultsIter<'_>;
}

impl<'a, T: Arguments> Arguments for &'a T {
    type Arg<'b> = T::Arg<'b>
    where
		'a: 'b;
    type ArgsIter<'b> = T::ArgsIter<'b>
    where
		'a: 'b;
    type DefaultsIter<'b> = T::DefaultsIter<'b>
    where
		'a: 'b;
    type Expr<'b> = T::Expr<'b>
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
    fn vararg(&self) -> Option<Self::Arg<'_>> {
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
    fn kwarg(&self) -> Option<Self::Arg<'_>> {
        T::kwarg(self)
    }

    #[inline(always)]
    fn defaults(&self) -> Self::DefaultsIter<'_> {
        T::defaults(self)
    }
}

pub trait Keyword {
    type Ident<'a>: Ident + 'a
    where
        Self: 'a;
    type Expr<'a>: Expr + 'a
    where
        Self: 'a;
    fn arg(&self) -> Option<Self::Ident<'_>>;
    fn value(&self) -> Self::Expr<'_>;
}

impl<'a, T: Keyword> Keyword for &'a T {
    type Expr<'b> = T::Expr<'b>
    where
		'a: 'b;
    type Ident<'b> = T::Ident<'b>
    where
		'a: 'b;

    #[inline(always)]
    fn arg(&self) -> Option<Self::Ident<'_>> {
        T::arg(self)
    }

    #[inline(always)]
    fn value(&self) -> Self::Expr<'_> {
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

// TODO if there's internal requirements of BigInt an interface should be
// required
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
    type Constant<'a>: Constant + 'a
    where
        Self: 'a;
    type BigInt<'a>: BigInt + 'a
    where
        Self: 'a;
    type StrIter<'a>: Iterator<Item = char> + 'a
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
    ) -> ConstantKind<Self::StrIter<'_>, Self::BytesIter<'_>, Self::TupleIter<'_>, Self::BigInt<'_>>;
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
    type StrIter<'b> = T::StrIter<'b>
    where
		'a: 'b;
    type TupleIter<'b> = T::TupleIter<'b>
    where
		'a: 'b;

    #[inline(always)]
    fn value(
        &self,
    ) -> ConstantKind<Self::StrIter<'_>, Self::BytesIter<'_>, Self::TupleIter<'_>, Self::BigInt<'_>>
    {
        T::value(self)
    }
}

pub trait Comprehension {
    type Expr<'a>: Expr + 'a
    where
        Self: 'a;
    type IfsIter<'a>: Iterator<Item = Self::Expr<'a>>
    where
        Self: 'a;
    fn target(&self) -> Self::Expr<'_>;
    fn iter(&self) -> Self::Expr<'_>;
    fn ifs(&self) -> Self::IfsIter<'_>;
    fn is_async(&self) -> usize;
}

impl<'a, T: Comprehension> Comprehension for &'a T {
    type Expr<'b> = T::Expr<'b>
    where
		'a: 'b;
    type IfsIter<'b> = T::IfsIter<'b>
    where
		'a: 'b;

    fn target(&self) -> Self::Expr<'_> {
        T::target(self)
    }

    #[inline(always)]
    fn iter(&self) -> Self::Expr<'_> {
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
    type Expr<'a>: Expr + 'a
    where
        Self: 'a;
    type ValuesIter<'a>: Iterator<Item = Self::Expr<'a>>
    where
        Self: 'a;
    fn op(&self) -> Boolop;
    fn values(&self) -> Self::ValuesIter<'_>;
}

impl<'a, T: BoolOp> BoolOp for &'a T {
    type Expr<'b> = T::Expr<'b>
    where
		'a: 'b;
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
    type Expr<'a>: Expr + 'a
    where
        Self: 'a;
    fn target(&self) -> Self::Expr<'_>;
    fn value(&self) -> Self::Expr<'_>;
}

impl<'a, T: NamedExpr> NamedExpr for &'a T {
    type Expr<'b> = T::Expr<'b>
    where
		'a: 'b;

    #[inline(always)]
    fn target(&self) -> Self::Expr<'_> {
        T::target(self)
    }

    #[inline(always)]
    fn value(&self) -> Self::Expr<'_> {
        T::value(self)
    }
}

pub trait BinOp {
    type Expr<'a>: Expr + 'a
    where
        Self: 'a;
    fn left(&self) -> Self::Expr<'_>;
    fn op(&self) -> Operator;
    fn right(&self) -> Self::Expr<'_>;
}

impl<'a, T: BinOp> BinOp for &'a T {
    type Expr<'b> = T::Expr<'b>
    where
		'a: 'b;

    #[inline(always)]
    fn left(&self) -> Self::Expr<'_> {
        T::left(self)
    }

    #[inline(always)]
    fn op(&self) -> Operator {
        T::op(self)
    }

    #[inline(always)]
    fn right(&self) -> Self::Expr<'_> {
        T::right(self)
    }
}

pub trait UnaryOp {
    type Expr<'a>: Expr + 'a
    where
        Self: 'a;
    fn op(&self) -> Unaryop;
    fn operand(&self) -> Self::Expr<'_>;
}

impl<'a, T: UnaryOp> UnaryOp for &'a T {
    type Expr<'b> = T::Expr<'b>
    where
		'a: 'b;

    #[inline(always)]
    fn op(&self) -> Unaryop {
        T::op(self)
    }

    #[inline(always)]
    fn operand(&self) -> Self::Expr<'_> {
        T::operand(self)
    }
}

pub trait Lambda {
    type Arguments<'a>: Arguments + 'a
    where
        Self: 'a;
    type Expr<'a>: Expr + 'a
    where
        Self: 'a;
    fn args(&self) -> Self::Arguments<'_>;
    fn body(&self) -> Self::Expr<'_>;
}

impl<'a, T: Lambda> Lambda for &'a T {
    type Arguments<'b> = T::Arguments<'b>
    where
		'a: 'b;
    type Expr<'b> = T::Expr<'b>
    where
		'a: 'b;

    #[inline(always)]
    fn args(&self) -> Self::Arguments<'_> {
        T::args(self)
    }

    #[inline(always)]
    fn body(&self) -> Self::Expr<'_> {
        T::body(self)
    }
}

pub trait IfExp {
    type Expr<'a>: Expr + 'a
    where
        Self: 'a;
    fn test(&self) -> Self::Expr<'_>;
    fn body(&self) -> Self::Expr<'_>;
    fn orelse(&self) -> Self::Expr<'_>;
}

impl<'a, T: IfExp> IfExp for &'a T {
    type Expr<'b> = T::Expr<'b>
    where
		'a: 'b;

    #[inline(always)]
    fn test(&self) -> Self::Expr<'_> {
        T::test(self)
    }

    #[inline(always)]
    fn body(&self) -> Self::Expr<'_> {
        T::body(self)
    }

    #[inline(always)]
    fn orelse(&self) -> Self::Expr<'_> {
        T::orelse(self)
    }
}

pub trait Dict {
    type Expr<'a>: Expr + 'a
    where
        Self: 'a;
    type KeysIter<'a>: Iterator<Item = Self::Expr<'a>>
    where
        Self: 'a;
    type ValuesIter<'a>: Iterator<Item = Self::Expr<'a>>
    where
        Self: 'a;
    fn keys(&self) -> Self::KeysIter<'_>;
    fn values(&self) -> Self::ValuesIter<'_>;
}

impl<'a, T: Dict> Dict for &'a T {
    type Expr<'b> = T::Expr<'b>
    where
		'a: 'b;
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
    type Expr<'a>: Expr + 'a
    where
        Self: 'a;
    type EltsIter<'a>: Iterator<Item = Self::Expr<'a>>
    where
        Self: 'a;
    fn elts(&self) -> Self::EltsIter<'_>;
}

impl<'a, T: Set> Set for &'a T {
    type EltsIter<'b> = T::EltsIter<'b>
    where
		'a: 'b;
    type Expr<'b> = T::Expr<'b>
    where
		'a: 'b;

    #[inline(always)]
    fn elts(&self) -> Self::EltsIter<'_> {
        T::elts(self)
    }
}

pub trait ListComp {
    type Expr<'a>: Expr + 'a
    where
        Self: 'a;
    type Comprehension<'a>: Comprehension + 'a
    where
        Self: 'a;
    type GeneratorsIter<'a>: Iterator<Item = Self::Comprehension<'a>>
    where
        Self: 'a;
    fn elt(&self) -> Self::Expr<'_>;
    fn generators(&self) -> Self::GeneratorsIter<'_>;
}

impl<'a, T: ListComp> ListComp for &'a T {
    type Comprehension<'b> = T::Comprehension<'b>
    where
		'a: 'b;
    type Expr<'b> = T::Expr<'b>
    where
		'a: 'b;
    type GeneratorsIter<'b> = T::GeneratorsIter<'b>
    where
		'a: 'b;

    #[inline(always)]
    fn elt(&self) -> Self::Expr<'_> {
        T::elt(self)
    }

    #[inline(always)]
    fn generators(&self) -> Self::GeneratorsIter<'_> {
        T::generators(self)
    }
}

pub trait SetComp {
    type Expr<'a>: Expr + 'a
    where
        Self: 'a;
    type Comprehension<'a>: Comprehension + 'a
    where
        Self: 'a;
    type GeneratorsIter<'a>: Iterator<Item = Self::Comprehension<'a>>
    where
        Self: 'a;
    fn elt(&self) -> Self::Expr<'_>;
    fn generators(&self) -> Self::GeneratorsIter<'_>;
}

impl<'a, T: SetComp> SetComp for &'a T {
    type Comprehension<'b> = T::Comprehension<'b>
    where
		'a: 'b;
    type Expr<'b> = T::Expr<'b>
    where
		'a: 'b;
    type GeneratorsIter<'b> = T::GeneratorsIter<'b>
    where
		'a: 'b;

    #[inline(always)]
    fn elt(&self) -> Self::Expr<'_> {
        T::elt(self)
    }

    #[inline(always)]
    fn generators(&self) -> Self::GeneratorsIter<'_> {
        T::generators(self)
    }
}

pub trait DictComp {
    type Expr<'a>: Expr + 'a
    where
        Self: 'a;
    type Comprehension<'a>: Comprehension + 'a
    where
        Self: 'a;
    type GeneratorsIter<'a>: Iterator<Item = Self::Comprehension<'a>>
    where
        Self: 'a;
    fn key(&self) -> Self::Expr<'_>;
    fn value(&self) -> Self::Expr<'_>;
    fn generators(&self) -> Self::GeneratorsIter<'_>;
}

impl<'a, T: DictComp> DictComp for &'a T {
    type Comprehension<'b> = T::Comprehension<'b>
    where
		'a: 'b;
    type Expr<'b> = T::Expr<'b>
    where
		'a: 'b;
    type GeneratorsIter<'b> = T::GeneratorsIter<'b>
    where
		'a: 'b;

    #[inline(always)]
    fn key(&self) -> Self::Expr<'_> {
        T::key(self)
    }

    #[inline(always)]
    fn value(&self) -> Self::Expr<'_> {
        T::value(self)
    }

    #[inline(always)]
    fn generators(&self) -> Self::GeneratorsIter<'_> {
        T::generators(self)
    }
}

pub trait GeneratorExp {
    type Expr<'a>: Expr + 'a
    where
        Self: 'a;
    type Comprehension<'a>: Comprehension + 'a
    where
        Self: 'a;
    type GeneratorsIter<'a>: Iterator<Item = Self::Comprehension<'a>>
    where
        Self: 'a;
    fn elt(&self) -> Self::Expr<'_>;
    fn generators(&self) -> Self::GeneratorsIter<'_>;
}

impl<'a, T: GeneratorExp> GeneratorExp for &'a T {
    type Comprehension<'b> = T::Comprehension<'b>
    where
		'a: 'b;
    type Expr<'b> = T::Expr<'b>
    where
		'a: 'b;
    type GeneratorsIter<'b> = T::GeneratorsIter<'b>
    where
		'a: 'b;

    #[inline(always)]
    fn elt(&self) -> Self::Expr<'_> {
        T::elt(self)
    }

    #[inline(always)]
    fn generators(&self) -> Self::GeneratorsIter<'_> {
        T::generators(self)
    }
}

pub trait Await {
    type Expr<'a>: Expr + 'a
    where
        Self: 'a;
    fn value(&self) -> Self::Expr<'_>;
}

impl<'a, T: Await> Await for &'a T {
    type Expr<'b> = T::Expr<'b>
    where
		'a: 'b;

    #[inline(always)]
    fn value(&self) -> Self::Expr<'_> {
        T::value(self)
    }
}

pub trait Yield {
    type Expr<'a>: Expr + 'a
    where
        Self: 'a;
    fn value(&self) -> Option<Self::Expr<'_>>;
}

impl<'a, T: Yield> Yield for &'a T {
    type Expr<'b> = T::Expr<'b>
    where
		'a: 'b;

    #[inline(always)]
    fn value(&self) -> Option<Self::Expr<'_>> {
        T::value(self)
    }
}

pub trait YieldFrom {
    type Expr<'a>: Expr + 'a
    where
        Self: 'a;
    fn value(&self) -> Self::Expr<'_>;
}

impl<'a, T: YieldFrom> YieldFrom for &'a T {
    type Expr<'b> = T::Expr<'b>
    where
        'a: 'b;

    #[inline(always)]
    fn value(&self) -> Self::Expr<'_> {
        T::value(self)
    }
}

pub trait Compare {
    type Expr<'a>: Expr + 'a
    where
        Self: 'a;
    type CmpopIter<'a>: Iterator<Item = Cmpop> + 'a
    where
        Self: 'a;
    fn left(&self) -> Self::Expr<'_>;
    fn ops(&self) -> Self::CmpopIter<'_>;
}

impl<'a, T: Compare> Compare for &'a T {
    type CmpopIter<'b> = T::CmpopIter<'b>
    where
        'a: 'b;
    type Expr<'b> = T::Expr<'b>
    where
        'a: 'b;

    #[inline(always)]
    fn left(&self) -> Self::Expr<'_> {
        T::left(self)
    }

    #[inline(always)]
    fn ops(&self) -> Self::CmpopIter<'_> {
        T::ops(self)
    }
}

pub trait Call {
    type Expr<'a>: Expr + 'a
    where
        Self: 'a;
    type Keyword<'a>: Keyword + 'a
    where
        Self: 'a;
    type ArgsIter<'a>: Iterator<Item = Self::Expr<'a>>
    where
        Self: 'a;
    type KeywordsIter<'a>: Iterator<Item = Self::Keyword<'a>>
    where
        Self: 'a;
    fn func(&self) -> Self::Expr<'_>;
    fn args(&self) -> Self::ArgsIter<'_>;
    fn keywords(&self) -> Self::KeywordsIter<'_>;
}

impl<'a, T: Call> Call for &'a T {
    type ArgsIter<'b> = T::ArgsIter<'b>
    where
		'a: 'b;
    type Expr<'b> = T::Expr<'b>
    where
		'a: 'b;
    type Keyword<'b> = T::Keyword<'b>
    where
		'a: 'b;
    type KeywordsIter<'b> = T::KeywordsIter<'b>
    where
		'a: 'b;

    #[inline(always)]
    fn func(&self) -> Self::Expr<'_> {
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
    type Expr<'a>: Expr + 'a
    where
        Self: 'a;
    fn value(&self) -> Self::Expr<'_>;
    fn conversion(&self) -> usize;
    fn format_spec(&self) -> Option<Self::Expr<'_>>;
}

impl<'a, T: FormattedValue> FormattedValue for &'a T {
    type Expr<'b> = T::Expr<'b>
    where
		'a: 'b;

    #[inline(always)]
    fn value(&self) -> Self::Expr<'_> {
        T::value(self)
    }

    #[inline(always)]
    fn conversion(&self) -> usize {
        T::conversion(self)
    }

    #[inline(always)]
    fn format_spec(&self) -> Option<Self::Expr<'_>> {
        T::format_spec(self)
    }
}

pub trait JoinedStr {
    type Expr<'a>: Expr + 'a
    where
        Self: 'a;
    type ValuesIter<'a>: Iterator<Item = Self::Expr<'a>>
    where
        Self: 'a;
    fn values(&self) -> Self::ValuesIter<'_>;
}

impl<'a, T: JoinedStr> JoinedStr for &'a T {
    type Expr<'b> = T::Expr<'b>
    where
		'a: 'b;
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
    type Constant<'a>: Constant + 'a
    where
        Self: 'a;
    fn value(&self) -> Self::Constant<'_>;
    fn kind(&self) -> Option<&str>;
}

impl<'a, T: ConstantExpr> ConstantExpr for &'a T {
    type Constant<'b> = T::Constant<'b>
    where
        'a: 'b;

    #[inline(always)]
    fn value(&self) -> Self::Constant<'_> {
        T::value(self)
    }

    #[inline(always)]
    fn kind(&self) -> Option<&str> {
        T::kind(self)
    }
}

pub trait Attribute {
    type Ident<'a>: Ident + 'a
    where
        Self: 'a;
    type Expr<'a>: Expr + 'a
    where
        Self: 'a;
    fn value(&self) -> Self::Expr<'_>;
    fn attr(&self) -> Self::Ident<'_>;
    fn ctx(&self) -> ExprContext;
}

impl<'a, T: Attribute> Attribute for &'a T {
    type Expr<'b> = T::Expr<'b>
    where
        'a: 'b;
    type Ident<'b> = T::Ident<'b>
    where
        'a: 'b;

    #[inline(always)]
    fn value(&self) -> Self::Expr<'_> {
        T::value(self)
    }

    #[inline(always)]
    fn attr(&self) -> Self::Ident<'_> {
        T::attr(self)
    }

    #[inline(always)]
    fn ctx(&self) -> ExprContext {
        T::ctx(self)
    }
}

pub trait Subscript {
    type Expr<'a>: Expr + 'a
    where
        Self: 'a;
    fn value(&self) -> Self::Expr<'_>;
    fn slice(&self) -> Self::Expr<'_>;
    fn ctx(&self) -> ExprContext;
}

impl<'a, T: Subscript> Subscript for &'a T {
    type Expr<'b> = T::Expr<'b>
    where
        'a: 'b;

    #[inline(always)]
    fn value(&self) -> Self::Expr<'_> {
        T::value(self)
    }

    #[inline(always)]
    fn slice(&self) -> Self::Expr<'_> {
        T::slice(self)
    }

    #[inline(always)]
    fn ctx(&self) -> ExprContext {
        T::ctx(self)
    }
}

pub trait Starred {
    type Expr<'a>: Expr + 'a
    where
        Self: 'a;
    fn value(&self) -> Self::Expr<'_>;
    fn ctx(&self) -> ExprContext;
}

impl<'a, T: Starred> Starred for &'a T {
    type Expr<'b> = T::Expr<'b>
    where
        'a: 'b;

    #[inline(always)]
    fn value(&self) -> Self::Expr<'_> {
        T::value(self)
    }

    #[inline(always)]
    fn ctx(&self) -> ExprContext {
        T::ctx(self)
    }
}

pub trait Name {
    type Ident<'a>: Ident + 'a
    where
        Self: 'a;
    fn id(&self) -> Self::Ident<'_>;
    fn ctx(&self) -> ExprContext;
}

impl<'a, T: Name> Name for &'a T {
    type Ident<'b> = T::Ident<'b>
    where
        'a: 'b;

    #[inline(always)]
    fn id(&self) -> Self::Ident<'_> {
        T::id(self)
    }

    #[inline(always)]
    fn ctx(&self) -> ExprContext {
        T::ctx(self)
    }
}

pub trait List {
    type Expr<'a>: Expr + 'a
    where
        Self: 'a;
    type EltsIter<'a>: Iterator<Item = Self::Expr<'a>>
    where
        Self: 'a;
    fn elts(&self) -> Self::EltsIter<'_>;
    fn ctx(&self) -> ExprContext;
}

impl<'a, T: List> List for &'a T {
    type EltsIter<'b> = T::EltsIter<'b>
    where
        'a: 'b;
    type Expr<'b> = T::Expr<'b>
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
    type Expr<'a>: Expr + 'a
    where
        Self: 'a;
    type EltsIter<'a>: Iterator<Item = Self::Expr<'a>>
    where
        Self: 'a;
    fn elts(&self) -> Self::EltsIter<'_>;
    fn ctx(&self) -> ExprContext;
}

impl<'a, T: Tuple> Tuple for &'a T {
    type EltsIter<'b> = T::EltsIter<'b>
    where
        'a: 'b;
    type Expr<'b> = T::Expr<'b>
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
    type Expr<'a>: Expr + 'a
    where
        Self: 'a;
    fn lower(&self) -> Option<Self::Expr<'_>>;
    fn upper(&self) -> Option<Self::Expr<'_>>;
    fn step(&self) -> Option<Self::Expr<'_>>;
}

impl<'a, T: Slice> Slice for &'a T {
    type Expr<'b> = T::Expr<'b>
    where
        'a: 'b;

    #[inline(always)]
    fn lower(&self) -> Option<Self::Expr<'_>> {
        T::lower(self)
    }

    #[inline(always)]
    fn upper(&self) -> Option<Self::Expr<'_>> {
        T::upper(self)
    }

    #[inline(always)]
    fn step(&self) -> Option<Self::Expr<'_>> {
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

#[allow(clippy::type_complexity)]
pub trait Expr: Located {
    type BoolOp<'a>: BoolOp + 'a
    where
        Self: 'a;
    type NamedExpr<'a>: NamedExpr + 'a
    where
        Self: 'a;
    type BinOp<'a>: BinOp + 'a
    where
        Self: 'a;
    type UnaryOp<'a>: UnaryOp + 'a
    where
        Self: 'a;
    type Lambda<'a>: Lambda + 'a
    where
        Self: 'a;
    type IfExp<'a>: IfExp + 'a
    where
        Self: 'a;
    type Dict<'a>: Dict + 'a
    where
        Self: 'a;
    type Set<'a>: Set + 'a
    where
        Self: 'a;
    type ListComp<'a>: ListComp + 'a
    where
        Self: 'a;
    type SetComp<'a>: SetComp + 'a
    where
        Self: 'a;
    type DictComp<'a>: DictComp + 'a
    where
        Self: 'a;
    type GeneratorExp<'a>: GeneratorExp + 'a
    where
        Self: 'a;
    type Await<'a>: Await + 'a
    where
        Self: 'a;
    type Yield<'a>: Yield + 'a
    where
        Self: 'a;
    type YieldFrom<'a>: YieldFrom + 'a
    where
        Self: 'a;
    type Compare<'a>: Compare + 'a
    where
        Self: 'a;
    type Call<'a>: Call + 'a
    where
        Self: 'a;
    type FormattedValue<'a>: FormattedValue + 'a
    where
        Self: 'a;
    type JoinedStr<'a>: JoinedStr + 'a
    where
        Self: 'a;
    type ConstantExpr<'a>: ConstantExpr + 'a
    where
        Self: 'a;
    type Attribute<'a>: Attribute + 'a
    where
        Self: 'a;
    type Subscript<'a>: Subscript + 'a
    where
        Self: 'a;
    type Starred<'a>: Starred + 'a
    where
        Self: 'a;
    type Name<'a>: Name + 'a
    where
        Self: 'a;
    type List<'a>: List + 'a
    where
        Self: 'a;
    type Tuple<'a>: Tuple + 'a
    where
        Self: 'a;
    type Slice<'a>: Slice + 'a
    where
        Self: 'a;
    fn expr(
        &self,
    ) -> ExprKind<
        Self::BoolOp<'_>,
        Self::NamedExpr<'_>,
        Self::BinOp<'_>,
        Self::UnaryOp<'_>,
        Self::Lambda<'_>,
        Self::IfExp<'_>,
        Self::Dict<'_>,
        Self::Set<'_>,
        Self::ListComp<'_>,
        Self::SetComp<'_>,
        Self::DictComp<'_>,
        Self::GeneratorExp<'_>,
        Self::Await<'_>,
        Self::Yield<'_>,
        Self::YieldFrom<'_>,
        Self::Compare<'_>,
        Self::Call<'_>,
        Self::FormattedValue<'_>,
        Self::JoinedStr<'_>,
        Self::ConstantExpr<'_>,
        Self::Attribute<'_>,
        Self::Subscript<'_>,
        Self::Starred<'_>,
        Self::Name<'_>,
        Self::List<'_>,
        Self::Tuple<'_>,
        Self::Slice<'_>,
    >;
}

impl<'a, T: Expr> Expr for &'a T {
    type Attribute<'b> = T::Attribute<'b>
    where
        'a: 'b;
    type Await<'b> = T::Await<'b>
    where
        'a: 'b;
    type BinOp<'b> = T::BinOp<'b>
    where
        'a: 'b;
    type BoolOp<'b> = T::BoolOp<'b>
    where
        'a: 'b;
    type Call<'b> = T::Call<'b>
    where
        'a: 'b;
    type Compare<'b> = T::Compare<'b>
    where
        'a: 'b;
    type ConstantExpr<'b> = T::ConstantExpr<'b>
    where
        'a: 'b;
    type Dict<'b> = T::Dict<'b>
    where
        'a: 'b;
    type DictComp<'b> = T::DictComp<'b>
    where
        'a: 'b;
    type FormattedValue<'b> = T::FormattedValue<'b>
    where
        'a: 'b;
    type GeneratorExp<'b> = T::GeneratorExp<'b>
    where
        'a: 'b;
    type IfExp<'b> = T::IfExp<'b>
    where
        'a: 'b;
    type JoinedStr<'b> = T::JoinedStr<'b>
    where
        'a: 'b;
    type Lambda<'b> = T::Lambda<'b>
    where
        'a: 'b;
    type List<'b> = T::List<'b>
    where
        'a: 'b;
    type ListComp<'b> = T::ListComp<'b>
    where
        'a: 'b;
    type Name<'b> = T::Name<'b>
    where
        'a: 'b;
    type NamedExpr<'b> = T::NamedExpr<'b>
    where
        'a: 'b;
    type Set<'b> = T::Set<'b>
    where
        'a: 'b;
    type SetComp<'b> = T::SetComp<'b>
    where
        'a: 'b;
    type Slice<'b> = T::Slice<'b>
    where
        'a: 'b;
    type Starred<'b> = T::Starred<'b>
    where
        'a: 'b;
    type Subscript<'b> = T::Subscript<'b>
    where
        'a: 'b;
    type Tuple<'b> = T::Tuple<'b>
    where
        'a: 'b;
    type UnaryOp<'b> = T::UnaryOp<'b>
    where
        'a: 'b;
    type Yield<'b> = T::Yield<'b>
    where
        'a: 'b;
    type YieldFrom<'b> = T::YieldFrom<'b>
    where
        'a: 'b;

    #[inline(always)]
    fn expr(
        &self,
    ) -> ExprKind<
        Self::BoolOp<'_>,
        Self::NamedExpr<'_>,
        Self::BinOp<'_>,
        Self::UnaryOp<'_>,
        Self::Lambda<'_>,
        Self::IfExp<'_>,
        Self::Dict<'_>,
        Self::Set<'_>,
        Self::ListComp<'_>,
        Self::SetComp<'_>,
        Self::DictComp<'_>,
        Self::GeneratorExp<'_>,
        Self::Await<'_>,
        Self::Yield<'_>,
        Self::YieldFrom<'_>,
        Self::Compare<'_>,
        Self::Call<'_>,
        Self::FormattedValue<'_>,
        Self::JoinedStr<'_>,
        Self::ConstantExpr<'_>,
        Self::Attribute<'_>,
        Self::Subscript<'_>,
        Self::Starred<'_>,
        Self::Name<'_>,
        Self::List<'_>,
        Self::Tuple<'_>,
        Self::Slice<'_>,
    > {
        T::expr(self)
    }
}

pub trait ExceptHandler: Located {
    type Ident<'a>: Ident + 'a
    where
        Self: 'a;
    type Expr<'a>: Expr + 'a
    where
        Self: 'a;
    type Stmt<'a>: Stmt + 'a
    where
        Self: 'a;
    type BodyIter<'a>: Iterator<Item = Self::Stmt<'a>>
    where
        Self: 'a;
    fn type_(&self) -> Option<Self::Expr<'_>>;
    fn name(&self) -> Option<Self::Ident<'_>>;
    fn body(&self) -> Self::BodyIter<'_>;
}

impl<'a, T: ExceptHandler> ExceptHandler for &'a T {
    type BodyIter<'b> = T::BodyIter<'b>
    where
        'a: 'b;
    type Expr<'b> = T::Expr<'b>
    where
        'a: 'b;
    type Ident<'b> = T::Ident<'b>
    where
        'a: 'b;
    type Stmt<'b> = T::Stmt<'b>
    where
        'a: 'b;

    #[inline(always)]
    fn type_(&self) -> Option<Self::Expr<'_>> {
        T::type_(self)
    }

    #[inline(always)]
    fn name(&self) -> Option<Self::Ident<'_>> {
        T::name(self)
    }

    #[inline(always)]
    fn body(&self) -> Self::BodyIter<'_> {
        T::body(self)
    }
}

pub trait MatchValue {
    type Expr<'a>: Expr + 'a
    where
        Self: 'a;
    fn value(&self) -> Self::Expr<'_>;
}

impl<'a, T: MatchValue> MatchValue for &'a T {
    type Expr<'b> = T::Expr<'b>
    where
        'a: 'b;

    #[inline(always)]
    fn value(&self) -> Self::Expr<'_> {
        T::value(self)
    }
}

pub trait MatchSingleton {
    type Constant<'a>: Constant + 'a
    where
        Self: 'a;
    fn value(&self) -> Self::Constant<'_>;
}

impl<'a, T: MatchSingleton> MatchSingleton for &'a T {
    type Constant<'b> = T::Constant<'b>
    where
        'a: 'b;

    #[inline(always)]
    fn value(&self) -> Self::Constant<'_> {
        T::value(self)
    }
}

pub trait MatchSequence {
    type Pattern<'a>: Pattern + 'a
    where
        Self: 'a;
    type PatternsIter<'a>: Iterator<Item = Self::Pattern<'a>>
    where
        Self: 'a;
    fn patterns(&self) -> Self::PatternsIter<'_>;
}

impl<'a, T: MatchSequence> MatchSequence for &'a T {
    type Pattern<'b> = T::Pattern<'b>
    where
        'a: 'b;
    type PatternsIter<'b> = T::PatternsIter<'b>
    where
        'a: 'b;

    #[inline(always)]
    fn patterns(&self) -> Self::PatternsIter<'_> {
        T::patterns(self)
    }
}

pub trait MatchMapping {
    type Ident<'a>: Ident + 'a
    where
        Self: 'a;
    type Expr<'a>: Expr + 'a
    where
        Self: 'a;
    type Pattern<'a>: Pattern + 'a
    where
        Self: 'a;
    type KeysIter<'a>: Iterator<Item = Self::Expr<'a>>
    where
        Self: 'a;
    type PatternsIter<'a>: Iterator<Item = Self::Pattern<'a>>
    where
        Self: 'a;
    fn keys(&self) -> Self::KeysIter<'_>;
    fn patterns(&self) -> Self::PatternsIter<'_>;
    fn rest(&self) -> Option<Self::Ident<'_>>;
}

impl<'a, T: MatchMapping> MatchMapping for &'a T {
    type Expr<'b> = T::Expr<'b>
    where
        'a: 'b;
    type Ident<'b> = T::Ident<'b>
    where
        'a: 'b;
    type KeysIter<'b> = T::KeysIter<'b>
    where
        'a: 'b;
    type Pattern<'b> = T::Pattern<'b>
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
    fn rest(&self) -> Option<Self::Ident<'_>> {
        T::rest(self)
    }
}

pub trait MatchClass {
    type Expr<'a>: Expr + 'a
    where
        Self: 'a;
    type Pattern<'a>: Pattern + 'a
    where
        Self: 'a;
    type Ident<'a>: Ident + 'a
    where
        Self: 'a;
    type PatternsIter<'a>: Iterator<Item = Self::Pattern<'a>>
    where
        Self: 'a;
    type KwdAttrsIter<'a>: Iterator<Item = Self::Ident<'a>>
    where
        Self: 'a;
    type KwdPatternsIter<'a>: Iterator<Item = Self::Pattern<'a>>
    where
        Self: 'a;
    fn cls(&self) -> Self::Expr<'_>;
    fn patterns(&self) -> Self::PatternsIter<'_>;
    fn kwd_attrs(&self) -> Self::KwdAttrsIter<'_>;
    fn kwd_patterns(&self) -> Self::KwdPatternsIter<'_>;
}

impl<'a, T: MatchClass> MatchClass for &'a T {
    type Expr<'b> = T::Expr<'b>
    where
        'a: 'b;
    type Ident<'b> = T::Ident<'b>
    where
        'a: 'b;
    type KwdAttrsIter<'b> = T::KwdAttrsIter<'b>
    where
        'a: 'b;
    type KwdPatternsIter<'b> = T::KwdPatternsIter<'b>
    where
        'a: 'b;
    type Pattern<'b> = T::Pattern<'b>
    where
        'a: 'b;
    type PatternsIter<'b> = T::PatternsIter<'b>
    where
        'a: 'b;

    #[inline(always)]
    fn cls(&self) -> Self::Expr<'_> {
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
    type Ident<'a>: Ident + 'a
    where
        Self: 'a;
    fn name(&self) -> Option<Self::Ident<'_>>;
}

impl<'a, T: MatchStar> MatchStar for &'a T {
    type Ident<'b> = T::Ident<'b>
    where
        'a: 'b;

    #[inline(always)]
    fn name(&self) -> Option<Self::Ident<'_>> {
        T::name(self)
    }
}

pub trait MatchAs {
    type Ident<'a>: Ident + 'a
    where
        Self: 'a;
    type Pattern<'a>: Pattern + 'a
    where
        Self: 'a;
    fn pattern(&self) -> Option<Self::Pattern<'_>>;
    fn name(&self) -> Option<Self::Ident<'_>>;
}

impl<'a, T: MatchAs> MatchAs for &'a T {
    type Ident<'b> = T::Ident<'b>
    where
        'a: 'b;
    type Pattern<'b> = T::Pattern<'b>
    where
        'a: 'b;

    #[inline(always)]
    fn pattern(&self) -> Option<Self::Pattern<'_>> {
        T::pattern(self)
    }

    #[inline(always)]
    fn name(&self) -> Option<Self::Ident<'_>> {
        T::name(self)
    }
}

pub trait MatchOr {
    type Pattern<'a>: Pattern + 'a
    where
        Self: 'a;
    type PatternsIter<'a>: Iterator<Item = Self::Pattern<'a>>
    where
        Self: 'a;
    fn patterns(&self) -> Self::PatternsIter<'_>;
}

impl<'a, T: MatchOr> MatchOr for &'a T {
    type Pattern<'b> = T::Pattern<'b>
    where
        'a: 'b;
    type PatternsIter<'b> = T::PatternsIter<'b>
    where
        'a: 'b;

    fn patterns(&self) -> Self::PatternsIter<'_> {
        T::patterns(self)
    }
}

// Type complexity required due to need to support circular
// associated types.
// Enum variant names correspond to python grammar
#[allow(clippy::type_complexity)]
#[allow(clippy::enum_variant_names)]
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

// Type complexity required due to need to support circular
// associated types.
#[allow(clippy::type_complexity)]
pub trait Pattern: Located {
    type MatchValue<'a>: MatchValue + 'a
    where
        Self: 'a;
    type MatchSingleton<'a>: MatchSingleton + 'a
    where
        Self: 'a;
    type MatchSequence<'a>: MatchSequence + 'a
    where
        Self: 'a;
    type MatchMapping<'a>: MatchMapping + 'a
    where
        Self: 'a;
    type MatchClass<'a>: MatchClass + 'a
    where
        Self: 'a;
    type MatchStar<'a>: MatchStar + 'a
    where
        Self: 'a;
    type MatchAs<'a>: MatchAs + 'a
    where
        Self: 'a;
    type MatchOr<'a>: MatchOr + 'a
    where
        Self: 'a;
    fn pattern(
        &self,
    ) -> PatternKind<
        Self::MatchValue<'_>,
        Self::MatchSingleton<'_>,
        Self::MatchSequence<'_>,
        Self::MatchMapping<'_>,
        Self::MatchClass<'_>,
        Self::MatchStar<'_>,
        Self::MatchAs<'_>,
        Self::MatchOr<'_>,
    >;
}

impl<'a, T: Pattern> Pattern for &'a T {
    type MatchAs<'b> = T::MatchAs<'b>
    where
        'a: 'b;
    type MatchClass<'b> = T::MatchClass<'b>
    where
        'a: 'b;
    type MatchMapping<'b> = T::MatchMapping<'b>
    where
        'a: 'b;
    type MatchOr<'b> = T::MatchOr<'b>
    where
        'a: 'b;
    type MatchSequence<'b> = T::MatchSequence<'b>
    where
        'a: 'b;
    type MatchSingleton<'b> = T::MatchSingleton<'b>
    where
        'a: 'b;
    type MatchStar<'b> = T::MatchStar<'b>
    where
        'a: 'b;
    type MatchValue<'b> = T::MatchValue<'b>
    where
        'a: 'b;

    #[inline(always)]
    fn pattern(
        &self,
    ) -> PatternKind<
        Self::MatchValue<'_>,
        Self::MatchSingleton<'_>,
        Self::MatchSequence<'_>,
        Self::MatchMapping<'_>,
        Self::MatchClass<'_>,
        Self::MatchStar<'_>,
        Self::MatchAs<'_>,
        Self::MatchOr<'_>,
    > {
        T::pattern(self)
    }
}

pub trait Withitem {
    type Expr<'a>: Expr + 'a
    where
        Self: 'a;
    fn context_expr(&self) -> Self::Expr<'_>;
    fn optional_vars(&self) -> Option<Self::Expr<'_>>;
}

impl<'a, T: Withitem> Withitem for &'a T {
    type Expr<'b> = T::Expr<'b>
    where
        Self: 'b;

    #[inline(always)]
    fn context_expr(&self) -> Self::Expr<'_> {
        T::context_expr(self)
    }

    #[inline(always)]
    fn optional_vars(&self) -> Option<Self::Expr<'_>> {
        T::optional_vars(self)
    }
}

pub trait MatchCase {
    type Expr<'a>: Expr + 'a
    where
        Self: 'a;
    type Pattern<'a>: Pattern + 'a
    where
        Self: 'a;
    type Stmt<'a>: Stmt + 'a
    where
        Self: 'a;
    type BodyIter<'a>: Iterator<Item = Self::Stmt<'a>>
    where
        Self: 'a;
    fn pattern(&self) -> Self::Pattern<'_>;
    fn guard(&self) -> Option<Self::Expr<'_>>;
    fn body(&self) -> Self::BodyIter<'_>;
}

impl<'a, T: MatchCase> MatchCase for &'a T {
    type BodyIter<'b> = T::BodyIter<'b>
    where
        'a: 'b;
    type Expr<'b> = T::Expr<'b>
    where
        'a: 'b;
    type Pattern<'b> = T::Pattern<'b>
    where
        'a: 'b;
    type Stmt<'b> = T::Stmt<'b>
    where
        'a: 'b;

    #[inline(always)]
    fn pattern(&self) -> Self::Pattern<'_> {
        T::pattern(self)
    }

    #[inline(always)]
    fn guard(&self) -> Option<Self::Expr<'_>> {
        T::guard(self)
    }

    #[inline(always)]
    fn body(&self) -> Self::BodyIter<'_> {
        T::body(self)
    }
}

// TODO add type comment associated type

pub trait FunctionDef {
    type Ident<'a>: Ident + 'a
    where
        Self: 'a;
    type Arguments<'a>: Arguments + 'a
    where
        Self: 'a;
    type Expr<'a>: Expr + 'a
    where
        Self: 'a;
    type Stmt<'a>: Stmt + 'a
    where
        Self: 'a;
    type BodyIter<'a>: Iterator<Item = Self::Stmt<'a>>
    where
        Self: 'a;
    type DecoratorListIter<'a>: Iterator<Item = Self::Expr<'a>>
    where
        Self: 'a;
    fn name(&self) -> Self::Ident<'_>;
    fn args(&self) -> Self::Arguments<'_>;
    fn body(&self) -> Self::BodyIter<'_>;
    fn decorator_list(&self) -> Self::DecoratorListIter<'_>;
    fn returns(&self) -> Option<Self::Expr<'_>>;
    fn type_comment(&self) -> Option<&str>;
}

impl<'a, T: FunctionDef> FunctionDef for &'a T {
    type Arguments<'b> = T::Arguments<'b>
    where
        'a: 'b;
    type BodyIter<'b> = T::BodyIter<'b>
    where
        'a: 'b;
    type DecoratorListIter<'b> = T::DecoratorListIter<'b>
    where
        'a: 'b;
    type Expr<'b> = T::Expr<'b>
    where
        'a: 'b;
    type Ident<'b> = T::Ident<'b>
    where
        'a: 'b;
    type Stmt<'b> = T::Stmt<'b>
    where
        'a: 'b;

    #[inline(always)]
    fn name(&self) -> Self::Ident<'_> {
        T::name(self)
    }

    #[inline(always)]
    fn args(&self) -> Self::Arguments<'_> {
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
    fn returns(&self) -> Option<Self::Expr<'_>> {
        T::returns(self)
    }

    #[inline(always)]
    fn type_comment(&self) -> Option<&str> {
        T::type_comment(self)
    }
}

pub trait AsyncFunctionDef {
    type Ident<'a>: Ident + 'a
    where
        Self: 'a;
    type Arguments<'a>: Arguments + 'a
    where
        Self: 'a;
    type Stmt<'a>: Stmt + 'a
    where
        Self: 'a;
    type Expr<'a>: Expr + 'a
    where
        Self: 'a;
    type BodyIter<'a>: Iterator<Item = Self::Stmt<'a>>
    where
        Self: 'a;
    type DecoratorListIter<'a>: Iterator<Item = Self::Expr<'a>>
    where
        Self: 'a;
    fn name(&self) -> Self::Ident<'_>;
    fn args(&self) -> Self::Arguments<'_>;
    fn body(&self) -> Self::BodyIter<'_>;
    fn decorator_list(&self) -> Self::DecoratorListIter<'_>;
    fn returns(&self) -> Option<Self::Expr<'_>>;
    fn type_comment(&self) -> Option<&str>;
}

impl<'a, T: AsyncFunctionDef> AsyncFunctionDef for &'a T {
    type Arguments<'b> = T::Arguments<'b>
    where
        'a: 'b;
    type BodyIter<'b> = T::BodyIter<'b>
    where
        'a: 'b;
    type DecoratorListIter<'b> = T::DecoratorListIter<'b>
    where
        'a: 'b;
    type Expr<'b> = T::Expr<'b>
    where
        'a: 'b;
    type Ident<'b> = T::Ident<'b>
    where
        'a: 'b;
    type Stmt<'b> = T::Stmt<'b>
    where
        'a: 'b;

    #[inline(always)]
    fn name(&self) -> Self::Ident<'_> {
        T::name(self)
    }

    #[inline(always)]
    fn args(&self) -> Self::Arguments<'_> {
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
    fn returns(&self) -> Option<Self::Expr<'_>> {
        T::returns(self)
    }

    #[inline(always)]
    fn type_comment(&self) -> Option<&str> {
        T::type_comment(self)
    }
}

pub trait ClassDef {
    type Ident<'a>: Ident + 'a
    where
        Self: 'a;
    type Stmt<'a>: Stmt + 'a
    where
        Self: 'a;
    type Expr<'a>: Expr + 'a
    where
        Self: 'a;
    type Keyword<'a>: Keyword + 'a
    where
        Self: 'a;
    type BasesIter<'a>: Iterator<Item = Self::Expr<'a>>
    where
        Self: 'a;
    type KeywordsIter<'a>: Iterator<Item = Self::Keyword<'a>>
    where
        Self: 'a;
    type BodyIter<'a>: Iterator<Item = Self::Stmt<'a>>
    where
        Self: 'a;
    type DecoratorListIter<'a>: Iterator<Item = Self::Expr<'a>>
    where
        Self: 'a;
    fn name(&self) -> Self::Ident<'_>;
    fn bases(&self) -> Self::BasesIter<'_>;
    fn keywords(&self) -> Self::KeywordsIter<'_>;
    fn body(&self) -> Self::BodyIter<'_>;
    fn decorator_list(&self) -> Self::DecoratorListIter<'_>;
}

impl<'a, T: ClassDef> ClassDef for &'a T {
    type BasesIter<'b> = T::BasesIter<'b>
    where
        'a: 'b;
    type BodyIter<'b> = T::BodyIter<'b>
    where
        'a: 'b;
    type DecoratorListIter<'b> = T::DecoratorListIter<'b>
    where
        'a: 'b;
    type Expr<'b> = T::Expr<'b>
    where
        'a: 'b;
    type Ident<'b> = T::Ident<'b>
    where
        'a: 'b;
    type Keyword<'b> = T::Keyword<'b>
    where
        'a: 'b;
    type KeywordsIter<'b> = T::KeywordsIter<'b>
    where
        'a: 'b;
    type Stmt<'b> = T::Stmt<'b>
    where
        'a: 'b;

    #[inline(always)]
    fn name(&self) -> Self::Ident<'_> {
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
    type Expr<'a>: Expr + 'a
    where
        Self: 'a;
    fn value(&self) -> Option<Self::Expr<'_>>;
}

impl<'a, T: Return> Return for &'a T {
    type Expr<'b> = T::Expr<'b>
    where
        'a: 'b;

    #[inline(always)]
    fn value(&self) -> Option<Self::Expr<'_>> {
        T::value(self)
    }
}

pub trait Delete {
    type Expr<'a>: Expr + 'a
    where
        Self: 'a;
    type TargetsIter<'a>: Iterator<Item = Self::Expr<'a>>
    where
        Self: 'a;
    fn targets(&self) -> Self::TargetsIter<'_>;
}

impl<'a, T: Delete> Delete for &'a T {
    type Expr<'b> = T::Expr<'b>
    where
        'a: 'b;
    type TargetsIter<'b> = T::TargetsIter<'b>
    where
        'a: 'b;

    #[inline(always)]
    fn targets(&self) -> Self::TargetsIter<'_> {
        T::targets(self)
    }
}

pub trait Assign {
    type Expr<'a>: Expr + 'a
    where
        Self: 'a;
    type TargetsIter<'a>: Iterator<Item = Self::Expr<'a>>
    where
        Self: 'a;
    fn targets(&self) -> Self::TargetsIter<'_>;
    fn value(&self) -> Self::Expr<'_>;
    fn type_comment(&self) -> Option<&str>;
}

impl<'a, T: Assign> Assign for &'a T {
    type Expr<'b> = T::Expr<'b>
    where
        'a: 'b;
    type TargetsIter<'b> = T::TargetsIter<'b>
    where
        'a: 'b;

    #[inline(always)]
    fn targets(&self) -> Self::TargetsIter<'_> {
        T::targets(self)
    }

    #[inline(always)]
    fn value(&self) -> Self::Expr<'_> {
        T::value(self)
    }

    #[inline(always)]
    fn type_comment(&self) -> Option<&str> {
        T::type_comment(self)
    }
}

pub trait AugAssign {
    type Expr<'a>: Expr + 'a
    where
        Self: 'a;
    fn target(&self) -> Self::Expr<'_>;
    fn op(&self) -> Operator;
    fn value(&self) -> Self::Expr<'_>;
}

impl<'a, T: AugAssign> AugAssign for &'a T {
    type Expr<'b> = T::Expr<'b>
    where
        'a: 'b;

    #[inline(always)]
    fn target(&self) -> Self::Expr<'_> {
        T::target(self)
    }

    #[inline(always)]
    fn op(&self) -> Operator {
        T::op(self)
    }

    #[inline(always)]
    fn value(&self) -> Self::Expr<'_> {
        T::value(self)
    }
}

pub trait AnnAssign {
    type Expr<'a>: Expr + 'a
    where
        Self: 'a;
    fn target(&self) -> Self::Expr<'_>;
    fn annotation(&self) -> Self::Expr<'_>;
    fn value(&self) -> Option<Self::Expr<'_>>;
    fn simple(&self) -> usize;
}

impl<'a, T: AnnAssign> AnnAssign for &'a T {
    type Expr<'b> = T::Expr<'b>
    where
        'a: 'b;

    #[inline(always)]
    fn target(&self) -> Self::Expr<'_> {
        T::target(self)
    }

    #[inline(always)]
    fn annotation(&self) -> Self::Expr<'_> {
        T::annotation(self)
    }

    #[inline(always)]
    fn value(&self) -> Option<Self::Expr<'_>> {
        T::value(self)
    }

    #[inline(always)]
    fn simple(&self) -> usize {
        T::simple(self)
    }
}

// TODO add type_comment associated type

pub trait For {
    type Expr<'a>: Expr + 'a
    where
        Self: 'a;
    type Stmt<'a>: Stmt + 'a
    where
        Self: 'a;
    type BodyIter<'a>: Iterator<Item = Self::Stmt<'a>>
    where
        Self: 'a;
    type OrelseIter<'a>: Iterator<Item = Self::Stmt<'a>>
    where
        Self: 'a;
    fn target(&self) -> Self::Expr<'_>;
    fn iter(&self) -> Self::Expr<'_>;
    fn body(&self) -> Self::BodyIter<'_>;
    fn orelse(&self) -> Self::OrelseIter<'_>;
    fn type_comment(&self) -> Option<&str>;
}

impl<'a, T: For> For for &'a T {
    type BodyIter<'b> = T::BodyIter<'b>
    where
        'a: 'b;
    type Expr<'b> = T::Expr<'b>
    where
        'a: 'b;
    type OrelseIter<'b> = T::OrelseIter<'b>
    where
        'a: 'b;
    type Stmt<'b> = T::Stmt<'b>
    where
        'a: 'b;

    #[inline(always)]
    fn target(&self) -> Self::Expr<'_> {
        T::target(self)
    }

    #[inline(always)]
    fn iter(&self) -> Self::Expr<'_> {
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
    type Expr<'a>: Expr + 'a
    where
        Self: 'a;
    type Stmt<'a>: Stmt + 'a
    where
        Self: 'a;
    type BodyIter<'a>: Iterator<Item = Self::Stmt<'a>>
    where
        Self: 'a;
    type OrelseIter<'a>: Iterator<Item = Self::Stmt<'a>>
    where
        Self: 'a;
    fn target(&self) -> Self::Expr<'_>;
    fn iter(&self) -> Self::Expr<'_>;
    fn body(&self) -> Self::BodyIter<'_>;
    fn orelse(&self) -> Self::OrelseIter<'_>;
    fn type_comment(&self) -> Option<&str>;
}

impl<'a, T: AsyncFor> AsyncFor for &'a T {
    type BodyIter<'b> = T::BodyIter<'b>
    where
        'a: 'b;
    type Expr<'b> = T::Expr<'b>
    where
        'a: 'b;
    type OrelseIter<'b> = T::OrelseIter<'b>
    where
        'a: 'b;
    type Stmt<'b> = T::Stmt<'b>
    where
        'a: 'b;

    #[inline(always)]
    fn target(&self) -> Self::Expr<'_> {
        T::target(self)
    }

    #[inline(always)]
    fn iter(&self) -> Self::Expr<'_> {
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
    type Expr<'a>: Expr + 'a
    where
        Self: 'a;
    type Stmt<'a>: Stmt + 'a
    where
        Self: 'a;
    type BodyIter<'a>: Iterator<Item = Self::Stmt<'a>>
    where
        Self: 'a;
    type OrelseIter<'a>: Iterator<Item = Self::Stmt<'a>>
    where
        Self: 'a;
    fn test(&self) -> Self::Expr<'_>;
    fn body(&self) -> Self::BodyIter<'_>;
    fn orelse(&self) -> Self::OrelseIter<'_>;
}

impl<'a, T: While> While for &'a T {
    type BodyIter<'b> = T::BodyIter<'b>
    where
        'a: 'b;
    type Expr<'b> = T::Expr<'b>
    where
        'a: 'b;
    type OrelseIter<'b> = T::OrelseIter<'b>
    where
        'a: 'b;
    type Stmt<'b> = T::Stmt<'b>
    where
        'a: 'b;

    #[inline(always)]
    fn test(&self) -> Self::Expr<'_> {
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
    type Expr<'a>: Expr + 'a
    where
        Self: 'a;
    type Stmt<'a>: Stmt + 'a
    where
        Self: 'a;
    type BodyIter<'a>: Iterator<Item = Self::Stmt<'a>>
    where
        Self: 'a;
    type OrelseIter<'a>: Iterator<Item = Self::Stmt<'a>>
    where
        Self: 'a;
    fn test(&self) -> Self::Expr<'_>;
    fn body(&self) -> Self::BodyIter<'_>;
    fn orelse(&self) -> Self::OrelseIter<'_>;
}

impl<'a, T: If> If for &'a T {
    type BodyIter<'b> = T::BodyIter<'b>
    where
        'a: 'b;
    type Expr<'b> = T::Expr<'b>
    where
        'a: 'b;
    type OrelseIter<'b> = T::OrelseIter<'b>
    where
        'a: 'b;
    type Stmt<'b> = T::Stmt<'b>
    where
        'a: 'b;

    #[inline(always)]
    fn test(&self) -> Self::Expr<'_> {
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
    type Withitem<'a>: Withitem + 'a
    where
        Self: 'a;
    type Stmt<'a>: Stmt + 'a
    where
        Self: 'a;
    type ItemsIter<'a>: Iterator<Item = Self::Withitem<'a>>
    where
        Self: 'a;
    type BodyIter<'a>: Iterator<Item = Self::Stmt<'a>>
    where
        Self: 'a;
    fn items(&self) -> Self::ItemsIter<'_>;
    fn body(&self) -> Self::BodyIter<'_>;
    fn type_comment(&self) -> Option<&str>;
}

impl<'a, T: With> With for &'a T {
    type BodyIter<'b> = T::BodyIter<'b>
    where
        'a: 'b;
    type ItemsIter<'b> = T::ItemsIter<'b>
    where
        'a: 'b;
    type Stmt<'b> = T::Stmt<'b>
    where
        'a: 'b;
    type Withitem<'b> = T::Withitem<'b>
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
    type Withitem<'a>: Withitem + 'a
    where
        Self: 'a;
    type Stmt<'a>: Stmt + 'a
    where
        Self: 'a;
    type ItemsIter<'a>: Iterator<Item = Self::Withitem<'a>>
    where
        Self: 'a;
    type BodyIter<'a>: Iterator<Item = Self::Stmt<'a>>
    where
        Self: 'a;
    fn items(&self) -> Self::ItemsIter<'_>;
    fn body(&self) -> Self::BodyIter<'_>;
    fn type_comment(&self) -> Option<&str>;
}

impl<'a, T: AsyncWith> AsyncWith for &'a T {
    type BodyIter<'b> = T::BodyIter<'b>
    where
        'a: 'b;
    type ItemsIter<'b> = T::ItemsIter<'b>
    where
        'a: 'b;
    type Stmt<'b> = T::Stmt<'b>
    where
        'a: 'b;
    type Withitem<'b> = T::Withitem<'b>
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
    type Expr<'a>: Expr + 'a
    where
        Self: 'a;
    type MatchCase<'a>: MatchCase + 'a
    where
        Self: 'a;
    type CasesIter<'a>: Iterator<Item = Self::MatchCase<'a>>
    where
        Self: 'a;
    fn subject(&self) -> Self::Expr<'_>;
    fn cases(&self) -> Self::CasesIter<'_>;
}

impl<'a, T: Match> Match for &'a T {
    type CasesIter<'b> = T::CasesIter<'b>
    where
        'a: 'b;
    type Expr<'b> = T::Expr<'b>
    where
        'a: 'b;
    type MatchCase<'b> = T::MatchCase<'b>
    where
        'a: 'b;

    #[inline(always)]
    fn subject(&self) -> Self::Expr<'_> {
        T::subject(self)
    }

    #[inline(always)]
    fn cases(&self) -> Self::CasesIter<'_> {
        T::cases(self)
    }
}

pub trait Raise {
    type Expr<'a>: Expr + 'a
    where
        Self: 'a;
    fn exc(&self) -> Option<Self::Expr<'_>>;
    fn cause(&self) -> Option<Self::Expr<'_>>;
}

impl<'a, T: Raise> Raise for &'a T {
    type Expr<'b> = T::Expr<'b>
    where
        'a: 'b;

    #[inline(always)]
    fn exc(&self) -> Option<Self::Expr<'_>> {
        T::exc(self)
    }

    #[inline(always)]
    fn cause(&self) -> Option<Self::Expr<'_>> {
        T::cause(self)
    }
}

pub trait Try {
    type Stmt<'a>: Stmt + 'a
    where
        Self: 'a;
    type ExceptHandler<'a>: ExceptHandler + 'a
    where
        Self: 'a;
    type BodyIter<'a>: Iterator<Item = Self::Stmt<'a>>
    where
        Self: 'a;
    type HandlersIter<'a>: Iterator<Item = Self::ExceptHandler<'a>>
    where
        Self: 'a;
    type OrelseIter<'a>: Iterator<Item = Self::Stmt<'a>>
    where
        Self: 'a;
    type FinalbodyIter<'a>: Iterator<Item = Self::Stmt<'a>>
    where
        Self: 'a;
    fn body(&self) -> Self::BodyIter<'_>;
    fn handlers(&self) -> Self::HandlersIter<'_>;
    fn orelse(&self) -> Self::OrelseIter<'_>;
    fn finalbody(&self) -> Self::FinalbodyIter<'_>;
}

impl<'a, T: Try> Try for &'a T {
    type BodyIter<'b> = T::BodyIter<'b>
    where
        'a: 'b;
    type ExceptHandler<'b> = T::ExceptHandler<'b>
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
    type Stmt<'b> = T::Stmt<'b>
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
    type Expr<'a>: Expr + 'a
    where
        Self: 'a;
    fn test(&self) -> Self::Expr<'_>;
    fn msg(&self) -> Option<Self::Expr<'_>>;
}

impl<'a, T: Assert> Assert for &'a T {
    type Expr<'b> = T::Expr<'b>
    where
        'a: 'b;

    #[inline(always)]
    fn test(&self) -> Self::Expr<'_> {
        T::test(self)
    }

    #[inline(always)]
    fn msg(&self) -> Option<Self::Expr<'_>> {
        T::msg(self)
    }
}

pub trait Import {
    type Alias<'a>: Alias + 'a
    where
        Self: 'a;
    type NamesIter<'a>: Iterator<Item = Self::Alias<'a>>
    where
        Self: 'a;
    fn names(&self) -> Self::NamesIter<'_>;
}

impl<'a, T: Import> Import for &'a T {
    type Alias<'b> = T::Alias<'b>
    where
        'a: 'b;
    type NamesIter<'b> = T::NamesIter<'b>
    where
        'a: 'b;

    #[inline(always)]
    fn names(&self) -> Self::NamesIter<'_> {
        T::names(self)
    }
}

pub trait ImportFrom {
    type Alias<'a>: Alias<Ident<'a> = Self::Ident<'a>>
    where
        Self: 'a;
    type Ident<'a>: Ident + 'a
    where
        Self: 'a;
    type NamesIter<'a>: Iterator<Item = Self::Alias<'a>>
    where
        Self: 'a;
    fn module(&self) -> Option<Self::Ident<'_>>;
    fn names(&self) -> Self::NamesIter<'_>;
    fn level(&self) -> Option<usize>;
}

impl<'a, T: ImportFrom> ImportFrom for &'a T {
    type Alias<'b> = T::Alias<'b>
    where
        'a: 'b;
    type Ident<'b> = T::Ident<'b>
    where
        'a: 'b;
    type NamesIter<'b> = T::NamesIter<'b>
    where
        'a: 'b;

    #[inline(always)]
    fn module(&self) -> Option<Self::Ident<'_>> {
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
    type Ident<'a>: Ident + 'a
    where
        Self: 'a;
    type NamesIter<'a>: Iterator<Item = Self::Ident<'a>>
    where
        Self: 'a;
    fn names(&self) -> Self::NamesIter<'_>;
}

impl<'a, T: Global> Global for &'a T {
    type Ident<'b> = T::Ident<'b>
    where
        'a: 'b;
    type NamesIter<'b> = T::NamesIter<'b>
    where
        'a: 'b;

    #[inline(always)]
    fn names(&self) -> Self::NamesIter<'_> {
        T::names(self)
    }
}

pub trait Nonlocal {
    type Ident<'a>: Ident + 'a
    where
        Self: 'a;
    type NamesIter<'a>: Iterator<Item = Self::Ident<'a>>
    where
        Self: 'a;
    fn names(&self) -> Self::NamesIter<'_>;
}

impl<'a, T: Nonlocal> Nonlocal for &'a T {
    type Ident<'b> = T::Ident<'b>
    where
        'a: 'b;
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

// Type complexity required due to need to support circular
// associated types.
#[allow(clippy::type_complexity)]
pub trait Stmt: Located {
    type FunctionDef<'a>: FunctionDef + 'a
    where
        Self: 'a;
    type AsyncFunctionDef<'a>: AsyncFunctionDef + 'a
    where
        Self: 'a;
    type ClassDef<'a>: ClassDef + 'a
    where
        Self: 'a;
    type Return<'a>: Return + 'a
    where
        Self: 'a;
    type Delete<'a>: Delete + 'a
    where
        Self: 'a;
    type Assign<'a>: Assign + 'a
    where
        Self: 'a;
    type AugAssign<'a>: AugAssign + 'a
    where
        Self: 'a;
    type AnnAssign<'a>: AnnAssign + 'a
    where
        Self: 'a;
    type For<'a>: For + 'a
    where
        Self: 'a;
    type AsyncFor<'a>: AsyncFor + 'a
    where
        Self: 'a;
    type While<'a>: While + 'a
    where
        Self: 'a;
    type If<'a>: If + 'a
    where
        Self: 'a;
    type With<'a>: With + 'a
    where
        Self: 'a;
    type AsyncWith<'a>: AsyncWith + 'a
    where
        Self: 'a;
    type Match<'a>: Match + 'a
    where
        Self: 'a;
    type Raise<'a>: Raise + 'a
    where
        Self: 'a;
    type Try<'a>: Try + 'a
    where
        Self: 'a;
    type Assert<'a>: Assert + 'a
    where
        Self: 'a;
    type Import<'a>: Import + 'a
    where
        Self: 'a;
    type ImportFrom<'a>: ImportFrom + 'a
    where
        Self: 'a;
    type Global<'a>: Global + 'a
    where
        Self: 'a;
    type Nonlocal<'a>: Nonlocal + 'a
    where
        Self: 'a;
    type Expr<'a>: Expr + 'a
    where
        Self: 'a;
    fn stmt(
        &self,
    ) -> StmtKind<
        Self::FunctionDef<'_>,
        Self::AsyncFunctionDef<'_>,
        Self::ClassDef<'_>,
        Self::Return<'_>,
        Self::Delete<'_>,
        Self::Assign<'_>,
        Self::AugAssign<'_>,
        Self::AnnAssign<'_>,
        Self::For<'_>,
        Self::AsyncFor<'_>,
        Self::While<'_>,
        Self::If<'_>,
        Self::With<'_>,
        Self::AsyncWith<'_>,
        Self::Match<'_>,
        Self::Raise<'_>,
        Self::Try<'_>,
        Self::Assert<'_>,
        Self::Import<'_>,
        Self::ImportFrom<'_>,
        Self::Global<'_>,
        Self::Nonlocal<'_>,
        Self::Expr<'_>,
    >;
}

impl<'a, T: Stmt> Stmt for &'a T {
    type AnnAssign<'b> = T::AnnAssign<'b>
    where
        'a: 'b;
    type Assert<'b> = T::Assert<'b>
    where
        'a: 'b;
    type Assign<'b> = T::Assign<'b>
    where
        'a: 'b;
    type AsyncFor<'b> = T::AsyncFor<'b>
    where
        'a: 'b;
    type AsyncFunctionDef<'b> = T::AsyncFunctionDef<'b>
    where
        'a: 'b;
    type AsyncWith<'b> = T::AsyncWith<'b>
    where
        'a: 'b;
    type AugAssign<'b> = T::AugAssign<'b>
    where
        'a: 'b;
    type ClassDef<'b> = T::ClassDef<'b>
    where
        'a: 'b;
    type Delete<'b> = T::Delete<'b>
    where
        'a: 'b;
    type Expr<'b> = T::Expr<'b>
    where
        'a: 'b;
    type For<'b> = T::For<'b>
    where
        'a: 'b;
    type FunctionDef<'b> = T::FunctionDef<'b>
    where
        'a: 'b;
    type Global<'b> = T::Global<'b>
    where
        'a: 'b;
    type If<'b> = T::If<'b>
    where
        'a: 'b;
    type Import<'b> = T::Import<'b>
    where
        'a: 'b;
    type ImportFrom<'b> = T::ImportFrom<'b>
    where
        'a: 'b;
    type Match<'b> = T::Match<'b>
    where
        'a: 'b;
    type Nonlocal<'b> = T::Nonlocal<'b>
    where
        'a: 'b;
    type Raise<'b> = T::Raise<'b>
    where
        'a: 'b;
    type Return<'b> = T::Return<'b>
    where
        'a: 'b;
    type Try<'b> = T::Try<'b>
    where
        'a: 'b;
    type While<'b> = T::While<'b>
    where
        'a: 'b;
    type With<'b> = T::With<'b>
    where
        'a: 'b;

    #[inline(always)]
    fn stmt(
        &self,
    ) -> StmtKind<
        Self::FunctionDef<'_>,
        Self::AsyncFunctionDef<'_>,
        Self::ClassDef<'_>,
        Self::Return<'_>,
        Self::Delete<'_>,
        Self::Assign<'_>,
        Self::AugAssign<'_>,
        Self::AnnAssign<'_>,
        Self::For<'_>,
        Self::AsyncFor<'_>,
        Self::While<'_>,
        Self::If<'_>,
        Self::With<'_>,
        Self::AsyncWith<'_>,
        Self::Match<'_>,
        Self::Raise<'_>,
        Self::Try<'_>,
        Self::Assert<'_>,
        Self::Import<'_>,
        Self::ImportFrom<'_>,
        Self::Global<'_>,
        Self::Nonlocal<'_>,
        Self::Expr<'_>,
    > {
        T::stmt(self)
    }
}

pub trait Ast {
    type Ident<'a>: Ident + 'a
    where
        Self: 'a;
    type Alias<'a>: Alias<Ident<'a> = Self::Ident<'a>> + 'a
    where
        Self: 'a;
    type Arg<'a>: Arg<Ident<'a> = Self::Ident<'a>, Expr<'a> = Self::Expr<'a>> + 'a
    where
        Self: 'a;
    type Arguments<'a>: Arguments<Arg<'a> = Self::Arg<'a>, Expr<'a> = Self::Expr<'a>> + 'a
    where
        Self: 'a;
    type Keyword<'a>: Keyword<Expr<'a> = Self::Expr<'a>, Ident<'a> = Self::Ident<'a>> + 'a
    where
        Self: 'a;
    type BigInt<'a>: BigInt + 'a
    where
        Self: 'a;
    type Constant<'a>: Constant<Constant<'a> = Self::Constant<'a>, BigInt<'a> = Self::BigInt<'a>>
        + 'a
    where
        Self: 'a;
    type Comprehension<'a>: Comprehension<Expr<'a> = Self::Expr<'a>> + 'a
    where
        Self: 'a;
    type BoolOp<'a>: BoolOp<Expr<'a> = Self::Expr<'a>> + 'a
    where
        Self: 'a;
    type NamedExpr<'a>: NamedExpr<Expr<'a> = Self::Expr<'a>> + 'a
    where
        Self: 'a;
    type BinOp<'a>: BinOp<Expr<'a> = Self::Expr<'a>> + 'a
    where
        Self: 'a;
    type UnaryOp<'a>: UnaryOp<Expr<'a> = Self::Expr<'a>> + 'a
    where
        Self: 'a;
    type Lambda<'a>: Lambda<Arguments<'a> = Self::Arguments<'a>, Expr<'a> = Self::Expr<'a>> + 'a
    where
        Self: 'a;
    type IfExp<'a>: IfExp<Expr<'a> = Self::Expr<'a>> + 'a
    where
        Self: 'a;
    type Dict<'a>: Dict<Expr<'a> = Self::Expr<'a>> + 'a
    where
        Self: 'a;
    type Set<'a>: Set<Expr<'a> = Self::Expr<'a>> + 'a
    where
        Self: 'a;
    type ListComp<'a>: ListComp<Expr<'a> = Self::Expr<'a>, Comprehension<'a> = Self::Comprehension<'a>>
        + 'a
    where
        Self: 'a;
    type SetComp<'a>: SetComp<Expr<'a> = Self::Expr<'a>, Comprehension<'a> = Self::Comprehension<'a>>
        + 'a
    where
        Self: 'a;
    type DictComp<'a>: DictComp<Expr<'a> = Self::Expr<'a>, Comprehension<'a> = Self::Comprehension<'a>>
        + 'a
    where
        Self: 'a;
    type GeneratorExp<'a>: GeneratorExp<Expr<'a> = Self::Expr<'a>, Comprehension<'a> = Self::Comprehension<'a>>
        + 'a
    where
        Self: 'a;
    type Await<'a>: Await<Expr<'a> = Self::Expr<'a>> + 'a
    where
        Self: 'a;
    type Yield<'a>: Yield<Expr<'a> = Self::Expr<'a>> + 'a
    where
        Self: 'a;
    type YieldFrom<'a>: YieldFrom<Expr<'a> = Self::Expr<'a>> + 'a
    where
        Self: 'a;
    type Compare<'a>: Compare<Expr<'a> = Self::Expr<'a>> + 'a
    where
        Self: 'a;
    type Call<'a>: Call<Expr<'a> = Self::Expr<'a>, Keyword<'a> = Self::Keyword<'a>> + 'a
    where
        Self: 'a;
    type FormattedValue<'a>: FormattedValue<Expr<'a> = Self::Expr<'a>> + 'a
    where
        Self: 'a;
    type JoinedStr<'a>: JoinedStr<Expr<'a> = Self::Expr<'a>> + 'a
    where
        Self: 'a;
    type ConstantExpr<'a>: ConstantExpr<Constant<'a> = Self::Constant<'a>> + 'a
    where
        Self: 'a;
    type Attribute<'a>: Attribute<Ident<'a> = Self::Ident<'a>, Expr<'a> = Self::Expr<'a>> + 'a
    where
        Self: 'a;
    type Subscript<'a>: Subscript<Expr<'a> = Self::Expr<'a>> + 'a
    where
        Self: 'a;
    type Starred<'a>: Starred<Expr<'a> = Self::Expr<'a>> + 'a
    where
        Self: 'a;
    type Name<'a>: Name<Ident<'a> = Self::Ident<'a>> + 'a
    where
        Self: 'a;
    type List<'a>: List<Expr<'a> = Self::Expr<'a>> + 'a
    where
        Self: 'a;
    type Tuple<'a>: Tuple<Expr<'a> = Self::Expr<'a>> + 'a
    where
        Self: 'a;
    type Slice<'a>: Slice<Expr<'a> = Self::Expr<'a>> + 'a
    where
        Self: 'a;
    type Expr<'a>: Expr<
            BoolOp<'a> = Self::BoolOp<'a>,
            NamedExpr<'a> = Self::NamedExpr<'a>,
            BinOp<'a> = Self::BinOp<'a>,
            UnaryOp<'a> = Self::UnaryOp<'a>,
            Lambda<'a> = Self::Lambda<'a>,
            IfExp<'a> = Self::IfExp<'a>,
            Dict<'a> = Self::Dict<'a>,
            Set<'a> = Self::Set<'a>,
            ListComp<'a> = Self::ListComp<'a>,
            SetComp<'a> = Self::SetComp<'a>,
            DictComp<'a> = Self::DictComp<'a>,
            GeneratorExp<'a> = Self::GeneratorExp<'a>,
            Await<'a> = Self::Await<'a>,
            Yield<'a> = Self::Yield<'a>,
            YieldFrom<'a> = Self::YieldFrom<'a>,
            Compare<'a> = Self::Compare<'a>,
            Call<'a> = Self::Call<'a>,
            FormattedValue<'a> = Self::FormattedValue<'a>,
            JoinedStr<'a> = Self::JoinedStr<'a>,
            ConstantExpr<'a> = Self::ConstantExpr<'a>,
            Attribute<'a> = Self::Attribute<'a>,
            Subscript<'a> = Self::Subscript<'a>,
            Starred<'a> = Self::Starred<'a>,
            Name<'a> = Self::Name<'a>,
            List<'a> = Self::List<'a>,
            Tuple<'a> = Self::Tuple<'a>,
            Slice<'a> = Self::Slice<'a>,
        > + 'a
    where
        Self: 'a;
    type ExceptHandler<'a>: ExceptHandler<
            Ident<'a> = Self::Ident<'a>,
            Expr<'a> = Self::Expr<'a>,
            Stmt<'a> = Self::Stmt<'a>,
        > + 'a
    where
        Self: 'a;
    type MatchValue<'a>: MatchValue<Expr<'a> = Self::Expr<'a>> + 'a
    where
        Self: 'a;
    type MatchSingleton<'a>: MatchSingleton<Constant<'a> = Self::Constant<'a>> + 'a
    where
        Self: 'a;
    type MatchSequence<'a>: MatchSequence<Pattern<'a> = Self::Pattern<'a>> + 'a
    where
        Self: 'a;
    type MatchMapping<'a>: MatchMapping<
            Ident<'a> = Self::Ident<'a>,
            Expr<'a> = Self::Expr<'a>,
            Pattern<'a> = Self::Pattern<'a>,
        > + 'a
    where
        Self: 'a;
    type MatchClass<'a>: MatchClass<
            Expr<'a> = Self::Expr<'a>,
            Pattern<'a> = Self::Pattern<'a>,
            Ident<'a> = Self::Ident<'a>,
        > + 'a
    where
        Self: 'a;
    type MatchStar<'a>: MatchStar<Ident<'a> = Self::Ident<'a>> + 'a
    where
        Self: 'a;
    type MatchAs<'a>: MatchAs<Ident<'a> = Self::Ident<'a>, Pattern<'a> = Self::Pattern<'a>> + 'a
    where
        Self: 'a;
    type MatchOr<'a>: MatchOr<Pattern<'a> = Self::Pattern<'a>> + 'a
    where
        Self: 'a;
    type Pattern<'a>: Pattern<
            MatchValue<'a> = Self::MatchValue<'a>,
            MatchSingleton<'a> = Self::MatchSingleton<'a>,
            MatchSequence<'a> = Self::MatchSequence<'a>,
            MatchMapping<'a> = Self::MatchMapping<'a>,
            MatchClass<'a> = Self::MatchClass<'a>,
            MatchStar<'a> = Self::MatchStar<'a>,
            MatchAs<'a> = Self::MatchAs<'a>,
            MatchOr<'a> = Self::MatchOr<'a>,
        > + 'a
    where
        Self: 'a;
    type Withitem<'a>: Withitem<Expr<'a> = Self::Expr<'a>> + 'a
    where
        Self: 'a;
    type MatchCase<'a>: MatchCase<
            Expr<'a> = Self::Expr<'a>,
            Pattern<'a> = Self::Pattern<'a>,
            Stmt<'a> = Self::Stmt<'a>,
        > + 'a
    where
        Self: 'a;
    type FunctionDef<'a>: FunctionDef<
            Ident<'a> = Self::Ident<'a>,
            Arguments<'a> = Self::Arguments<'a>,
            Expr<'a> = Self::Expr<'a>,
            Stmt<'a> = Self::Stmt<'a>,
        > + 'a
    where
        Self: 'a;
    type AsyncFunctionDef<'a>: AsyncFunctionDef<
            Ident<'a> = Self::Ident<'a>,
            Arguments<'a> = Self::Arguments<'a>,
            Stmt<'a> = Self::Stmt<'a>,
            Expr<'a> = Self::Expr<'a>,
        > + 'a
    where
        Self: 'a;
    type ClassDef<'a>: ClassDef<
            Ident<'a> = Self::Ident<'a>,
            Stmt<'a> = Self::Stmt<'a>,
            Expr<'a> = Self::Expr<'a>,
            Keyword<'a> = Self::Keyword<'a>,
        > + 'a
    where
        Self: 'a;
    type Return<'a>: Return<Expr<'a> = Self::Expr<'a>> + 'a
    where
        Self: 'a;
    type Delete<'a>: Delete<Expr<'a> = Self::Expr<'a>> + 'a
    where
        Self: 'a;
    type Assign<'a>: Assign<Expr<'a> = Self::Expr<'a>> + 'a
    where
        Self: 'a;
    type AugAssign<'a>: AugAssign<Expr<'a> = Self::Expr<'a>> + 'a
    where
        Self: 'a;
    type AnnAssign<'a>: AnnAssign<Expr<'a> = Self::Expr<'a>> + 'a
    where
        Self: 'a;
    type For<'a>: For<Expr<'a> = Self::Expr<'a>, Stmt<'a> = Self::Stmt<'a>> + 'a
    where
        Self: 'a;
    type AsyncFor<'a>: AsyncFor<Expr<'a> = Self::Expr<'a>, Stmt<'a> = Self::Stmt<'a>> + 'a
    where
        Self: 'a;
    type While<'a>: While<Expr<'a> = Self::Expr<'a>, Stmt<'a> = Self::Stmt<'a>> + 'a
    where
        Self: 'a;
    type If<'a>: If<Expr<'a> = Self::Expr<'a>, Stmt<'a> = Self::Stmt<'a>> + 'a
    where
        Self: 'a;
    type With<'a>: With<Withitem<'a> = Self::Withitem<'a>, Stmt<'a> = Self::Stmt<'a>> + 'a
    where
        Self: 'a;
    type AsyncWith<'a>: AsyncWith<Withitem<'a> = Self::Withitem<'a>, Stmt<'a> = Self::Stmt<'a>> + 'a
    where
        Self: 'a;
    type Match<'a>: Match<Expr<'a> = Self::Expr<'a>, MatchCase<'a> = Self::MatchCase<'a>> + 'a
    where
        Self: 'a;
    type Raise<'a>: Raise<Expr<'a> = Self::Expr<'a>> + 'a
    where
        Self: 'a;
    type Try<'a>: Try<Stmt<'a> = Self::Stmt<'a>, ExceptHandler<'a> = Self::ExceptHandler<'a>> + 'a
    where
        Self: 'a;
    type Assert<'a>: Assert<Expr<'a> = Self::Expr<'a>> + 'a
    where
        Self: 'a;
    type Import<'a>: Import<Alias<'a> = Self::Alias<'a>> + 'a
    where
        Self: 'a;
    type ImportFrom<'a>: ImportFrom<Alias<'a> = Self::Alias<'a>> + 'a
    where
        Self: 'a;
    type Global<'a>: Global<Ident<'a> = Self::Ident<'a>> + 'a
    where
        Self: 'a;
    type Nonlocal<'a>: Nonlocal<Ident<'a> = Self::Ident<'a>> + 'a
    where
        Self: 'a;
    type Stmt<'a>: Stmt<
            FunctionDef<'a> = Self::FunctionDef<'a>,
            AsyncFunctionDef<'a> = Self::AsyncFunctionDef<'a>,
            ClassDef<'a> = Self::ClassDef<'a>,
            Return<'a> = Self::Return<'a>,
            Delete<'a> = Self::Delete<'a>,
            Assign<'a> = Self::Assign<'a>,
            AugAssign<'a> = Self::AugAssign<'a>,
            AnnAssign<'a> = Self::AnnAssign<'a>,
            For<'a> = Self::For<'a>,
            AsyncFor<'a> = Self::AsyncFor<'a>,
            While<'a> = Self::While<'a>,
            If<'a> = Self::If<'a>,
            With<'a> = Self::With<'a>,
            AsyncWith<'a> = Self::AsyncWith<'a>,
            Match<'a> = Self::Match<'a>,
            Raise<'a> = Self::Raise<'a>,
            Try<'a> = Self::Try<'a>,
            Assert<'a> = Self::Assert<'a>,
            Import<'a> = Self::Import<'a>,
            ImportFrom<'a> = Self::ImportFrom<'a>,
            Global<'a> = Self::Global<'a>,
            Nonlocal<'a> = Self::Nonlocal<'a>,
            Expr<'a> = Self::Expr<'a>,
        > + 'a
    where
        Self: 'a;
    type StmtsIter<'a>: Iterator<Item = Self::Stmt<'a>>
    where
        Self: 'a;
    fn stmts(&self) -> Self::StmtsIter<'_>;
}

// RustPython ast impls
// TODO(Seamooo) make below a compilation feature
pub mod rustpython_impl;
// pub mod tree_sitter_impl;
