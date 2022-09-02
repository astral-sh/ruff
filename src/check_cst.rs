use std::borrow::Borrow;
use std::collections::BTreeMap;

use bat::PrettyPrinter;
use bumpalo::Bump;
use libcst_native::{
    AnnAssign, Annotation, Arg, AsName, Assert, Assign, AssignEqual, AssignTarget,
    AssignTargetExpression, Asynchronous, Attribute, AugAssign, Await, BinaryOp, BinaryOperation,
    BooleanOp, BooleanOperation, Break, Call, ClassDef, Codegen, CompFor, CompIf, CompOp,
    Comparison, ComparisonTarget, CompoundStatement, ConcatenatedString, Continue, Decorator, Del,
    DelTargetExpression, Dict, DictComp, DictElement, Element, Ellipsis, Else, ExceptHandler,
    ExceptStarHandler, Expr, Expression, Finally, Float, For, FormattedString,
    FormattedStringContent, FormattedStringExpression, FormattedStringText, FunctionDef,
    GeneratorExp, Global, If, IfExp, Imaginary, Import, ImportAlias, ImportFrom, ImportStar,
    IndentedBlock, Index, Integer, Lambda, List, ListComp, Match, Module, Name, NameItem,
    NamedExpr, Nonlocal, OrElse, Param, ParamStar, Parameters, Pass, Raise, Return, Set, SetComp,
    SimpleStatementLine, SimpleStatementSuite, SimpleString, Slice, SmallStatement,
    StarredDictElement, StarredElement, Statement, Subscript, SubscriptElement, Try, TryStar,
    Tuple, UnaryOp, UnaryOperation, While, With, WithItem, Yield, YieldValue,
};
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
    bump: Bump,
    settings: &'a Settings,
    checks: Vec<Check>,
}

impl Checker<'_> {
    pub fn new(settings: &Settings) -> Checker {
        Checker {
            bump: Bump::new(),
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

    fn visit_Expression<'a, 'b>(&'b mut self, node: &'a Expression<'a>) -> Expression<'a>
    where
        'b: 'a,
    {
        match node {
            Expression::FormattedString(node) => match &node.parts[..] {
                [node] => match node {
                    FormattedStringContent::Text(node) => {
                        let x = node.value.to_string();
                        println!("Found: {:?}", node);
                        return Expression::SimpleString(Box::new(SimpleString {
                            value: self.bump.alloc(format!("\"{}\"", x)),
                            lpar: vec![],
                            rpar: vec![],
                        }));
                    }
                    _ => {}
                },
                _ => {}
            },
            _ => {}
        }

        cst_visitor::walk_Expression(self, node)
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
