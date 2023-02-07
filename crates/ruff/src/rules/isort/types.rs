use std::borrow::Cow;

use rustc_hash::FxHashMap;

use crate::ast;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum TrailingComma {
    Present,
    #[default]
    Absent,
}

#[derive(Debug, Hash, Ord, PartialOrd, Eq, PartialEq, Clone)]
pub struct ImportFromData<'a> {
    pub module: Option<&'a str>,
    pub level: Option<&'a usize>,
}

#[derive(Debug, Hash, Ord, PartialOrd, Eq, PartialEq)]
pub struct AliasData<'a> {
    pub name: &'a str,
    pub asname: Option<&'a str>,
}

#[derive(Debug, Default, Clone)]
pub struct CommentSet<'a> {
    pub atop: Vec<Cow<'a, str>>,
    pub inline: Vec<Cow<'a, str>>,
}

pub trait Importable {
    fn module_name(&self) -> String;
    fn module_base(&self) -> String;
}

impl Importable for AliasData<'_> {
    fn module_name(&self) -> String {
        self.name.to_string()
    }

    fn module_base(&self) -> String {
        self.module_name().split('.').next().unwrap().to_string()
    }
}

impl Importable for ImportFromData<'_> {
    fn module_name(&self) -> String {
        ast::helpers::format_import_from(self.level, self.module)
    }

    fn module_base(&self) -> String {
        self.module_name().split('.').next().unwrap().to_string()
    }
}

#[derive(Debug, Default)]
pub struct ImportBlock<'a> {
    // Set of (name, asname), used to track regular imports.
    // Ex) `import module`
    pub import: FxHashMap<AliasData<'a>, CommentSet<'a>>,
    // Map from (module, level) to `AliasData`, used to track 'from' imports.
    // Ex) `from module import member`
    pub import_from: FxHashMap<
        ImportFromData<'a>,
        (
            CommentSet<'a>,
            FxHashMap<AliasData<'a>, CommentSet<'a>>,
            TrailingComma,
        ),
    >,
    // Set of (module, level, name, asname), used to track re-exported 'from' imports.
    // Ex) `from module import member as member`
    pub import_from_as: FxHashMap<(ImportFromData<'a>, AliasData<'a>), CommentSet<'a>>,
    // Map from (module, level) to `AliasData`, used to track star imports.
    // Ex) `from module import *`
    pub import_from_star: FxHashMap<ImportFromData<'a>, CommentSet<'a>>,
}

type AliasDataWithComments<'a> = (AliasData<'a>, CommentSet<'a>);

type Import<'a> = AliasDataWithComments<'a>;

type ImportFrom<'a> = (
    ImportFromData<'a>,
    CommentSet<'a>,
    TrailingComma,
    Vec<AliasDataWithComments<'a>>,
);

pub enum EitherImport<'a> {
    Import(Import<'a>),
    ImportFrom(ImportFrom<'a>),
}

#[derive(Debug, Default)]
pub struct OrderedImportBlock<'a> {
    pub import: Vec<Import<'a>>,
    pub import_from: Vec<ImportFrom<'a>>,
}
