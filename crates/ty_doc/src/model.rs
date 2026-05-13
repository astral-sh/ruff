use std::collections::{BTreeMap, BTreeSet};

use ruff_db::diagnostic::Diagnostic;
use ruff_python_ast::token::Tokens;
use serde::Serialize;

#[derive(Debug)]
pub(crate) struct Documentation {
    pub(crate) project_name: String,
    pub(crate) project_slug: String,
    pub(crate) generator_version: String,
    pub(crate) modules: BTreeMap<String, ModuleDoc>,
    pub(crate) type_index: BTreeMap<String, TypeIndexEntry>,
    pub(crate) warnings: Vec<Diagnostic>,
    pub(crate) documented_files: usize,
}

impl Documentation {
    pub(crate) fn top_level_modules(&self) -> impl Iterator<Item = &ModuleDoc> {
        self.modules
            .values()
            .filter(|module| parent_module(&module.name).is_none())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum TypeIndexEntry {
    Unique(TypeLinkTarget),
    Ambiguous,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TypeLinkTarget {
    pub(crate) module: String,
    pub(crate) kind: TypeLinkKind,
    pub(crate) name: String,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(crate) enum TypeLinkKind {
    Class,
    TypeAlias,
    Variable,
}

pub(crate) fn build_type_index(
    modules: &BTreeMap<String, ModuleDoc>,
) -> BTreeMap<String, TypeIndexEntry> {
    let mut index = BTreeMap::new();

    for module in modules.values() {
        for class in &module.classes {
            insert_type_target(
                &mut index,
                TypeLinkTarget {
                    module: module.name.clone(),
                    kind: TypeLinkKind::Class,
                    name: class.name.clone(),
                },
            );
        }

        for variable in &module.variables {
            if variable.kind == VariableKind::TypeAlias
                || is_signature_type_identifier(&variable.name)
            {
                insert_type_target(
                    &mut index,
                    TypeLinkTarget {
                        module: module.name.clone(),
                        kind: match variable.kind {
                            VariableKind::Variable => TypeLinkKind::Variable,
                            VariableKind::TypeAlias => TypeLinkKind::TypeAlias,
                        },
                        name: variable.name.clone(),
                    },
                );
            }
        }
    }

    index
}

fn insert_type_target(index: &mut BTreeMap<String, TypeIndexEntry>, target: TypeLinkTarget) {
    match index.get_mut(&target.name) {
        Some(TypeIndexEntry::Unique(existing)) if existing == &target => {}
        Some(entry) => *entry = TypeIndexEntry::Ambiguous,
        None => {
            index.insert(target.name.clone(), TypeIndexEntry::Unique(target));
        }
    }
}

#[derive(Debug)]
pub(crate) struct ExtractedModule {
    pub(crate) module: Option<ModuleDoc>,
    pub(crate) warnings: Vec<Diagnostic>,
}

#[derive(Debug)]
pub(crate) struct ModuleDoc {
    pub(crate) name: String,
    pub(crate) docstring: Option<String>,
    pub(crate) source: Option<SourceDoc>,
    pub(crate) submodules: BTreeSet<String>,
    pub(crate) public_items: BTreeSet<String>,
    pub(crate) classes: Vec<ClassDoc>,
    pub(crate) functions: Vec<FunctionDoc>,
    pub(crate) variables: Vec<VariableDoc>,
}

impl ModuleDoc {
    pub(crate) fn synthetic(name: String) -> Self {
        Self {
            name,
            docstring: None,
            source: None,
            submodules: BTreeSet::new(),
            public_items: BTreeSet::new(),
            classes: Vec::new(),
            functions: Vec::new(),
            variables: Vec::new(),
        }
    }

    pub(crate) fn summary(&self) -> &str {
        doc_summary(self.docstring.as_deref())
    }
}

#[derive(Debug)]
pub(crate) struct SourceDoc {
    pub(crate) path: String,
    pub(crate) text: String,
    pub(crate) tokens: Tokens,
}

#[derive(Debug)]
pub(crate) struct ClassDoc {
    pub(crate) name: String,
    pub(crate) signature: String,
    pub(crate) signature_links: BTreeMap<String, String>,
    pub(crate) base_classes: Vec<ClassBaseDoc>,
    pub(crate) enum_member_names: BTreeSet<String>,
    pub(crate) docstring: Option<String>,
    pub(crate) source_line: String,
    pub(crate) methods: Vec<FunctionDoc>,
    pub(crate) attributes: Vec<VariableDoc>,
}

impl ClassDoc {
    pub(crate) fn summary(&self) -> &str {
        doc_summary(self.docstring.as_deref())
    }
}

#[derive(Debug)]
pub(crate) struct ClassBaseDoc {
    pub(crate) module: String,
    pub(crate) name: String,
}

#[derive(Debug)]
pub(crate) struct FunctionDoc {
    pub(crate) name: String,
    pub(crate) signature: String,
    pub(crate) signature_links: BTreeMap<String, String>,
    pub(crate) docstring: Option<String>,
    pub(crate) source_line: String,
    pub(crate) overloads: Vec<FunctionSignatureDoc>,
    pub(crate) overload_only: bool,
}

impl FunctionDoc {
    pub(crate) fn summary(&self) -> &str {
        doc_summary(self.documentation())
    }

    pub(crate) fn documentation(&self) -> Option<&str> {
        self.docstring.as_deref().or_else(|| {
            self.overloads
                .iter()
                .find_map(FunctionSignatureDoc::docstring)
        })
    }

    pub(crate) fn overloads_to_render(&self) -> &[FunctionSignatureDoc] {
        if self.overload_only {
            self.overloads.get(1..).unwrap_or_default()
        } else {
            &self.overloads
        }
    }
}

#[derive(Debug)]
pub(crate) struct FunctionSignatureDoc {
    pub(crate) signature: String,
    pub(crate) signature_links: BTreeMap<String, String>,
    pub(crate) docstring: Option<String>,
    pub(crate) source_line: String,
}

impl FunctionSignatureDoc {
    pub(crate) fn from_function(function: &FunctionDoc) -> Self {
        Self {
            signature: function.signature.clone(),
            signature_links: function.signature_links.clone(),
            docstring: function.docstring.clone(),
            source_line: function.source_line.clone(),
        }
    }

    pub(crate) fn docstring(&self) -> Option<&str> {
        self.docstring.as_deref()
    }
}

#[derive(Debug)]
pub(crate) struct VariableDoc {
    pub(crate) name: String,
    pub(crate) signature: String,
    pub(crate) signature_links: BTreeMap<String, String>,
    pub(crate) docstring: Option<String>,
    pub(crate) source_line: String,
    pub(crate) kind: VariableKind,
}

impl VariableDoc {
    pub(crate) fn summary(&self) -> &str {
        doc_summary(self.docstring.as_deref())
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(crate) enum VariableKind {
    Variable,
    TypeAlias,
}

impl VariableKind {
    pub(crate) const fn anchor_prefix(self) -> &'static str {
        match self {
            VariableKind::Variable => "var",
            VariableKind::TypeAlias => "type",
        }
    }

    pub(crate) const fn search_kind(self) -> &'static str {
        match self {
            VariableKind::Variable => "variable",
            VariableKind::TypeAlias => "type alias",
        }
    }
}

#[derive(Serialize)]
pub(crate) struct SearchItem(
    pub(crate) &'static str,
    pub(crate) String,
    pub(crate) String,
    pub(crate) String,
    pub(crate) String,
);

pub(crate) fn parent_module(module: &str) -> Option<&str> {
    module.rsplit_once('.').map(|(parent, _)| parent)
}

pub(crate) fn module_short_name(module: &str) -> &str {
    module.rsplit('.').next().unwrap_or(module)
}

pub(crate) fn parent_modules(module: &str) -> Vec<String> {
    let mut parents = Vec::new();
    let mut current = module;
    while let Some(parent) = parent_module(current) {
        parents.push(parent.to_string());
        current = parent;
    }
    parents
}

pub(crate) fn sanitize_path_segment(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    for character in value.chars() {
        if character == '_'
            || character == '-'
            || character == '.'
            || character.is_ascii_alphanumeric()
        {
            output.push(character);
        } else {
            output.push('_');
        }
    }
    if output.is_empty() {
        "index".to_string()
    } else {
        output
    }
}

pub(crate) fn is_signature_type_identifier(token: &str) -> bool {
    token
        .chars()
        .next()
        .is_some_and(|character| character.is_ascii_uppercase())
}

fn first_doc_line(docstring: &str) -> Option<&str> {
    docstring
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
}

fn doc_summary(docstring: Option<&str>) -> &str {
    docstring.and_then(first_doc_line).unwrap_or_default()
}
