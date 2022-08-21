//! Implement python as a virtual machine with bytecodes. This module
//! implements bytecode structure.

#![doc(html_logo_url = "https://raw.githubusercontent.com/RustPython/RustPython/main/logo.png")]
#![doc(html_root_url = "https://docs.rs/rustpython-bytecode/")]

use bitflags::bitflags;
use bstr::ByteSlice;
use itertools::Itertools;
use num_bigint::BigInt;
use num_complex::Complex64;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, fmt, hash};

/// Sourcecode location.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Location {
    row: u32,
    column: u32,
}

impl Location {
    /// Creates a new Location object at the given row and column.
    ///
    /// # Example
    /// ```
    /// use rustpython_bytecode::Location;
    /// let loc = Location::new(10, 10);
    /// ```
    pub fn new(row: usize, column: usize) -> Self {
        let row = row.try_into().expect("Location::row over u32");
        let column = column.try_into().expect("Location::column over u32");
        Location { row, column }
    }

    /// Current row
    pub fn row(&self) -> usize {
        self.row as usize
    }

    /// Current column
    pub fn column(&self) -> usize {
        self.column as usize
    }
}

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
            ConstantData::Tuple { elements } => Tuple {
                elements: Box::new(elements.iter().map(|e| e.borrow_constant())),
            },
            ConstantData::None => None,
            ConstantData::Ellipsis => Ellipsis,
        }
    }
}

/// A Constant Bag
pub trait ConstantBag: Sized + Copy {
    type Constant: Constant;
    fn make_constant<C: Constant>(&self, constant: BorrowedConstant<C>) -> Self::Constant;
    fn make_name(&self, name: &str) -> <Self::Constant as Constant>::Name;
}

#[derive(Clone, Copy)]
pub struct BasicBag;

impl ConstantBag for BasicBag {
    type Constant = ConstantData;
    fn make_constant<C: Constant>(&self, constant: BorrowedConstant<C>) -> Self::Constant {
        constant.to_owned()
    }
    fn make_name(&self, name: &str) -> <Self::Constant as Constant>::Name {
        name.to_owned()
    }
}

/// Primary container of a single code object. Each python function has
/// a codeobject. Also a module has a codeobject.
#[derive(Clone, Serialize, Deserialize)]
pub struct CodeObject<C: Constant = ConstantData> {
    pub instructions: Box<[Instruction]>,
    pub locations: Box<[Location]>,
    pub flags: CodeFlags,
    pub posonlyarg_count: usize,
    // Number of positional-only arguments
    pub arg_count: usize,
    pub kwonlyarg_count: usize,
    pub source_path: C::Name,
    pub first_line_number: usize,
    pub max_stackdepth: u32,
    pub obj_name: C::Name,
    // Name of the object that created this code object
    pub cell2arg: Option<Box<[isize]>>,
    pub constants: Box<[C]>,
    #[serde(bound(
        deserialize = "C::Name: serde::Deserialize<'de>",
        serialize = "C::Name: serde::Serialize"
    ))]
    pub names: Box<[C::Name]>,
    pub varnames: Box<[C::Name]>,
    pub cellvars: Box<[C::Name]>,
    pub freevars: Box<[C::Name]>,
}

