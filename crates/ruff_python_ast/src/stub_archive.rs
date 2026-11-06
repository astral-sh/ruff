//! Binary syntax trees for the immutable stubs packaged with ty.
//!
//! The producer and consumer are built from the same syntax definitions. Files containing
//! other Python constructs remain available as source and use the ordinary parser.

use crate::name::Name;
use crate::token::{Token, TokenFlags, TokenKind, Tokens};
use crate::{
    Alias, Arguments, AtomicNodeIndex, BoolOp, BytesLiteral, BytesLiteralValue, CmpOp, Decorator,
    DictItem, ElifElseClause, Expr, ExprAttribute, ExprBinOp, ExprBoolOp, ExprBooleanLiteral,
    ExprBytesLiteral, ExprCall, ExprCompare, ExprContext, ExprDict, ExprEllipsisLiteral, ExprIf,
    ExprList, ExprName, ExprNamed, ExprNoneLiteral, ExprNumberLiteral, ExprSet, ExprSlice,
    ExprStarred, ExprStringLiteral, ExprSubscript, ExprTuple, ExprUnaryOp, Identifier, Keyword,
    ModModule, Number, Operator, Parameter, ParameterWithDefault, Parameters, Stmt, StmtAnnAssign,
    StmtAssign, StmtAugAssign, StmtClassDef, StmtExpr, StmtFunctionDef, StmtIf, StmtImport,
    StmtImportFrom, StmtPass, StmtReturn, StmtTypeAlias, StringLiteral, StringLiteralValue,
    TypeParam, TypeParamParamSpec, TypeParamTypeVar, TypeParamTypeVarTuple, TypeParams, UnaryOp,
    str,
};
use ruff_text_size::{Ranged, TextRange, TextSize};
use thin_vec::ThinVec;

pub fn encode(module: &ModModule, tokens: &Tokens) -> Option<Vec<u8>> {
    let mut output = Vec::new();
    module.write(&mut output)?;
    write_tokens(tokens, &mut output)?;
    Some(output)
}

/// Decode the AST, leaving the token stream available for consumers that need it.
pub fn decode(mut input: &[u8]) -> Option<(ModModule, &[u8])> {
    let module = ModModule::read(&mut input)?;
    Some((module, input))
}

/// Token ranges are normally consecutive or separated by a few spaces. Keep the
/// common gap and length in one byte. An extended gap stores the absolute start,
/// so overlapping or coincident ranges also round trip.
fn write_tokens(tokens: &Tokens, out: &mut Vec<u8>) -> Option<()> {
    write_length(u32::try_from(tokens.len()).ok()?, out);
    let mut last_end = 0;
    for token in tokens {
        let kind = u8::try_from(token.kind() as usize).ok()?;
        if kind >= 128 {
            return None;
        }
        let flags = token.archive_flags().bits();
        out.push(kind | if flags == 0 { 0 } else { 128 });
        if flags != 0 {
            flags.write(out)?;
        }
        let start = u32::from(token.start());
        let length = u32::from(token.range().len());
        let gap = start.checked_sub(last_end).unwrap_or(15).min(15);
        let short_length = length.min(15);
        out.push(u8::try_from(gap * 16 + short_length).ok()?);
        if gap == 15 {
            start.write(out)?;
        }
        if short_length == 15 {
            write_length(length, out);
        }
        last_end = u32::from(token.end());
    }
    Some(())
}

pub fn decode_tokens(mut input: &[u8]) -> Option<Tokens> {
    let count = read_length(&mut input)? as usize;
    if count > input.len() / 2 {
        return None;
    }
    let mut tokens = Vec::with_capacity(count);
    let mut last_end = 0u32;
    for _ in 0..count {
        let header = u8::read(&mut input)?;
        let kind = TokenKind::from_repr((header & 127).into())?;
        let flags = if header & 128 == 0 {
            TokenFlags::empty()
        } else {
            TokenFlags::from_bits(u16::read(&mut input)?)?
        };
        let geometry = u8::read(&mut input)?;
        let start = match geometry >> 4 {
            15 => u32::read(&mut input)?,
            gap => last_end.checked_add(u32::from(gap))?,
        };
        let length = match geometry & 15 {
            15 => read_length(&mut input)?,
            small => u32::from(small),
        };
        last_end = start.checked_add(length)?;
        tokens.push(Token::new(
            kind,
            TextRange::new(TextSize::new(start), TextSize::new(last_end)),
            flags,
        ));
    }
    input.is_empty().then(|| Tokens::new(tokens))
}

pub(crate) trait Codec: Sized {
    fn write(&self, output: &mut Vec<u8>) -> Option<()>;
    fn read(input: &mut &[u8]) -> Option<Self>;
}

