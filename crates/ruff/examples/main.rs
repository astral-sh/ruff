use std::mem::size_of_val;

use bitflags::bitflags;

bitflags! {
    #[derive(Default, Debug, Copy, Clone, PartialEq, Eq)]
    pub struct AllNamesFlags: u8 {
        const INVALID_FORMAT = 0b0000_0001;
        const INVALID_OBJECT = 0b0000_0010;
    }
}

#[derive(Clone, Debug)]
pub struct Export<'a> {
    /// The names of the bindings exported via `__all__`.
    pub names: Box<[&'a str]>,
    // pub flags: AllNamesFlags,
}

#[derive(Clone, Debug)]
pub struct Importation<'a> {
    pub full_name: &'a str,
}

#[derive(Clone, Debug)]
pub struct FromImportation {
    pub full_name: String,
}

#[derive(Clone, Debug)]
pub struct SubmoduleImportation<'a> {
    pub full_name: &'a str,
}

// If we box, this goes from 48 to 16
// If we use a u32 pointer, this goes to 8
#[derive(Clone, Debug)]
pub enum BindingKind {
    // Annotation,
    // Argument,
    // Assignment,
    // NamedExprAssignment,
    // Binding,
    // LoopVar,
    // Global,
    // Nonlocal,
    // Builtin,
    // ClassDefinition,
    // FunctionDefinition,
    // 32
    Export(Export<'static>),
    // FutureImportation,
    // 40
    // Importation(Importation<'static>),
    // 48
    // FromImportation(FromImportation),
    // 48
    // SubmoduleImportation(SubmoduleImportation<'static>),
}

fn main() {
    let smart = BindingKind::Export(Export {
        names: vec![].into_boxed_slice(),
    });
    dbg!(size_of_val(&smart));
}
