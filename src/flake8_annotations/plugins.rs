use rustpython_ast::{Arguments, Constant, Expr, ExprKind, Stmt, StmtKind};

use crate::ast::types::Range;
use crate::ast::visitor;
use crate::ast::visitor::Visitor;
use crate::check_ast::Checker;
use crate::checks::{CheckCode, CheckKind};
use crate::docstrings::definition::{Definition, DefinitionKind};
use crate::visibility::Visibility;
use crate::{visibility, Check};

#[derive(Default)]
struct ReturnStatementVisitor<'a> {
    returns: Vec<&'a Option<Box<Expr>>>,
}

impl<'a, 'b> Visitor<'b> for ReturnStatementVisitor<'a>
where
    'b: 'a,
{
    fn visit_stmt(&mut self, stmt: &'b Stmt) {
        match &stmt.node {
            StmtKind::FunctionDef { .. } | StmtKind::AsyncFunctionDef { .. } => {
                // No recurse.
            }
            StmtKind::Return { value } => self.returns.push(value),
            _ => visitor::walk_stmt(self, stmt),
        }
    }
}

fn is_none_returning(stmt: &Stmt) -> bool {
    let mut visitor: ReturnStatementVisitor = Default::default();
    for stmt in match_body(stmt) {
        visitor.visit_stmt(stmt);
    }
    for expr in visitor.returns.into_iter().flatten() {
        if !matches!(
            expr.node,
            ExprKind::Constant {
                value: Constant::None,
                ..
            }
        ) {
            return false;
        }
    }
    true
}

fn match_args(stmt: &Stmt) -> &Arguments {
    match &stmt.node {
        StmtKind::FunctionDef { args, .. } | StmtKind::AsyncFunctionDef { args, .. } => args,
        _ => panic!("Found non-FunctionDef in match_args"),
    }
}

fn match_body(stmt: &Stmt) -> &Vec<Stmt> {
    match &stmt.node {
        StmtKind::FunctionDef { body, .. } | StmtKind::AsyncFunctionDef { body, .. } => body,
        _ => panic!("Found non-FunctionDef in match_body"),
    }
}

fn match_returns(stmt: &Stmt) -> &Option<Box<Expr>> {
    match &stmt.node {
        StmtKind::FunctionDef { returns, .. } | StmtKind::AsyncFunctionDef { returns, .. } => {
            returns
        }
        _ => panic!("Found non-FunctionDef in match_returns"),
    }
}