fn take<'a>(input: &mut &'a [u8], count: usize) -> Option<&'a [u8]> {
    let (field, rest) = input.split_at_checked(count)?;
    *input = rest;
    Some(field)
}

macro_rules! scalar {
    ($($ty:ty),* $(,)?) => {
        $(
            impl Codec for $ty {
                fn write(&self, out: &mut Vec<u8>) -> Option<()> {
                    out.extend_from_slice(&self.to_le_bytes());
                    Some(())
                }

                fn read(input: &mut &[u8]) -> Option<Self> {
                    let bytes = take(input, size_of::<Self>())?.try_into().ok()?;
                    Some(Self::from_le_bytes(bytes))
                }
            }
        )*
    };
}

scalar!(u8, u16, u32, u64, f64);

impl Codec for bool {
    fn write(&self, out: &mut Vec<u8>) -> Option<()> {
        u8::from(*self).write(out)
    }
    fn read(input: &mut &[u8]) -> Option<Self> {
        match u8::read(input)? {
            0 => Some(false),
            1 => Some(true),
            _ => None,
        }
    }
}

impl Codec for AtomicNodeIndex {
    fn write(&self, _: &mut Vec<u8>) -> Option<()> {
        Some(())
    }
    fn read(_: &mut &[u8]) -> Option<Self> {
        Some(Self::default())
    }
}

impl Codec for TextSize {
    fn write(&self, out: &mut Vec<u8>) -> Option<()> {
        u32::from(*self).write(out)
    }
    fn read(input: &mut &[u8]) -> Option<Self> {
        Some(Self::new(u32::read(input)?))
    }
}

/// Lengths of identifiers, child lists, and most syntax ranges fit in one byte.
fn write_length(length: u32, out: &mut Vec<u8>) {
    if let Ok(small @ 0..=254) = u8::try_from(length) {
        out.push(small);
    } else {
        out.push(u8::MAX);
        out.extend_from_slice(&length.to_le_bytes());
    }
}

fn read_length(input: &mut &[u8]) -> Option<u32> {
    match u8::read(input)? {
        u8::MAX => u32::read(input),
        small => Some(u32::from(small)),
    }
}

impl Codec for TextRange {
    fn write(&self, out: &mut Vec<u8>) -> Option<()> {
        let [a, b, c, high] = u32::from(self.start()).to_le_bytes();
        // The build can leave a very large stub in source form.
        if high != 0 {
            return None;
        }
        out.extend_from_slice(&[a, b, c]);
        write_length(u32::from(self.len()), out);
        Some(())
    }
    fn read(input: &mut &[u8]) -> Option<Self> {
        let [a, b, c]: [u8; 3] = take(input, 3)?.try_into().ok()?;
        let start = u32::from_le_bytes([a, b, c, 0]);
        let end = start.checked_add(read_length(input)?)?;
        Some(Self::new(TextSize::new(start), TextSize::new(end)))
    }
}

fn write_text(text: &str, out: &mut Vec<u8>) -> Option<()> {
    write_length(u32::try_from(text.len()).ok()?, out);
    out.extend_from_slice(text.as_bytes());
    Some(())
}
fn read_text<'a>(input: &mut &'a [u8]) -> Option<&'a str> {
    let length = read_length(input)?;
    std::str::from_utf8(take(input, length as usize)?).ok()
}
impl Codec for Box<str> {
    fn write(&self, out: &mut Vec<u8>) -> Option<()> {
        write_text(self, out)
    }
    fn read(input: &mut &[u8]) -> Option<Self> {
        Some(read_text(input)?.into())
    }
}
impl Codec for Name {
    fn write(&self, out: &mut Vec<u8>) -> Option<()> {
        write_text(self.as_str(), out)
    }
    fn read(input: &mut &[u8]) -> Option<Self> {
        Some(Name::new(read_text(input)?))
    }
}
impl<T: Codec> Codec for Box<T> {
    fn write(&self, out: &mut Vec<u8>) -> Option<()> {
        (**self).write(out)
    }
    fn read(input: &mut &[u8]) -> Option<Self> {
        Some(Box::new(T::read(input)?))
    }
}
impl<T: Codec> Codec for Option<T> {
    fn write(&self, out: &mut Vec<u8>) -> Option<()> {
        self.is_some().write(out)?;
        if let Some(value) = self {
            value.write(out)?;
        }
        Some(())
    }
    fn read(input: &mut &[u8]) -> Option<Self> {
        if bool::read(input)? {
            Some(Some(T::read(input)?))
        } else {
            Some(None)
        }
    }
}

fn write_slice<T: Codec>(items: &[T], out: &mut Vec<u8>) -> Option<()> {
    write_length(u32::try_from(items.len()).ok()?, out);
    for item in items {
        item.write(out)?;
    }
    Some(())
}

