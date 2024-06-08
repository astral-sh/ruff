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
    pub(crate) level: u32,
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

pub(crate) trait Importable<'a> {
    fn module_name(&self) -> Cow<'a, str>;

    fn module_base(&self) -> Cow<'a, str> {
        match self.module_name() {
            Cow::Borrowed(module_name) => Cow::Borrowed(
                module_name
                    .split('.')
                    .next()
                    .expect("module to include at least one segment"),
            ),
            Cow::Owned(module_name) => Cow::Owned(
                module_name
                    .split('.')
                    .next()
                    .expect("module to include at least one segment")
                    .to_owned(),
            ),
        }
    }
}

impl<'a> Importable<'a> for AliasData<'a> {
    fn module_name(&self) -> Cow<'a, str> {
        Cow::Borrowed(self.name)
    }
}

impl<'a> Importable<'a> for ImportFromData<'a> {
    fn module_name(&self) -> Cow<'a, str> {
        format_import_from(self.level, self.module)
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

#[derive(Debug)]
pub(crate) enum EitherImport<'a> {
    Import(Import<'a>),
    ImportFrom(ImportFrom<'a>),
}
