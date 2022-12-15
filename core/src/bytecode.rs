//! Implement python as a virtual machine with bytecodes. This module
//! implements bytecode structure.

use crate::marshal::MarshalError;
use crate::{marshal, Location};
use bitflags::bitflags;
use itertools::Itertools;
use num_bigint::BigInt;
use num_complex::Complex64;
use std::marker::PhantomData;
use std::{collections::BTreeSet, fmt, hash, mem};

pub trait Constant: Sized {
    type Name: AsRef<str>;

    /// Transforms the given Constant to a BorrowedConstant
    fn borrow_constant(&self) -> BorrowedConstant<Self>;
}

impl Constant for ConstantData {
    type Name = String;
    fn borrow_constant(&self) -> BorrowedConstant<Self> {
        use BorrowedConstant::*;
        match self {
            ConstantData::Integer { value } => Integer { value },
            ConstantData::Float { value } => Float { value: *value },
            ConstantData::Complex { value } => Complex { value: *value },
            ConstantData::Boolean { value } => Boolean { value: *value },
            ConstantData::Str { value } => Str { value },
            ConstantData::Bytes { value } => Bytes { value },
            ConstantData::Code { code } => Code { code },
            ConstantData::Tuple { elements } => Tuple { elements },
            ConstantData::None => None,
            ConstantData::Ellipsis => Ellipsis,
        }
    }
}

/// A Constant Bag
pub trait ConstantBag: Sized + Copy {
    type Constant: Constant;
    fn make_constant<C: Constant>(&self, constant: BorrowedConstant<C>) -> Self::Constant;
    fn make_int(&self, value: BigInt) -> Self::Constant;
    fn make_tuple(&self, elements: impl Iterator<Item = Self::Constant>) -> Self::Constant;
    fn make_code(&self, code: CodeObject<Self::Constant>) -> Self::Constant;
    fn make_name(&self, name: &str) -> <Self::Constant as Constant>::Name;
}

#[derive(Clone, Copy)]
pub struct BasicBag;

impl ConstantBag for BasicBag {
    type Constant = ConstantData;
    fn make_constant<C: Constant>(&self, constant: BorrowedConstant<C>) -> Self::Constant {
        constant.to_owned()
    }
    fn make_int(&self, value: BigInt) -> Self::Constant {
        ConstantData::Integer { value }
    }
    fn make_tuple(&self, elements: impl Iterator<Item = Self::Constant>) -> Self::Constant {
        ConstantData::Tuple {
            elements: elements.collect(),
        }
    }
    fn make_code(&self, code: CodeObject<Self::Constant>) -> Self::Constant {
        ConstantData::Code {
            code: Box::new(code),
        }
    }
    fn make_name(&self, name: &str) -> <Self::Constant as Constant>::Name {
        name.to_owned()
    }
}

/// Primary container of a single code object. Each python function has
/// a codeobject. Also a module has a codeobject.
#[derive(Clone)]
pub struct CodeObject<C: Constant = ConstantData> {
    pub instructions: Box<[CodeUnit]>,
    pub locations: Box<[Location]>,
    pub flags: CodeFlags,
    pub posonlyarg_count: u32,
    // Number of positional-only arguments
    pub arg_count: u32,
    pub kwonlyarg_count: u32,
    pub source_path: C::Name,
    pub first_line_number: u32,
    pub max_stackdepth: u32,
    pub obj_name: C::Name,
    // Name of the object that created this code object
    pub cell2arg: Option<Box<[i32]>>,
    pub constants: Box<[C]>,
    pub names: Box<[C::Name]>,
    pub varnames: Box<[C::Name]>,
    pub cellvars: Box<[C::Name]>,
    pub freevars: Box<[C::Name]>,
}

bitflags! {
    pub struct CodeFlags: u16 {
        const NEW_LOCALS = 0x01;
        const IS_GENERATOR = 0x02;
        const IS_COROUTINE = 0x04;
        const HAS_VARARGS = 0x08;
        const HAS_VARKEYWORDS = 0x10;
        const IS_OPTIMIZED = 0x20;
    }
}

impl CodeFlags {
    pub const NAME_MAPPING: &'static [(&'static str, CodeFlags)] = &[
        ("GENERATOR", CodeFlags::IS_GENERATOR),
        ("COROUTINE", CodeFlags::IS_COROUTINE),
        (
            "ASYNC_GENERATOR",
            Self::from_bits_truncate(Self::IS_GENERATOR.bits | Self::IS_COROUTINE.bits),
        ),
        ("VARARGS", CodeFlags::HAS_VARARGS),
        ("VARKEYWORDS", CodeFlags::HAS_VARKEYWORDS),
    ];
}

/// an opcode argument that may be extended by a prior ExtendedArg
#[derive(Copy, Clone, PartialEq, Eq)]
#[repr(transparent)]
pub struct OpArgByte(pub u8);
impl OpArgByte {
    pub const fn null() -> Self {
        OpArgByte(0)
    }
}
impl fmt::Debug for OpArgByte {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// a full 32-bit oparg, including any possible ExtendedArg extension
#[derive(Copy, Clone, Debug)]
#[repr(transparent)]
pub struct OpArg(pub u32);
impl OpArg {
    pub const fn null() -> Self {
        OpArg(0)
    }

    /// Returns how many CodeUnits a instruction with this oparg will be encoded as
    #[inline]
    pub fn instr_size(self) -> usize {
        (self.0 > 0xff) as usize + (self.0 > 0xff_ff) as usize + (self.0 > 0xff_ff_ff) as usize + 1
    }

