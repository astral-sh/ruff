use log::debug;

use rustc_hash::{FxHashMap, FxHashSet};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::{Import, Imports};

#[violation]
pub struct CyclicImport {
    pub cycle: String,
}

impl Violation for CyclicImport {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Cyclic import ({})", self.cycle)
    }
}

struct CyclicImportChecker<'a> {
    imports: &'a FxHashMap<String, Vec<Import>>,
}

impl CyclicImportChecker<'_> {
    fn has_cycles(&self) -> Option<Vec<Vec<&str>>> {
        let mut cycles: Vec<Vec<&str>> = Vec::new();
        let mut fully_visited: FxHashSet<&str> = FxHashSet::default();
        for (name, vec) in self.imports.iter() {
            if !vec.is_empty() && !fully_visited.contains(name as &str) {
                let mut stack: Vec<&str> = vec![name];
                self.has_cycles_helper(name, &mut stack, &mut cycles, &mut fully_visited, 0);
            }
        }
        if cycles.is_empty() {
            None
        } else {
            Some(cycles)
        }
    }

    fn has_cycles_helper<'a>(
        &'a self,
        name: &'a str,
        stack: &mut Vec<&'a str>,
        cycles: &mut Vec<Vec<&'a str>>,
        fully_visited: &mut FxHashSet<&'a str>,
        level: usize,
    ) {
        if let Some(imports) = self.imports.get(name) {
            let tabs = "\t".repeat(level);
            debug!("{tabs}{name}");
            for import in imports.iter() {
                debug!("{tabs}\timport: {}", import.name);
                if let Some((idx, _)) = stack.iter().enumerate().find(|(_, &s)| s == import.name) {
                    debug!("{tabs}\t\t cycles: {:?}", stack[idx..].to_vec());
                    cycles.push(stack[idx..].to_vec());
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
pub fn cyclic_import(imports: &Imports) -> Option<Vec<(String, Diagnostic)>> {
    let cyclic_import_checker = CyclicImportChecker {
        imports: &imports.imports_per_module,
    };
    if let Some(cycles) = cyclic_import_checker.has_cycles() {
        let mut out_vec: Vec<(String, Diagnostic)> = Vec::new();
        for cycle in &cycles {
            let current_module = cycle.first().unwrap();
            let current_import = imports
                .imports_per_module
                .get(*current_module)
                .unwrap()
                .first()
                .unwrap();
            let diagnostic = Diagnostic::new(
                CyclicImport {
                    cycle: cycle.join(" -> "),
                },
                current_import.into(),
            );
            out_vec.push((
                imports
                    .module_to_path_mapping
                    .get(*current_module)
                    .unwrap()
                    .clone(),
                diagnostic,
            ));
        }
        Some(out_vec)
    } else {
        None
    }
}