bitflags! {
    #[derive(Serialize, Deserialize)]
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

#[derive(Serialize, Debug, Deserialize, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd)]
#[repr(transparent)]
// XXX: if you add a new instruction that stores a Label, make sure to add it in
// Instruction::label_arg{,_mut}
pub struct Label(pub u32);

impl fmt::Display for Label {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Transforms a value prior to formatting it.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
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

impl TryFrom<usize> for ConversionFlag {
    type Error = usize;
    fn try_from(b: usize) -> Result<Self, Self::Error> {
        let b = b.try_into().map_err(|_| b)?;
        match b {
            0 => Ok(Self::None),
            b's' => Ok(Self::Str),
            b'a' => Ok(Self::Ascii),
            b'r' => Ok(Self::Repr),
            b => Err(b as usize),
        }
    }
}

/// The kind of Raise that occurred.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RaiseKind {
    Reraise,
    Raise,
    RaiseCause,
}

pub type NameIdx = u32;

/// A Single bytecode instruction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Instruction {
    /// Importing by name
    ImportName {
        idx: NameIdx,
    },
    /// Importing without name
    ImportNameless,
    /// Import *
    ImportStar,
    /// from ... import ...
    ImportFrom {
        idx: NameIdx,
    },
    LoadFast(NameIdx),
    LoadNameAny(NameIdx),
    LoadGlobal(NameIdx),
    LoadDeref(NameIdx),
    LoadClassDeref(NameIdx),
    StoreFast(NameIdx),
    StoreLocal(NameIdx),
    StoreGlobal(NameIdx),
    StoreDeref(NameIdx),
    DeleteFast(NameIdx),
    DeleteLocal(NameIdx),
    DeleteGlobal(NameIdx),
    DeleteDeref(NameIdx),
    LoadClosure(NameIdx),
    Subscript,
    StoreSubscript,
    DeleteSubscript,
    StoreAttr {
        idx: NameIdx,
    },
    DeleteAttr {
        idx: NameIdx,
    },
    LoadConst {
        /// index into constants vec
        idx: u32,
    },
    UnaryOperation {
        op: UnaryOperator,
    },
    BinaryOperation {
        op: BinaryOperator,
    },
    BinaryOperationInplace {
        op: BinaryOperator,
    },
    LoadAttr {
        idx: NameIdx,
    },
    TestOperation {
        op: TestOperator,
    },
    CompareOperation {
        op: ComparisonOperator,
    },
    Pop,
    Rotate2,
    Rotate3,
    Duplicate,
    Duplicate2,
    GetIter,
    Continue {
        target: Label,
    },
    Break {
        target: Label,
    },
    Jump {
        target: Label,
    },
    /// Pop the top of the stack, and jump if this value is true.
    JumpIfTrue {
        target: Label,
    },
    /// Pop the top of the stack, and jump if this value is false.
    JumpIfFalse {
        target: Label,
    },
    /// Peek at the top of the stack, and jump if this value is true.
    /// Otherwise, pop top of stack.
    JumpIfTrueOrPop {
        target: Label,
    },
    /// Peek at the top of the stack, and jump if this value is false.
    /// Otherwise, pop top of stack.
    JumpIfFalseOrPop {
        target: Label,
    },
    MakeFunction(MakeFunctionFlags),
    CallFunctionPositional {
        nargs: u32,
    },
    CallFunctionKeyword {
        nargs: u32,
    },
    CallFunctionEx {
        has_kwargs: bool,
    },
    LoadMethod {
        idx: NameIdx,
    },
    CallMethodPositional {
        nargs: u32,
    },
    CallMethodKeyword {
        nargs: u32,
    },
    CallMethodEx {
        has_kwargs: bool,
    },
    ForIter {
        target: Label,
    },
    ReturnValue,
    YieldValue,
    YieldFrom,
    SetupAnnotation,
    SetupLoop {
        break_target: Label,
    },

    /// Setup a finally handler, which will be called whenever one of this events occurs:
    /// - the block is popped
    /// - the function returns
    /// - an exception is returned
    SetupFinally {
        handler: Label,
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
        handler: Label,
    },
    SetupWith {
        end: Label,
    },
    WithCleanupStart,
    WithCleanupFinish,
    PopBlock,
    Raise {
        kind: RaiseKind,
    },
    BuildString {
        size: u32,
    },
    BuildTuple {
        unpack: bool,
        size: u32,
    },
    BuildList {
        unpack: bool,
        size: u32,
    },
    BuildSet {
        unpack: bool,
        size: u32,
    },
    BuildMap {
        unpack: bool,
        for_call: bool,
        size: u32,
    },
    DictUpdate,
    BuildSlice {
        /// whether build a slice with a third step argument
        step: bool,
    },
    ListAppend {
        i: u32,
    },
    SetAdd {
        i: u32,
    },
    MapAdd {
        i: u32,
    },

    PrintExpr,
    LoadBuildClass,
    UnpackSequence {
        size: u32,
    },
    UnpackEx {
        before: u8,
        after: u8,
    },
    FormatValue {
        conversion: ConversionFlag,
    },
    PopException,
    Reverse {
        amount: u32,
    },
    GetAwaitable,
    BeforeAsyncWith,
    SetupAsyncWith {
        end: Label,
    },
    GetAIter,
    GetANext,
    EndAsyncFor,
}
static_assertions::assert_eq_size!(Instruction, u64);

