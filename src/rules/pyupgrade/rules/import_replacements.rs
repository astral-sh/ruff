use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::cst::matchers::{match_import, match_import_from, match_module};
use crate::fix::Fix;
use crate::registry::Diagnostic;
use crate::violations;
use libcst_native::{Codegen, CodegenState, ImportAlias, ImportNames, NameOrAttribute};
use rustpython_ast::{AliasData, Located, Stmt, StmtKind};

type Aliases<'a> = Vec<ImportAlias<'a>>;

const BAD_MODULES: &[&str] = &[
    "collections",
    "pipes",
    "six",
    "six.moves",
    "six.moves.urllib",
    "mypy_extensions",
    "typing_extensions",
    "typing",
    "typing.re",
];

const COLLECTIONS_TO_ABC: &[&str] = &[
    "AsyncGenerator",
    "AsyncIterable",
    "AsyncIterator",
    "Awaitable",
    "ByteString",
    "Callable",
    "Collection",
    "Container",
    "Coroutine",
    "Generator",
    "Hashable",
    "ItemsView",
    "Iterable",
    "Iterator",
    "KeysView",
    "Mapping",
    "MappingView",
    "MutableMapping",
    "MutableSequence",
    "MutableSet",
    "Reversible",
    "Sequence",
    "Set",
    "Sized",
    "ValuesView",
];

/// Fixes the new list of imports to have correct formatting
fn correct_formatting<'a>(original: &Aliases<'a>, new: &mut Aliases<'a>) {
    // If the new list is empty, there is nothing to format
    if new.is_empty() {
        return;
    }
    // If there were less than two items in the original, there is nothing to format
    if original.len() < 2 {
        return;
    }
    let first = original.first().unwrap();
    let middle = original.get(1).unwrap();
    let last = original.last().unwrap();

    let mut i = 0;
    for mut name in new {
        if i == 0 {
            name.comma = first.comma.clone();
        }
        i += 1;
    }
}

/// Returns a list of imports that does and does not have a match in the given list of matches
fn get_import_lists<'a>(names: &ImportNames<'a>, matches: &[&str]) -> (Aliases<'a>, Aliases<'a>) {
    let mut unmatching_names: Aliases<'a> = vec![];
    let mut matching_names: Aliases<'a> = vec![];

    if let ImportNames::Aliases(names) = names {
        for name in names {
            if let NameOrAttribute::N(sub_item) = &name.name {
                if matches.contains(&sub_item.value) {
                    matching_names.push(name.clone());
                } else {
                    unmatching_names.push(name.clone());
                }
            }
        }
        correct_formatting(names, &mut matching_names);
        correct_formatting(names, &mut unmatching_names);
    }
    (matching_names, unmatching_names)
}

/// Converts the string of imports into new one
fn create_new_str(original: &str, matches: &[&str], replace: &str) -> Option<String> {
    let mut tree = match_module(original).unwrap();
    let old_import = match_import_from(&mut tree).unwrap();
    println!("{:?}", old_import);
    let (matching_names, unmatching_names) = get_import_lists(&old_import.names, matches);
    let unmatching = if unmatching_names.is_empty() {
        "".to_string()
    } else {
        let mut unmatching_imports = old_import.clone();
        unmatching_imports.names = ImportNames::Aliases(unmatching_names);
        let mut state = CodegenState::default();
        unmatching_imports.codegen(&mut state);
        state.to_string()
    };
    let matching = if matching_names.is_empty() {
        "".to_string()
    } else {
        let mut matching_imports = old_import.clone();
        if let Some(NameOrAttribute::N(name)) = &matching_imports.module {
            println!("{:?}", name);
            let mut new_name = name.clone();
            new_name.value = replace;
            matching_imports.module = Some(NameOrAttribute::N(new_name));
        } else {
            return None
        }
        matching_imports.names = ImportNames::Aliases(matching_names);
        let mut state = CodegenState::default();
        matching_imports.codegen(&mut state);
        state.to_string()
    };
    if !unmatching.is_empty() && !matching.is_empty() {
        Some(format!("{unmatching}\n{matching}"))
    } else if !unmatching.is_empty() {
        Some(unmatching)
    } else if !matching.is_empty() {
        Some(matching)
    } else {
        None
    }
}

fn check_repalcement(module: &str, original: &str) -> Option<String> {
    match module {
        "collections" => create_new_str(original, COLLECTIONS_TO_ABC, "collections.abc"),
        _ => return None,
    }
}

/// UP035
pub fn import_replacements(
    checker: &mut Checker,
    stmt: &Stmt,
    names: &Vec<Located<AliasData>>,
    module: &Option<String>,
) {
    // Pyupgrade only works with import_from statements, so this linter does that as well
    let clean_mod = match module {
        None => return,
        Some(item) => item,
    };
    if !BAD_MODULES.contains(&clean_mod.as_str()) {
        return;
    }
    let module_text = checker
        .locator
        .slice_source_code_range(&Range::from_located(stmt));
    let clean_result = check_repalcement(clean_mod, &module_text);
    println!("{:?}", clean_result);
}
