use rustc_hash::FxHashSet;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{self as ast, Expr, Stmt};
use ruff_text_size::{Ranged, TextRange};

use crate::Violation;
use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for attributes that are defined outside the allowed defining methods.
///
/// ## Why is this bad?
/// Attributes should be defined in specific methods (like `__init__`) to make the object's structure
/// clear and predictable. Defining attributes outside these designated methods can make the
/// code harder to understand and maintain, as the instance attributes are not
/// immediately visible when the object is created.
///
/// ## Known problems
/// This rule can't detect attribute definitions in superclasses, and
/// so limits its analysis to classes that inherit from (at most) `object`.
/// This rule also ignores decorated classes since decorators may define
/// attributes dynamically.
///
/// ## Example
/// ```python
/// class Student:
///     def register(self):
///         self.is_registered = True  # [attribute-defined-outside-init]
/// ```
///
/// Use instead:
/// ```python
/// class Student:
///     def __init__(self):
///         self.is_registered = False
///
///     def register(self):
///         self.is_registered = True
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct AttributeDefinedOutsideInit {
    name: String,
}

impl Violation for AttributeDefinedOutsideInit {
    #[derive_message_formats]
    fn message(&self) -> String {
        let AttributeDefinedOutsideInit { name } = self;
        format!("Attribute `{name}` defined outside `__init__`")
    }
}

/// PLW0201
pub(crate) fn attribute_defined_outside_init(checker: &Checker, class_def: &ast::StmtClassDef) {
    let semantic = checker.semantic();

    // Skip if the class inherits from another class (aside from `object`), as the parent
    // class might define the attribute.
    if !class_def
        .bases()
        .iter()
        .all(|base| semantic.match_builtin_expr(base, "object"))
    {
        return;
    }

    // Skip if the class has decorators, as decorators might define attributes.
    if !class_def.decorator_list.is_empty() {
        return;
    }

    for attribute in find_attributes_defined_outside_init(
        &class_def.body,
        &checker.settings.pylint.defining_attr_methods,
    ) {
        checker.report_diagnostic(
            AttributeDefinedOutsideInit {
                name: attribute.name.to_string(),
            },
            attribute.range(),
        );
    }
}

#[derive(Debug)]
struct AttributeAssignment<'a> {
    /// The name of the attribute that is assigned to.
    name: &'a str,
    /// The range of the attribute that is assigned to.
    range: TextRange,
}

impl Ranged for AttributeAssignment<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}

/// Find attributes that are defined outside the allowed defining methods.
fn find_attributes_defined_outside_init<'a>(
    body: &'a [Stmt],
    defining_attr_methods: &[String],
) -> Vec<AttributeAssignment<'a>> {
    // First, expand the set of defining methods to include any methods called from them
    let expanded_defining_methods = expand_defining_methods(body, defining_attr_methods);

    // Then, collect all attributes that are defined in allowed defining methods.
    let mut allowed_attributes = FxHashSet::default();
    for statement in body {
        let Stmt::FunctionDef(ast::StmtFunctionDef { name, body, .. }) = statement else {
            continue;
        };

        if expanded_defining_methods.contains(name.as_str()) {
            collect_self_attributes(body, &mut allowed_attributes);
        }
    }

    // Also collect property setter method names.
    let mut property_setters = FxHashSet::default();
    for statement in body {
        let Stmt::FunctionDef(ast::StmtFunctionDef {
            name: _,
            decorator_list,
            ..
        }) = statement
        else {
            continue;
        };

        for decorator in decorator_list {
            let Some(ast::ExprAttribute { value, attr, .. }) =
                decorator.expression.as_attribute_expr()
            else {
                continue;
            };

            if attr == "setter" {
                let Some(ast::ExprName { id, .. }) = value.as_name_expr() else {
                    continue;
                };
                property_setters.insert(id.as_str());
            }
        }
    }

    // Also collect attributes defined at the class level (class variables).
    for statement in body {
        match statement {
            // Ex) `attr = value`
            Stmt::Assign(ast::StmtAssign { targets, .. }) => {
                for target in targets {
                    if let Expr::Name(ast::ExprName { id, .. }) = target {
                        allowed_attributes.insert(id.as_str());
                    }
                }
            }
            // Ex) `attr: Type = value`
            Stmt::AnnAssign(ast::StmtAnnAssign { target, .. }) => {
                if let Expr::Name(ast::ExprName { id, .. }) = target.as_ref() {
                    allowed_attributes.insert(id.as_str());
                }
            }
            _ => {}
        }
    }

    // Now, find attributes that are assigned to `self` outside of `__init__`.
    let mut outside_attributes = vec![];
    for statement in body {
        let Stmt::FunctionDef(ast::StmtFunctionDef { name, body, .. }) = statement else {
            continue;
        };

        // Skip allowed defining methods since those are allowed.
        if expanded_defining_methods.contains(name.as_str()) {
            continue;
        }

        // Skip property setters since those are allowed.
        if property_setters.contains(name.as_str()) {
            continue;
        }

        for statement in body {
            match statement {
                // Ex) `self.name = name`
                Stmt::Assign(ast::StmtAssign { targets, .. }) => {
                    for target in targets {
                        if let Expr::Attribute(attribute) = target {
                            if let Expr::Name(ast::ExprName { id, .. }) = attribute.value.as_ref() {
                                if id == "self"
                                    && !allowed_attributes.contains(attribute.attr.as_str())
                                {
                                    outside_attributes.push(AttributeAssignment {
                                        name: &attribute.attr,
                                        range: attribute.range(),
                                    });
                                }
                            }
                        }
                    }
                }

                // Ex) `self.name: str = name`
                Stmt::AnnAssign(ast::StmtAnnAssign { target, .. }) => {
                    if let Expr::Attribute(attribute) = target.as_ref() {
                        if let Expr::Name(ast::ExprName { id, .. }) = attribute.value.as_ref() {
                            if id == "self" && !allowed_attributes.contains(attribute.attr.as_str())
                            {
                                outside_attributes.push(AttributeAssignment {
                                    name: &attribute.attr,
                                    range: attribute.range(),
                                });
                            }
                        }
                    }
                }

                // Ex) `self.name += name`
                Stmt::AugAssign(ast::StmtAugAssign { target, .. }) => {
                    if let Expr::Attribute(attribute) = target.as_ref() {
                        if let Expr::Name(ast::ExprName { id, .. }) = attribute.value.as_ref() {
                            if id == "self" && !allowed_attributes.contains(attribute.attr.as_str())
                            {
                                outside_attributes.push(AttributeAssignment {
                                    name: &attribute.attr,
                                    range: attribute.range(),
                                });
                            }
                        }
                    }
                }

                _ => {}
            }
        }
    }

    outside_attributes
}