    /// returns the arg split into any necessary ExtendedArg components (in big-endian order) and
    /// the arg for the real opcode itself
    #[inline(always)]
    pub fn split(self) -> (impl ExactSizeIterator<Item = OpArgByte>, OpArgByte) {
        let mut it = self
            .0
            .to_le_bytes()
            .map(OpArgByte)
            .into_iter()
            .take(self.instr_size());
        let lo = it.next().unwrap();
        (it.rev(), lo)
    }
}

#[derive(Default, Copy, Clone)]
#[repr(transparent)]
pub struct OpArgState {
    state: u32,
}

impl OpArgState {
    #[inline(always)]
    pub fn get(&mut self, ins: CodeUnit) -> (Instruction, OpArg) {
        let arg = self.extend(ins.arg);
        if ins.op != Instruction::ExtendedArg {
            self.reset();
        }
        (ins.op, arg)
    }
    #[inline(always)]
    pub fn extend(&mut self, arg: OpArgByte) -> OpArg {
        self.state = self.state << 8 | u32::from(arg.0);
        OpArg(self.state)
    }
    #[inline(always)]
    pub fn reset(&mut self) {
        self.state = 0
    }
}

pub trait OpArgType: Copy {
    fn from_oparg(x: u32) -> Option<Self>;
    fn to_oparg(self) -> u32;
}

impl OpArgType for u32 {
    #[inline(always)]
    fn from_oparg(x: u32) -> Option<Self> {
        Some(x)
    }
    #[inline(always)]
    fn to_oparg(self) -> u32 {
        self
    }
}

impl OpArgType for bool {
    #[inline(always)]
    fn from_oparg(x: u32) -> Option<Self> {
        Some(x != 0)
    }
    #[inline(always)]
    fn to_oparg(self) -> u32 {
        self as u32
    }
}

#[derive(Copy, Clone)]
pub struct Arg<T: OpArgType>(PhantomData<T>);

impl<T: OpArgType> Arg<T> {
    #[inline]
    pub fn marker() -> Self {
        Arg(PhantomData)
    }
    #[inline]
    pub fn new(arg: T) -> (Self, OpArg) {
        (Self(PhantomData), OpArg(arg.to_oparg()))
    }
    #[inline]
    pub fn new_single(arg: T) -> (Self, OpArgByte)
    where
        T: Into<u8>,
    {
        (Self(PhantomData), OpArgByte(arg.into()))
    }
    #[inline(always)]
    pub fn get(self, arg: OpArg) -> T {
        self.try_get(arg).unwrap()
    }
    #[inline(always)]
    pub fn try_get(self, arg: OpArg) -> Option<T> {
        T::from_oparg(arg.0)
    }
    #[inline(always)]
    /// # Safety
    /// T::from_oparg(self) must succeed
    pub unsafe fn get_unchecked(self, arg: OpArg) -> T {
        match T::from_oparg(arg.0) {
            Some(t) => t,
            None => std::hint::unreachable_unchecked(),
        }
    }
}

impl<T: OpArgType> PartialEq for Arg<T> {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}
impl<T: OpArgType> Eq for Arg<T> {}

impl<T: OpArgType> fmt::Debug for Arg<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Arg<{}>", std::any::type_name::<T>())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd)]
#[repr(transparent)]
// XXX: if you add a new instruction that stores a Label, make sure to add it in
// Instruction::label_arg
pub struct Label(pub u32);

impl OpArgType for Label {
    #[inline(always)]
    fn from_oparg(x: u32) -> Option<Self> {
        Some(Label(x))
    }
    #[inline(always)]
    fn to_oparg(self) -> u32 {
        self.0
    }
}

impl fmt::Display for Label {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Transforms a value prior to formatting it.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ConversionFlag {
    /// No conversion
    None = 0, // CPython uses -1 but not pleasure for us
    /// Converts by calling `str(<value>)`.
    Str = b's',
    /// Converts by calling `ascii(<value>)`.
    Ascii = b'a',
    /// Converts by calling `repr(<value>)`.
    Repr = b'r',
}

impl OpArgType for ConversionFlag {
    fn to_oparg(self) -> u32 {
        self as u32
    }
    fn from_oparg(x: u32) -> Option<Self> {
        Some(match u8::try_from(x).ok()? {
            0 => Self::None,
            b's' => Self::Str,
            b'a' => Self::Ascii,
            b'r' => Self::Repr,
            _ => return None,
        })
    }
}

impl TryFrom<usize> for ConversionFlag {
    type Error = usize;
    fn try_from(b: usize) -> Result<Self, Self::Error> {
        u32::try_from(b).ok().and_then(Self::from_oparg).ok_or(b)
    }
}

/// The kind of Raise that occurred.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum RaiseKind {
    Reraise,
    Raise,
    RaiseCause,
}

impl OpArgType for RaiseKind {
    fn to_oparg(self) -> u32 {
        self as u32
    }
    fn from_oparg(x: u32) -> Option<Self> {
        Some(match x {
            0 => Self::Reraise,
            1 => Self::Raise,
            2 => Self::RaiseCause,
            _ => return None,
        })
    }
}

pub type NameIdx = u32;

/// A Single bytecode instruction.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[repr(u8)]
pub enum Instruction {
    /// Importing by name
    ImportName {
        idx: Arg<NameIdx>,
    },
    /// Importing without name
    ImportNameless,
    /// Import *
    ImportStar,
    /// from ... import ...
    ImportFrom {
        idx: Arg<NameIdx>,
    },
    LoadFast(Arg<NameIdx>),
    LoadNameAny(Arg<NameIdx>),
    LoadGlobal(Arg<NameIdx>),
    LoadDeref(Arg<NameIdx>),
    LoadClassDeref(Arg<NameIdx>),
    StoreFast(Arg<NameIdx>),
    StoreLocal(Arg<NameIdx>),
    StoreGlobal(Arg<NameIdx>),
    StoreDeref(Arg<NameIdx>),
    DeleteFast(Arg<NameIdx>),
    DeleteLocal(Arg<NameIdx>),
    DeleteGlobal(Arg<NameIdx>),
    DeleteDeref(Arg<NameIdx>),
    LoadClosure(Arg<NameIdx>),
    Subscript,
    StoreSubscript,
    DeleteSubscript,
    StoreAttr {
        idx: Arg<NameIdx>,
    },
    DeleteAttr {
        idx: Arg<NameIdx>,
    },
    LoadConst {
        /// index into constants vec
        idx: Arg<u32>,
    },
    UnaryOperation {
        op: Arg<UnaryOperator>,
    },
    BinaryOperation {
        op: Arg<BinaryOperator>,
    },
    BinaryOperationInplace {
        op: Arg<BinaryOperator>,
    },
    LoadAttr {
        idx: Arg<NameIdx>,
    },
    TestOperation {
        op: Arg<TestOperator>,
    },
    CompareOperation {
        op: Arg<ComparisonOperator>,
    },
    Pop,
    Rotate2,
    Rotate3,
    Duplicate,
    Duplicate2,
    GetIter,
    Continue {
        target: Arg<Label>,
    },
    Break {
        target: Arg<Label>,
    },
    Jump {
        target: Arg<Label>,
    },
    /// Pop the top of the stack, and jump if this value is true.
    JumpIfTrue {
        target: Arg<Label>,
    },
    /// Pop the top of the stack, and jump if this value is false.
    JumpIfFalse {
        target: Arg<Label>,
    },
    /// Peek at the top of the stack, and jump if this value is true.
    /// Otherwise, pop top of stack.
    JumpIfTrueOrPop {
        target: Arg<Label>,
    },
    /// Peek at the top of the stack, and jump if this value is false.
    /// Otherwise, pop top of stack.
    JumpIfFalseOrPop {
        target: Arg<Label>,
    },
    MakeFunction(Arg<MakeFunctionFlags>),
    CallFunctionPositional {
        nargs: Arg<u32>,
    },
    CallFunctionKeyword {
        nargs: Arg<u32>,
    },
    CallFunctionEx {
        has_kwargs: Arg<bool>,
    },
    LoadMethod {
        idx: Arg<NameIdx>,
    },
    CallMethodPositional {
        nargs: Arg<u32>,
    },
    CallMethodKeyword {
        nargs: Arg<u32>,
    },
    CallMethodEx {
        has_kwargs: Arg<bool>,
    },
    ForIter {
        target: Arg<Label>,
    },
    ReturnValue,
    YieldValue,
    YieldFrom,
    SetupAnnotation,
    SetupLoop,

