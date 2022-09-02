use std::collections::BTreeMap;

use libcst_native::{Arg, ClassDef, Codegen, Expression, If, Module};
use rustpython_parser::ast::Location;
use bat::PrettyPrinter;


use crate::checks::{Check, CheckKind};
use crate::cst_visitor;
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
        let mut bases: Vec<Arg<'a>> = node
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
        if bases.is_empty() {
            transformed.lpar = None;
            transformed.rpar = None;
        } else {
            let node = bases.last_mut().unwrap();
            node.comma = None;
        }
        transformed.bases = bases;
        transformed
    }
}

pub fn check_cst<'a>(python_cst: &'a Module<'a>, settings: &Settings) -> Vec<Check> {
    // // Create a new arena to bump allocate into.
    // let bump = Bump::new();
    //
    // // Allocate values into the arena.
    // let scooter = bump.alloc(python_cst.clone());

    let mut x = python_cst.clone();
    let mut s = Default::default();
    x.codegen(&mut s);

    println!("Starting from source:");
    println!("```");
    let source = s.to_string().into_bytes();
    PrettyPrinter::new()
    .input_from_bytes(&source)
    .language("python")
    .print()
    .unwrap();
    println!("```");

    let mut checker = Checker::new(settings);
    let mut transformed = checker.visit_Module(python_cst);

    let mut state = Default::default();
    transformed.codegen(&mut state);

    println!("");
    println!("Generated output:");
println!("```");
    let source = state.to_string().into_bytes();
    PrettyPrinter::new()
    .input_from_bytes(&source)
    .language("python")
    .print()
    .unwrap();
    println!("```");

    checker.checks
}
