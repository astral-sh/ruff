use std::collections::{HashMap, HashSet};

use itertools::Itertools;
use ruff_python_ast::cast;
use ruff_python_semantic::{
    SemanticModel, {Definition, Member, MemberKind},
};
use rustpython_parser::ast::{Expr, ExprName, Ranged, Stmt, StmtClassDef};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks that abstract classes with `abc.ABCMeta` as metaclass and abstract methods
/// are not instantiated.
///
/// ## Why is this bad?
/// Abstract classes are not meant to be instantiated, they are meant to be subclassed.
/// Using an instance of an abstract class can lead to unexpected behavior.
///
/// ## Example
/// ```import abc
///
///
/// class Animal(abc.ABC):
///     @abc.abstractmethod
///     def make_sound(self):
///         pass
///
///
/// sheep = Animal()```
///
///
/// ## References
/// - [Python documentation](https://docs.python.org/3/library/abc.html)
/// - [PEP 3119](https://peps.python.org/pep-3119/#the-abc-module-an-abc-support-framework)
#[violation]
pub struct AbstractClassInstantiated {
    class_name: String,
}

impl Violation for AbstractClassInstantiated {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Abstract class '{}' with abstract methods instantiated",
            self.class_name
        )
    }
}

const ABC_METHODS: [&str; 4] = [
    "abc.abstractproperty",
    "abc.abstractmethod",
    "abc.abstractclassmethod",
    "abc.abstractstaticmethod",
];

const ABC_METACLASSES: [&str; 2] = ["_py_abc.ABCMeta", "abc.ABCMeta"];

/// Remove typing.Generic from the MRO
fn clean_typing_generic_mro(bases: &mut Vec<String>, bases_mro: &mut Vec<Vec<String>>) {
    if let Some((position_in_bases, _)) = bases
        .iter()
        .find_position(|base| base.as_str() == "typing.Generic")
    {
        if bases_mro
            .iter()
            .enumerate()
            .filter(|&(i, _)| i != position_in_bases)
            .any(|(_, mro)| mro.iter().any(|n| n.as_str() == "typing.Generic"))
        {
            bases.remove(position_in_bases);
            bases_mro.remove(position_in_bases);
        }
    }
}

/// Checks if a MRO has duplicates
fn has_duplicates(mro: &[String]) -> bool {
    let mut seen = HashSet::new();
    for class_def in mro.iter() {
        if seen.contains(class_def) {
            return true;
        }
        seen.insert(class_def);
    }
    false
}

/// Recursively calculates the MRO of a class
fn method_resolution_order(model: &SemanticModel, class_name: String) -> Option<Vec<String>> {
    let mut bases_mro = Vec::new();
    let mut bases = Vec::new();
    let class_definitions = class_definitions(model).collect_vec();
    let Some(class_def) =  class_definitions.iter().find(|class_def| class_def.name.as_str() == class_name) else {
        return None;
    };

    for base in &class_def.bases {
        if base.is_attribute_expr() {
            let Some(call_path) = model.resolve_call_path(base) else {
                continue;
            };
            bases.push(call_path.join("."));
            continue;
        };

        let id = match base {
            Expr::Name(expr) => expr.id.as_str(),
            _ => continue,
        };

        let Some(class_def) =  class_definitions.iter().find(|c| c.name.as_str() == id) else {
            continue;
        };

        match method_resolution_order(model, class_def.name.to_string()) {
            Some(mros) => bases_mro.push(mros),
            None => return None,
        }

        bases.push(class_def.name.to_string());
    }

    if bases_mro.iter().any(|b| has_duplicates(b)) || has_duplicates(&bases) {
        // Probably there's an inconsistent hierarchy. There must be
        // something wrong going on, don't try to figure it out.
        return None;
    }

    clean_typing_generic_mro(&mut bases, &mut bases_mro);
    let mut unmerged_mro = vec![vec![class_name]];
    unmerged_mro.extend(bases_mro.into_iter());
    unmerged_mro.push(bases);

    merge_mro(unmerged_mro)
}

/// Tries to find the next base in the MRO
fn find_next_base(bases: &mut [Vec<String>]) -> Option<String> {
    for i in 0..bases.len() {
        let head = match bases[i].get(0) {
            Some(v) => v.clone(),
            None => continue,
        };
        if !bases.iter().any(|m| m.iter().skip(1).any(|s| s == &head)) {
            for item in bases.iter_mut() {
                let Some(first) = item.get(0) else {
                    continue;
                };
                if first == &head {
                    item.remove(0);
                }
            }
            return Some(head);
        }
    }
    None
}

/// Merge the MROs in a single vector
fn merge_mro(mut mros: Vec<Vec<String>>) -> Option<Vec<String>> {
    let mut result = vec![];
    while !mros.is_empty() {
        let Some(head) = find_next_base(&mut mros) else {
            return None;
        };
        result.push(head);
        mros.retain(|s| !s.is_empty());
    }
    Some(result)
}