    /// Setup a finally handler, which will be called whenever one of this events occurs:
    /// - the block is popped
    /// - the function returns
    /// - an exception is returned
    SetupFinally {
        handler: Arg<Label>,
    },

    /// Enter a finally block, without returning, excepting, just because we are there.
    EnterFinally,

    /// Marker bytecode for the end of a finally sequence.
    /// When this bytecode is executed, the eval loop does one of those things:
    /// - Continue at a certain bytecode position
    /// - Propagate the exception
    /// - Return from a function
    /// - Do nothing at all, just continue
    EndFinally,

    SetupExcept {
        handler: Arg<Label>,
    },
    SetupWith {
        end: Arg<Label>,
    },
    WithCleanupStart,
    WithCleanupFinish,
    PopBlock,
    Raise {
        kind: Arg<RaiseKind>,
    },
    BuildString {
        size: Arg<u32>,
    },
    BuildTuple {
        size: Arg<u32>,
    },
    BuildTupleUnpack {
        size: Arg<u32>,
    },
    BuildList {
        size: Arg<u32>,
    },
    BuildListUnpack {
        size: Arg<u32>,
    },
    BuildSet {
        size: Arg<u32>,
    },
    BuildSetUnpack {
        size: Arg<u32>,
    },
    BuildMap {
        size: Arg<u32>,
    },
    BuildMapForCall {
        size: Arg<u32>,
    },
    DictUpdate,
    BuildSlice {
        /// whether build a slice with a third step argument
        step: Arg<bool>,
    },
    ListAppend {
        i: Arg<u32>,
    },
    SetAdd {
        i: Arg<u32>,
    },
    MapAdd {
        i: Arg<u32>,
    },

    PrintExpr,
    LoadBuildClass,
    UnpackSequence {
        size: Arg<u32>,
    },
    UnpackEx {
        args: Arg<UnpackExArgs>,
    },
    FormatValue {
        conversion: Arg<ConversionFlag>,
    },
    PopException,
    Reverse {
        amount: Arg<u32>,
    },
    GetAwaitable,
    BeforeAsyncWith,
    SetupAsyncWith {
        end: Arg<Label>,
    },
    GetAIter,
    GetANext,
    EndAsyncFor,
    ExtendedArg,
}
const _: () = assert!(mem::size_of::<Instruction>() == 1);

impl From<Instruction> for u8 {
    #[inline]
    fn from(ins: Instruction) -> u8 {
        // SAFETY: there's no padding bits
        unsafe { std::mem::transmute::<Instruction, u8>(ins) }
    }
}

impl TryFrom<u8> for Instruction {
    type Error = crate::marshal::MarshalError;