/// Expand the set of defining methods to include any methods called from them.
/// This performs a transitive closure to find all methods that are indirectly
/// called from the original defining methods.
fn expand_defining_methods<'a>(
    body: &'a [Stmt],
    initial_methods: &'a [String],
) -> FxHashSet<&'a str> {
    let mut defining_methods: FxHashSet<&str> =
        initial_methods.iter().map(|s| s.as_str()).collect();
    let mut changed = true;

    // Keep expanding until no new methods are found
    while changed {
        changed = false;
        let current_methods = defining_methods.clone();

        // For each function in the class
        for statement in body {
            let Stmt::FunctionDef(ast::StmtFunctionDef { name, body, .. }) = statement else {
                continue;
            };

            // If this method is currently in our defining methods set
            if current_methods.contains(name.as_str()) {
                // Find all methods it calls and add them to the set
                let called_methods = find_called_methods(body);
                for called_method in called_methods {
                    if !defining_methods.contains(called_method) {
                        defining_methods.insert(called_method);
                        changed = true;
                    }
                }
            }
        }
    }

    defining_methods
}

/// Find all methods called on `self` within the given statements.
fn find_called_methods(body: &[Stmt]) -> FxHashSet<&str> {
    let mut called_methods = FxHashSet::default();

    for statement in body {
        collect_method_calls_from_stmt(statement, &mut called_methods);
    }

    called_methods
}

/// Recursively collect method calls on `self` from a statement.
fn collect_method_calls_from_stmt<'a>(stmt: &'a Stmt, called_methods: &mut FxHashSet<&'a str>) {
    match stmt {
        // Expression statements might contain method calls
        Stmt::Expr(ast::StmtExpr { value, .. }) => {
            collect_method_calls_from_expr(value, called_methods);
        }

        // Assignment statements might contain method calls in the value
        Stmt::Assign(ast::StmtAssign { value, .. }) => {
            collect_method_calls_from_expr(value, called_methods);
        }

        // Annotated assignments might contain method calls in the value
        Stmt::AnnAssign(ast::StmtAnnAssign {
            value: Some(value), ..
        }) => {
            collect_method_calls_from_expr(value, called_methods);
        }

        // If statements contain expressions in the test and nested statements
        Stmt::If(ast::StmtIf {
            test,
            body,
            elif_else_clauses,
            ..
        }) => {
            collect_method_calls_from_expr(test, called_methods);
            for stmt in body {
                collect_method_calls_from_stmt(stmt, called_methods);
            }
            for clause in elif_else_clauses {
                for stmt in &clause.body {
                    collect_method_calls_from_stmt(stmt, called_methods);
                }
                if let Some(test) = &clause.test {
                    collect_method_calls_from_expr(test, called_methods);
                }
            }
        }

        // For loops
        Stmt::For(ast::StmtFor {
            iter, body, orelse, ..
        }) => {
            collect_method_calls_from_expr(iter, called_methods);
            for stmt in body {
                collect_method_calls_from_stmt(stmt, called_methods);
            }
            for stmt in orelse {
                collect_method_calls_from_stmt(stmt, called_methods);
            }
        }

        // While loops
        Stmt::While(ast::StmtWhile {
            test, body, orelse, ..
        }) => {
            collect_method_calls_from_expr(test, called_methods);
            for stmt in body {
                collect_method_calls_from_stmt(stmt, called_methods);
            }
            for stmt in orelse {
                collect_method_calls_from_stmt(stmt, called_methods);
            }
        }

        // Try blocks
        Stmt::Try(ast::StmtTry {
            body,
            handlers,
            orelse,
            finalbody,
            ..
        }) => {
            for stmt in body {
                collect_method_calls_from_stmt(stmt, called_methods);
            }
            for handler in handlers {
                match handler {
                    ast::ExceptHandler::ExceptHandler(ast::ExceptHandlerExceptHandler {
                        body,
                        ..
                    }) => {
                        for stmt in body {
                            collect_method_calls_from_stmt(stmt, called_methods);
                        }
                    }
                }
            }
            for stmt in orelse {
                collect_method_calls_from_stmt(stmt, called_methods);
            }
            for stmt in finalbody {
                collect_method_calls_from_stmt(stmt, called_methods);
            }
        }

        // With statements
        Stmt::With(ast::StmtWith { body, .. }) => {
            for stmt in body {
                collect_method_calls_from_stmt(stmt, called_methods);
            }
        }

        _ => {}
    }
}