use self::Instruction::*;

bitflags! {
    #[derive(Serialize, Deserialize)]
    pub struct MakeFunctionFlags: u8 {
        const CLOSURE = 0x01;
        const ANNOTATIONS = 0x02;
        const KW_ONLY_DEFAULTS = 0x04;
        const DEFAULTS = 0x08;
    }
}

/// A Constant (which usually encapsulates data within it)
///
/// # Examples
/// ```
/// use rustpython_bytecode::ConstantData;
/// let a = ConstantData::Float {value: 120f64};
/// let b = ConstantData::Boolean {value: false};
/// assert_ne!(a, b);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
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
        std::mem::discriminant(self).hash(state);
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
    Tuple { elements: BorrowedTupleIter<'a, C> },
    None,
    Ellipsis,
}

type BorrowedTupleIter<'a, C> = Box<dyn Iterator<Item = BorrowedConstant<'a, C>> + 'a>;

impl<C: Constant> BorrowedConstant<'_, C> {
    // takes `self` because we need to consume the iterator
    pub fn fmt_display(self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            BorrowedConstant::Integer { value } => write!(f, "{}", value),
            BorrowedConstant::Float { value } => write!(f, "{}", value),
            BorrowedConstant::Complex { value } => write!(f, "{}", value),
            BorrowedConstant::Boolean { value } => {
                write!(f, "{}", if value { "True" } else { "False" })
            }
            BorrowedConstant::Str { value } => write!(f, "{:?}", value),
            BorrowedConstant::Bytes { value } => write!(f, "b{:?}", value.as_bstr()),
            BorrowedConstant::Code { code } => write!(f, "{:?}", code),
            BorrowedConstant::Tuple { elements } => {
                write!(f, "(")?;
                let mut first = true;
                for c in elements {
                    if first {
                        first = false
                    } else {
                        write!(f, ", ")?;
                    }
                    c.fmt_display(f)?;
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
                elements: elements.map(BorrowedConstant::to_owned).collect(),
            },
            BorrowedConstant::None => None,
            BorrowedConstant::Ellipsis => Ellipsis,
        }
    }
}

/// The possible comparison operators
#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TestOperator {
    In,
    NotIn,
    Is,
    IsNot,
    /// two exceptions that match?
    ExceptionMatch,
}

/// The possible Binary operators
/// # Examples
///
/// ```
/// use rustpython_bytecode::Instruction::BinaryOperation;
/// use rustpython_bytecode::BinaryOperator::Add;
/// let op = BinaryOperation {op: Add};
/// ```
#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

/// The possible unary operators
#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnaryOperator {
    Not,
    Invert,
    Minus,
    Plus,
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
        let nargs = self.arg_count;
        let nkwargs = self.kwonlyarg_count;
        let mut varargspos = nargs + nkwargs;
        let posonlyargs = &self.varnames[..self.posonlyarg_count];
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
        for instruction in &*self.instructions {
            if let Some(l) = instruction.label_arg() {
                label_targets.insert(*l);
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
        for (offset, instruction) in self.instructions.iter().enumerate() {
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
            instruction.fmt_dis(
                f,
                &self.constants,
                &self.names,
                &self.varnames,
                &self.cellvars,
                &self.freevars,
                expand_codeobjects,
                level,
            )?;
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

/// Error that occurs during code deserialization
#[derive(Debug)]
#[non_exhaustive]
pub enum CodeDeserializeError {
    /// Unexpected End Of File
    Eof,
    /// Invalid Bytecode
    Other,
}

impl fmt::Display for CodeDeserializeError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Eof => f.write_str("unexpected end of data"),
            Self::Other => f.write_str("invalid bytecode"),
        }
    }
}