    #[inline]
    fn try_from(value: u8) -> Result<Self, crate::marshal::MarshalError> {
        if value <= u8::from(Instruction::ExtendedArg) {
            Ok(unsafe { std::mem::transmute::<u8, Instruction>(value) })
        } else {
            Err(crate::marshal::MarshalError::InvalidBytecode)
        }
    }
}

#[derive(Copy, Clone)]
#[repr(C)]
pub struct CodeUnit {
    pub op: Instruction,
    pub arg: OpArgByte,
}

const _: () = assert!(mem::size_of::<CodeUnit>() == 2);

impl CodeUnit {
    pub fn new(op: Instruction, arg: OpArgByte) -> Self {
        Self { op, arg }
    }
}

use self::Instruction::*;

bitflags! {
    pub struct MakeFunctionFlags: u8 {
        const CLOSURE = 0x01;
        const ANNOTATIONS = 0x02;
        const KW_ONLY_DEFAULTS = 0x04;
        const DEFAULTS = 0x08;
    }
}
impl OpArgType for MakeFunctionFlags {
    #[inline(always)]
    fn from_oparg(x: u32) -> Option<Self> {
        Some(unsafe { MakeFunctionFlags::from_bits_unchecked(x as u8) })
    }
    #[inline(always)]
    fn to_oparg(self) -> u32 {
        self.bits().into()
    }
}

/// A Constant (which usually encapsulates data within it)
///
/// # Examples
/// ```
/// use rustpython_compiler_core::ConstantData;
/// let a = ConstantData::Float {value: 120f64};
/// let b = ConstantData::Boolean {value: false};
/// assert_ne!(a, b);
/// ```
#[derive(Debug, Clone)]
pub enum ConstantData {
    Tuple { elements: Vec<ConstantData> },
    Integer { value: BigInt },
    Float { value: f64 },
    Complex { value: Complex64 },
    Boolean { value: bool },
    Str { value: String },
    Bytes { value: Vec<u8> },
    Code { code: Box<CodeObject> },
    None,
    Ellipsis,
}

impl PartialEq for ConstantData {
    fn eq(&self, other: &Self) -> bool {
        use ConstantData::*;
        match (self, other) {
            (Integer { value: a }, Integer { value: b }) => a == b,
            // we want to compare floats *by actual value* - if we have the *exact same* float
            // already in a constant cache, we want to use that
            (Float { value: a }, Float { value: b }) => a.to_bits() == b.to_bits(),
            (Complex { value: a }, Complex { value: b }) => {
                a.re.to_bits() == b.re.to_bits() && a.im.to_bits() == b.im.to_bits()
            }
            (Boolean { value: a }, Boolean { value: b }) => a == b,
            (Str { value: a }, Str { value: b }) => a == b,
            (Bytes { value: a }, Bytes { value: b }) => a == b,
            (Code { code: a }, Code { code: b }) => std::ptr::eq(a.as_ref(), b.as_ref()),
            (Tuple { elements: a }, Tuple { elements: b }) => a == b,
            (None, None) => true,
            (Ellipsis, Ellipsis) => true,
            _ => false,
        }
    }
}

impl Eq for ConstantData {}

impl hash::Hash for ConstantData {
    fn hash<H: hash::Hasher>(&self, state: &mut H) {
        use ConstantData::*;
        mem::discriminant(self).hash(state);
        match self {
            Integer { value } => value.hash(state),
            Float { value } => value.to_bits().hash(state),
            Complex { value } => {
                value.re.to_bits().hash(state);
                value.im.to_bits().hash(state);
            }
            Boolean { value } => value.hash(state),
            Str { value } => value.hash(state),
            Bytes { value } => value.hash(state),
            Code { code } => std::ptr::hash(code.as_ref(), state),
            Tuple { elements } => elements.hash(state),
            None => {}
            Ellipsis => {}
        }
    }
}

/// A borrowed Constant
pub enum BorrowedConstant<'a, C: Constant> {
    Integer { value: &'a BigInt },
    Float { value: f64 },
    Complex { value: Complex64 },
    Boolean { value: bool },
    Str { value: &'a str },
    Bytes { value: &'a [u8] },
    Code { code: &'a CodeObject<C> },
    Tuple { elements: &'a [C] },
    None,
    Ellipsis,
}

impl<C: Constant> Copy for BorrowedConstant<'_, C> {}
impl<C: Constant> Clone for BorrowedConstant<'_, C> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<C: Constant> BorrowedConstant<'_, C> {
    pub fn fmt_display(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            BorrowedConstant::Integer { value } => write!(f, "{value}"),
            BorrowedConstant::Float { value } => write!(f, "{value}"),
            BorrowedConstant::Complex { value } => write!(f, "{value}"),
            BorrowedConstant::Boolean { value } => {
                write!(f, "{}", if *value { "True" } else { "False" })
            }
            BorrowedConstant::Str { value } => write!(f, "{value:?}"),
            BorrowedConstant::Bytes { value } => write!(f, "b\"{}\"", value.escape_ascii()),
            BorrowedConstant::Code { code } => write!(f, "{code:?}"),
            BorrowedConstant::Tuple { elements } => {
                write!(f, "(")?;
                let mut first = true;
                for c in *elements {
                    if first {
                        first = false
                    } else {
                        write!(f, ", ")?;
                    }
                    c.borrow_constant().fmt_display(f)?;
                }
                write!(f, ")")
            }
            BorrowedConstant::None => write!(f, "None"),
            BorrowedConstant::Ellipsis => write!(f, "..."),
        }
    }
    pub fn to_owned(self) -> ConstantData {
        use ConstantData::*;
        match self {
            BorrowedConstant::Integer { value } => Integer {
                value: value.clone(),
            },
            BorrowedConstant::Float { value } => Float { value },
            BorrowedConstant::Complex { value } => Complex { value },
            BorrowedConstant::Boolean { value } => Boolean { value },
            BorrowedConstant::Str { value } => Str {
                value: value.to_owned(),
            },
            BorrowedConstant::Bytes { value } => Bytes {
                value: value.to_owned(),
            },
            BorrowedConstant::Code { code } => Code {
                code: Box::new(code.map_clone_bag(&BasicBag)),
            },
            BorrowedConstant::Tuple { elements } => Tuple {
                elements: elements
                    .iter()
                    .map(|c| c.borrow_constant().to_owned())
                    .collect(),
            },
            BorrowedConstant::None => None,
            BorrowedConstant::Ellipsis => Ellipsis,
        }
    }
}

/// The possible comparison operators
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[repr(u8)]
pub enum ComparisonOperator {
    // be intentional with bits so that we can do eval_ord with just a bitwise and
    // bits: | Equal | Greater | Less |
    Less = 0b001,
    Greater = 0b010,
    NotEqual = 0b011,
    Equal = 0b100,
    LessOrEqual = 0b101,
    GreaterOrEqual = 0b110,
}

impl OpArgType for ComparisonOperator {
    fn to_oparg(self) -> u32 {
        self as u32
    }
    fn from_oparg(x: u32) -> Option<Self> {
        Some(match x {
            0b001 => Self::Less,
            0b010 => Self::Greater,
            0b011 => Self::NotEqual,
            0b100 => Self::Equal,
            0b101 => Self::LessOrEqual,
            0b110 => Self::GreaterOrEqual,
            _ => return None,
        })
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[repr(u8)]
pub enum TestOperator {
    In,
    NotIn,
    Is,
    IsNot,
    /// two exceptions that match?
    ExceptionMatch,
}

impl OpArgType for TestOperator {
    fn to_oparg(self) -> u32 {
        self as u32
    }
    fn from_oparg(x: u32) -> Option<Self> {
        Some(match x {
            0 => Self::In,
            1 => Self::NotIn,
            2 => Self::Is,
            3 => Self::IsNot,
            4 => Self::ExceptionMatch,
            _ => return None,
        })
    }
}

/// The possible Binary operators
/// # Examples
///
/// ```ignore
/// use rustpython_compiler_core::Instruction::BinaryOperation;
/// use rustpython_compiler_core::BinaryOperator::Add;
/// let op = BinaryOperation {op: Add};
/// ```
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[repr(u8)]
pub enum BinaryOperator {
    Power,
    Multiply,
    MatrixMultiply,
    Divide,
    FloorDivide,
    Modulo,
    Add,
    Subtract,
    Lshift,
    Rshift,
    And,
    Xor,
    Or,
}

impl OpArgType for BinaryOperator {
    fn to_oparg(self) -> u32 {
        self as u32
    }
    fn from_oparg(x: u32) -> Option<Self> {
        Some(match x {
            0 => Self::Power,
            1 => Self::Multiply,
            2 => Self::MatrixMultiply,
            3 => Self::Divide,
            4 => Self::FloorDivide,
            5 => Self::Modulo,
            6 => Self::Add,
            7 => Self::Subtract,
            8 => Self::Lshift,
            9 => Self::Rshift,
            10 => Self::And,
            11 => Self::Xor,
            12 => Self::Or,
            _ => return None,
        })
    }
}

/// The possible unary operators
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[repr(u8)]
pub enum UnaryOperator {
    Not,
    Invert,
    Minus,
    Plus,
}

impl OpArgType for UnaryOperator {
    fn to_oparg(self) -> u32 {
        self as u32
    }
    fn from_oparg(x: u32) -> Option<Self> {
        Some(match x {
            0 => Self::Not,
            1 => Self::Invert,
            2 => Self::Minus,
            3 => Self::Plus,
            _ => return None,
        })
    }
}

#[derive(Copy, Clone)]
pub struct UnpackExArgs {
    pub before: u8,
    pub after: u8,
}

impl OpArgType for UnpackExArgs {
    #[inline(always)]
    fn from_oparg(x: u32) -> Option<Self> {
        let [before, after, ..] = x.to_le_bytes();
        Some(Self { before, after })
    }
    #[inline(always)]
    fn to_oparg(self) -> u32 {
        u32::from_le_bytes([self.before, self.after, 0, 0])
    }
}
impl fmt::Display for UnpackExArgs {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "before: {}, after: {}", self.before, self.after)
    }
}

/*
Maintain a stack of blocks on the VM.
pub enum BlockType {
    Loop,
    Except,
}
*/

/// Argument structure
pub struct Arguments<'a, N: AsRef<str>> {
    pub posonlyargs: &'a [N],
    pub args: &'a [N],
    pub vararg: Option<&'a N>,
    pub kwonlyargs: &'a [N],
    pub varkwarg: Option<&'a N>,
}

impl<N: AsRef<str>> fmt::Debug for Arguments<'_, N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        macro_rules! fmt_slice {
            ($x:expr) => {
                format_args!("[{}]", $x.iter().map(AsRef::as_ref).format(", "))
            };
        }
        f.debug_struct("Arguments")
            .field("posonlyargs", &fmt_slice!(self.posonlyargs))
            .field("args", &fmt_slice!(self.posonlyargs))
            .field("vararg", &self.vararg.map(N::as_ref))
            .field("kwonlyargs", &fmt_slice!(self.kwonlyargs))
            .field("varkwarg", &self.varkwarg.map(N::as_ref))
            .finish()
    }
}

