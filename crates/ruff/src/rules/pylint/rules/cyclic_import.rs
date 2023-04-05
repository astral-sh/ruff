use std::path::Path;
use std::sync::Arc;

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

struct VisitedAndCycles {
    fully_visited: FxHashSet<Arc<str>>,
    cycles: Option<FxHashSet<Vec<Arc<str>>>>,
}

impl VisitedAndCycles {
    fn new(fully_visited: FxHashSet<Arc<str>>, cycles: FxHashSet<Vec<Arc<str>>>) -> Self {
        if cycles.is_empty() {
            Self { fully_visited, cycles: None }
        } else {
            Self { fully_visited, cycles: Some(cycles)}
        }
    }
}

struct CyclicImportChecker<'a> {
    imports: &'a FxHashMap<Arc<str>, Vec<ModuleImport>>,
}

impl CyclicImportChecker<'_> {
    fn has_cycles(&self, name: &Arc<str>) -> VisitedAndCycles {
        let mut fully_visited: FxHashSet<Arc<str>> = FxHashSet::default();
        let mut cycles: FxHashSet<Vec<Arc<str>>> = FxHashSet::default();
        let mut stack: Vec<Arc<str>> = vec![name.clone()];
        self.has_cycles_helper(name, &mut stack, &mut cycles, &mut fully_visited, 0);
        VisitedAndCycles::new(fully_visited, cycles)
    }

    fn has_cycles_helper(
        &self,
        name: &Arc<str>,
        stack: &mut Vec<Arc<str>>,
        cycles: &mut FxHashSet<Vec<Arc<str>>>,
        fully_visited: &mut FxHashSet<Arc<str>>,
        level: usize,
    ) {
        if let Some(imports) = self.imports.get(name) {
            let tabs = "\t".repeat(level);
            debug!("{tabs}{name}");
            for import in imports.iter() {
                debug!("{tabs}\timport: {}", import.module);
                if let Some(idx) = stack.iter().position(|s| s == &import.module) {
                    debug!("{tabs}\t\t cycles: {:?}", stack[idx..].to_vec());
                    cycles.insert(stack[idx..].to_vec());
                } else {
                    stack.push(import.module.clone());
                    self.has_cycles_helper(&import.module, stack, cycles, fully_visited, level + 1);
                    stack.pop();
                }
            }
        }
        fully_visited.insert(name.clone());
    }
}

/// PLR0914
pub fn cyclic_import(
    path: &Path,
    package: Option<&Path>,
    imports: &ImportMap,
    cycles: &mut FxHashMap<Arc<str>, FxHashSet<Vec<Arc<str>>>>,
) -> Option<Vec<Diagnostic>> {
    let module_name = to_module_path(package.unwrap(), path).unwrap().join(".");
    // if the module name isn't in the import map, it can't possibly have cycles
    debug!("Checking module {module_name}");
    let Some((module_name, _)) = imports.module_to_imports.get_key_value(&module_name as &str) else {
        return None;
    };
    if let Some(existing_cycles) = cycles.get(module_name) {
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
                            cycle: cycle
                                .iter()
                                .map(std::string::ToString::to_string)
                                .collect::<Vec<_>>()
                                .join(" -> "),
                        },
                        imports.module_to_imports[module_name][1].as_ref().into(),
                    )
                })
                .collect::<Vec<Diagnostic>>(),
        )
    } else {
        let cyclic_import_checker = CyclicImportChecker {
            imports: &imports.module_to_imports,
        };
        let VisitedAndCycles {
            fully_visited: mut visited,
            cycles: new_cycles,
        } = cyclic_import_checker.has_cycles(module_name);
        // we'll always have new visited stuff if we have
        let mut out_vec: Vec<Diagnostic> = Vec::new();
        if let Some(new_cycles) = new_cycles {
            debug!("New cycles {new_cycles:#?}");
            for new_cycle in &new_cycles {
                if let [first, the_rest @ ..] = &new_cycle[..] {
                    if first == module_name {
                        out_vec.push(Diagnostic::new(
                            CyclicImport {
                                cycle: new_cycle
                                    .iter()
                                    .map(std::string::ToString::to_string)
                                    .collect::<Vec<_>>()
                                    .join(" -> "),
                            },
                            imports.module_to_imports[module_name]
                                .iter()
                                .find(|m| &(m.module) == the_rest.first().unwrap())
                                .unwrap()
                                .into(),
                        ));
                    }
                }
                for involved_module in new_cycle.iter() {
                    // we re-order the cycles for the modules involved here
                    let pos = new_cycle.iter().position(|s| s == involved_module).unwrap();
                    let cycle_to_insert = new_cycle[pos..]
                        .iter()
                        .chain(new_cycle[..pos].iter())
                        .map(std::clone::Clone::clone)
                        .collect::<Vec<_>>();
                    if let Some(existing) = cycles.get_mut(involved_module) {
                        existing.insert(cycle_to_insert);
                    } else {
                        let mut new_set = FxHashSet::default();
                        new_set.insert(cycle_to_insert);
                        cycles.insert(involved_module.clone(), new_set);
                    }
                    visited.remove(involved_module);
                }
            }
        }
        // process the visited nodes which don't have cycles
        for visited_module in &visited {
            cycles.insert(visited_module.clone(), FxHashSet::default());
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

    #[test]
    fn cyclic_import_simple() {
        let mut map = FxHashMap::default();
        let location = Location::new(1, 1);
        let grand_a = ModuleImport::new("grand_a".to_string(), location, location);
        let grand_b = ModuleImport::new("grand_b".to_string(), location, location);
        let grand_parent_a = ModuleImport::new("grand.parent.a".to_string(), location, location);
        map.insert(
            grand_a.module.clone(),
            vec![
                grand_b.clone(),
                grand_parent_a.clone(),
            ],
        );
        map.insert(
            grand_b.module.clone(),
            vec![grand_a.clone()],
        );
        let imports = ImportMap::new(map);
        let cyclic_checker = CyclicImportChecker {
            imports: &imports.module_to_imports,
        };
        let VisitedAndCycles {
            fully_visited: visited,
            cycles
        } = cyclic_checker.has_cycles(&grand_a.module);

        let mut check_visited = FxHashSet::default();
        check_visited.insert(grand_a.module.clone());
        check_visited.insert(grand_b.module.clone());
        check_visited.insert(grand_parent_a.module.clone());
        assert_eq!(visited, check_visited);
        let mut check_cycles = FxHashSet::default();
        check_cycles.insert(vec![grand_a.module.clone(), grand_b.module.clone()]);
        assert_eq!(cycles, Some(check_cycles));

        let VisitedAndCycles {
            fully_visited: visited,
            cycles
        } = cyclic_checker.has_cycles(&grand_b.module);        // assert_eq!(visited, check_visited);
        let mut check_cycles = FxHashSet::default();
        check_cycles.insert(vec![grand_b.module.clone(), grand_a.module.clone()]);
        assert_eq!(cycles, Some(check_cycles));
        assert_eq!(visited.contains(&grand_parent_a.module), true);
    }
}
