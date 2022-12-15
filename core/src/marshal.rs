use core::fmt;
use std::convert::Infallible;

use num_bigint::{BigInt, Sign};
use num_complex::Complex64;

use crate::{bytecode::*, Location};

pub const FORMAT_VERSION: u32 = 4;

#[derive(Debug)]
pub enum MarshalError {
    /// Unexpected End Of File
    Eof,
    /// Invalid Bytecode
    InvalidBytecode,
    /// Invalid utf8 in string
    InvalidUtf8,
    /// Bad type marker
    BadType,
}

impl fmt::Display for MarshalError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Eof => f.write_str("unexpected end of data"),
            Self::InvalidBytecode => f.write_str("invalid bytecode"),
            Self::InvalidUtf8 => f.write_str("invalid utf8"),
            Self::BadType => f.write_str("bad type marker"),
        }
    }
}

impl From<std::str::Utf8Error> for MarshalError {
    fn from(_: std::str::Utf8Error) -> Self {
        Self::InvalidUtf8
    }
}

impl std::error::Error for MarshalError {}

type Result<T, E = MarshalError> = std::result::Result<T, E>;

#[repr(u8)]
enum Type {
    // Null = b'0',
    None = b'N',
    False = b'F',
    True = b'T',
    StopIter = b'S',
    Ellipsis = b'.',
    Int = b'i',
    Float = b'g',
    Complex = b'y',
    // Long = b'l',  // i32
    Bytes = b's', // = TYPE_STRING
    // Interned = b't',
    // Ref = b'r',
    Tuple = b'(',
    List = b'[',
    Dict = b'{',
    Code = b'c',
    Unicode = b'u',
    // Unknown = b'?',
    Set = b'<',
    FrozenSet = b'>',
    Ascii = b'a',
    // AsciiInterned = b'A',
    // SmallTuple = b')',
    // ShortAscii = b'z',
    // ShortAsciiInterned = b'Z',
}
// const FLAG_REF: u8 = b'\x80';

impl TryFrom<u8> for Type {
    type Error = MarshalError;
    fn try_from(value: u8) -> Result<Self> {
        use Type::*;
        Ok(match value {
            // b'0' => Null,
            b'N' => None,
            b'F' => False,
            b'T' => True,
            b'S' => StopIter,
            b'.' => Ellipsis,
            b'i' => Int,
            b'g' => Float,
            b'y' => Complex,
            // b'l' => Long,
            b's' => Bytes,
            // b't' => Interned,
            // b'r' => Ref,
            b'(' => Tuple,
            b'[' => List,
            b'{' => Dict,
            b'c' => Code,
            b'u' => Unicode,
            // b'?' => Unknown,
            b'<' => Set,
            b'>' => FrozenSet,
            b'a' => Ascii,
            // b'A' => AsciiInterned,
            // b')' => SmallTuple,
            // b'z' => ShortAscii,
            // b'Z' => ShortAsciiInterned,
            _ => return Err(MarshalError::BadType),
        })
    }
}

pub trait Read {
    fn read_slice(&mut self, n: u32) -> Result<&[u8]>;
    fn read_array<const N: usize>(&mut self) -> Result<&[u8; N]> {
        self.read_slice(N as u32).map(|s| s.try_into().unwrap())
    }
    fn read_str(&mut self, len: u32) -> Result<&str> {
        Ok(std::str::from_utf8(self.read_slice(len)?)?)
    }
    fn read_u8(&mut self) -> Result<u8> {
        Ok(u8::from_le_bytes(*self.read_array()?))
    }
    fn read_u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(*self.read_array()?))
    }
    fn read_u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(*self.read_array()?))
    }
    fn read_u64(&mut self) -> Result<u64> {
        Ok(u64::from_le_bytes(*self.read_array()?))
    }
}

impl Read for &[u8] {
    fn read_slice(&mut self, n: u32) -> Result<&[u8]> {
        let data = self.get(..n as usize).ok_or(MarshalError::Eof)?;
        *self = &self[n as usize..];
        Ok(data)
    }
}

pub struct Cursor<B> {
    pub data: B,
    pub position: usize,
}