impl<C: Constant> CodeObject<C> {
    /// Get all arguments of the code object
    /// like inspect.getargs
    pub fn arg_names(&self) -> Arguments<C::Name> {
        let nargs = self.arg_count as usize;
        let nkwargs = self.kwonlyarg_count as usize;
        let mut varargspos = nargs + nkwargs;
        let posonlyargs = &self.varnames[..self.posonlyarg_count as usize];
        let args = &self.varnames[..nargs];
        let kwonlyargs = &self.varnames[nargs..varargspos];

        let vararg = if self.flags.contains(CodeFlags::HAS_VARARGS) {
            let vararg = &self.varnames[varargspos];
            varargspos += 1;
            Some(vararg)
        } else {
            None
        };
        let varkwarg = if self.flags.contains(CodeFlags::HAS_VARKEYWORDS) {
            Some(&self.varnames[varargspos])
        } else {
            None
        };

        Arguments {
            posonlyargs,
            args,
            vararg,
            kwonlyargs,
            varkwarg,
        }
    }

    /// Return the labels targeted by the instructions of this CodeObject
    pub fn label_targets(&self) -> BTreeSet<Label> {
        let mut label_targets = BTreeSet::new();
        let mut arg_state = OpArgState::default();
        for instruction in &*self.instructions {
            let (instruction, arg) = arg_state.get(*instruction);
            if let Some(l) = instruction.label_arg() {
                label_targets.insert(l.get(arg));
            }
        }
        label_targets
    }

    fn display_inner(
        &self,
        f: &mut fmt::Formatter,
        expand_codeobjects: bool,
        level: usize,
    ) -> fmt::Result {
        let label_targets = self.label_targets();
        let line_digits = (3).max(self.locations.last().unwrap().row.to_string().len());
        let offset_digits = (4).max(self.instructions.len().to_string().len());
        let mut last_line = u32::MAX;
        let mut arg_state = OpArgState::default();
        for (offset, &instruction) in self.instructions.iter().enumerate() {
            let (instruction, arg) = arg_state.get(instruction);
            // optional line number
            let line = self.locations[offset].row;
            if line != last_line {
                if last_line != u32::MAX {
                    writeln!(f)?;
                }
                last_line = line;
                write!(f, "{line:line_digits$}")?;
            } else {
                for _ in 0..line_digits {
                    write!(f, " ")?;
                }
            }
            write!(f, " ")?;

            // level indent
            for _ in 0..level {
                write!(f, "    ")?;
            }

            // arrow and offset
            let arrow = if label_targets.contains(&Label(offset as u32)) {
                ">>"
            } else {
                "  "
            };
            write!(f, "{arrow} {offset:offset_digits$} ")?;

            // instruction
            instruction.fmt_dis(arg, f, self, expand_codeobjects, 21, level)?;
            writeln!(f)?;
        }
        Ok(())
    }

    /// Recursively display this CodeObject
    pub fn display_expand_codeobjects(&self) -> impl fmt::Display + '_ {
        struct Display<'a, C: Constant>(&'a CodeObject<C>);
        impl<C: Constant> fmt::Display for Display<'_, C> {
            fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
                self.0.display_inner(f, true, 1)
            }
        }
        Display(self)
    }

    /// Map this CodeObject to one that holds a Bag::Constant
    pub fn map_bag<Bag: ConstantBag>(self, bag: Bag) -> CodeObject<Bag::Constant> {
        let map_names = |names: Box<[C::Name]>| {
            names
                .into_vec()
                .into_iter()
                .map(|x| bag.make_name(x.as_ref()))
                .collect::<Box<[_]>>()
        };
        CodeObject {
            constants: self
                .constants
                .into_vec()
                .into_iter()
                .map(|x| bag.make_constant(x.borrow_constant()))
                .collect(),
            names: map_names(self.names),
            varnames: map_names(self.varnames),
            cellvars: map_names(self.cellvars),
            freevars: map_names(self.freevars),
            source_path: bag.make_name(self.source_path.as_ref()),
            obj_name: bag.make_name(self.obj_name.as_ref()),

            instructions: self.instructions,
            locations: self.locations,
            flags: self.flags,
            posonlyarg_count: self.posonlyarg_count,
            arg_count: self.arg_count,
            kwonlyarg_count: self.kwonlyarg_count,
            first_line_number: self.first_line_number,
            max_stackdepth: self.max_stackdepth,
            cell2arg: self.cell2arg,
        }
    }

    /// Same as `map_bag` but clones `self`
    pub fn map_clone_bag<Bag: ConstantBag>(&self, bag: &Bag) -> CodeObject<Bag::Constant> {
        let map_names =
            |names: &[C::Name]| names.iter().map(|x| bag.make_name(x.as_ref())).collect();
        CodeObject {
            constants: self
                .constants
                .iter()
                .map(|x| bag.make_constant(x.borrow_constant()))
                .collect(),
            names: map_names(&self.names),
            varnames: map_names(&self.varnames),
            cellvars: map_names(&self.cellvars),
            freevars: map_names(&self.freevars),
            source_path: bag.make_name(self.source_path.as_ref()),
            obj_name: bag.make_name(self.obj_name.as_ref()),

            instructions: self.instructions.clone(),
            locations: self.locations.clone(),
            flags: self.flags,
            posonlyarg_count: self.posonlyarg_count,
            arg_count: self.arg_count,
            kwonlyarg_count: self.kwonlyarg_count,
            first_line_number: self.first_line_number,
            max_stackdepth: self.max_stackdepth,
            cell2arg: self.cell2arg.clone(),
        }
    }
}