impl std::error::Error for CodeDeserializeError {}

impl CodeObject<ConstantData> {
    /// Load a code object from bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self, CodeDeserializeError> {
        use lz4_flex::block::DecompressError;
        let raw_bincode = lz4_flex::decompress_size_prepended(data).map_err(|e| match e {
            DecompressError::OutputTooSmall { .. } | DecompressError::ExpectedAnotherByte => {
                CodeDeserializeError::Eof
            }
            _ => CodeDeserializeError::Other,
        })?;
        let data = bincode::deserialize(&raw_bincode).map_err(|e| match *e {
            bincode::ErrorKind::Io(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                CodeDeserializeError::Eof
            }
            _ => CodeDeserializeError::Other,
        })?;
        Ok(data)
    }

    /// Serialize this bytecode to bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        let data = bincode::serialize(&self).expect("CodeObject is not serializable");
        lz4_flex::compress_prepend_size(&data)
    }
}

impl<C: Constant> fmt::Display for CodeObject<C> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.display_inner(f, false, 1)?;
        for constant in &*self.constants {
            if let BorrowedConstant::Code { code } = constant.borrow_constant() {
                writeln!(f, "\nDisassembly of {:?}", code)?;
                code.fmt(f)?;
            }
        }
        Ok(())
    }
}