macro_rules! sequence {
    ($($seq:ident),* $(,)?) => {
        $(
            impl<T: Codec> Codec for $seq<T> {
                fn write(&self, out: &mut Vec<u8>) -> Option<()> {
                    write_slice(self, out)
                }

                fn read(input: &mut &[u8]) -> Option<Self> {
                    let length = read_length(input)? as usize;
                    if length > input.len() {
                        return None;
                    }
                    let mut items = Self::with_capacity(length);
                    for _ in 0..length {
                        items.push(T::read(input)?);
                    }
                    Some(items)
                }
            }
        )*
    };
}

sequence!(Vec, ThinVec);
impl<T: Codec> Codec for Box<[T]> {
    fn write(&self, out: &mut Vec<u8>) -> Option<()> {
        write_slice(self, out)
    }
    fn read(input: &mut &[u8]) -> Option<Self> {
        Some(Vec::read(input)?.into_boxed_slice())
    }
}

macro_rules! structure {
    ($name:ident { $($field:ident),* $(,)? }) => {
        impl Codec for $name {
            fn write(&self, out: &mut Vec<u8>) -> Option<()> {
                $(self.$field.write(out)?;)*
                Some(())
            }

            fn read(input: &mut &[u8]) -> Option<Self> {
                Some(Self {
                    $($field: Codec::read(input)?),*
                })
            }
        }
    };
}

macro_rules! alternatives {
    ($name:ident { $($tag:literal => $variant:ident),* $(,)? }) => {
        impl Codec for $name {
            fn write(&self, out: &mut Vec<u8>) -> Option<()> {
                #[allow(unreachable_patterns, reason = "the syntax image may cover a subset of the variants")]
                match self {
                    $(Self::$variant(value) => {
                        out.push($tag);
                        value.write(out)
                    },)*
                    _ => None,
                }
            }

            fn read(input: &mut &[u8]) -> Option<Self> {
                match u8::read(input)? {
                    $($tag => Some(Self::$variant(Codec::read(input)?)),)*
                    _ => None,
                }
            }
        }
    };
}

macro_rules! unit_variants {
    ($($name:ident),* $(,)?) => {
        $(
            impl Codec for $name {
                fn write(&self, out: &mut Vec<u8>) -> Option<()> {
                    u8::try_from(*self as usize).ok()?.write(out)
                }

                fn read(input: &mut &[u8]) -> Option<Self> {
                    Self::from_repr(u8::read(input)?.into())
                }
            }
        )*
    };
}

impl Codec for Number {
    fn write(&self, out: &mut Vec<u8>) -> Option<()> {
        match self {
            Self::Int(value) => {
                out.push(0);
                value.as_u64()?.write(out)
            }
            Self::Float(value) => {
                out.push(1);
                value.write(out)
            }
            Self::Complex { real, imag } => {
                out.push(2);
                real.write(out)?;
                imag.write(out)
            }
        }
    }
    fn read(input: &mut &[u8]) -> Option<Self> {
        match u8::read(input)? {
            0 => Some(Self::Int(u64::read(input)?.into())),
            1 => Some(Self::Float(f64::read(input)?)),
            2 => Some(Self::Complex {
                real: f64::read(input)?,
                imag: f64::read(input)?,
            }),
            _ => None,
        }
    }
}

impl Codec for StringLiteralValue {
    fn write(&self, out: &mut Vec<u8>) -> Option<()> {
        write_slice(self.as_slice(), out)
    }
    fn read(input: &mut &[u8]) -> Option<Self> {
        let items = Vec::<StringLiteral>::read(input)?;
        match items.len() {
            0 => None,
            1 => Some(Self::single(items.into_iter().next()?)),
            _ => Some(Self::concatenated(items)),
        }
    }
}

impl Codec for BytesLiteralValue {
    fn write(&self, out: &mut Vec<u8>) -> Option<()> {
        write_slice(self.as_slice(), out)
    }
    fn read(input: &mut &[u8]) -> Option<Self> {
        let items = Vec::<BytesLiteral>::read(input)?;
        match items.len() {
            0 => None,
            1 => Some(Self::single(items.into_iter().next()?)),
            _ => Some(Self::concatenated(items)),
        }
    }
}