impl CodeObject<ConstantData> {
    /// Load a code object from bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self, MarshalError> {
        use lz4_flex::block::DecompressError;
        let raw_bincode = lz4_flex::decompress_size_prepended(data).map_err(|e| match e {
            DecompressError::OutputTooSmall { .. } | DecompressError::ExpectedAnotherByte => {
                MarshalError::Eof
            }
            _ => MarshalError::InvalidBytecode,
        })?;
        marshal::deserialize_code(&mut &raw_bincode[..], BasicBag)
    }

    /// Serialize this bytecode to bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut data = Vec::new();
        marshal::serialize_code(&mut data, self);
        lz4_flex::compress_prepend_size(&data)
    }
}

impl<C: Constant> fmt::Display for CodeObject<C> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.display_inner(f, false, 1)?;
        for constant in &*self.constants {
            if let BorrowedConstant::Code { code } = constant.borrow_constant() {
                writeln!(f, "\nDisassembly of {code:?}")?;
                code.fmt(f)?;
            }
        }
        Ok(())
    }
}

impl Instruction {
    /// Gets the label stored inside this instruction, if it exists
    #[inline]
    pub fn label_arg(&self) -> Option<Arg<Label>> {
        match self {
            Jump { target: l }
            | JumpIfTrue { target: l }
            | JumpIfFalse { target: l }
            | JumpIfTrueOrPop { target: l }
            | JumpIfFalseOrPop { target: l }
            | ForIter { target: l }
            | SetupFinally { handler: l }
            | SetupExcept { handler: l }
            | SetupWith { end: l }
            | SetupAsyncWith { end: l }
            | Break { target: l }
            | Continue { target: l } => Some(*l),
            _ => None,
        }
    }

    /// Whether this is an unconditional branching
    ///
    /// # Examples
    ///
    /// ```
    /// use rustpython_compiler_core::{Arg, Instruction};
    /// let jump_inst = Instruction::Jump { target: Arg::marker() };
    /// assert!(jump_inst.unconditional_branch())
    /// ```
    pub fn unconditional_branch(&self) -> bool {
        matches!(
            self,
            Jump { .. } | Continue { .. } | Break { .. } | ReturnValue | Raise { .. }
        )
    }

    /// What effect this instruction has on the stack
    ///
    /// # Examples
    ///
    /// ```
    /// use rustpython_compiler_core::{Arg, Instruction, Label, UnaryOperator};
    /// let (target, jump_arg) = Arg::new(Label(0xF));
    /// let jump_instruction = Instruction::Jump { target };
    /// let (op, invert_arg) = Arg::new(UnaryOperator::Invert);
    /// let invert_instruction = Instruction::UnaryOperation { op };
    /// assert_eq!(jump_instruction.stack_effect(jump_arg, true), 0);
    /// assert_eq!(invert_instruction.stack_effect(invert_arg, false), 0);
    /// ```
    ///
    pub fn stack_effect(&self, arg: OpArg, jump: bool) -> i32 {
        match self {
            ImportName { .. } | ImportNameless => -1,
            ImportStar => -1,
            ImportFrom { .. } => 1,
            LoadFast(_) | LoadNameAny(_) | LoadGlobal(_) | LoadDeref(_) | LoadClassDeref(_) => 1,
            StoreFast(_) | StoreLocal(_) | StoreGlobal(_) | StoreDeref(_) => -1,
            DeleteFast(_) | DeleteLocal(_) | DeleteGlobal(_) | DeleteDeref(_) => 0,
            LoadClosure(_) => 1,
            Subscript => -1,
            StoreSubscript => -3,
            DeleteSubscript => -2,
            LoadAttr { .. } => 0,
            StoreAttr { .. } => -2,
            DeleteAttr { .. } => -1,
            LoadConst { .. } => 1,
            UnaryOperation { .. } => 0,
            BinaryOperation { .. }
            | BinaryOperationInplace { .. }
            | TestOperation { .. }
            | CompareOperation { .. } => -1,
            Pop => -1,
            Rotate2 | Rotate3 => 0,
            Duplicate => 1,
            Duplicate2 => 2,
            GetIter => 0,
            Continue { .. } => 0,
            Break { .. } => 0,
            Jump { .. } => 0,
            JumpIfTrue { .. } | JumpIfFalse { .. } => -1,
            JumpIfTrueOrPop { .. } | JumpIfFalseOrPop { .. } => {
                if jump {
                    0
                } else {
                    -1
                }
            }
            MakeFunction(flags) => {
                let flags = flags.get(arg);
                -2 - flags.contains(MakeFunctionFlags::CLOSURE) as i32
                    - flags.contains(MakeFunctionFlags::ANNOTATIONS) as i32
                    - flags.contains(MakeFunctionFlags::KW_ONLY_DEFAULTS) as i32
                    - flags.contains(MakeFunctionFlags::DEFAULTS) as i32
                    + 1
            }
            CallFunctionPositional { nargs } => -(nargs.get(arg) as i32) - 1 + 1,
            CallMethodPositional { nargs } => -(nargs.get(arg) as i32) - 3 + 1,
            CallFunctionKeyword { nargs } => -1 - (nargs.get(arg) as i32) - 1 + 1,
            CallMethodKeyword { nargs } => -1 - (nargs.get(arg) as i32) - 3 + 1,
            CallFunctionEx { has_kwargs } => -1 - (has_kwargs.get(arg) as i32) - 1 + 1,
            CallMethodEx { has_kwargs } => -1 - (has_kwargs.get(arg) as i32) - 3 + 1,
            LoadMethod { .. } => -1 + 3,
            ForIter { .. } => {
                if jump {
                    -1
                } else {
                    1
                }
            }
            ReturnValue => -1,
            YieldValue => 0,
            YieldFrom => -1,
            SetupAnnotation | SetupLoop | SetupFinally { .. } | EnterFinally | EndFinally => 0,
            SetupExcept { .. } => jump as i32,
            SetupWith { .. } => (!jump) as i32,
            WithCleanupStart => 0,
            WithCleanupFinish => -1,
            PopBlock => 0,
            Raise { kind } => -(kind.get(arg) as u8 as i32),
            BuildString { size }
            | BuildTuple { size, .. }
            | BuildTupleUnpack { size, .. }
            | BuildList { size, .. }
            | BuildListUnpack { size, .. }
            | BuildSet { size, .. }
            | BuildSetUnpack { size, .. } => -(size.get(arg) as i32) + 1,
            BuildMap { size } => {
                let nargs = size.get(arg) * 2;
                -(nargs as i32) + 1
            }
            BuildMapForCall { size } => {
                let nargs = size.get(arg);
                -(nargs as i32) + 1
            }
            DictUpdate => -1,
            BuildSlice { step } => -2 - (step.get(arg) as i32) + 1,
            ListAppend { .. } | SetAdd { .. } => -1,
            MapAdd { .. } => -2,
            PrintExpr => -1,
            LoadBuildClass => 1,
            UnpackSequence { size } => -1 + size.get(arg) as i32,
            UnpackEx { args } => {
                let UnpackExArgs { before, after } = args.get(arg);
                -1 + before as i32 + 1 + after as i32
            }
            FormatValue { .. } => -1,
            PopException => 0,
            Reverse { .. } => 0,
            GetAwaitable => 0,
            BeforeAsyncWith => 1,
            SetupAsyncWith { .. } => {
                if jump {
                    -1
                } else {
                    0
                }
            }
            GetAIter => 0,
            GetANext => 1,
            EndAsyncFor => -2,
            ExtendedArg => 0,
        }
    }