/// Recursively collect method calls on `self` from an expression.
fn collect_method_calls_from_expr<'a>(expr: &'a Expr, called_methods: &mut FxHashSet<&'a str>) {
    match expr {
        // Method call: self.method_name(...)
        Expr::Call(ast::ExprCall {
            func, arguments, ..
        }) => {
            if let Expr::Attribute(ast::ExprAttribute { value, attr, .. }) = func.as_ref() {
                if let Expr::Name(ast::ExprName { id, .. }) = value.as_ref() {
                    if id == "self" {
                        called_methods.insert(attr.as_str());
                    }
                }
            }

            // Also check arguments for nested method calls
            collect_method_calls_from_expr(func, called_methods);
            for arg in &arguments.args {
                collect_method_calls_from_expr(arg, called_methods);
            }
            for keyword in &arguments.keywords {
                collect_method_calls_from_expr(&keyword.value, called_methods);
            }
        }

        // Binary operations
        Expr::BinOp(ast::ExprBinOp { left, right, .. }) => {
            collect_method_calls_from_expr(left, called_methods);
            collect_method_calls_from_expr(right, called_methods);
        }

        // Unary operations
        Expr::UnaryOp(ast::ExprUnaryOp { operand, .. }) => {
            collect_method_calls_from_expr(operand, called_methods);
        }

        // Conditional expressions
        Expr::If(ast::ExprIf {
            test, body, orelse, ..
        }) => {
            collect_method_calls_from_expr(test, called_methods);
            collect_method_calls_from_expr(body, called_methods);
            collect_method_calls_from_expr(orelse, called_methods);
        }

        // Attribute access (might be part of a larger expression)
        Expr::Attribute(ast::ExprAttribute { value, .. }) => {
            collect_method_calls_from_expr(value, called_methods);
        }

        // Lists, tuples, sets might contain method calls
        Expr::List(ast::ExprList { elts, .. })
        | Expr::Tuple(ast::ExprTuple { elts, .. })
        | Expr::Set(ast::ExprSet { elts, .. }) => {
            for elt in elts {
                collect_method_calls_from_expr(elt, called_methods);
            }
        }

        // Dictionaries
        Expr::Dict(ast::ExprDict { items, .. }) => {
            for item in items {
                if let Some(key) = &item.key {
                    collect_method_calls_from_expr(key, called_methods);
                }
                collect_method_calls_from_expr(&item.value, called_methods);
            }
        }

        _ => {}
    }
}

/// Collect all attributes that are assigned to `self` in the given statements.
fn collect_self_attributes<'a>(body: &'a [Stmt], attributes: &mut FxHashSet<&'a str>) {
    for statement in body {
        match statement {
            // Ex) `self.name = name`
            Stmt::Assign(ast::StmtAssign { targets, .. }) => {
                for target in targets {
                    if let Expr::Attribute(attribute) = target {
                        if let Expr::Name(ast::ExprName { id, .. }) = attribute.value.as_ref() {
                            if id == "self" {
                                attributes.insert(&attribute.attr);
                            }
                        }
                    }
                }
            }

            // Ex) `self.name: str = name`
            Stmt::AnnAssign(ast::StmtAnnAssign { target, .. }) => {
                if let Expr::Attribute(attribute) = target.as_ref() {
                    if let Expr::Name(ast::ExprName { id, .. }) = attribute.value.as_ref() {
                        if id == "self" {
                            attributes.insert(&attribute.attr);
                        }
                    }
                }
            }

            // Ex) `self.name += name`
            Stmt::AugAssign(ast::StmtAugAssign { target, .. }) => {
                if let Expr::Attribute(attribute) = target.as_ref() {
                    if let Expr::Name(ast::ExprName { id, .. }) = attribute.value.as_ref() {
                        if id == "self" {
                            attributes.insert(&attribute.attr);
                        }
                    }
                }
            }

            _ => {}
        }
    }
}