impl<B: AsRef<[u8]>> Read for Cursor<B> {
    fn read_slice(&mut self, n: u32) -> Result<&[u8]> {
        let data = &self.data.as_ref()[self.position..];
        let slice = data.get(..n as usize).ok_or(MarshalError::Eof)?;
        self.position += n as usize;
        Ok(slice)
    }
}

pub fn deserialize_code<R: Read, Bag: ConstantBag>(
    rdr: &mut R,
    bag: Bag,
) -> Result<CodeObject<Bag::Constant>> {
    let len = rdr.read_u32()?;
    let instructions = rdr.read_slice(len * 2)?;
    let instructions = instructions
        .chunks_exact(2)
        .map(|cu| {
            let op = Instruction::try_from(cu[0])?;
            let arg = OpArgByte(cu[1]);
            Ok(CodeUnit { op, arg })
        })
        .collect::<Result<Box<[CodeUnit]>>>()?;

    let len = rdr.read_u32()?;
    let locations = (0..len)
        .map(|_| {
            Ok(Location {
                row: rdr.read_u32()?,
                column: rdr.read_u32()?,
            })
        })
        .collect::<Result<Box<[Location]>>>()?;

    let flags = CodeFlags::from_bits_truncate(rdr.read_u16()?);

    let posonlyarg_count = rdr.read_u32()?;
    let arg_count = rdr.read_u32()?;
    let kwonlyarg_count = rdr.read_u32()?;

    let len = rdr.read_u32()?;
    let source_path = bag.make_name(rdr.read_str(len)?);

    let first_line_number = rdr.read_u32()?;
    let max_stackdepth = rdr.read_u32()?;

    let len = rdr.read_u32()?;
    let obj_name = bag.make_name(rdr.read_str(len)?);

    let len = rdr.read_u32()?;
    let cell2arg = (len != 0)
        .then(|| {
            (0..len)
                .map(|_| Ok(rdr.read_u32()? as i32))
                .collect::<Result<Box<[i32]>>>()
        })
        .transpose()?;

    let len = rdr.read_u32()?;
    let constants = (0..len)
        .map(|_| deserialize_value(rdr, bag))
        .collect::<Result<Box<[_]>>>()?;

    let mut read_names = || {
        let len = rdr.read_u32()?;
        (0..len)
            .map(|_| {
                let len = rdr.read_u32()?;
                Ok(bag.make_name(rdr.read_str(len)?))
            })
            .collect::<Result<Box<[_]>>>()
    };

    let names = read_names()?;
    let varnames = read_names()?;
    let cellvars = read_names()?;
    let freevars = read_names()?;

    Ok(CodeObject {
        instructions,
        locations,
        flags,
        posonlyarg_count,
        arg_count,
        kwonlyarg_count,
        source_path,
        first_line_number,
        max_stackdepth,
        obj_name,
        cell2arg,
        constants,
        names,
        varnames,
        cellvars,
        freevars,
    })
}

pub trait MarshalBag: Copy {
    type Value;
    fn make_bool(&self, value: bool) -> Self::Value;
    fn make_none(&self) -> Self::Value;
    fn make_ellipsis(&self) -> Self::Value;
    fn make_float(&self, value: f64) -> Self::Value;
    fn make_complex(&self, value: Complex64) -> Self::Value;
    fn make_str(&self, value: &str) -> Self::Value;
    fn make_bytes(&self, value: &[u8]) -> Self::Value;
    fn make_int(&self, value: BigInt) -> Self::Value;
    fn make_tuple(&self, elements: impl Iterator<Item = Self::Value>) -> Self::Value;
    fn make_code(
        &self,
        code: CodeObject<<Self::ConstantBag as ConstantBag>::Constant>,
    ) -> Self::Value;
    fn make_stop_iter(&self) -> Result<Self::Value>;
    fn make_list(&self, it: impl Iterator<Item = Self::Value>) -> Result<Self::Value>;
    fn make_set(&self, it: impl Iterator<Item = Self::Value>) -> Result<Self::Value>;
    fn make_frozenset(&self, it: impl Iterator<Item = Self::Value>) -> Result<Self::Value>;
    fn make_dict(
        &self,
        it: impl Iterator<Item = (Self::Value, Self::Value)>,
    ) -> Result<Self::Value>;
    type ConstantBag: ConstantBag;
    fn constant_bag(self) -> Self::ConstantBag;
}