    pub fn display<'a>(
        &'a self,
        arg: OpArg,
        ctx: &'a impl InstrDisplayContext,
    ) -> impl fmt::Display + 'a {
        struct FmtFn<F>(F);
        impl<F: Fn(&mut fmt::Formatter) -> fmt::Result> fmt::Display for FmtFn<F> {
            fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
                (self.0)(f)
            }
        }
        FmtFn(move |f: &mut fmt::Formatter| self.fmt_dis(arg, f, ctx, false, 0, 0))
    }

    #[allow(clippy::too_many_arguments)]
    fn fmt_dis(
        &self,
        arg: OpArg,
        f: &mut fmt::Formatter,
        ctx: &impl InstrDisplayContext,
        expand_codeobjects: bool,
        pad: usize,
        level: usize,
    ) -> fmt::Result {
        macro_rules! w {
            ($variant:ident) => {
                write!(f, stringify!($variant))
            };
            ($variant:ident, $map:ident = $argmarker:expr) => {{
                let arg = $argmarker.get(arg);
                write!(f, "{:pad$}({}, {})", stringify!($variant), arg, $map(arg))
            }};
            ($variant:ident, $argmarker:expr) => {
                write!(f, "{:pad$}({})", stringify!($variant), $argmarker.get(arg))
            };
            ($variant:ident, ?$argmarker:expr) => {
                write!(
                    f,
                    "{:pad$}({:?})",
                    stringify!($variant),
                    $argmarker.get(arg)
                )
            };
        }

        let varname = |i: u32| ctx.get_varname(i as usize);
        let name = |i: u32| ctx.get_name(i as usize);
        let cellname = |i: u32| ctx.get_cellname(i as usize);

        match self {
            ImportName { idx } => w!(ImportName, name = idx),
            ImportNameless => w!(ImportNameless),
            ImportStar => w!(ImportStar),
            ImportFrom { idx } => w!(ImportFrom, name = idx),
            LoadFast(idx) => w!(LoadFast, varname = idx),
            LoadNameAny(idx) => w!(LoadNameAny, name = idx),
            LoadGlobal(idx) => w!(LoadGlobal, name = idx),
            LoadDeref(idx) => w!(LoadDeref, cellname = idx),
            LoadClassDeref(idx) => w!(LoadClassDeref, cellname = idx),
            StoreFast(idx) => w!(StoreFast, varname = idx),
            StoreLocal(idx) => w!(StoreLocal, name = idx),
            StoreGlobal(idx) => w!(StoreGlobal, name = idx),
            StoreDeref(idx) => w!(StoreDeref, cellname = idx),
            DeleteFast(idx) => w!(DeleteFast, varname = idx),
            DeleteLocal(idx) => w!(DeleteLocal, name = idx),
            DeleteGlobal(idx) => w!(DeleteGlobal, name = idx),
            DeleteDeref(idx) => w!(DeleteDeref, cellname = idx),
            LoadClosure(i) => w!(LoadClosure, cellname = i),
            Subscript => w!(Subscript),
            StoreSubscript => w!(StoreSubscript),
            DeleteSubscript => w!(DeleteSubscript),
            StoreAttr { idx } => w!(StoreAttr, name = idx),
            DeleteAttr { idx } => w!(DeleteAttr, name = idx),
            LoadConst { idx } => {
                let value = ctx.get_constant(idx.get(arg) as usize);
                match value.borrow_constant() {
                    BorrowedConstant::Code { code } if expand_codeobjects => {
                        write!(f, "{:pad$}({:?}):", "LoadConst", code)?;
                        code.display_inner(f, true, level + 1)?;
                        Ok(())
                    }
                    c => {
                        write!(f, "{:pad$}(", "LoadConst")?;
                        c.fmt_display(f)?;
                        write!(f, ")")
                    }
                }
            }
            UnaryOperation { op } => w!(UnaryOperation, ?op),
            BinaryOperation { op } => w!(BinaryOperation, ?op),
            BinaryOperationInplace { op } => w!(BinaryOperationInplace, ?op),
            LoadAttr { idx } => w!(LoadAttr, name = idx),
            TestOperation { op } => w!(TestOperation, ?op),
            CompareOperation { op } => w!(CompareOperation, ?op),
            Pop => w!(Pop),
            Rotate2 => w!(Rotate2),
            Rotate3 => w!(Rotate3),
            Duplicate => w!(Duplicate),
            Duplicate2 => w!(Duplicate2),
            GetIter => w!(GetIter),
            Continue { target } => w!(Continue, target),
            Break { target } => w!(Break, target),
            Jump { target } => w!(Jump, target),
            JumpIfTrue { target } => w!(JumpIfTrue, target),
            JumpIfFalse { target } => w!(JumpIfFalse, target),
            JumpIfTrueOrPop { target } => w!(JumpIfTrueOrPop, target),
            JumpIfFalseOrPop { target } => w!(JumpIfFalseOrPop, target),
            MakeFunction(flags) => w!(MakeFunction, ?flags),
            CallFunctionPositional { nargs } => w!(CallFunctionPositional, nargs),
            CallFunctionKeyword { nargs } => w!(CallFunctionKeyword, nargs),
            CallFunctionEx { has_kwargs } => w!(CallFunctionEx, has_kwargs),
            LoadMethod { idx } => w!(LoadMethod, name = idx),
            CallMethodPositional { nargs } => w!(CallMethodPositional, nargs),
            CallMethodKeyword { nargs } => w!(CallMethodKeyword, nargs),
            CallMethodEx { has_kwargs } => w!(CallMethodEx, has_kwargs),
            ForIter { target } => w!(ForIter, target),
            ReturnValue => w!(ReturnValue),
            YieldValue => w!(YieldValue),
            YieldFrom => w!(YieldFrom),
            SetupAnnotation => w!(SetupAnnotation),
            SetupLoop => w!(SetupLoop),
            SetupExcept { handler } => w!(SetupExcept, handler),
            SetupFinally { handler } => w!(SetupFinally, handler),
            EnterFinally => w!(EnterFinally),
            EndFinally => w!(EndFinally),
            SetupWith { end } => w!(SetupWith, end),
            WithCleanupStart => w!(WithCleanupStart),
            WithCleanupFinish => w!(WithCleanupFinish),
            BeforeAsyncWith => w!(BeforeAsyncWith),
            SetupAsyncWith { end } => w!(SetupAsyncWith, end),
            PopBlock => w!(PopBlock),
            Raise { kind } => w!(Raise, ?kind),
            BuildString { size } => w!(BuildString, size),
            BuildTuple { size } => w!(BuildTuple, size),
            BuildTupleUnpack { size } => w!(BuildTupleUnpack, size),
            BuildList { size } => w!(BuildList, size),
            BuildListUnpack { size } => w!(BuildListUnpack, size),
            BuildSet { size } => w!(BuildSet, size),
            BuildSetUnpack { size } => w!(BuildSetUnpack, size),
            BuildMap { size } => w!(BuildMap, size),
            BuildMapForCall { size } => w!(BuildMap, size),
            DictUpdate => w!(DictUpdate),
            BuildSlice { step } => w!(BuildSlice, step),
            ListAppend { i } => w!(ListAppend, i),
            SetAdd { i } => w!(SetAdd, i),
            MapAdd { i } => w!(MapAdd, i),
            PrintExpr => w!(PrintExpr),
            LoadBuildClass => w!(LoadBuildClass),
            UnpackSequence { size } => w!(UnpackSequence, size),
            UnpackEx { args } => w!(UnpackEx, args),
            FormatValue { conversion } => w!(FormatValue, ?conversion),
            PopException => w!(PopException),
            Reverse { amount } => w!(Reverse, amount),
            GetAwaitable => w!(GetAwaitable),
            GetAIter => w!(GetAIter),
            GetANext => w!(GetANext),
            EndAsyncFor => w!(EndAsyncFor),
            ExtendedArg => w!(ExtendedArg, Arg::<u32>::marker()),
        }
    }
}

