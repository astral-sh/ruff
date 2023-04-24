use std::path::Path;
use std::sync::Arc;

use log::debug;

use rustc_hash::{FxHashMap, FxHashSet};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::to_module_path;
use ruff_python_ast::imports::{CyclicImportHelper, ModuleImport, ModuleMapping};

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
    fully_visited: FxHashSet<u32>,
    cycles: Option<FxHashSet<Vec<u32>>>,
}

impl VisitedAndCycles {
    fn new(fully_visited: FxHashSet<u32>, cycles: FxHashSet<Vec<u32>>) -> Self {
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
    fn has_cycles(&self, name: &Arc<str>, module_mapping: &ModuleMapping) -> VisitedAndCycles {
        let mut stack: Vec<u32> = vec![*module_mapping.to_id(name).unwrap()]; // we check before hand that the name is is in in the imports, ergo it will be in the module mapping
        let mut fully_visited: FxHashSet<u32> = FxHashSet::default();
        let mut cycles: FxHashSet<Vec<u32>> = FxHashSet::default();
        self.has_cycles_helper(
            name,
            module_mapping,
            &mut stack,
            &mut cycles,
            &mut fully_visited,
            0,
        );
        VisitedAndCycles::new(fully_visited, cycles)
    }

    fn has_cycles_helper(
        &self,
        name: &Arc<str>,
        module_mapping: &ModuleMapping,
        stack: &mut Vec<u32>,
        cycles: &mut FxHashSet<Vec<u32>>,
        fully_visited: &mut FxHashSet<u32>,
        level: usize,
    ) {
        let Some(&module_id) = module_mapping.to_id(name) else { return; };
        if let Some(imports) = self.imports.get(name) {
            let tabs = "\t".repeat(level);
            debug!("{tabs}{name}");
            for import in imports.iter() {
                debug!("{tabs}\timport: {}", import.module);
                let Some(check_module_id) = module_mapping.to_id(&import.module) else { continue; };
                if let Some(idx) = stack.iter().position(|s| s == check_module_id) {
                    debug!("{tabs}\t\t cycles: {:?}", stack[idx..].to_vec());
                    // when the length is 1 and the only item is the import we're looking at
                    // then we're importing self, could we report this so we don't have to
                    // do this again for import-self W0406?
                    if stack[idx..].len() == 1 && stack[idx] == module_id {
                        continue;
                    }
                    cycles.insert(stack[idx..].to_vec());
                } else {
                    stack.push(*check_module_id);
                    self.has_cycles_helper(
                        &import.module,
                        module_mapping,
                        stack,
                        cycles,
                        fully_visited,
                        level + 1,
                    );
                    stack.pop();
                }
            }
        }
        fully_visited.insert(module_id);
    }
}

/// PLR0914
pub fn cyclic_import(
    path: &Path,
    package: Option<&Path>,
    imports: &FxHashMap<Arc<str>, Vec<ModuleImport>>,
    cyclic_import_helper: &mut CyclicImportHelper,
) -> Option<Vec<Diagnostic>> {
    let Some(package) = package else {
        return None;
    };
    let Some(module_name) = to_module_path(package, path) else {
        return None;
    };
    let module_name = module_name.join(".");
    // if the module name isn't in the import map, it can't possibly have cycles
    // this also allows us to use `unwrap` whenever we use methods on the `ModuleMapping`
    // as any modules as part of cycles are guaranteed to be in the `ModuleMapping`
    debug!("Checking module {module_name}");
    let Some((module_name, _)) = imports.get_key_value(&module_name as &str) else {
        return None;
    };
    if let Some(existing_cycles) = cyclic_import_helper.cycles.get(
        cyclic_import_helper
            .module_mapping
            .to_id(module_name)
            .unwrap(),
    ) {
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
                                .map(|id| {
                                    cyclic_import_helper
                                        .module_mapping
                                        .to_module(id)
                                        .unwrap()
                                        .to_string()
                                })
                                .collect::<Vec<_>>()
                                .join(" -> "),
                        },
                        (&imports[module_name][0]).into(),
                    )
                })
                .collect::<Vec<Diagnostic>>(),
        )
    } else {
        let cyclic_import_checker = CyclicImportChecker { imports };
        let VisitedAndCycles {
            fully_visited: mut visited,
            cycles: new_cycles,
        } = cyclic_import_checker.has_cycles(module_name, &cyclic_import_helper.module_mapping);
        // we'll always have new visited stuff if we have
        let mut out_vec: Vec<Diagnostic> = Vec::new();
        if let Some(new_cycles) = new_cycles {
            debug!("New cycles {new_cycles:#?}");
            for new_cycle in &new_cycles {
                if let [first, the_rest @ ..] = &new_cycle[..] {
                    if first
                        == cyclic_import_helper
                            .module_mapping
                            .to_id(module_name)
                            .unwrap()
                    {
                        out_vec.push(Diagnostic::new(
                            CyclicImport {
                                cycle: new_cycle
                                    .iter()
                                    .map(|id| {
                                        cyclic_import_helper
                                            .module_mapping
                                            .to_module(id)
                                            .unwrap()
                                            .to_string()
                                    })
                                    .collect::<Vec<_>>()
                                    .join(" -> "),
                            },
                            imports[module_name]
                                .iter()
                                .find(|m| {
                                    &m.module
                                        == cyclic_import_helper
                                            .module_mapping
                                            .to_module(&the_rest[0])
                                            .unwrap()
                                })
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
                    if let Some(existing) = cyclic_import_helper.cycles.get_mut(involved_module) {
                        existing.insert(cycle_to_insert);
                    } else {
                        let mut new_set = FxHashSet::default();
                        new_set.insert(cycle_to_insert);
                        cyclic_import_helper
                            .cycles
                            .insert(*involved_module, new_set);
                    }
                    visited.remove(involved_module);
                }
            }
        }
        // process the visited nodes which don't have cycles
        for visited_module in &visited {
            cyclic_import_helper
                .cycles
                .insert(*visited_module, FxHashSet::default());
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
    use ruff_python_ast::imports::ImportMap;
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
        let import_map = ImportMap::new(map);
        let cyclic_checker = CyclicImportChecker {
            imports: &import_map.module_to_imports,
        };

        let cycle_helper = CyclicImportHelper::new(&import_map);

        let VisitedAndCycles {
            fully_visited: visited,
            cycles,
        } = cyclic_checker.has_cycles(&a.module, &cycle_helper.module_mapping);
        let mut check_visited = FxHashSet::default();
        check_visited.insert(*cycle_helper.module_mapping.to_id(&a.module).unwrap());
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
        let import_map = ImportMap::new(map);
        let cyclic_checker = CyclicImportChecker {
            imports: &import_map.module_to_imports,
        };

        let cycle_helper = CyclicImportHelper::new(&import_map);

        let VisitedAndCycles {
            fully_visited: visited,
            cycles,
        } = cyclic_checker.has_cycles(&a.module, &cycle_helper.module_mapping);

        let mut check_visited = FxHashSet::default();
        let a_id = *cycle_helper.module_mapping.to_id(&a.module).unwrap();
        let b_id = *cycle_helper.module_mapping.to_id(&b.module).unwrap();
        let c_id = *cycle_helper.module_mapping.to_id(&c.module).unwrap();
        let d_id = *cycle_helper.module_mapping.to_id(&d.module).unwrap();
        check_visited.insert(a_id);
        check_visited.insert(b_id);
        check_visited.insert(c_id);
        check_visited.insert(d_id);
        assert_eq!(visited, check_visited);

        let mut check_cycles = FxHashSet::default();
        check_cycles.insert(vec![a_id, b_id, c_id, d_id]);
        check_cycles.insert(vec![a_id, c_id, b_id, d_id]);
        check_cycles.insert(vec![a_id, c_id, d_id]);
        check_cycles.insert(vec![a_id, b_id, d_id]);
        check_cycles.insert(vec![c_id, b_id]);
        check_cycles.insert(vec![b_id, c_id]);
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
        let import_map = ImportMap::new(map);

        let path_a = Path::new("a/a");
        let path_b = Path::new("a/b");
        let path_c = Path::new("a/c");
        let package = Some(Path::new("a"));

        let mut cycle_helper = CyclicImportHelper::new(&import_map);
        let diagnostic = cyclic_import(
            path_a,
            package,
            &import_map.module_to_imports,
            &mut cycle_helper,
        );

        let a_a_id = *cycle_helper.module_mapping.to_id(&a_a.module).unwrap();
        let a_b_id = *cycle_helper.module_mapping.to_id(&a_b.module).unwrap();

        let mut set_a: FxHashSet<Vec<u32>> = FxHashSet::default();
        set_a.insert(vec![a_b_id, a_a_id]);
        let mut set_b: FxHashSet<Vec<u32>> = FxHashSet::default();
        set_b.insert(vec![a_a_id, a_b_id]);

        assert_eq!(
            diagnostic,
            Some(vec![Diagnostic::new(
                CyclicImport {
                    cycle: "a.a -> a.b".to_string(),
                },
                (&b_in_a).into(),
            )])
        );
        let mut check_cycles: FxHashMap<u32, FxHashSet<Vec<u32>>> = FxHashMap::default();
        check_cycles.insert(a_b_id, set_a);
        check_cycles.insert(a_a_id, set_b);
        assert_eq!(cycle_helper.cycles, check_cycles);

        let diagnostic = cyclic_import(
            path_b,
            package,
            &import_map.module_to_imports,
            &mut cycle_helper,
        );
        assert_eq!(
            diagnostic,
            Some(vec![Diagnostic::new(
                CyclicImport {
                    cycle: "a.b -> a.a".to_string(),
                },
                (&a_in_b).into(),
            )])
        );
        assert!(cyclic_import(
            path_c,
            package,
            &import_map.module_to_imports,
            &mut cycle_helper
        )
        .is_none());
    }

    #[test]
    fn cyclic_import_test_no_cycles_on_import_self() {
        let location = Location::new(1, 1);
        let a = ModuleImport::new("a".to_string(), location, location);
        let mut map = FxHashMap::default();
        map.insert(a.module.clone(), vec![a.clone()]);

        let import_map = ImportMap::new(map);
        let cycle_helper = CyclicImportHelper::new(&import_map);

        let cyclic_checker = CyclicImportChecker {
            imports: &import_map.module_to_imports,
        };
        let VisitedAndCycles {
            fully_visited: visited,
            cycles,
        } = cyclic_checker.has_cycles(&a.module, &cycle_helper.module_mapping);

        let mut check_visited = FxHashSet::default();
        check_visited.insert(*cycle_helper.module_mapping.to_id(&a.module).unwrap());
        assert_eq!(visited, check_visited);

        assert!(cycles.is_none());
    }
}
