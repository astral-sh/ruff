use std::collections::BTreeMap;

use libcst_native::{Expression, If, Module};
use rustpython_parser::ast::Location;

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
    fn visit_If<'a>(&'a mut self, node: &'a If) -> &'a If {
        if let Expression::Tuple { .. } = node.test {
            self.checks.push(Check {
                kind: CheckKind::IfTuple,
                location: Default::default(),
            });
        }
        cst_visitor::walk_If(self, node);
        node
    }
}

pub fn check_cst(python_cst: &Module, settings: &Settings) -> Vec<Check> {
    let mut checker = Checker::new(settings);
    for node in &python_cst.body {
        checker.visit_Statement(node);
    }
    checker.checks
}