pub trait InstrDisplayContext {
    type Constant: Constant;
    fn get_constant(&self, i: usize) -> &Self::Constant;
    fn get_name(&self, i: usize) -> &str;
    fn get_varname(&self, i: usize) -> &str;
    fn get_cellname(&self, i: usize) -> &str;
}

impl<C: Constant> InstrDisplayContext for CodeObject<C> {
    type Constant = C;
    fn get_constant(&self, i: usize) -> &C {
        &self.constants[i]
    }
    fn get_name(&self, i: usize) -> &str {
        self.names[i].as_ref()
    }
    fn get_varname(&self, i: usize) -> &str {
        self.varnames[i].as_ref()
    }
    fn get_cellname(&self, i: usize) -> &str {
        self.cellvars
            .get(i)
            .unwrap_or_else(|| &self.freevars[i - self.cellvars.len()])
            .as_ref()
    }
}

impl fmt::Display for ConstantData {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.borrow_constant().fmt_display(f)
    }
}

impl<C: Constant> fmt::Debug for CodeObject<C> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "<code object {} at ??? file {:?}, line {}>",
            self.obj_name.as_ref(),
            self.source_path.as_ref(),
            self.first_line_number
        )
    }
}

/// A frozen module. Holds a code object and whether it is part of a package
#[derive(Debug)]
pub struct FrozenModule {
    pub code: CodeObject<ConstantData>,
    pub package: bool,
}

pub mod frozen_lib {
    use super::*;
    use marshal::{Read, Write};

    /// Decode a library to a iterable of frozen modules
    pub fn decode_lib(bytes: &[u8]) -> FrozenModulesIter {
        let data = lz4_flex::decompress_size_prepended(bytes).unwrap();
        let mut data = marshal::Cursor { data, position: 0 };
        let remaining = data.read_u32().unwrap();
        FrozenModulesIter { remaining, data }
    }

    pub struct FrozenModulesIter {
        remaining: u32,
        data: marshal::Cursor<Vec<u8>>,
    }

    impl Iterator for FrozenModulesIter {
        type Item = (String, FrozenModule);

        fn next(&mut self) -> Option<Self::Item> {
            if self.remaining > 0 {
                let entry = read_entry(&mut self.data).unwrap();
                self.remaining -= 1;
                Some(entry)
            } else {
                None
            }
        }

        fn size_hint(&self) -> (usize, Option<usize>) {
            (self.remaining as usize, Some(self.remaining as usize))
        }
    }
    impl ExactSizeIterator for FrozenModulesIter {}

    fn read_entry(rdr: &mut impl Read) -> Result<(String, FrozenModule), marshal::MarshalError> {
        let len = rdr.read_u32()?;
        let name = rdr.read_str(len)?.to_owned();
        let code = marshal::deserialize_code(rdr, BasicBag)?;
        let package = rdr.read_u8()? != 0;
        Ok((name, FrozenModule { code, package }))
    }

    /// Encode the given iterator of frozen modules into a compressed vector of bytes
    pub fn encode_lib<'a, I>(lib: I) -> Vec<u8>
    where
        I: IntoIterator<Item = (&'a str, &'a FrozenModule)>,
        I::IntoIter: ExactSizeIterator + Clone,
    {
        let iter = lib.into_iter();
        let mut data = Vec::new();
        write_lib(&mut data, iter);
        lz4_flex::compress_prepend_size(&data)
    }

    fn write_lib<'a>(
        buf: &mut impl Write,
        lib: impl ExactSizeIterator<Item = (&'a str, &'a FrozenModule)>,
    ) {
        marshal::write_len(buf, lib.len());
        for (name, module) in lib {
            write_entry(buf, name, module);
        }
    }

    fn write_entry(buf: &mut impl Write, name: &str, module: &FrozenModule) {
        marshal::write_len(buf, name.len());
        buf.write_slice(name.as_bytes());
        marshal::serialize_code(buf, &module.code);
        buf.write_u8(module.package as u8);
    }
}
