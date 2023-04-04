use std::path::Path;

use log::debug;

use rustc_hash::{FxHashMap, FxHashSet};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::to_module_path;
use ruff_python_ast::imports::{ImportMap, ModuleImport};

#[violation]
pub struct CyclicImport {
    pub cycle: String,
}

impl Violation for CyclicImport {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Cyclic import ({}) (cyclic-import)", self.cycle)
    }
}

struct CyclicImportChecker<'a> {
    imports: &'a FxHashMap<String, Vec<ModuleImport>>,
}

impl CyclicImportChecker<'_> {
    fn has_cycles<'a>(
        &'a self,
        name: &'a str,
    ) -> (FxHashSet<&str>, Option<FxHashSet<Vec<String>>>) {
        let mut fully_visited: FxHashSet<&str> = FxHashSet::default();
        let mut cycles: FxHashSet<Vec<String>> = FxHashSet::default();
        let mut stack: Vec<&str> = vec![name];
        self.has_cycles_helper(name, &mut stack, &mut cycles, &mut fully_visited, 0);
        if cycles.is_empty() {
            (fully_visited, None)
        } else {
            (fully_visited, Some(cycles))
        }
    }

    fn has_cycles_helper<'a>(
        &'a self,
        name: &'a str,
        stack: &mut Vec<&'a str>,
        cycles: &mut FxHashSet<Vec<String>>,
        fully_visited: &mut FxHashSet<&'a str>,
        level: usize,
    ) {
        if let Some(imports) = self.imports.get(name) {
            let tabs = "\t".repeat(level);
            debug!("{tabs}{name}");
            for import in imports.iter() {
                debug!("{tabs}\timport: {}", import.module);
                if let Some(idx) = stack.iter().position(|&s| s == import.module) {
                    debug!("{tabs}\t\t cycles: {:?}", stack[idx..].to_vec());
                    cycles.insert(
                        stack[idx..]
                            .iter()
                            .map(|&s| s.into())
                            .collect::<Vec<String>>(),
                    );
                } else {
                    stack.push(&import.module);
                    self.has_cycles_helper(&import.module, stack, cycles, fully_visited, level + 1);
                    stack.pop();
                }
            }
        }
        fully_visited.insert(name);
    }
}

/// PLR0914
pub fn cyclic_import(
    path: &Path,
    package: Option<&Path>,
    imports: &ImportMap,
    cycles: &mut FxHashMap<String, FxHashSet<Vec<String>>>,
) -> Option<Vec<Diagnostic>> {
    let module_name = to_module_path(package.unwrap(), path).unwrap().join(".");
    debug!("Checking module {module_name}");
    if let Some(existing_cycles) = cycles.get(&module_name as &str) {
        if existing_cycles.is_empty() {
            return None;
        }
        debug!("Existing cycles: {existing_cycles:#?}");
        Some(
            existing_cycles
                .iter()
                .map(|cycle| {
                    Diagnostic::new(
                        CyclicImport {
                            // need to reorder the detected cycle
                            cycle: cycle.join(" -> "),
                        },
                        imports.module_to_imports[&module_name][1].as_ref().into(),
                    )
                })
                .collect::<Vec<Diagnostic>>(),
        )
    } else {
        let cyclic_import_checker = CyclicImportChecker {
            imports: &imports.module_to_imports,
        };
        let (mut visited, new_cycles) = cyclic_import_checker.has_cycles(&module_name);
        // we'll always have new visited stuff if we have
        let mut out_vec: Vec<Diagnostic> = Vec::new();
        if let Some(new_cycles) = new_cycles {
            debug!("New cycles {new_cycles:#?}");
            for new_cycle in &new_cycles {
                if let [first, the_rest @ ..] = &new_cycle[..] {
                    if first == &module_name {
                        out_vec.push(Diagnostic::new(
                            CyclicImport {
                                cycle: new_cycle
                                    .iter()
                                    .map(std::clone::Clone::clone)
                                    .collect::<Vec<_>>()
                                    .join(" -> "),
                            },
                            imports.module_to_imports[&module_name]
                                .iter()
                                .find(|m| &m.module == the_rest.first().unwrap())
                                .unwrap()
                                .into(),
                        ));
                    }
                }
                for involved_module in new_cycle.iter() {
                    let pos = new_cycle.iter().position(|s| s == involved_module).unwrap();
                    let cycle_to_insert = new_cycle[pos..]
                        .iter()
                        .chain(new_cycle[..pos].iter())
                        .map(std::clone::Clone::clone)
                        .collect::<Vec<_>>();
                    if let Some(existing) = cycles.get_mut(involved_module as &str) {
                        existing.insert(cycle_to_insert);
                    } else {
                        let mut new_set = FxHashSet::default();
                        new_set.insert(cycle_to_insert);
                        cycles.insert(involved_module.to_string(), new_set);
                    }
                    visited.remove(involved_module as &str);
                }
            }
        }
        // process the visited nodes which don't have cycles
        for visited_module in &visited {
            cycles.insert((*visited_module).to_string(), FxHashSet::default());
        }
        if out_vec.is_empty() {
            None
        } else {
            Some(out_vec)
        }
    }
}

#[cfg(test)]
mod tests {
    use rustpython_parser::ast::Location;

    use super::*;

    fn test_simple_cycle_helper() -> ImportMap {
        let mut map = FxHashMap::default();
        let location = Location::new(1, 1);
        map.insert(
            "grand.a".to_string(),
            vec![
                ModuleImport::new("grand.b".to_string(), location, location),
                ModuleImport::new("grand.parent.a".to_string(), location, location),
            ],
        );
        map.insert(
            "grand.b".to_string(),
            vec![ModuleImport::new("grand.a".to_string(), location, location)],
        );
        ImportMap {
            module_to_imports: map,
        }
    }

    #[test]
    fn cyclic_import_simple_one() {
        let imports = test_simple_cycle_helper();
        let cyclic_checker = CyclicImportChecker {
            imports: &imports.module_to_imports,
        };
        let (visited, cycles) = cyclic_checker.has_cycles("grand.a");

        let mut check_visited = FxHashSet::default();
        check_visited.insert("grand.a");
        check_visited.insert("grand.b");
        check_visited.insert("grand.parent.a");
        assert_eq!(visited, check_visited);
        let mut check_cycles = FxHashSet::default();
        check_cycles.insert(vec!["grand.a".to_string(), "grand.b".to_string()]);
        assert_eq!(cycles, Some(check_cycles));

        let (visited, cycles) = cyclic_checker.has_cycles("grand.b");
        assert_eq!(visited, check_visited);
        let mut check_cycles = FxHashSet::default();
        check_cycles.insert(vec!["grand.b".to_string(), "grand.a".to_string()]);
        assert_eq!(cycles, Some(check_cycles));
    }
}