impl<Bag: ConstantBag> MarshalBag for Bag {
    type Value = Bag::Constant;
    fn make_bool(&self, value: bool) -> Self::Value {
        self.make_constant::<Bag::Constant>(BorrowedConstant::Boolean { value })
    }
    fn make_none(&self) -> Self::Value {
        self.make_constant::<Bag::Constant>(BorrowedConstant::None)
    }
    fn make_ellipsis(&self) -> Self::Value {
        self.make_constant::<Bag::Constant>(BorrowedConstant::Ellipsis)
    }
    fn make_float(&self, value: f64) -> Self::Value {
        self.make_constant::<Bag::Constant>(BorrowedConstant::Float { value })
    }
    fn make_complex(&self, value: Complex64) -> Self::Value {
        self.make_constant::<Bag::Constant>(BorrowedConstant::Complex { value })
    }
    fn make_str(&self, value: &str) -> Self::Value {
        self.make_constant::<Bag::Constant>(BorrowedConstant::Str { value })
    }
    fn make_bytes(&self, value: &[u8]) -> Self::Value {
        self.make_constant::<Bag::Constant>(BorrowedConstant::Bytes { value })
    }
    fn make_int(&self, value: BigInt) -> Self::Value {
        self.make_int(value)
    }
    fn make_tuple(&self, elements: impl Iterator<Item = Self::Value>) -> Self::Value {
        self.make_tuple(elements)
    }
    fn make_code(
        &self,
        code: CodeObject<<Self::ConstantBag as ConstantBag>::Constant>,
    ) -> Self::Value {
        self.make_code(code)
    }
    fn make_stop_iter(&self) -> Result<Self::Value> {
        Err(MarshalError::BadType)
    }
    fn make_list(&self, _: impl Iterator<Item = Self::Value>) -> Result<Self::Value> {
        Err(MarshalError::BadType)
    }
    fn make_set(&self, _: impl Iterator<Item = Self::Value>) -> Result<Self::Value> {
        Err(MarshalError::BadType)
    }
    fn make_frozenset(&self, _: impl Iterator<Item = Self::Value>) -> Result<Self::Value> {
        Err(MarshalError::BadType)
    }
    fn make_dict(
        &self,
        _: impl Iterator<Item = (Self::Value, Self::Value)>,
    ) -> Result<Self::Value> {
        Err(MarshalError::BadType)
    }
    type ConstantBag = Self;
    fn constant_bag(self) -> Self::ConstantBag {
        self
    }
}

pub fn deserialize_value<R: Read, Bag: MarshalBag>(rdr: &mut R, bag: Bag) -> Result<Bag::Value> {
    let typ = Type::try_from(rdr.read_u8()?)?;
    let value = match typ {
        Type::True => bag.make_bool(true),
        Type::False => bag.make_bool(false),
        Type::None => bag.make_none(),
        Type::StopIter => bag.make_stop_iter()?,
        Type::Ellipsis => bag.make_ellipsis(),
        Type::Int => {
            let len = rdr.read_u32()? as i32;
            let sign = if len < 0 { Sign::Minus } else { Sign::Plus };
            let bytes = rdr.read_slice(len.unsigned_abs())?;
            let int = BigInt::from_bytes_le(sign, bytes);
            bag.make_int(int)
        }
        Type::Float => {
            let value = f64::from_bits(rdr.read_u64()?);
            bag.make_float(value)
        }
        Type::Complex => {
            let re = f64::from_bits(rdr.read_u64()?);
            let im = f64::from_bits(rdr.read_u64()?);
            let value = Complex64 { re, im };
            bag.make_complex(value)
        }
        Type::Ascii | Type::Unicode => {
            let len = rdr.read_u32()?;
            let value = rdr.read_str(len)?;
            bag.make_str(value)
        }
        Type::Tuple => {
            let len = rdr.read_u32()?;
            let it = (0..len).map(|_| deserialize_value(rdr, bag));
            itertools::process_results(it, |it| bag.make_tuple(it))?
        }
        Type::List => {
            let len = rdr.read_u32()?;
            let it = (0..len).map(|_| deserialize_value(rdr, bag));
            itertools::process_results(it, |it| bag.make_list(it))??
        }
        Type::Set => {
            let len = rdr.read_u32()?;
            let it = (0..len).map(|_| deserialize_value(rdr, bag));
            itertools::process_results(it, |it| bag.make_set(it))??
        }
        Type::FrozenSet => {
            let len = rdr.read_u32()?;
            let it = (0..len).map(|_| deserialize_value(rdr, bag));
            itertools::process_results(it, |it| bag.make_frozenset(it))??
        }
        Type::Dict => {
            let len = rdr.read_u32()?;
            let it = (0..len).map(|_| {
                let k = deserialize_value(rdr, bag)?;
                let v = deserialize_value(rdr, bag)?;
                Ok::<_, MarshalError>((k, v))
            });
            itertools::process_results(it, |it| bag.make_dict(it))??
        }
        Type::Bytes => {
            // Following CPython, after marshaling, byte arrays are converted into bytes.
            let len = rdr.read_u32()?;
            let value = rdr.read_slice(len)?;
            bag.make_bytes(value)
        }
        Type::Code => bag.make_code(deserialize_code(rdr, bag.constant_bag())?),
    };
    Ok(value)
}

