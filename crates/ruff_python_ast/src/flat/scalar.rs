use crate::{
    BoolOp, CmpOp, ConversionFlag, ExprContext, IpyEscapeKind, Operator, Singleton, UnaryOp,
};

pub(super) trait Word: Copy {
    fn encode(self) -> u64;
    fn decode(word: u64) -> Self;
}

// Field encodings are independent of Rust's choice of enum representation.
// When every bit pattern of a tag is a valid variant, only read those bits from the row.
macro_rules! tagged {
    ($ty:ident $([$mask:literal])? { $($variant:ident),+ $(,)? }) => {
        impl Word for $ty {
            #[inline]
            fn encode(self) -> u64 {
                let tag = self as u8;
                $(debug_assert_eq!(tag & $mask, tag);)?
                u64::from(tag)
            }

            #[inline]
            fn decode(word: u64) -> Self {
                let tag = word.to_le_bytes()[0];
                $(let tag = tag & $mask;)?
                match tag {
                    $(value if value == Self::$variant as u8 => Self::$variant,)+
                    _ => unreachable!("invalid AST scalar tag"),
                }
            }
        }
    };
}

tagged!(BoolOp[0b1] { And, Or });
tagged!(CmpOp {
    Eq,
    NotEq,
    Lt,
    LtE,
    Gt,
    GtE,
    Is,
    IsNot,
    In,
    NotIn
});
tagged!(ConversionFlag {
    None,
    Str,
    Ascii,
    Repr
});
tagged!(ExprContext[0b11] {
    Load,
    Store,
    Del,
    Invalid
});
tagged!(IpyEscapeKind {
    Shell,
    ShCap,
    Help,
    Help2,
    Magic,
    Magic2,
    Quote,
    Quote2,
    Paren
});
tagged!(Operator {
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
    FloorDiv
});
tagged!(Singleton { None, True, False });
tagged!(UnaryOp[0b11] {
    Invert,
    Not,
    UAdd,
    USub
});
