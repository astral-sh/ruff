use std::collections::BTreeSet;
use std::path::Path;

use itertools::izip;
use rustpython_parser::ast::{
    Arg, Arguments, Cmpop, Constant, Excepthandler, ExcepthandlerKind, Expr, ExprContext, ExprKind,
    Location, Stmt, StmtKind, Suite, Unaryop,
};
use rustpython_parser::parser;

use crate::ast_ops::{
    extract_all_names, Binding, BindingKind, Scope, ScopeKind, SourceCodeLocator,
};
use crate::builtins::{BUILTINS, MAGIC_GLOBALS};
use crate::checks::{Check, CheckCode, CheckKind, Fix, RejectedCmpop};
use crate::settings::Settings;
use crate::visitor::{walk_excepthandler, Visitor};
use crate::{autofix, fixer, visitor};

pub const GLOBAL_SCOPE_INDEX: usize = 0;

struct Checker<'a> {
    // Input data.
    locator: SourceCodeLocator<'a>,
    settings: &'a Settings,
    autofix: &'a autofix::Mode,
    path: &'a str,
    // Computed checks.
    checks: Vec<Check>,
    // Scope tracking: retain all scopes, along with a stack of indexes to track which scopes are
    // active.
    scopes: Vec<Scope>,
    scope_stack: Vec<usize>,
    parents: Vec<&'a Stmt>,
    dead_scopes: Vec<usize>,
    deferred_annotations: Vec<&'a str>,
    deferred_functions: Vec<(&'a Stmt, Vec<usize>)>,
    deferred_lambdas: Vec<(&'a Expr, Vec<usize>)>,
    // Derivative state.
    in_f_string: bool,
    in_annotation: bool,
    seen_non_import: bool,
    seen_docstring: bool,
}

impl<'a> Checker<'a> {
    pub fn new(
        settings: &'a Settings,
        autofix: &'a autofix::Mode,
        path: &'a str,
        content: &'a str,
    ) -> Checker<'a> {
        Checker {
            settings,
            autofix,
            path,
            locator: SourceCodeLocator::new(content),
            checks: vec![],
            scope_stack: vec![],
            scopes: vec![],
            dead_scopes: vec![],
            deferred_annotations: vec![],
            deferred_functions: vec![],
            deferred_lambdas: vec![],
            parents: vec![],
            in_f_string: false,
            in_annotation: false,
            seen_non_import: false,
            seen_docstring: false,
        }
    }
}