/// Returns true if `class_def` has at least an abstract method
fn has_abstract_method(model: &SemanticModel, class_def: &StmtClassDef) -> bool {
    let Some(mro) = method_resolution_order(model, class_def.name.to_string()) else {
        return false;
    };
    let class_definitions = class_definitions(model).collect_vec();
    let mut abstract_methods = HashSet::new();
    for ancestor in mro.iter().rev() {
        // There could be multiple definition of a class, maybe hidden behind a conditional.
        // Here we get all the bodies of all classes with that name and merge them into
        // a single vector.
        // Merging them doesn't cause any resolution issues, multiple definitions of a function
        // won't clash in any case.
        let bodies = class_definitions
            .iter()
            .filter(|c| c.name.as_str() == ancestor)
            .flat_map(|c| &c.body)
            .collect_vec();

        for stmt in bodies {
            // Get the the name of the function
            let names = match stmt {
                Stmt::FunctionDef(_) | Stmt::AsyncFunctionDef(_) => vec![cast::name(stmt)],
                Stmt::AnnAssign(a) => vec![a.target.as_name_expr().unwrap().id.as_str()],
                // Assignment staments can have multiple targets (e.g. a, b = 1, 2), so we need to
                // get all of them in case there is an assignment to a inherited abstract method that
                // is overridden in the current class.
                Stmt::Assign(a) => a
                    .targets
                    .iter()
                    .map(|t| t.as_name_expr().unwrap().id.as_str())
                    .collect_vec(),
                _ => continue,
            };

            let decorators = match stmt {
                Stmt::FunctionDef(_) | Stmt::AsyncFunctionDef(_) => cast::decorator_list(stmt),
                _ => &[],
            };

            if decorators.is_empty() {
                // If the current function doesn't have decorators, we can safely assume that
                // it's not abstract. If it was found as abstract in an ancestor it's now overridden.
                // Also we remove all names as there could be multiple assignments to different functions.
                // (e.g. a, b = foo, bar)
                for name in &names {
                    abstract_methods.remove(name);
                }
            }

            for decorator in decorators {
                let Some(call_path) = model.resolve_call_path(&decorator.expression) else {
                    continue;
                };
                if ABC_METHODS.contains(&call_path.join(".").as_str()) {
                    // We get only the first name as we reach this point only if at least
                    // a decorator is present, so we can safely assume that this is a
                    // function definition.
                    abstract_methods.insert(names[0]);
                }
            }
        }
    }
    !abstract_methods.is_empty()
}

/// Get all the class definitions in `model`
fn class_definitions<'a>(model: &'a SemanticModel) -> impl Iterator<Item = &'a StmtClassDef> {
    fn filter_map_statement<'b>(def: &'b Definition) -> Option<&'b Stmt> {
        let Definition::Member(Member{
            kind: MemberKind::Class | MemberKind::NestedClass,
            stmt, ..
        }) = def  else {
            return None;
        };
        Some(*stmt)
    }
    fn filter_map_class(stmt: &Stmt) -> Option<&StmtClassDef> {
        match stmt {
            Stmt::ClassDef(def) => Some(def),
            _ => None,
        }
    }
    model
        .definitions
        .iter()
        .filter_map(filter_map_statement)
        .filter_map(filter_map_class)
}

/// Returns true if `class_def` has an abstract base
fn has_abstract_metaclass(model: &SemanticModel, class_def: &StmtClassDef) -> bool {
    for keyword in &class_def.keywords {
        let Some(id)  = &keyword.arg else {
            continue;
        };

        if id.as_str() != "metaclass" {
            continue;
        }

        let Some(call_path) = model.resolve_call_path(&keyword.value) else {
            continue;
        };

        if ABC_METACLASSES.contains(&call_path.join(".").as_str()) {
            return true;
        }
    }

    // No metaclass found, let's check the ancestors
    ancestors(model, class_def)
        .iter()
        .any(|item| has_abstract_metaclass(model, item.1))
}

/// Returns a map of all the ancestors of `class_def`
fn ancestors<'a>(
    model: &'a SemanticModel,
    class_def: &'a StmtClassDef,
) -> HashMap<&'a str, &'a StmtClassDef> {
    let mut visited: HashMap<&'a str, &'a StmtClassDef> = HashMap::new();
    let class_definitions = class_definitions(model).collect_vec();

    for base in &class_def.bases {
        let id = match base {
            Expr::Name(expr) => expr.id.as_str(),
            Expr::Attribute(expr) => expr.attr.as_str(),
            _ => continue,
        };

        if visited.get(id).is_some() {
            continue;
        }

        let Some(class_def) =  class_definitions.iter().find(|class_def| class_def.name.as_str() == id) else {
            continue;
        };

        visited.insert(id, class_def);
        visited.extend(ancestors(model, class_def));
    }

    visited
}

/// PLE0110
pub(crate) fn abstract_class_instantiated(checker: &mut Checker, expr: &Expr, func: &Expr) {
    if !expr.is_call_expr() {
        return;
    }

    let Expr::Name(ExprName{ id: func_name, ..}) = func else {
        return;
    };

    let definitions = class_definitions(checker.semantic()).collect_vec();
    for class_def in &definitions {
        if *func_name != class_def.name.as_str() {
            // This class is not being instantiated, skip it
            continue;
        }

        if !has_abstract_method(checker.semantic(), class_def) {
            // If the class doesn't have abstract methods it can be instatiated
            continue;
        }

        if has_abstract_metaclass(checker.semantic(), class_def) {
            checker.diagnostics.push(Diagnostic::new(
                AbstractClassInstantiated {
                    class_name: String::from(class_def.name.clone()),
                },
                expr.range(),
            ));
            return;
        }
    }
}