alternatives!(Stmt { 0 => FunctionDef, 1 => ClassDef, 2 => Return, 3 => TypeAlias, 4 => Assign, 5 => AugAssign, 6 => AnnAssign, 7 => If, 8 => Import, 9 => ImportFrom, 10 => Expr, 11 => Pass });
alternatives!(Expr { 0 => BoolOp, 1 => Named, 2 => BinOp, 3 => UnaryOp, 4 => If, 5 => Dict, 6 => Set, 7 => Compare, 8 => Call, 9 => NumberLiteral, 10 => StringLiteral, 11 => BooleanLiteral, 12 => NoneLiteral, 13 => EllipsisLiteral, 14 => Attribute, 15 => Subscript, 16 => Name, 17 => List, 18 => Tuple, 19 => Slice, 20 => Starred, 21 => BytesLiteral });
alternatives!(TypeParam { 0 => TypeVar, 1 => ParamSpec, 2 => TypeVarTuple });
structure!(ModModule {
    node_index,
    range,
    body
});
structure!(Parameters {
    range,
    node_index,
    posonlyargs,
    args,
    vararg,
    kwonlyargs,
    kwarg
});
structure!(Parameter {
    range,
    node_index,
    name,
    annotation
});
structure!(ParameterWithDefault {
    range,
    node_index,
    parameter,
    default
});
structure!(Decorator {
    range,
    node_index,
    expression
});
structure!(Identifier {
    id,
    range,
    node_index
});
structure!(Arguments {
    range,
    node_index,
    args,
    keywords
});
structure!(Keyword {
    range,
    node_index,
    arg,
    value
});
structure!(Alias {
    range,
    node_index,
    name,
    asname
});
structure!(ElifElseClause {
    range,
    node_index,
    test,
    body
});
structure!(TypeParams {
    range,
    node_index,
    type_params
});
structure!(DictItem { key, value });
structure!(StringLiteral {
    range,
    node_index,
    value,
    flags
});
structure!(StmtFunctionDef {
    node_index,
    range,
    is_async,
    decorator_list,
    name,
    type_params,
    parameters,
    returns,
    body
});
structure!(StmtClassDef {
    node_index,
    range,
    decorator_list,
    name,
    type_params,
    arguments,
    body
});
structure!(StmtReturn {
    node_index,
    range,
    value
});
structure!(StmtTypeAlias {
    node_index,
    range,
    name,
    type_params,
    value
});
structure!(StmtAssign {
    node_index,
    range,
    targets,
    value
});
structure!(StmtAugAssign {
    node_index,
    range,
    target,
    op,
    value
});
structure!(StmtAnnAssign {
    node_index,
    range,
    target,
    annotation,
    value,
    simple
});
structure!(StmtIf {
    node_index,
    range,
    test,
    body,
    elif_else_clauses
});
structure!(StmtImport {
    node_index,
    range,
    names,
    is_lazy
});
structure!(StmtImportFrom {
    node_index,
    range,
    module,
    names,
    level,
    is_lazy
});
structure!(StmtExpr {
    node_index,
    range,
    value
});
structure!(StmtPass { node_index, range });
structure!(ExprBoolOp {
    node_index,
    range,
    op,
    values
});
structure!(ExprNamed {
    node_index,
    range,
    target,
    value
});
structure!(ExprBinOp {
    node_index,
    range,
    left,
    op,
    right
});
structure!(ExprUnaryOp {
    node_index,
    range,
    op,
    operand
});
structure!(ExprIf {
    node_index,
    range,
    test,
    body,
    orelse
});
structure!(ExprDict {
    node_index,
    range,
    items
});
structure!(ExprSet {
    node_index,
    range,
    elts
});
structure!(ExprCompare {
    node_index,
    range,
    ops,
    operands
});
structure!(ExprCall {
    node_index,
    range_start,
    func,
    arguments
});
structure!(ExprNumberLiteral {
    node_index,
    range,
    value
});
structure!(ExprStringLiteral {
    node_index,
    range,
    value
});
structure!(ExprBooleanLiteral {
    node_index,
    range,
    value
});
structure!(ExprNoneLiteral { node_index, range });
structure!(ExprEllipsisLiteral { node_index, range });
structure!(ExprAttribute {
    node_index,
    range,
    value,
    attr,
    ctx
});
structure!(ExprSubscript {
    node_index,
    range,
    value,
    slice,
    ctx
});
structure!(ExprName {
    node_index,
    range,
    id,
    ctx
});
structure!(ExprList {
    node_index,
    range,
    elts,
    ctx
});
structure!(ExprTuple {
    node_index,
    range,
    elts,
    ctx,
    parenthesized
});
structure!(ExprSlice {
    node_index,
    range,
    lower,
    upper,
    step
});
structure!(ExprStarred {
    node_index,
    range,
    value,
    ctx
});
structure!(TypeParamTypeVar {
    node_index,
    range,
    name,
    bound,
    default
});
structure!(TypeParamParamSpec {
    node_index,
    range,
    name,
    default
});
structure!(TypeParamTypeVarTuple {
    node_index,
    range,
    name,
    default
});
unit_variants!(ExprContext, BoolOp, Operator, UnaryOp, CmpOp);

structure!(ExprBytesLiteral {
    node_index,
    range,
    value
});
structure!(BytesLiteral {
    range,
    node_index,
    value,
    flags
});
