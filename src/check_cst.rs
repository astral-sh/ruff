use std::collections::BTreeMap;

use libcst_native::{Codegen, Module};
use rustpython_parser::ast::Location;

use crate::checks::Check;
use crate::cst_visitor::CSTVisitor;
use crate::settings::Settings;

enum ScopeKind {
    Class,
    Function,
    Generator,
    Module,
}

struct Scope {
    kind: ScopeKind,
    values: BTreeMap<String, Binding>,
}

enum BindingKind {
    Argument,
    Assignment,
    ClassDefinition,
    Definition,
    FutureImportation,
    Importation,
    StarImportation,
    SubmoduleImportation,
}

struct Binding {
    kind: BindingKind,
    name: String,
    location: Location,
    used: bool,
}

struct Checker<'a> {
    settings: &'a Settings,
    checks: Vec<Check>,
}

impl Checker<'_> {
    pub fn new(settings: &Settings) -> Checker {
        Checker {
            settings,
            checks: vec![],
        }
    }
}

impl CSTVisitor for Checker<'_> {
    fn visit_If(&mut self, node: &If) {
        if let Expression::Tuple { .. } = node.test {
            self.checks.push(Check {
                kind: CheckKind::IfTuple,
                location: Default::default(),
            });
        }
        cst_visitor::walk_If(self, node);
    }

    fn visit_ClassDef<'a>(&mut self, node: &'a ClassDef<'a>) -> ClassDef<'a> {
        let bases: Vec<Arg<'a>> = node
            .bases
            .clone()
            .into_iter()
            .filter(|node| {
                if let Expression::Name(node) = &node.value {
                    node.value != "object"
                } else {
                    true
                }
            })
            .collect();

        let mut transformed: ClassDef<'a> = node.clone();
        transformed.bases = bases;
        transformed.lpar = None;
        transformed.rpar = None;
        transformed
    }
}

pub fn check_cst<'a>(python_cst: &'a Module<'a>, settings: &Settings) -> Vec<Check> {
    // // Create a new arena to bump allocate into.
    // let bump = Bump::new();
    //
    // // Allocate values into the arena.
    // let scooter = bump.alloc(python_cst.clone());

    let mut checker = Checker::new(settings);
    let mut transformed = checker.visit_Module(python_cst);

    let mut state = Default::default();
    transformed.codegen(&mut state);
    println!("{}", state);

    checker.checks
}
