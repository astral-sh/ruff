use std::borrow::Cow;

use rustc_hash::FxHashMap;

use ruff_python_ast::helpers::format_import_from;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TrailingComma {
    Present,
    #[default]
    Absent,
}

#[derive(Debug, Hash, Ord, PartialOrd, Eq, PartialEq, Clone)]
pub(crate) struct ImportFromData<'a> {
    pub(crate) module: Option<&'a str>,
    pub(crate) level: Option<u32>,
}

#[derive(Debug, Hash, Ord, PartialOrd, Eq, PartialEq)]
pub(crate) struct AliasData<'a> {
    pub(crate) name: &'a str,
    pub(crate) asname: Option<&'a str>,
}

#[derive(Debug, Default, Clone)]
pub(crate) struct CommentSet<'a> {
    pub(crate) atop: Vec<Cow<'a, str>>,
    pub(crate) inline: Vec<Cow<'a, str>>,
}

pub(crate) trait Importable {
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
        format_import_from(self.level, self.module)
    }

    fn module_base(&self) -> String {
        self.module_name().split('.').next().unwrap().to_string()
    }
}

#[derive(Debug, Default)]
pub(crate) struct ImportFromStatement<'a> {
    pub(crate) comments: CommentSet<'a>,
    pub(crate) aliases: FxHashMap<AliasData<'a>, CommentSet<'a>>,
    pub(crate) trailing_comma: TrailingComma,
}

#[derive(Debug, Default)]
pub(crate) struct ImportBlock<'a> {
    // Set of (name, asname), used to track regular imports.
    // Ex) `import module`
    pub(crate) import: FxHashMap<AliasData<'a>, CommentSet<'a>>,
    // Map from (module, level) to `AliasData`, used to track 'from' imports.
    // Ex) `from module import member`
    pub(crate) import_from: FxHashMap<ImportFromData<'a>, ImportFromStatement<'a>>,
    // Set of (module, level, name, asname), used to track re-exported 'from' imports.
    // Ex) `from module import member as member`
    pub(crate) import_from_as:
        FxHashMap<(ImportFromData<'a>, AliasData<'a>), ImportFromStatement<'a>>,
    // Map from (module, level) to `AliasData`, used to track star imports.
    // Ex) `from module import *`
    pub(crate) import_from_star: FxHashMap<ImportFromData<'a>, ImportFromStatement<'a>>,
}

type AliasDataWithComments<'a> = (AliasData<'a>, CommentSet<'a>);

type Import<'a> = AliasDataWithComments<'a>;

type ImportFrom<'a> = (
    ImportFromData<'a>,
    CommentSet<'a>,
    TrailingComma,
    Vec<AliasDataWithComments<'a>>,
);

pub(crate) enum EitherImport<'a> {
    Import(Import<'a>),
    ImportFrom(ImportFrom<'a>),
}

#[derive(Debug, Default)]
pub(crate) struct OrderedImportBlock<'a> {
    pub(crate) import: Vec<Import<'a>>,
    pub(crate) import_from: Vec<ImportFrom<'a>>,
}
