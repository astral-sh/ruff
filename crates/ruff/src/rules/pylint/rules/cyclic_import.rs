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
                let mut visited: Vec<&str> = Vec::new();
                visited.push(name);
                if let Some(cycle) = self.has_cycles_helper(name, &mut visited) {
                    cycles.push(cycle);
                }
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
        name: &str,
        visited: &mut Vec<&'a str>,
    ) -> Option<Vec<&'a str>> {
        if let Some(imports) = self.imports.get(name) {
            for import in imports.iter() {
                if visited.contains(&(&import.name as &str)) {
                    let (idx, _) = visited
                        .iter()
                        .enumerate()
                        .find(|(_, &s)| s == import.name)
                        .unwrap();
                    return Some(visited[idx..].to_vec());
                }
                visited.push(&import.name);
                if let Some(cycle) = self.has_cycles_helper(&import.name, visited) {
                    return Some(cycle);
                }
                visited.pop();
            }
        }
        None
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
