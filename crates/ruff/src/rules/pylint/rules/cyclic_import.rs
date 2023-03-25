use itertools::Itertools;

use rustc_hash::FxHashMap;

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
        for (name, vec) in self.imports.iter() {
            if !vec.is_empty() {
                let mut stack: Vec<&str> = vec![name];
                self.has_cycles_helper(name, &mut stack, &mut cycles, 0);
            }
        }
        if cycles.is_empty() {
            None
        } else {
            Some(cycles.into_iter().unique().collect())
        }
    }

    fn has_cycles_helper<'a>(
        &'a self,
        name: &str,
        stack: &mut Vec<&'a str>,
        cycles: &mut Vec<Vec<&'a str>>,
        level: usize,
    ) {
        if let Some(imports) = self.imports.get(name) {
            let tabs = "\t".repeat(level);
            log::debug!("{tabs}check {name}");
            for import in imports.iter() {
                log::debug!("{tabs}\timport {}", import.name);
                if let Some((idx, _)) = stack.iter().enumerate().find(|(_, &s)| s == import.name) {
                    log::debug!("{tabs}\t\tcycle {:?}", &stack[idx..]);
                    cycles.push(stack[idx..].to_vec());
                } else {
                    stack.push(&import.name);
                    self.has_cycles_helper(&import.name, stack, cycles, level + 1);
                    stack.pop();
                }
            }
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
                    .to_string_lossy()
                    .to_string(),
                diagnostic,
            ));
        }
        Some(out_vec)
    } else {
        None
    }
}
