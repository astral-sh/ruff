use std::path::Path;

use log::debug;

use rustc_hash::{FxHashMap, FxHashSet};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{
    helpers::to_module_path,
    types::{Import, Imports},
};

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
    imports: &'a FxHashMap<String, Vec<Import>>,
}

impl CyclicImportChecker<'_> {
    fn has_cycles<'a>(&'a self, name: &'a str) -> (FxHashSet<&str>, Option<Vec<Vec<String>>>) {
        let mut fully_visited: FxHashSet<&str> = FxHashSet::default();
        let mut cycles: Vec<Vec<String>> = Vec::new();
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
        cycles: &mut Vec<Vec<String>>,
        fully_visited: &mut FxHashSet<&'a str>,
        level: usize,
    ) {
        if let Some(imports) = self.imports.get(name) {
            let tabs = "\t".repeat(level);
            debug!("{tabs}{name}");
            for import in imports.iter() {
                debug!("{tabs}\timport: {}", import.name);
                if let Some(idx) = stack.iter().position(|&s| s == import.name) {
                    debug!("{tabs}\t\t cycles: {:?}", stack[idx..].to_vec());
                    cycles.push(
                        stack[idx..]
                            .iter()
                            .map(|&s| s.into())
                            .collect::<Vec<String>>(),
                    );
                } else {
                    stack.push(&import.name);
                    self.has_cycles_helper(&import.name, stack, cycles, fully_visited, level + 1);
                    stack.pop();
                }
            }
            fully_visited.insert(name);
        }
    }
}

/// PLR0914
pub fn cyclic_import(
    path: &Path,
    package: Option<&Path>,
    imports: &Imports,
    cycles: &mut FxHashMap<String, Vec<Vec<String>>>,
) -> Option<Vec<Diagnostic>> {
    let module_name = to_module_path(package.unwrap(), path).unwrap().join(".");
    if let Some(existing_cycles) = cycles.get(&module_name as &str) {
        if existing_cycles.is_empty() {
            return None;
        }
        Some(
            existing_cycles
                .iter()
                .map(|cycle| {
                    let pos = cycle.iter().position(|s| s == &module_name).unwrap();
                    let next_import = if pos == cycle.len() - 1 { 0 } else { pos + 1 };
                    Diagnostic::new(
                        CyclicImport {
                            // need to reorder the detected cycle
                            cycle: cycle[pos..]
                                .iter()
                                .chain(cycle[..pos].iter())
                                .map(std::clone::Clone::clone)
                                .collect::<Vec<_>>()
                                .join(" -> "),
                        },
                        imports
                            .imports_per_module
                            .get(&module_name)
                            .unwrap()
                            .iter()
                            .find(|m| m.name == cycle[next_import])
                            .unwrap()
                            .into(),
                    )
                })
                .collect::<Vec<Diagnostic>>(),
        )
    } else {
        if imports.imports_per_module.get(&module_name).is_some() {
            let cyclic_import_checker = CyclicImportChecker {
                imports: &imports.imports_per_module,
            };
            let (mut visited, new_cycles) = cyclic_import_checker.has_cycles(&module_name);
            // we'll always have new visited stuff if we have
            let mut out_vec: Vec<Diagnostic> = Vec::new();
            if let Some(new_cycles) = new_cycles {
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
                                imports
                                    .imports_per_module
                                    .get(&module_name)
                                    .unwrap()
                                    .iter()
                                    .find(|m| &m.name == the_rest.first().unwrap())
                                    .unwrap()
                                    .into(),
                            ));
                        } else {
                            // these should always be new and start off with the module that is the start of a cycle
                            cycles
                                .entry(first.clone())
                                .and_modify(|cycle| cycle.push(new_cycle.clone()))
                                .or_insert(vec![new_cycle.clone()]);
                        }
                        visited.remove(first as &str);
                        // process the_rest
                        for involved_module in the_rest.iter() {
                            cycles
                                .entry(involved_module.clone())
                                .and_modify(|cycle| cycle.push(new_cycle.clone()))
                                .or_insert(vec![new_cycle.clone()]);
                        }
                    }
                }
            }
            // process the visited nodes which don't have cycles
            for visited_module in &visited {
                cycles.insert((*visited_module).to_string(), vec![]);
            }
            if out_vec.is_empty() {
                None
            } else {
                Some(out_vec)
            }
        } else {
            None
        }
    }
}