impl Instruction {
    /// Gets the label stored inside this instruction, if it exists
    #[inline]
    pub fn label_arg(&self) -> Option<&Label> {
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
            | SetupLoop { break_target: l }
            | Continue { target: l } => Some(l),
            _ => None,
        }
    }

    /// Gets a mutable reference to the label stored inside this instruction, if it exists
    #[inline]
    pub fn label_arg_mut(&mut self) -> Option<&mut Label> {
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
            | SetupLoop { break_target: l }
            | Continue { target: l } => Some(l),
            _ => None,
        }
    }

    /// Whether this is an unconditional branching
    ///
    /// # Examples
    ///
    /// ```
    /// use rustpython_bytecode::{Instruction, Label};
    /// let label = Label(0xF);
    /// let jump_inst = Instruction::Jump {target: label};
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
    /// use rustpython_bytecode::{Instruction, Label, UnaryOperator};
    /// let jump_instruction = Instruction::Jump {target: Label(0xF)};
    /// let invert_instruction = Instruction::UnaryOperation {op: UnaryOperator::Invert};
    /// assert_eq!(jump_instruction.stack_effect(true), 0);
    /// assert_eq!(invert_instruction.stack_effect(false), 0);
    /// ```
    ///
    pub fn stack_effect(&self, jump: bool) -> i32 {
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
                -2 - flags.contains(MakeFunctionFlags::CLOSURE) as i32
                    - flags.contains(MakeFunctionFlags::ANNOTATIONS) as i32
                    - flags.contains(MakeFunctionFlags::KW_ONLY_DEFAULTS) as i32
                    - flags.contains(MakeFunctionFlags::DEFAULTS) as i32
                    + 1
            }
            CallFunctionPositional { nargs } => -(*nargs as i32) - 1 + 1,
            CallMethodPositional { nargs } => -(*nargs as i32) - 3 + 1,
            CallFunctionKeyword { nargs } => -1 - (*nargs as i32) - 1 + 1,
            CallMethodKeyword { nargs } => -1 - (*nargs as i32) - 3 + 1,
            CallFunctionEx { has_kwargs } => -1 - (*has_kwargs as i32) - 1 + 1,
            CallMethodEx { has_kwargs } => -1 - (*has_kwargs as i32) - 3 + 1,
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
            SetupAnnotation
            | SetupLoop { .. }
            | SetupFinally { .. }
            | EnterFinally
            | EndFinally => 0,
            SetupExcept { .. } => {
                if jump {
                    1
                } else {
                    0
                }
            }
            SetupWith { .. } => {
                if jump {
                    0
                } else {
                    1
                }
            }
            WithCleanupStart => 0,
            WithCleanupFinish => -1,
            PopBlock => 0,
            Raise { kind } => -(*kind as u8 as i32),
            BuildString { size }
            | BuildTuple { size, .. }
            | BuildList { size, .. }
            | BuildSet { size, .. } => -(*size as i32) + 1,
            BuildMap { unpack, size, .. } => {
                let nargs = if *unpack { *size } else { *size * 2 };
                -(nargs as i32) + 1
            }
            DictUpdate => -1,
            BuildSlice { step } => -2 - (*step as i32) + 1,
            ListAppend { .. } | SetAdd { .. } => -1,
            MapAdd { .. } => -2,
            PrintExpr => -1,
            LoadBuildClass => 1,
            UnpackSequence { size } => -1 + *size as i32,
            UnpackEx { before, after } => -1 + *before as i32 + 1 + *after as i32,
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
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn fmt_dis<C: Constant>(
        &self,
        f: &mut fmt::Formatter,
        constants: &[C],
        names: &[C::Name],
        varnames: &[C::Name],
        cellvars: &[C::Name],
        freevars: &[C::Name],
        expand_codeobjects: bool,
        level: usize,
    ) -> fmt::Result {
        macro_rules! w {
            ($variant:ident) => {
                writeln!(f, stringify!($variant))
            };
            ($variant:ident, $var:expr) => {
                writeln!(f, "{:20} ({})", stringify!($variant), $var)
            };
            ($variant:ident, $var1:expr, $var2:expr) => {
                writeln!(f, "{:20} ({}, {})", stringify!($variant), $var1, $var2)
            };
            ($variant:ident, $var1:expr, $var2:expr, $var3:expr) => {
                writeln!(
                    f,
                    "{:20} ({}, {}, {})",
                    stringify!($variant),
                    $var1,
                    $var2,
                    $var3
                )
            };
        }

        let varname = |i: u32| varnames[i as usize].as_ref();
        let name = |i: u32| names[i as usize].as_ref();
        let cellname = |i: u32| {
            cellvars
                .get(i as usize)
                .unwrap_or_else(|| &freevars[i as usize - cellvars.len()])
                .as_ref()
        };

        match self {
            ImportName { idx } => w!(ImportName, name(*idx)),
            ImportNameless => w!(ImportNameless),
            ImportStar => w!(ImportStar),
            ImportFrom { idx } => w!(ImportFrom, name(*idx)),
            LoadFast(idx) => w!(LoadFast, *idx, varname(*idx)),
            LoadNameAny(idx) => w!(LoadNameAny, *idx, name(*idx)),
            LoadGlobal(idx) => w!(LoadGlobal, *idx, name(*idx)),
            LoadDeref(idx) => w!(LoadDeref, *idx, cellname(*idx)),
            LoadClassDeref(idx) => w!(LoadClassDeref, *idx, cellname(*idx)),
            StoreFast(idx) => w!(StoreFast, *idx, varname(*idx)),
            StoreLocal(idx) => w!(StoreLocal, *idx, name(*idx)),
            StoreGlobal(idx) => w!(StoreGlobal, *idx, name(*idx)),
            StoreDeref(idx) => w!(StoreDeref, *idx, cellname(*idx)),
            DeleteFast(idx) => w!(DeleteFast, *idx, varname(*idx)),
            DeleteLocal(idx) => w!(DeleteLocal, *idx, name(*idx)),
            DeleteGlobal(idx) => w!(DeleteGlobal, *idx, name(*idx)),
            DeleteDeref(idx) => w!(DeleteDeref, *idx, cellname(*idx)),
            LoadClosure(i) => w!(LoadClosure, *i, cellname(*i)),
            Subscript => w!(Subscript),
            StoreSubscript => w!(StoreSubscript),
            DeleteSubscript => w!(DeleteSubscript),
            StoreAttr { idx } => w!(StoreAttr, name(*idx)),
            DeleteAttr { idx } => w!(DeleteAttr, name(*idx)),
            LoadConst { idx } => {
                let value = &constants[*idx as usize];
                match value.borrow_constant() {
                    BorrowedConstant::Code { code } if expand_codeobjects => {
                        writeln!(f, "{:20} ({:?}):", "LoadConst", code)?;
                        code.display_inner(f, true, level + 1)?;
                        Ok(())
                    }
                    c => {
                        write!(f, "{:20} (", "LoadConst")?;
                        c.fmt_display(f)?;
                        writeln!(f, ")")
                    }
                }
            }
            UnaryOperation { op } => w!(UnaryOperation, format_args!("{:?}", op)),
            BinaryOperation { op } => w!(BinaryOperation, format_args!("{:?}", op)),
            BinaryOperationInplace { op } => {
                w!(BinaryOperationInplace, format_args!("{:?}", op))
            }
            LoadAttr { idx } => w!(LoadAttr, name(*idx)),
            TestOperation { op } => w!(TestOperation, format_args!("{:?}", op)),
            CompareOperation { op } => w!(CompareOperation, format_args!("{:?}", op)),
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
            MakeFunction(flags) => w!(MakeFunction, format_args!("{:?}", flags)),
            CallFunctionPositional { nargs } => w!(CallFunctionPositional, nargs),
            CallFunctionKeyword { nargs } => w!(CallFunctionKeyword, nargs),
            CallFunctionEx { has_kwargs } => w!(CallFunctionEx, has_kwargs),
            LoadMethod { idx } => w!(LoadMethod, name(*idx)),
            CallMethodPositional { nargs } => w!(CallMethodPositional, nargs),
            CallMethodKeyword { nargs } => w!(CallMethodKeyword, nargs),
            CallMethodEx { has_kwargs } => w!(CallMethodEx, has_kwargs),
            ForIter { target } => w!(ForIter, target),
            ReturnValue => w!(ReturnValue),
            YieldValue => w!(YieldValue),
            YieldFrom => w!(YieldFrom),
            SetupAnnotation => w!(SetupAnnotation),
            SetupLoop { break_target } => w!(SetupLoop, break_target),
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
            Raise { kind } => w!(Raise, format_args!("{:?}", kind)),
            BuildString { size } => w!(BuildString, size),
            BuildTuple { size, unpack } => w!(BuildTuple, size, unpack),
            BuildList { size, unpack } => w!(BuildList, size, unpack),
            BuildSet { size, unpack } => w!(BuildSet, size, unpack),
            BuildMap {
                size,
                unpack,
                for_call,
            } => w!(BuildMap, size, unpack, for_call),
            DictUpdate => w!(DictUpdate),
            BuildSlice { step } => w!(BuildSlice, step),
            ListAppend { i } => w!(ListAppend, i),
            SetAdd { i } => w!(SetAdd, i),
            MapAdd { i } => w!(MapAdd, i),
            PrintExpr => w!(PrintExpr),
            LoadBuildClass => w!(LoadBuildClass),
            UnpackSequence { size } => w!(UnpackSequence, size),
            UnpackEx { before, after } => w!(UnpackEx, before, after),
            FormatValue { conversion } => w!(FormatValue, format_args!("{:?}", conversion)),
            PopException => w!(PopException),
            Reverse { amount } => w!(Reverse, amount),
            GetAwaitable => w!(GetAwaitable),
            GetAIter => w!(GetAIter),
            GetANext => w!(GetANext),
            EndAsyncFor => w!(EndAsyncFor),
        }
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
#[derive(Serialize, Deserialize, Debug)]
pub struct FrozenModule {
    pub code: CodeObject<ConstantData>,
    pub package: bool,
}

pub mod frozen_lib {
    use super::*;
    use bincode::{options, Options};
    use std::io;

    /// Decode a library to a iterable of frozen modules
    pub fn decode_lib(bytes: &[u8]) -> FrozenModulesIter {
        let data = lz4_flex::decompress_size_prepended(bytes).unwrap();
        let r = VecReader { data, pos: 0 };
        let mut de = bincode::Deserializer::with_bincode_read(r, options());
        let len = u64::deserialize(&mut de).unwrap().try_into().unwrap();
        FrozenModulesIter { len, de }
    }

    pub struct FrozenModulesIter {
        len: usize,
        // ideally this could be a SeqAccess, but I think that would require existential types
        de: bincode::Deserializer<VecReader, bincode::DefaultOptions>,
    }

    impl Iterator for FrozenModulesIter {
        type Item = (String, FrozenModule);

        fn next(&mut self) -> Option<Self::Item> {
            // manually mimic bincode's seq encoding, which is <len:u64> <element*len>
            // This probably won't change (bincode doesn't require padding or anything), but
            // it's not guaranteed by semver as far as I can tell
            if self.len > 0 {
                let entry = Deserialize::deserialize(&mut self.de).unwrap();
                self.len -= 1;
                Some(entry)
            } else {
                None
            }
        }

        fn size_hint(&self) -> (usize, Option<usize>) {
            (self.len, Some(self.len))
        }
    }

    impl ExactSizeIterator for FrozenModulesIter {}

    /// Encode the given iterator of frozen modules into a compressed vector of bytes
    pub fn encode_lib<'a, I>(lib: I) -> Vec<u8>
    where
        I: IntoIterator<Item = (&'a str, &'a FrozenModule)>,
        I::IntoIter: ExactSizeIterator + Clone,
    {
        let iter = lib.into_iter();
        let data = options().serialize(&SerializeLib { iter }).unwrap();
        lz4_flex::compress_prepend_size(&data)
    }

    struct SerializeLib<I> {
        iter: I,
    }

    impl<'a, I> Serialize for SerializeLib<I>
    where
        I: ExactSizeIterator<Item = (&'a str, &'a FrozenModule)> + Clone,
    {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
        {
            serializer.collect_seq(self.iter.clone())
        }
    }

    /// Owned version of bincode::de::read::SliceReader<'a>
    struct VecReader {
        data: Vec<u8>,
        pos: usize,
    }

    impl io::Read for VecReader {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            let mut subslice = &self.data[self.pos..];
            let n = io::Read::read(&mut subslice, buf)?;
            self.pos += n;
            Ok(n)
        }
        fn read_exact(&mut self, buf: &mut [u8]) -> io::Result<()> {
            self.get_byte_slice(buf.len())
                .map(|data| buf.copy_from_slice(data))
        }
    }

    impl VecReader {
        #[inline(always)]
        fn get_byte_slice(&mut self, length: usize) -> io::Result<&[u8]> {
            let subslice = &self.data[self.pos..];
            match subslice.get(..length) {
                Some(ret) => {
                    self.pos += length;
                    Ok(ret)
                }
                None => Err(io::ErrorKind::UnexpectedEof.into()),
            }
        }
    }

    impl<'storage> bincode::BincodeRead<'storage> for VecReader {
        fn forward_read_str<V>(&mut self, length: usize, visitor: V) -> bincode::Result<V::Value>
        where
            V: serde::de::Visitor<'storage>,
        {
            let bytes = self.get_byte_slice(length)?;
            match ::std::str::from_utf8(bytes) {
                Ok(s) => visitor.visit_str(s),
                Err(e) => Err(bincode::ErrorKind::InvalidUtf8Encoding(e).into()),
            }
        }

        fn get_byte_buffer(&mut self, length: usize) -> bincode::Result<Vec<u8>> {
            self.get_byte_slice(length)
                .map(|x| x.to_vec())
                .map_err(Into::into)
        }

        fn forward_read_bytes<V>(&mut self, length: usize, visitor: V) -> bincode::Result<V::Value>
        where
            V: serde::de::Visitor<'storage>,
        {
            visitor.visit_bytes(self.get_byte_slice(length)?)
        }
    }
}