#[derive(Debug, PartialEq)]
enum DictionaryKey<'a> {
    Constant(&'a Constant),
    Variable(&'a String),
}

fn convert_to_value(expr: &Expr) -> Option<DictionaryKey> {
    match &expr.node {
        ExprKind::Constant { value, .. } => Some(DictionaryKey::Constant(value)),
        ExprKind::Name { id, .. } => Some(DictionaryKey::Variable(id)),
        _ => None,
    }
}

impl<'a, 'b> Visitor<'b> for Checker<'a>
where
    'b: 'a,
{
    fn visit_stmt(&mut self, stmt: &'b Stmt) {
        self.parents.push(stmt);

        // Pre-visit.
        match &stmt.node {
            StmtKind::Global { names } | StmtKind::Nonlocal { names } => {
                // TODO(charlie): Handle doctests.
                let global_scope_id = self.scopes[GLOBAL_SCOPE_INDEX].id;

                let current_scope =
                    &self.scopes[*(self.scope_stack.last().expect("No current scope found."))];
                let current_scope_id = current_scope.id;
                if current_scope_id != global_scope_id {
                    for name in names {
                        for scope in self.scopes.iter_mut().skip(GLOBAL_SCOPE_INDEX + 1) {
                            scope.values.insert(
                                name.to_string(),
                                Binding {
                                    kind: BindingKind::Assignment,
                                    used: Some(global_scope_id),
                                    location: stmt.location,
                                },
                            );
                        }
                    }
                }
            }
            StmtKind::FunctionDef {
                name,
                decorator_list,
                returns,
                ..
            }
            | StmtKind::AsyncFunctionDef {
                name,
                decorator_list,
                returns,
                ..
            } => {
                for expr in decorator_list {
                    self.visit_expr(expr);
                }
                for expr in returns {
                    self.visit_annotation(expr);
                }
                self.add_binding(
                    name.to_string(),
                    Binding {
                        kind: BindingKind::Definition,
                        used: None,
                        location: stmt.location,
                    },
                );
            }
            StmtKind::Return { .. } => {
                if self
                    .settings
                    .select
                    .contains(CheckKind::ReturnOutsideFunction.code())
                {
                    if let Some(scope_index) = self.scope_stack.last().cloned() {
                        match self.scopes[scope_index].kind {
                            ScopeKind::Class | ScopeKind::Module => {
                                self.checks.push(Check::new(
                                    CheckKind::ReturnOutsideFunction,
                                    stmt.location,
                                ));
                            }
                            _ => {}
                        }
                    }
                }
            }
            StmtKind::ClassDef {
                name,
                bases,
                keywords,
                decorator_list,
                ..
            } => {
                if self.settings.select.contains(&CheckCode::R001) {
                    for expr in bases {
                        if let ExprKind::Name { id, .. } = &expr.node {
                            if id == "object" {
                                let scope = &self.scopes
                                    [*(self.scope_stack.last().expect("No current scope found."))];
                                match scope.values.get(id) {
                                    None
                                    | Some(Binding {
                                        kind: BindingKind::Builtin,
                                        ..
                                    }) => {
                                        let mut check = Check::new(
                                            CheckKind::UselessObjectInheritance(name.to_string()),
                                            expr.location,
                                        );
                                        if matches!(self.autofix, autofix::Mode::Generate)
                                            || matches!(self.autofix, autofix::Mode::Apply)
                                        {
                                            if let Some(fix) = fixer::remove_class_def_base(
                                                &mut self.locator,
                                                &stmt.location,
                                                expr.location,
                                                bases,
                                                keywords,
                                            ) {
                                                check.amend(fix);
                                            }
                                        } else {
                                        }
                                        self.checks.push(check);
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }

                for expr in bases {
                    self.visit_expr(expr)
                }
                for keyword in keywords {
                    self.visit_keyword(keyword)
                }
                for expr in decorator_list {
                    self.visit_expr(expr)
                }
                self.push_scope(Scope::new(ScopeKind::Class))
            }
            StmtKind::Import { names } => {
                if self
                    .settings
                    .select
                    .contains(CheckKind::ModuleImportNotAtTopOfFile.code())
                    && self.seen_non_import
                    && stmt.location.column() == 1
                {
                    self.checks.push(Check::new(
                        CheckKind::ModuleImportNotAtTopOfFile,
                        stmt.location,
                    ));
                }

                for alias in names {
                    if alias.node.name.contains('.') && alias.node.asname.is_none() {
                        // TODO(charlie): Multiple submodule imports with the same parent module
                        // will be merged into a single binding.
                        self.add_binding(
                            alias.node.name.split('.').next().unwrap().to_string(),
                            Binding {
                                kind: BindingKind::SubmoduleImportation(
                                    alias.node.name.to_string(),
                                ),
                                used: None,
                                location: stmt.location,
                            },
                        )
                    } else {
                        self.add_binding(
                            alias
                                .node
                                .asname
                                .clone()
                                .unwrap_or_else(|| alias.node.name.clone()),
                            Binding {
                                kind: BindingKind::Importation(
                                    alias
                                        .node
                                        .asname
                                        .clone()
                                        .unwrap_or_else(|| alias.node.name.clone()),
                                ),
                                used: None,
                                location: stmt.location,
                            },
                        )
                    }
                }
            }
            StmtKind::ImportFrom { names, module, .. } => {
                if self
                    .settings
                    .select
                    .contains(CheckKind::ModuleImportNotAtTopOfFile.code())
                    && self.seen_non_import
                    && stmt.location.column() == 1
                {
                    self.checks.push(Check::new(
                        CheckKind::ModuleImportNotAtTopOfFile,
                        stmt.location,
                    ));
                }

                for alias in names {
                    let name = alias
                        .node
                        .asname
                        .clone()
                        .unwrap_or_else(|| alias.node.name.clone());
                    if let Some("__future__") = module.as_deref() {
                        self.add_binding(
                            name,
                            Binding {
                                kind: BindingKind::FutureImportation,
                                used: Some(
                                    self.scopes[*(self
                                        .scope_stack
                                        .last()
                                        .expect("No current scope found."))]
                                    .id,
                                ),
                                location: stmt.location,
                            },
                        );
                    } else if alias.node.name == "*" {
                        self.add_binding(
                            name,
                            Binding {
                                kind: BindingKind::StarImportation,
                                used: None,
                                location: stmt.location,
                            },
                        );

                        if self
                            .settings
                            .select
                            .contains(CheckKind::ImportStarUsage.code())
                        {
                            self.checks
                                .push(Check::new(CheckKind::ImportStarUsage, stmt.location));
                        }
                    } else {
                        let binding = Binding {
                            kind: BindingKind::Importation(match module {
                                None => name.clone(),
                                Some(parent) => format!("{}.{}", parent, name.clone()),
                            }),
                            used: None,
                            location: stmt.location,
                        };
                        self.add_binding(name, binding)
                    }
                }
            }
            StmtKind::If { test, .. } => {
                if self.settings.select.contains(CheckKind::IfTuple.code()) {
                    if let ExprKind::Tuple { elts, .. } = &test.node {
                        if !elts.is_empty() {
                            self.checks
                                .push(Check::new(CheckKind::IfTuple, stmt.location));
                        }
                    }
                }
            }
            StmtKind::Raise { exc, .. } => {
                if self
                    .settings
                    .select
                    .contains(CheckKind::RaiseNotImplemented.code())
                {
                    if let Some(expr) = exc {
                        match &expr.node {
                            ExprKind::Call { func, .. } => {
                                if let ExprKind::Name { id, .. } = &func.node {
                                    if id == "NotImplemented" {
                                        self.checks.push(Check::new(
                                            CheckKind::RaiseNotImplemented,
                                            stmt.location,
                                        ));
                                    }
                                }
                            }
                            ExprKind::Name { id, .. } => {
                                if id == "NotImplemented" {
                                    self.checks.push(Check::new(
                                        CheckKind::RaiseNotImplemented,
                                        stmt.location,
                                    ));
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            StmtKind::AugAssign { target, .. } => {
                self.seen_non_import = true;
                self.handle_node_load(target);
            }
            StmtKind::Assert { test, .. } => {
                self.seen_non_import = true;
                if self.settings.select.contains(CheckKind::AssertTuple.code()) {
                    if let ExprKind::Tuple { elts, .. } = &test.node {
                        if !elts.is_empty() {
                            self.checks
                                .push(Check::new(CheckKind::AssertTuple, stmt.location));
                        }
                    }
                }
            }
            StmtKind::Try { handlers, .. } => {
                if self
                    .settings
                    .select
                    .contains(CheckKind::DefaultExceptNotLast.code())
                {
                    for (idx, handler) in handlers.iter().enumerate() {
                        let ExcepthandlerKind::ExceptHandler { type_, .. } = &handler.node;
                        if type_.is_none() && idx < handlers.len() - 1 {
                            self.checks.push(Check::new(
                                CheckKind::DefaultExceptNotLast,
                                handler.location,
                            ));
                        }
                    }
                }
            }
            StmtKind::Expr { value } => {
                if !self.seen_docstring {
                    if let ExprKind::Constant {
                        value: Constant::Str(_),
                        ..
                    } = &value.node
                    {
                        self.seen_docstring = true;
                    }
                } else {
                    self.seen_non_import = true;
                }
            }
            StmtKind::Assign { value, .. } => {
                self.seen_non_import = true;
                if self
                    .settings
                    .select
                    .contains(CheckKind::DoNotAssignLambda.code())
                {
                    if let ExprKind::Lambda { .. } = &value.node {
                        self.checks
                            .push(Check::new(CheckKind::DoNotAssignLambda, stmt.location));
                    }
                }
            }
            StmtKind::AnnAssign { value, .. } => {
                self.seen_non_import = true;
                if self
                    .settings
                    .select
                    .contains(CheckKind::DoNotAssignLambda.code())
                {
                    if let Some(v) = value {
                        if let ExprKind::Lambda { .. } = v.node {
                            self.checks
                                .push(Check::new(CheckKind::DoNotAssignLambda, stmt.location));
                        }
                    }
                }
            }
            StmtKind::Delete { .. } => {
                self.seen_non_import = true;
            }
            _ => {}
        }

        // Recurse.
        match &stmt.node {
            StmtKind::FunctionDef { .. } | StmtKind::AsyncFunctionDef { .. } => {
                self.deferred_functions
                    .push((stmt, self.scope_stack.clone()));
            }
            StmtKind::ClassDef { body, .. } => {
                for stmt in body {
                    self.visit_stmt(stmt);
                }
            }
            _ => visitor::walk_stmt(self, stmt),
        };

        // Post-visit.
        if let StmtKind::ClassDef { name, .. } = &stmt.node {
            self.pop_scope();
            self.add_binding(
                name.to_string(),
                Binding {
                    kind: BindingKind::ClassDefinition,
                    used: None,
                    location: stmt.location,
                },
            );
        };

        self.parents.pop();
    }

    fn visit_annotation(&mut self, expr: &'b Expr) {
        let initial = self.in_annotation;
        self.in_annotation = true;
        self.visit_expr(expr);
        self.in_annotation = initial;
    }

    fn visit_expr(&mut self, expr: &'b Expr) {
        let initial = self.in_f_string;

        // Pre-visit.
        match &expr.node {
            ExprKind::Name { ctx, .. } => match ctx {
                ExprContext::Load => self.handle_node_load(expr),
                ExprContext::Store => {
                    let parent = self.parents.pop();
                    self.handle_node_store(expr, parent);
                    if let Some(parent) = parent {
                        self.parents.push(parent);
                    }
                }
                ExprContext::Del => self.handle_node_delete(expr),
            },
            ExprKind::Call { func, .. } => {
                if self.settings.select.contains(&CheckCode::R002) {
                    if let ExprKind::Attribute { value, attr, .. } = &func.node {
                        if attr == "assertEquals" {
                            if let ExprKind::Name { id, .. } = &value.node {
                                if id == "self" {
                                    let mut check =
                                        Check::new(CheckKind::NoAssertEquals, expr.location);
                                    if matches!(self.autofix, autofix::Mode::Generate)
                                        || matches!(self.autofix, autofix::Mode::Apply)
                                    {
                                        check.amend(Fix {
                                            content: "assertEqual".to_string(),
                                            start: Location::new(
                                                func.location.row(),
                                                func.location.column() + 1,
                                            ),
                                            end: Location::new(
                                                func.location.row(),
                                                func.location.column() + 1 + "assertEquals".len(),
                                            ),
                                            applied: false,
                                        });
                                    }
                                    self.checks.push(check);
                                }
                            }
                        }
                    }
                }
            }

            ExprKind::Dict { keys, .. } => {
                if self.settings.select.contains(&CheckCode::F601)
                    || self.settings.select.contains(&CheckCode::F602)
                {
                    let num_keys = keys.len();
                    for i in 0..num_keys {
                        let k1 = &keys[i];
                        let v1 = convert_to_value(k1);
                        for k2 in keys.iter().take(num_keys).skip(i + 1) {
                            let v2 = convert_to_value(k2);
                            match (&v1, &v2) {
                                (
                                    Some(DictionaryKey::Constant(v1)),
                                    Some(DictionaryKey::Constant(v2)),
                                ) => {
                                    if self.settings.select.contains(&CheckCode::F601) && v1 == v2 {
                                        self.checks.push(Check::new(
                                            CheckKind::MultiValueRepeatedKeyLiteral,
                                            k2.location,
                                        ))
                                    }
                                }
                                (
                                    Some(DictionaryKey::Variable(v1)),
                                    Some(DictionaryKey::Variable(v2)),
                                ) => {
                                    if self.settings.select.contains(&CheckCode::F602) && v1 == v2 {
                                        self.checks.push(Check::new(
                                            CheckKind::MultiValueRepeatedKeyVariable(
                                                v2.to_string(),
                                            ),
                                            k2.location,
                                        ))
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
            ExprKind::GeneratorExp { .. }
            | ExprKind::ListComp { .. }
            | ExprKind::DictComp { .. }
            | ExprKind::SetComp { .. } => self.push_scope(Scope::new(ScopeKind::Generator)),
            ExprKind::Yield { .. } | ExprKind::YieldFrom { .. } => {
                let scope =
                    &self.scopes[*(self.scope_stack.last().expect("No current scope found."))];
                if self
                    .settings
                    .select
                    .contains(CheckKind::YieldOutsideFunction.code())
                    && matches!(scope.kind, ScopeKind::Class)
                    || matches!(scope.kind, ScopeKind::Module)
                {
                    self.checks
                        .push(Check::new(CheckKind::YieldOutsideFunction, expr.location));
                }
            }
            ExprKind::JoinedStr { values } => {
                if !self.in_f_string
                    && self
                        .settings
                        .select
                        .contains(CheckKind::FStringMissingPlaceholders.code())
                    && !values
                        .iter()
                        .any(|value| matches!(value.node, ExprKind::FormattedValue { .. }))
                {
                    self.checks.push(Check::new(
                        CheckKind::FStringMissingPlaceholders,
                        expr.location,
                    ));
                }
                self.in_f_string = true;
            }
            ExprKind::UnaryOp { op, operand } => {
                if matches!(op, Unaryop::Not) {
                    if let ExprKind::Compare { ops, .. } = &operand.node {
                        match ops[..] {
                            [Cmpop::In] => {
                                if self.settings.select.contains(CheckKind::NotInTest.code()) {
                                    self.checks
                                        .push(Check::new(CheckKind::NotInTest, operand.location));
                                }
                            }
                            [Cmpop::Is] => {
                                if self.settings.select.contains(CheckKind::NotIsTest.code()) {
                                    self.checks
                                        .push(Check::new(CheckKind::NotIsTest, operand.location));
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            ExprKind::Compare {
                left,
                ops,
                comparators,
            } => {
                let op = ops.first().unwrap();
                let comparator = left;

                // Check `left`.
                if self.settings.select.contains(&CheckCode::E711)
                    && matches!(
                        comparator.node,
                        ExprKind::Constant {
                            value: Constant::None,
                            kind: None
                        }
                    )
                {
                    if matches!(op, Cmpop::Eq) {
                        self.checks.push(Check::new(
                            CheckKind::NoneComparison(RejectedCmpop::Eq),
                            comparator.location,
                        ));
                    }
                    if matches!(op, Cmpop::NotEq) {
                        self.checks.push(Check::new(
                            CheckKind::NoneComparison(RejectedCmpop::NotEq),
                            comparator.location,
                        ));
                    }
                }

                if self.settings.select.contains(&CheckCode::E712) {
                    if let ExprKind::Constant {
                        value: Constant::Bool(value),
                        kind: None,
                    } = comparator.node
                    {
                        if matches!(op, Cmpop::Eq) {
                            self.checks.push(Check::new(
                                CheckKind::TrueFalseComparison(value, RejectedCmpop::Eq),
                                comparator.location,
                            ));
                        }
                        if matches!(op, Cmpop::NotEq) {
                            self.checks.push(Check::new(
                                CheckKind::TrueFalseComparison(value, RejectedCmpop::NotEq),
                                comparator.location,
                            ));
                        }
                    }
                }

                // Check each comparator in order.
                for (op, comparator) in izip!(ops, comparators) {
                    if self.settings.select.contains(&CheckCode::E711)
                        && matches!(
                            comparator.node,
                            ExprKind::Constant {
                                value: Constant::None,
                                kind: None
                            }
                        )
                    {
                        if matches!(op, Cmpop::Eq) {
                            self.checks.push(Check::new(
                                CheckKind::NoneComparison(RejectedCmpop::Eq),
                                comparator.location,
                            ));
                        }
                        if matches!(op, Cmpop::NotEq) {
                            self.checks.push(Check::new(
                                CheckKind::NoneComparison(RejectedCmpop::NotEq),
                                comparator.location,
                            ));
                        }
                    }

                    if self.settings.select.contains(&CheckCode::E712) {
                        if let ExprKind::Constant {
                            value: Constant::Bool(value),
                            kind: None,
                        } = comparator.node
                        {
                            if matches!(op, Cmpop::Eq) {
                                self.checks.push(Check::new(
                                    CheckKind::TrueFalseComparison(value, RejectedCmpop::Eq),
                                    comparator.location,
                                ));
                            }
                            if matches!(op, Cmpop::NotEq) {
                                self.checks.push(Check::new(
                                    CheckKind::TrueFalseComparison(value, RejectedCmpop::NotEq),
                                    comparator.location,
                                ));
                            }
                        }
                    }
                }
            }
            ExprKind::Constant {
                value: Constant::Str(value),
                ..
            } if self.in_annotation => self.deferred_annotations.push(value),
            _ => {}
        };

        // Recurse.
        match &expr.node {
            ExprKind::Lambda { .. } => {
                self.deferred_lambdas.push((expr, self.scope_stack.clone()));
            }
            _ => visitor::walk_expr(self, expr),
        }

        // Post-visit.
        match &expr.node {
            ExprKind::GeneratorExp { .. }
            | ExprKind::ListComp { .. }
            | ExprKind::DictComp { .. }
            | ExprKind::SetComp { .. } => {
                self.pop_scope();
            }
            ExprKind::JoinedStr { .. } => {
                self.in_f_string = initial;
            }
            _ => {}
        };
    }

    fn visit_excepthandler(&mut self, excepthandler: &'b Excepthandler) {
        match &excepthandler.node {
            ExcepthandlerKind::ExceptHandler { name, .. } => match name {
                Some(name) => {
                    let scope =
                        &self.scopes[*(self.scope_stack.last().expect("No current scope found."))];
                    if scope.values.contains_key(name) {
                        let parent = self.parents.pop();
                        self.handle_node_store(
                            &Expr::new(
                                excepthandler.location,
                                ExprKind::Name {
                                    id: name.to_string(),
                                    ctx: ExprContext::Store,
                                },
                            ),
                            parent,
                        );
                        if let Some(parent) = parent {
                            self.parents.push(parent);
                        }
                    }

                    let parent = self.parents.pop();
                    let scope =
                        &self.scopes[*(self.scope_stack.last().expect("No current scope found."))];
                    let definition = scope.values.get(name).cloned();
                    self.handle_node_store(
                        &Expr::new(
                            excepthandler.location,
                            ExprKind::Name {
                                id: name.to_string(),
                                ctx: ExprContext::Store,
                            },
                        ),
                        parent,
                    );
                    if let Some(parent) = parent {
                        self.parents.push(parent);
                    }

                    walk_excepthandler(self, excepthandler);

                    let scope = &mut self.scopes
                        [*(self.scope_stack.last().expect("No current scope found."))];
                    if let Some(binding) = &scope.values.remove(name) {
                        if self.settings.select.contains(&CheckCode::F841) && binding.used.is_none()
                        {
                            self.checks.push(Check::new(
                                CheckKind::UnusedVariable(name.to_string()),
                                excepthandler.location,
                            ));
                        }
                    }

                    if let Some(binding) = definition {
                        scope.values.insert(name.to_string(), binding);
                    }
                }
                None => walk_excepthandler(self, excepthandler),
            },
        }
    }

    fn visit_arguments(&mut self, arguments: &'b Arguments) {
        if self
            .settings
            .select
            .contains(CheckKind::DuplicateArgumentName.code())
        {
            // Collect all the arguments into a single vector.
            let mut all_arguments: Vec<&Arg> = arguments
                .args
                .iter()
                .chain(arguments.posonlyargs.iter())
                .chain(arguments.kwonlyargs.iter())
                .collect();
            if let Some(arg) = &arguments.vararg {
                all_arguments.push(arg);
            }
            if let Some(arg) = &arguments.kwarg {
                all_arguments.push(arg);
            }

            // Search for duplicates.
            let mut idents: BTreeSet<&str> = BTreeSet::new();
            for arg in all_arguments {
                let ident = &arg.node.arg;
                if idents.contains(ident.as_str()) {
                    self.checks
                        .push(Check::new(CheckKind::DuplicateArgumentName, arg.location));
                    break;
                }
                idents.insert(ident);
            }
        }

        visitor::walk_arguments(self, arguments);
    }

    fn visit_arg(&mut self, arg: &'b Arg) {
        self.add_binding(
            arg.node.arg.to_string(),
            Binding {
                kind: BindingKind::Argument,
                used: None,
                location: arg.location,
            },
        );
        visitor::walk_arg(self, arg);
    }
}

impl<'a> Checker<'a> {
    fn push_scope(&mut self, scope: Scope) {
        self.scope_stack.push(self.scopes.len());
        self.scopes.push(scope);
    }

    fn pop_scope(&mut self) {
        self.dead_scopes.push(
            self.scope_stack
                .pop()
                .expect("Attempted to pop without scope."),
        );
    }

    fn bind_builtins(&mut self) {
        let scope = &mut self.scopes[*(self.scope_stack.last().expect("No current scope found."))];

        for builtin in BUILTINS {
            scope.values.insert(
                builtin.to_string(),
                Binding {
                    kind: BindingKind::Builtin,
                    location: Default::default(),
                    used: None,
                },
            );
        }
        for builtin in MAGIC_GLOBALS {
            scope.values.insert(
                builtin.to_string(),
                Binding {
                    kind: BindingKind::Builtin,
                    location: Default::default(),
                    used: None,
                },
            );
        }
    }

    fn add_binding(&mut self, name: String, binding: Binding) {
        let scope = &mut self.scopes[*(self.scope_stack.last().expect("No current scope found."))];

        // TODO(charlie): Don't treat annotations as assignments if there is an existing value.
        let binding = match scope.values.get(&name) {
            None => binding,
            Some(existing) => Binding {
                kind: binding.kind,
                location: binding.location,
                used: existing.used,
            },
        };
        scope.values.insert(name, binding);
    }

    fn handle_node_load(&mut self, expr: &Expr) {
        if let ExprKind::Name { id, .. } = &expr.node {
            let scope_id =
                self.scopes[*(self.scope_stack.last().expect("No current scope found."))].id;

            let mut first_iter = true;
            let mut in_generator = false;
            for scope_index in self.scope_stack.iter().rev() {
                let scope = &mut self.scopes[*scope_index];
                if matches!(scope.kind, ScopeKind::Class) {
                    if id == "__class__" {
                        return;
                    } else if !first_iter && !in_generator {
                        continue;
                    }
                }
                if let Some(binding) = scope.values.get_mut(id) {
                    binding.used = Some(scope_id);
                    return;
                }

                first_iter = false;
                in_generator = matches!(scope.kind, ScopeKind::Generator);
            }

            if self.settings.select.contains(&CheckCode::F821) {
                self.checks.push(Check::new(
                    CheckKind::UndefinedName(id.clone()),
                    expr.location,
                ))
            }
        }
    }

    fn handle_node_store(&mut self, expr: &Expr, parent: Option<&Stmt>) {
        if let ExprKind::Name { id, .. } = &expr.node {
            let current =
                &self.scopes[*(self.scope_stack.last().expect("No current scope found."))];

            if self.settings.select.contains(&CheckCode::F823)
                && matches!(current.kind, ScopeKind::Function)
                && !current.values.contains_key(id)
            {
                for scope in self.scopes.iter().rev().skip(1) {
                    if matches!(scope.kind, ScopeKind::Function)
                        || matches!(scope.kind, ScopeKind::Module)
                    {
                        let used = scope
                            .values
                            .get(id)
                            .map(|binding| binding.used)
                            .unwrap_or_default();
                        if let Some(scope_id) = used {
                            if scope_id == current.id {
                                self.checks.push(Check::new(
                                    CheckKind::UndefinedLocal(id.clone()),
                                    expr.location,
                                ));
                            }
                        }
                    }
                }
            }

            // TODO(charlie): Handle alternate binding types (like `Annotation`).
            if id == "__all__"
                && matches!(current.kind, ScopeKind::Module)
                && parent
                    .map(|stmt| {
                        matches!(stmt.node, StmtKind::Assign { .. })
                            || matches!(stmt.node, StmtKind::AugAssign { .. })
                            || matches!(stmt.node, StmtKind::AnnAssign { .. })
                    })
                    .unwrap_or_default()
            {
                // Really need parent here.
                self.add_binding(
                    id.to_string(),
                    Binding {
                        kind: BindingKind::Export(extract_all_names(parent.unwrap(), current)),
                        used: None,
                        location: expr.location,
                    },
                );
            } else {
                self.add_binding(
                    id.to_string(),
                    Binding {
                        kind: BindingKind::Assignment,
                        used: None,
                        location: expr.location,
                    },
                );
            }
        }
    }

    fn handle_node_delete(&mut self, expr: &Expr) {
        if let ExprKind::Name { id, .. } = &expr.node {
            let scope =
                &mut self.scopes[*(self.scope_stack.last().expect("No current scope found."))];
            if scope.values.remove(id).is_none() && self.settings.select.contains(&CheckCode::F821)
            {
                self.checks.push(Check::new(
                    CheckKind::UndefinedName(id.clone()),
                    expr.location,
                ))
            }
        }
    }

    fn check_deferred_annotations<'b>(&mut self, path: &str, allocator: &'b mut Vec<Expr>)
    where
        'b: 'a,
    {
        while !self.deferred_annotations.is_empty() {
            let value = self.deferred_annotations.pop().unwrap();
            if let Ok(expr) = parser::parse_expression(value, path) {
                allocator.push(expr);
            }
        }
        for expr in allocator {
            self.visit_expr(expr);
        }
    }

    fn check_deferred_functions(&mut self) {
        while !self.deferred_functions.is_empty() {
            let (stmt, scopes) = self.deferred_functions.pop().unwrap();

            self.scope_stack = scopes;
            self.push_scope(Scope::new(ScopeKind::Function));

            match &stmt.node {
                StmtKind::FunctionDef { body, args, .. }
                | StmtKind::AsyncFunctionDef { body, args, .. } => {
                    self.visit_arguments(args);
                    for stmt in body {
                        self.visit_stmt(stmt);
                    }
                }
                _ => {}
            }

            let scope = &self.scopes[*(self.scope_stack.last().expect("No current scope found."))];
            for (name, binding) in scope.values.iter() {
                // TODO(charlie): Ignore if using `locals`.
                if self.settings.select.contains(&CheckCode::F841)
                    && binding.used.is_none()
                    && name != "_"
                    && name != "__tracebackhide__"
                    && name != "__traceback_info__"
                    && name != "__traceback_supplement__"
                    && matches!(binding.kind, BindingKind::Assignment)
                {
                    self.checks.push(Check::new(
                        CheckKind::UnusedVariable(name.to_string()),
                        binding.location,
                    ));
                }
            }

            self.pop_scope();
        }
    }

    fn check_deferred_lambdas(&mut self) {
        while !self.deferred_lambdas.is_empty() {
            let (expr, scopes) = self.deferred_lambdas.pop().unwrap();

            self.scope_stack = scopes;
            self.push_scope(Scope::new(ScopeKind::Function));

            if let ExprKind::Lambda { args, body } = &expr.node {
                self.visit_arguments(args);
                self.visit_expr(body);
            }

            let scope = &self.scopes[*(self.scope_stack.last().expect("No current scope found."))];
            for (name, binding) in scope.values.iter() {
                // TODO(charlie): Ignore if using `locals`.
                if self.settings.select.contains(&CheckCode::F841)
                    && binding.used.is_none()
                    && name != "_"
                    && name != "__tracebackhide__"
                    && name != "__traceback_info__"
                    && name != "__traceback_supplement__"
                    && matches!(binding.kind, BindingKind::Assignment)
                {
                    self.checks.push(Check::new(
                        CheckKind::UnusedVariable(name.to_string()),
                        binding.location,
                    ));
                }
            }

            self.pop_scope();
        }
    }

    fn check_dead_scopes(&mut self) {
        if !self.settings.select.contains(&CheckCode::F822)
            && !self.settings.select.contains(&CheckCode::F401)
        {
            return;
        }

        for index in self.dead_scopes.clone() {
            let scope = &self.scopes[index];

            let all_binding = scope.values.get("__all__");
            let all_names = all_binding.and_then(|binding| match &binding.kind {
                BindingKind::Export(names) => Some(names),
                _ => None,
            });

            if self.settings.select.contains(&CheckCode::F822)
                && !Path::new(self.path).ends_with("__init__.py")
            {
                if let Some(binding) = all_binding {
                    if let Some(names) = all_names {
                        for name in names {
                            if !scope.values.contains_key(name) {
                                self.checks.push(Check::new(
                                    CheckKind::UndefinedExport(name.to_string()),
                                    binding.location,
                                ));
                            }
                        }
                    }
                }
            }

            if self.settings.select.contains(&CheckCode::F401) {
                for (name, binding) in scope.values.iter().rev() {
                    let used = binding.used.is_some()
                        || all_names
                            .map(|names| names.contains(name))
                            .unwrap_or_default();

                    if !used {
                        match &binding.kind {
                            BindingKind::Importation(full_name)
                            | BindingKind::SubmoduleImportation(full_name) => {
                                self.checks.push(Check::new(
                                    CheckKind::UnusedImport(full_name.to_string()),
                                    binding.location,
                                ));
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }
}

pub fn check_ast(
    python_ast: &Suite,
    content: &str,
    settings: &Settings,
    autofix: &autofix::Mode,
    path: &str,
) -> Vec<Check> {
    let mut checker = Checker::new(settings, autofix, path, content);
    checker.push_scope(Scope::new(ScopeKind::Module));
    checker.bind_builtins();

    // Iterate over the AST.
    for stmt in python_ast {
        checker.visit_stmt(stmt);
    }

    // Check any deferred statements.
    let mut allocator = vec![];
    checker.check_deferred_annotations(path, &mut allocator);
    checker.check_deferred_functions();
    checker.check_deferred_lambdas();

    // Reset the scope to module-level, and check all consumed scopes.
    checker.scope_stack = vec![GLOBAL_SCOPE_INDEX];
    checker.pop_scope();
    checker.check_dead_scopes();

    checker.checks
}