pub trait Dumpable: Sized {
    type Error;
    type Constant: Constant;
    fn with_dump<R>(&self, f: impl FnOnce(DumpableValue<'_, Self>) -> R) -> Result<R, Self::Error>;
}

pub enum DumpableValue<'a, D: Dumpable> {
    Integer(&'a BigInt),
    Float(f64),
    Complex(Complex64),
    Boolean(bool),
    Str(&'a str),
    Bytes(&'a [u8]),
    Code(&'a CodeObject<D::Constant>),
    Tuple(&'a [D]),
    None,
    Ellipsis,
    StopIter,
    List(&'a [D]),
    Set(&'a [D]),
    Frozenset(&'a [D]),
    Dict(&'a [(D, D)]),
}

impl<'a, C: Constant> From<BorrowedConstant<'a, C>> for DumpableValue<'a, C> {
    fn from(c: BorrowedConstant<'a, C>) -> Self {
        match c {
            BorrowedConstant::Integer { value } => Self::Integer(value),
            BorrowedConstant::Float { value } => Self::Float(value),
            BorrowedConstant::Complex { value } => Self::Complex(value),
            BorrowedConstant::Boolean { value } => Self::Boolean(value),
            BorrowedConstant::Str { value } => Self::Str(value),
            BorrowedConstant::Bytes { value } => Self::Bytes(value),
            BorrowedConstant::Code { code } => Self::Code(code),
            BorrowedConstant::Tuple { elements } => Self::Tuple(elements),
            BorrowedConstant::None => Self::None,
            BorrowedConstant::Ellipsis => Self::Ellipsis,
        }
    }
}

impl<C: Constant> Dumpable for C {
    type Error = Infallible;
    type Constant = Self;
    #[inline(always)]
    fn with_dump<R>(&self, f: impl FnOnce(DumpableValue<'_, Self>) -> R) -> Result<R, Self::Error> {
        Ok(f(self.borrow_constant().into()))
    }
}

pub trait Write {
    fn write_slice(&mut self, slice: &[u8]);
    fn write_u8(&mut self, v: u8) {
        self.write_slice(&v.to_le_bytes())
    }
    fn write_u16(&mut self, v: u16) {
        self.write_slice(&v.to_le_bytes())
    }
    fn write_u32(&mut self, v: u32) {
        self.write_slice(&v.to_le_bytes())
    }
    fn write_u64(&mut self, v: u64) {
        self.write_slice(&v.to_le_bytes())
    }
}

impl Write for Vec<u8> {
    fn write_slice(&mut self, slice: &[u8]) {
        self.extend_from_slice(slice)
    }
}

pub(crate) fn write_len<W: Write>(buf: &mut W, len: usize) {
    let Ok(len) = len.try_into() else { panic!("too long to serialize") };
    buf.write_u32(len);
}

pub fn serialize_value<W: Write, D: Dumpable>(
    buf: &mut W,
    constant: DumpableValue<'_, D>,
) -> Result<(), D::Error> {
    match constant {
        DumpableValue::Integer(int) => {
            buf.write_u8(Type::Int as u8);
            let (sign, bytes) = int.to_bytes_le();
            let len: i32 = bytes.len().try_into().expect("too long to serialize");
            let len = if sign == Sign::Minus { -len } else { len };
            buf.write_u32(len as u32);
            buf.write_slice(&bytes);
        }
        DumpableValue::Float(f) => {
            buf.write_u8(Type::Float as u8);
            buf.write_u64(f.to_bits());
        }
        DumpableValue::Complex(c) => {
            buf.write_u8(Type::Complex as u8);
            buf.write_u64(c.re.to_bits());
            buf.write_u64(c.im.to_bits());
        }
        DumpableValue::Boolean(b) => {
            buf.write_u8(if b { Type::True } else { Type::False } as u8);
        }
        DumpableValue::Str(s) => {
            buf.write_u8(Type::Unicode as u8);
            write_len(buf, s.len());
            buf.write_slice(s.as_bytes());
        }
        DumpableValue::Bytes(b) => {
            buf.write_u8(Type::Bytes as u8);
            write_len(buf, b.len());
            buf.write_slice(b);
        }
        DumpableValue::Code(c) => {
            buf.write_u8(Type::Code as u8);
            serialize_code(buf, c);
        }
        DumpableValue::Tuple(tup) => {
            buf.write_u8(Type::Tuple as u8);
            write_len(buf, tup.len());
            for val in tup {
                val.with_dump(|val| serialize_value(buf, val))??
            }
        }
        DumpableValue::None => {
            buf.write_u8(Type::None as u8);
        }
        DumpableValue::Ellipsis => {
            buf.write_u8(Type::Ellipsis as u8);
        }
        DumpableValue::StopIter => {
            buf.write_u8(Type::StopIter as u8);
        }
        DumpableValue::List(l) => {
            buf.write_u8(Type::List as u8);
            write_len(buf, l.len());
            for val in l {
                val.with_dump(|val| serialize_value(buf, val))??
            }
        }
        DumpableValue::Set(set) => {
            buf.write_u8(Type::Set as u8);
            write_len(buf, set.len());
            for val in set {
                val.with_dump(|val| serialize_value(buf, val))??
            }
        }
        DumpableValue::Frozenset(set) => {
            buf.write_u8(Type::FrozenSet as u8);
            write_len(buf, set.len());
            for val in set {
                val.with_dump(|val| serialize_value(buf, val))??
            }
        }
        DumpableValue::Dict(d) => {
            buf.write_u8(Type::Dict as u8);
            write_len(buf, d.len());
            for (k, v) in d {
                k.with_dump(|val| serialize_value(buf, val))??;
                v.with_dump(|val| serialize_value(buf, val))??;
            }
        }
    }
    Ok(())
}

pub fn serialize_code<W: Write, C: Constant>(buf: &mut W, code: &CodeObject<C>) {
    write_len(buf, code.instructions.len());
    // SAFETY: it's ok to transmute CodeUnit to [u8; 2]
    let (_, instructions_bytes, _) = unsafe { code.instructions.align_to() };
    buf.write_slice(instructions_bytes);

    write_len(buf, code.locations.len());
    for loc in &*code.locations {
        buf.write_u32(loc.row);
        buf.write_u32(loc.column);
    }

    buf.write_u16(code.flags.bits());

    buf.write_u32(code.posonlyarg_count);
    buf.write_u32(code.arg_count);
    buf.write_u32(code.kwonlyarg_count);

    write_len(buf, code.source_path.as_ref().len());
    buf.write_slice(code.source_path.as_ref().as_bytes());

    buf.write_u32(code.first_line_number);
    buf.write_u32(code.max_stackdepth);

    write_len(buf, code.obj_name.as_ref().len());
    buf.write_slice(code.obj_name.as_ref().as_bytes());

    let cell2arg = code.cell2arg.as_deref().unwrap_or(&[]);
    write_len(buf, cell2arg.len());
    for &i in cell2arg {
        buf.write_u32(i as u32)
    }

    write_len(buf, code.constants.len());
    for constant in &*code.constants {
        serialize_value(buf, constant.borrow_constant().into()).unwrap_or_else(|x| match x {})
    }

    let mut write_names = |names: &[C::Name]| {
        write_len(buf, names.len());
        for name in names {
            write_len(buf, name.as_ref().len());
            buf.write_slice(name.as_ref().as_bytes());
        }
    };

    write_names(&code.names);
    write_names(&code.varnames);
    write_names(&code.cellvars);
    write_names(&code.freevars);
}
