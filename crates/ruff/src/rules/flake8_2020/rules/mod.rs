pub(crate) use compare::{
    compare, SysVersionCmpStr10, SysVersionCmpStr3, SysVersionInfo0Eq3, SysVersionInfo1CmpInt,
    SysVersionInfoMinorCmpInt,
};
pub(crate) use name_or_attribute::{name_or_attribute, SixPY3};
pub(crate) use subscript::{
    subscript, SysVersion0, SysVersion2, SysVersionSlice1, SysVersionSlice3,
};

mod compare;
mod name_or_attribute;
mod subscript;