/// Generate flake8-annotation checks for a given `Definition`.
pub fn definition(checker: &mut Checker, definition: &Definition, visibility: &Visibility) {
    match &definition.kind {
        DefinitionKind::Module => {}
        DefinitionKind::Package => {}
        DefinitionKind::Class(_) => {}
        DefinitionKind::NestedClass(_) => {}
        DefinitionKind::Function(stmt) | DefinitionKind::NestedFunction(stmt) => {
            let args = match_args(stmt);
            let returns = match_returns(stmt);

            // ANN001
            for arg in args
                .args
                .iter()
                .chain(args.posonlyargs.iter())
                .chain(args.kwonlyargs.iter())
            {
                if arg.node.annotation.is_none() {
                    if !(checker.settings.flake8_annotations.suppress_dummy_args
                        && checker.settings.dummy_variable_rgx.is_match(&arg.node.arg))
                    {
                        if checker.settings.enabled.contains(&CheckCode::ANN001) {
                            checker.add_check(Check::new(
                                CheckKind::MissingTypeFunctionArgument,
                                Range::from_located(arg),
                            ));
                        }
                    }
                }
            }

            // ANN002
            if let Some(arg) = &args.vararg {
                if arg.node.annotation.is_none() {
                    if !(checker.settings.flake8_annotations.suppress_dummy_args
                        && checker.settings.dummy_variable_rgx.is_match(&arg.node.arg))
                    {
                        if checker.settings.enabled.contains(&CheckCode::ANN002) {
                            checker.add_check(Check::new(
                                CheckKind::MissingTypeArgs,
                                Range::from_located(arg),
                            ));
                        }
                    }
                }
            }

            // ANN003
            if let Some(arg) = &args.kwarg {
                if arg.node.annotation.is_none() {
                    if !(checker.settings.flake8_annotations.suppress_dummy_args
                        && checker.settings.dummy_variable_rgx.is_match(&arg.node.arg))
                    {
                        if checker.settings.enabled.contains(&CheckCode::ANN003) {
                            checker.add_check(Check::new(
                                CheckKind::MissingTypeKwargs,
                                Range::from_located(arg),
                            ));
                        }
                    }
                }
            }

            // ANN201, ANN202
            if returns.is_none() {
                // Allow omission of return annotation in `__init__` functions, if the function
                // only returns `None` (explicitly or implicitly).
                if checker.settings.flake8_annotations.suppress_none_returning
                    && is_none_returning(stmt)
                {
                    return;
                }

                match visibility {
                    Visibility::Public => {
                        if checker.settings.enabled.contains(&CheckCode::ANN201) {
                            checker.add_check(Check::new(
                                CheckKind::MissingReturnTypePublicFunction,
                                Range::from_located(stmt),
                            ));
                        }
                    }
                    Visibility::Private => {
                        if checker.settings.enabled.contains(&CheckCode::ANN202) {
                            checker.add_check(Check::new(
                                CheckKind::MissingReturnTypePrivateFunction,
                                Range::from_located(stmt),
                            ));
                        }
                    }
                }
            }
        }
        DefinitionKind::Method(stmt) => {
            let args = match_args(stmt);
            let returns = match_returns(stmt);
            let mut has_any_typed_arg = false;

            // ANN001
            for arg in args
                .args
                .iter()
                .chain(args.posonlyargs.iter())
                .chain(args.kwonlyargs.iter())
                .skip(
                    // If this is a non-static method, skip `cls` or `self`.
                    usize::from(!visibility::is_staticmethod(stmt)),
                )
            {
                if arg.node.annotation.is_none() {
                    if !(checker.settings.flake8_annotations.suppress_dummy_args
                        && checker.settings.dummy_variable_rgx.is_match(&arg.node.arg))
                    {
                        if checker.settings.enabled.contains(&CheckCode::ANN001) {
                            checker.add_check(Check::new(
                                CheckKind::MissingTypeFunctionArgument,
                                Range::from_located(arg),
                            ));
                        }
                    }
                } else {
                    has_any_typed_arg = true;
                }
            }

            // ANN002
            if let Some(arg) = &args.vararg {
                if arg.node.annotation.is_none() {
                    if !(checker.settings.flake8_annotations.suppress_dummy_args
                        && checker.settings.dummy_variable_rgx.is_match(&arg.node.arg))
                    {
                        if checker.settings.enabled.contains(&CheckCode::ANN002) {
                            checker.add_check(Check::new(
                                CheckKind::MissingTypeArgs,
                                Range::from_located(arg),
                            ));
                        }
                    }
                } else {
                    has_any_typed_arg = true;
                }
            }

            // ANN003
            if let Some(arg) = &args.kwarg {
                if arg.node.annotation.is_none() {
                    if !(checker.settings.flake8_annotations.suppress_dummy_args
                        && checker.settings.dummy_variable_rgx.is_match(&arg.node.arg))
                    {
                        if checker.settings.enabled.contains(&CheckCode::ANN003) {
                            checker.add_check(Check::new(
                                CheckKind::MissingTypeKwargs,
                                Range::from_located(arg),
                            ));
                        }
                    }
                } else {
                    has_any_typed_arg = true;
                }
            }

            // ANN101, ANN102
            if !visibility::is_staticmethod(stmt) {
                if let Some(arg) = args.args.first() {
                    if arg.node.annotation.is_none() {
                        if visibility::is_classmethod(stmt) {
                            if checker.settings.enabled.contains(&CheckCode::ANN101) {
                                checker.add_check(Check::new(
                                    CheckKind::MissingTypeCls,
                                    Range::from_located(arg),
                                ));
                            }
                        } else {
                            if checker.settings.enabled.contains(&CheckCode::ANN102) {
                                checker.add_check(Check::new(
                                    CheckKind::MissingTypeSelf,
                                    Range::from_located(arg),
                                ));
                            }
                        }
                    }
                }
            }

            // ANN201, ANN202
            if returns.is_none() {
                // Allow omission of return annotation in `__init__` functions, if the function
                // only returns `None` (explicitly or implicitly).
                if checker.settings.flake8_annotations.suppress_none_returning
                    && is_none_returning(stmt)
                {
                    return;
                }

                if visibility::is_classmethod(stmt) {
                    if checker.settings.enabled.contains(&CheckCode::ANN206) {
                        checker.add_check(Check::new(
                            CheckKind::MissingReturnTypeClassMethod,
                            Range::from_located(stmt),
                        ));
                    }
                } else if visibility::is_staticmethod(stmt) {
                    if checker.settings.enabled.contains(&CheckCode::ANN205) {
                        checker.add_check(Check::new(
                            CheckKind::MissingReturnTypeStaticMethod,
                            Range::from_located(stmt),
                        ));
                    }
                } else if visibility::is_magic(stmt) {
                    if checker.settings.enabled.contains(&CheckCode::ANN204) {
                        checker.add_check(Check::new(
                            CheckKind::MissingReturnTypeMagicMethod,
                            Range::from_located(stmt),
                        ));
                    }
                } else if visibility::is_init(stmt) {
                    // Allow omission of return annotation in `__init__` functions, as long as at
                    // least one argument is typed.
                    if checker.settings.enabled.contains(&CheckCode::ANN204) {
                        if !(checker.settings.flake8_annotations.mypy_init_return
                            && has_any_typed_arg)
                        {
                            checker.add_check(Check::new(
                                CheckKind::MissingReturnTypeMagicMethod,
                                Range::from_located(stmt),
                            ));
                        }
                    }
                } else {
                    match visibility {
                        Visibility::Public => {
                            if checker.settings.enabled.contains(&CheckCode::ANN201) {
                                checker.add_check(Check::new(
                                    CheckKind::MissingReturnTypePublicFunction,
                                    Range::from_located(stmt),
                                ));
                            }
                        }
                        Visibility::Private => {
                            if checker.settings.enabled.contains(&CheckCode::ANN202) {
                                checker.add_check(Check::new(
                                    CheckKind::MissingReturnTypePrivateFunction,
                                    Range::from_located(stmt),
                                ));
                            }
                        }
                    }
                }
            }
        }
    }
}
