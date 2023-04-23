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
            Self {
                fully_visited,
                cycles: None,
            }
        } else {
            Self {
                fully_visited,
                cycles: Some(cycles),
            }
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
                    // when the length is 1 and the only item is the import we're looking at
                    // then we're importing self, could we report this so we don't have to
                    // do this again for import-self W0406?
                    if stack[idx..].len() == 1 && stack[idx] == import.module {
                        continue;
                    }
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
    let Some(package) = package else {
        return None;
    };
    let Some(module_name) = to_module_path(package, path) else {
        return None;
    };
    let module_name = module_name.join(".");
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
                        (&imports.module_to_imports[module_name][0]).into(),
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
                                .find(|m| m.module == the_rest[0])
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
    fn cyclic_import_unrelated_module_not_traversed() {
        let mut map = FxHashMap::default();
        let location1 = Location::new(1, 1);
        let location2 = Location::new(2, 2);

        let a = ModuleImport::new("a".to_string(), location1, location1);
        let b = ModuleImport::new("b".to_string(), location2, location2);
        map.insert(a.module.clone(), vec![]);
        map.insert(b.module, vec![a.clone()]);
        let imports = ImportMap::new(map);
        let cyclic_checker = CyclicImportChecker {
            imports: &imports.module_to_imports,
        };

        let VisitedAndCycles {
            fully_visited: visited,
            cycles,
        } = cyclic_checker.has_cycles(&a.module);
        let mut check_visited = FxHashSet::default();
        check_visited.insert(a.module);
        assert_eq!(visited, check_visited);
        assert!(cycles.is_none());
    }

    #[test]
    fn cyclic_import_multiple_cycles() {
        let mut map = FxHashMap::default();
        let location1 = Location::new(1, 1);
        let location2 = Location::new(2, 2);

        let a = ModuleImport::new("a".to_string(), location1, location1);
        let b = ModuleImport::new("b".to_string(), location2, location2);
        let c = ModuleImport::new("c".to_string(), location1, location2);
        let d = ModuleImport::new("d".to_string(), location2, location2);

        map.insert(a.module.clone(), vec![b.clone(), c.clone()]);
        map.insert(b.module.clone(), vec![c.clone(), d.clone()]);
        map.insert(c.module.clone(), vec![b.clone(), d.clone()]);
        map.insert(d.module.clone(), vec![a.clone()]);
        let imports = ImportMap::new(map);
        let cyclic_checker = CyclicImportChecker {
            imports: &imports.module_to_imports,
        };
        let VisitedAndCycles {
            fully_visited: visited,
            cycles,
        } = cyclic_checker.has_cycles(&a.module);

        let mut check_visited = FxHashSet::default();
        check_visited.insert(a.module.clone());
        check_visited.insert(b.module.clone());
        check_visited.insert(c.module.clone());
        check_visited.insert(d.module.clone());
        assert_eq!(visited, check_visited);

        let mut check_cycles = FxHashSet::default();
        check_cycles.insert(vec![
            a.module.clone(),
            b.module.clone(),
            c.module.clone(),
            d.module.clone(),
        ]);
        check_cycles.insert(vec![
            a.module.clone(),
            c.module.clone(),
            b.module.clone(),
            d.module.clone(),
        ]);
        check_cycles.insert(vec![a.module.clone(), c.module.clone(), d.module.clone()]);
        check_cycles.insert(vec![a.module, b.module.clone(), d.module]);
        check_cycles.insert(vec![c.module.clone(), b.module.clone()]);
        check_cycles.insert(vec![b.module, c.module]);
        assert_eq!(cycles, Some(check_cycles));
    }

    #[test]
    fn cyclic_import_check_diagnostics() {
        let location1 = Location::new(1, 1);
        let location2 = Location::new(2, 2);
        let location3 = Location::new(3, 3);
        let location4 = Location::new(4, 4);

        let a_a = ModuleImport::new("a.a".to_string(), location1, location1);
        let a_b = ModuleImport::new("a.b".to_string(), location2, location2);
        let a_c = ModuleImport::new("a.c".to_string(), location1, location2);
        let b_in_a = ModuleImport::new("a.b".to_string(), location3, location3);
        let a_in_b = ModuleImport::new("a.a".to_string(), location4, location4);
        let mut map = FxHashMap::default();
        map.insert(a_a.module.clone(), vec![b_in_a.clone()]);
        map.insert(a_b.module.clone(), vec![a_in_b.clone()]);
        map.insert(a_c.module, vec![]);
        let imports = ImportMap::new(map);

        let path_a = Path::new("a/a");
        let path_b = Path::new("a/b");
        let path_c = Path::new("a/c");
        let package = Some(Path::new("a"));

        let mut cycles: FxHashMap<Arc<str>, FxHashSet<Vec<Arc<str>>>> = FxHashMap::default();
        let diagnostic = cyclic_import(path_a, package, &imports, &mut cycles);

        let mut set_a: FxHashSet<Vec<Arc<str>>> = FxHashSet::default();
        set_a.insert(vec![a_b.module.clone(), a_a.module.clone()]);
        let mut set_b: FxHashSet<Vec<Arc<str>>> = FxHashSet::default();
        set_b.insert(vec![a_a.module.clone(), a_b.module.clone()]);

        assert_eq!(
            diagnostic,
            Some(vec![Diagnostic::new(
                CyclicImport {
                    cycle: "a.a -> a.b".to_string(),
                },
                (&b_in_a).into(),
            )])
        );
        let mut check_map: FxHashMap<Arc<str>, FxHashSet<Vec<Arc<str>>>> = FxHashMap::default();
        check_map.insert(a_b.module, set_a);
        check_map.insert(a_a.module, set_b);

        let diagnostic = cyclic_import(path_b, package, &imports, &mut cycles);
        assert_eq!(
            diagnostic,
            Some(vec![Diagnostic::new(
                CyclicImport {
                    cycle: "a.b -> a.a".to_string(),
                },
                (&a_in_b).into(),
            )])
        );

        assert!(cyclic_import(path_c, package, &imports, &mut cycles).is_none());
    }

    #[test]
    fn cyclic_import_test_no_cycles_on_import_self() {
        let location = Location::new(1, 1);
        let a = ModuleImport::new("a".to_string(), location, location);
        let mut map = FxHashMap::default();
        map.insert(a.module.clone(), vec![a.clone()]);

        let imports = ImportMap::new(map);
        let cyclic_checker = CyclicImportChecker {
            imports: &imports.module_to_imports,
        };
        let VisitedAndCycles {
            fully_visited: visited,
            cycles,
        } = cyclic_checker.has_cycles(&a.module);

        let mut check_visited = FxHashSet::default();
        check_visited.insert(a.module);
        assert_eq!(visited, check_visited);

        assert!(cycles.is_none());
    }
}
