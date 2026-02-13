use std::collections::{HashMap, HashSet};

use crate::Db;
use crate::declare_lint;
use crate::lint::{Level, LintStatus};
use crate::place::{ConsideredDefinitions, RequiresExplicitReExport, place_by_id};
use crate::semantic_index::definition::Definition;
use crate::semantic_index::place::PlaceExpr;
use crate::semantic_index::{place_table, semantic_index, use_def_map};
use crate::types::StaticClassLiteral;
use crate::types::Type;
use ruff_db::files::File;
use ruff_db::parsed::parsed_module;
use ruff_db::source::source_text;
use ruff_python_ast::{self as ast, Expr, ExprCall};
use ruff_text_size::{Ranged, TextRange};
use ty_module_resolver::{ModuleName, file_to_module, list_modules, resolve_module};

declare_lint! {
    /// ## What it does
    /// Checks for Blender dynamic property definitions outside the `register()` function scope.
    ///
    /// ## Why is this bad?
    /// Blender properties should only be registered from the `register()` function
    /// (or functions it calls) in the project root `__init__.py`. Properties defined
    /// elsewhere will not be recognized by the type checker.
    ///
    /// ## Example
    /// ```python
    /// # Bad: property defined at module top level
    /// bpy.types.Scene.my_prop = bpy.props.StringProperty()
    ///
    /// # Good: property defined inside register()
    /// def register():
    ///     bpy.types.Scene.my_prop = bpy.props.StringProperty()
    /// ```
    pub(crate) static BLENDER_PROPERTY_OUTSIDE_REGISTER = {
        summary: "detects Blender property definitions outside register() scope",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

/// Checks if expression is a call to a Blender property
///  (e.g. `IntProperty(name="foo", default=0)`)
///  and returns the call expression if so.
/// It just compares the function name to known Blender property names,
/// but it is unlikely that Blender changes which properties are available,
/// so this check should be enough for all Blender versions.
pub(crate) fn as_blender_property(annotation_expr: &Expr) -> Option<&ExprCall> {
    match annotation_expr.as_call_expr() {
        Some(call_expr) => {
            let func = &call_expr.func;
            let func_name = if func.is_name_expr() {
                func.as_name_expr().unwrap().id.as_str()
            } else if func.is_attribute_expr() {
                func.as_attribute_expr().unwrap().attr.as_str()
            } else {
                "<unknown>"
            };
            match func_name {
                // Only allow Blender properties, not arbitrary call expressions.
                "BoolProperty"
                | "BoolVectorProperty"
                | "CollectionProperty"
                | "EnumProperty"
                | "FloatProperty"
                | "FloatVectorProperty"
                | "IntProperty"
                | "IntVectorProperty"
                | "PointerProperty"
                | "RemoveProperty"
                | "StringProperty" => Some(call_expr),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Formats the call expression as a code block, so it can be displayed as a docstring on hover.
pub(crate) fn get_call_expression_docstring(
    call_expr: &ExprCall,
    db: &dyn Db,
    file: File,
) -> String {
    let source = source_text(db, file);

    // Print arguments one-per-line when there is more than one argument.
    let args_len = call_expr.arguments.args.len() + call_expr.arguments.keywords.len();
    let mut call_docstring = String::new();
    call_docstring.push_str("```python\n");
    match args_len {
        0 => {
            // Print on one line.
            call_docstring.push_str(&source[call_expr.func.range()]);
            call_docstring.push_str("()");
        }
        1 => {
            // Still print on one line.
            call_docstring.push_str(&source[call_expr.func.range()]);
            call_docstring.push_str("(");
            if call_expr.arguments.args.len() == 1 {
                call_docstring.push_str(&source[call_expr.arguments.args[0].range()]);
            } else {
                call_docstring.push_str(&source[call_expr.arguments.keywords[0].range()]);
            }
            call_docstring.push(')');
        }
        _ => {
            // Print one argument per line.
            call_docstring.push_str(&source[call_expr.func.range()]);
            call_docstring.push_str("(\n");
            for arg in call_expr.arguments.args.iter() {
                call_docstring.push_str("    ");
                call_docstring.push_str(&source[arg.range()]);
                call_docstring.push_str(",\n");
            }
            for kw in call_expr.arguments.keywords.iter() {
                call_docstring.push_str("    ");
                call_docstring.push_str(&source[kw.range()]);
                call_docstring.push_str(",\n");
            }
            call_docstring.push(')');
        }
    }
    call_docstring.push_str("\n```");
    return call_docstring;
}

/// A location of a dynamic Blender property assignment within the register() scope.
#[derive(Debug, Clone, PartialEq, Eq)]
struct PropertyLocation {
    file: File,
    target_range: TextRange,
}

/// Registry of all dynamic Blender property assignments reachable from register().
/// Maps (class_name, prop_name) to the location(s) where they are defined.
/// Cached by Salsa via `blender_property_registry()`.
#[derive(Debug, PartialEq, Eq)]
struct BlenderPropertyRegistry {
    properties: HashMap<(String, String), Vec<PropertyLocation>>,
    /// Secondary index for O(1) `contains()` lookups by (file, range).
    all_locations: HashSet<(File, TextRange)>,
}

impl BlenderPropertyRegistry {
    fn new() -> Self {
        Self {
            properties: HashMap::new(),
            all_locations: HashSet::new(),
        }
    }

    fn add(&mut self, class_name: &str, prop_name: &str, file: File, target_range: TextRange) {
        self.properties
            .entry((class_name.to_string(), prop_name.to_string()))
            .or_default()
            .push(PropertyLocation { file, target_range });
        self.all_locations.insert((file, target_range));
    }

    fn get(&self, class_name: &str, prop_name: &str) -> Option<&Vec<PropertyLocation>> {
        self.properties
            .get(&(class_name.to_string(), prop_name.to_string()))
    }

    fn contains(&self, file: File, range: TextRange) -> bool {
        self.all_locations.contains(&(file, range))
    }
}

/// Collects assignments from a statement list (within a function body),
/// handling nested control flow but NOT recursing into nested function/class definitions.
fn collect_assignments_in_body(stmts: &[ast::Stmt]) -> Vec<&ast::StmtAssign> {
    let mut assignments = Vec::new();

    for stmt in stmts {
        match stmt {
            ast::Stmt::Assign(assign) => {
                assignments.push(assign);
            }
            ast::Stmt::If(if_stmt) => {
                assignments.extend(collect_assignments_in_body(&if_stmt.body));
                for elif in &if_stmt.elif_else_clauses {
                    assignments.extend(collect_assignments_in_body(&elif.body));
                }
            }
            ast::Stmt::With(with_stmt) => {
                assignments.extend(collect_assignments_in_body(&with_stmt.body));
            }
            ast::Stmt::Match(match_stmt) => {
                for case in &match_stmt.cases {
                    assignments.extend(collect_assignments_in_body(&case.body));
                }
            }
            ast::Stmt::For(for_stmt) => {
                assignments.extend(collect_assignments_in_body(&for_stmt.body));
                assignments.extend(collect_assignments_in_body(&for_stmt.orelse));
            }
            ast::Stmt::While(while_stmt) => {
                assignments.extend(collect_assignments_in_body(&while_stmt.body));
                assignments.extend(collect_assignments_in_body(&while_stmt.orelse));
            }
            ast::Stmt::Try(try_stmt) => {
                assignments.extend(collect_assignments_in_body(&try_stmt.body));
                for handler in &try_stmt.handlers {
                    let ast::ExceptHandler::ExceptHandler(handler_inner) = handler;
                    assignments.extend(collect_assignments_in_body(&handler_inner.body));
                }
                assignments.extend(collect_assignments_in_body(&try_stmt.orelse));
                assignments.extend(collect_assignments_in_body(&try_stmt.finalbody));
            }
            _ => {}
        }
    }

    assignments
}

/// Collects all simple function call names from a statement list.
/// Only collects calls like `foo()` or `bar()`, not method calls or complex expressions.
fn collect_function_calls_in_body(stmts: &[ast::Stmt]) -> Vec<String> {
    let mut calls = Vec::new();

    for stmt in stmts {
        match stmt {
            ast::Stmt::Expr(expr_stmt) => {
                if let Some(call) = expr_stmt.value.as_call_expr() {
                    if let Some(name) = call.func.as_name_expr() {
                        calls.push(name.id.to_string());
                    }
                }
            }
            ast::Stmt::If(if_stmt) => {
                calls.extend(collect_function_calls_in_body(&if_stmt.body));
                for elif in &if_stmt.elif_else_clauses {
                    calls.extend(collect_function_calls_in_body(&elif.body));
                }
            }
            ast::Stmt::With(with_stmt) => {
                calls.extend(collect_function_calls_in_body(&with_stmt.body));
            }
            ast::Stmt::Match(match_stmt) => {
                for case in &match_stmt.cases {
                    calls.extend(collect_function_calls_in_body(&case.body));
                }
            }
            ast::Stmt::For(for_stmt) => {
                calls.extend(collect_function_calls_in_body(&for_stmt.body));
                calls.extend(collect_function_calls_in_body(&for_stmt.orelse));
            }
            ast::Stmt::While(while_stmt) => {
                calls.extend(collect_function_calls_in_body(&while_stmt.body));
                calls.extend(collect_function_calls_in_body(&while_stmt.orelse));
            }
            ast::Stmt::Try(try_stmt) => {
                calls.extend(collect_function_calls_in_body(&try_stmt.body));
                for handler in &try_stmt.handlers {
                    let ast::ExceptHandler::ExceptHandler(handler_inner) = handler;
                    calls.extend(collect_function_calls_in_body(&handler_inner.body));
                }
                calls.extend(collect_function_calls_in_body(&try_stmt.orelse));
                calls.extend(collect_function_calls_in_body(&try_stmt.finalbody));
            }
            _ => {}
        }
    }

    calls
}

/// Represents where to find a function: which file and what name.
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
struct FunctionTarget {
    file: File,
    func_name: String,
}

/// Builds a map of imported names to their source (module_name, func_name) from a file's imports.
fn build_import_map(stmts: &[ast::Stmt]) -> HashMap<String, (String, String)> {
    let mut imports = HashMap::new();

    for stmt in stmts {
        match stmt {
            ast::Stmt::ImportFrom(import_from) => {
                if let Some(module) = &import_from.module {
                    let module_name = module.to_string();
                    for alias in &import_from.names {
                        let local_name = alias.asname.as_ref().unwrap_or(&alias.name).to_string();
                        let original_name = alias.name.to_string();
                        imports.insert(local_name, (module_name.clone(), original_name));
                    }
                }
            }
            _ => {}
        }
    }

    imports
}

/// Finds a top-level function definition by name in a list of statements.
fn find_function_def<'a>(stmts: &'a [ast::Stmt], name: &str) -> Option<&'a ast::StmtFunctionDef> {
    for stmt in stmts {
        if let ast::Stmt::FunctionDef(func_def) = stmt {
            if func_def.name.as_str() == name {
                return Some(func_def);
            }
        }
    }
    None
}

/// Builds the Blender property registry by walking from register() in the root __init__.py
/// and transitively following function calls.
/// Cached by Salsa — only re-evaluated when the root init file or called functions change.
#[salsa::tracked(returns(ref), no_eq)]
pub(crate) fn blender_property_registry(db: &dyn Db) -> BlenderPropertyRegistry {
    let mut registry = BlenderPropertyRegistry::new();

    // Find the root __init__.py (first-party package with no dots in module name)
    let root_file = find_root_init_file(db);
    let Some(root_file) = root_file else {
        return registry;
    };

    let parsed = parsed_module(db, root_file).load(db);
    let stmts = parsed.suite();

    // Find register() function
    let Some(register_func) = find_function_def(stmts, "register") else {
        return registry;
    };

    // Walk register() and all functions it calls
    let mut visited = HashSet::new();
    walk_function_for_properties(
        db,
        root_file,
        stmts,
        &register_func.body,
        &mut registry,
        &mut visited,
    );

    registry
}

/// Finds the root __init__.py file (first-party package at the top level).
/// Cached by Salsa — only re-evaluated when the module list changes.
#[salsa::tracked]
pub(crate) fn find_root_init_file(db: &dyn Db) -> Option<File> {
    for module in &list_modules(db) {
        let Some(search_path) = module.search_path(db) else {
            continue;
        };
        if !search_path.is_first_party() {
            continue;
        }
        if !module.kind(db).is_package() {
            continue;
        }
        // Top-level package: no dots in module name
        if module.name(db).as_str().contains('.') {
            continue;
        }
        if let Some(file) = module.file(db) {
            return Some(file);
        }
    }
    None
}

/// Recursively walks a function body and all functions it calls, collecting
/// Blender property assignments into the registry.
fn walk_function_for_properties(
    db: &dyn Db,
    file: File,
    file_stmts: &[ast::Stmt],
    body: &[ast::Stmt],
    registry: &mut BlenderPropertyRegistry,
    visited: &mut HashSet<FunctionTarget>,
) {
    // Collect property assignments in this function body
    let assignments = collect_assignments_in_body(body);
    for assign in &assignments {
        if assign.targets.len() != 1 {
            continue;
        }
        let target = &assign.targets[0];
        let Some((class_name, prop_name)) = parse_dynamic_blender_property_target(target) else {
            continue;
        };
        if as_blender_property(&assign.value).is_some() {
            registry.add(class_name, prop_name, file, target.range());
        }
    }

    // Collect function calls and follow them
    let calls = collect_function_calls_in_body(body);
    let import_map = build_import_map(file_stmts);

    for call_name in &calls {
        // Check if this is a local function in the same file
        if let Some(local_func) = find_function_def(file_stmts, call_name) {
            let target = FunctionTarget {
                file,
                func_name: call_name.clone(),
            };
            if visited.insert(target) {
                walk_function_for_properties(
                    db,
                    file,
                    file_stmts,
                    &local_func.body,
                    registry,
                    visited,
                );
            }
            continue;
        }

        // Check if this is an imported function
        if let Some((module_name_str, original_name)) = import_map.get(call_name) {
            let Some(module_name) = ModuleName::new(module_name_str) else {
                continue;
            };
            let Some(target_module) = resolve_module(db, file, &module_name) else {
                continue;
            };
            let Some(target_file) = target_module.file(db) else {
                continue;
            };

            let target = FunctionTarget {
                file: target_file,
                func_name: original_name.clone(),
            };
            if visited.insert(target) {
                let target_parsed = parsed_module(db, target_file).load(db);
                let target_stmts = target_parsed.suite();
                if let Some(func_def) = find_function_def(target_stmts, original_name) {
                    walk_function_for_properties(
                        db,
                        target_file,
                        target_stmts,
                        &func_def.body,
                        registry,
                        visited,
                    );
                }
            }
        }
    }
}

/// Checks if an `ExprAttribute` node matches the `<root>.types.<ClassName>.<prop_name>` pattern.
/// This is used to suppress diagnostics on Blender dynamic property registration/unregistration.
pub(crate) fn is_dynamic_blender_property_target_attr(target: &ast::ExprAttribute) -> bool {
    // target is the outermost: ExprAttribute { value: ..., attr: <prop_name> }
    // We need: value = ExprAttribute { value: ExprAttribute { value: ExprName, attr: "types" }, attr: <class_name> }
    let Some(middle) = target.value.as_attribute_expr() else {
        return false;
    };
    let Some(inner) = middle.value.as_attribute_expr() else {
        return false;
    };
    inner.attr.as_str() == "types" && inner.value.is_name_expr()
}

/// Checks if an assignment target expression matches the pattern
/// `<root>.types.<ClassName>.<prop_name>` by traversing the AST attribute chain.
/// For example, `bpy.types.Scene.my_string` returns `Some(("Scene", "my_string"))`.
pub(crate) fn parse_dynamic_blender_property_target<'a>(
    target: &'a Expr,
) -> Option<(&'a str, &'a str)> {
    // Outermost: ExprAttribute { value: ..., attr: "my_string" } -> prop_name
    let outer = target.as_attribute_expr()?;
    let prop_name = outer.attr.as_str();

    // Next: ExprAttribute { value: ..., attr: "Scene" } -> class_name
    let middle = outer.value.as_attribute_expr()?;
    let class_name = middle.attr.as_str();

    // Next: ExprAttribute { value: ExprName(...), attr: "types" } -> must be "types"
    let inner = middle.value.as_attribute_expr()?;
    if inner.attr.as_str() != "types" {
        return None;
    }

    // Root: must be a simple name (e.g., "bpy")
    if inner.value.is_name_expr() {
        Some((class_name, prop_name))
    } else {
        None
    }
}

/// Returns true if the given assignment (identified by file and target range)
/// is within the register() scope for Blender property registration.
pub(crate) fn is_in_register_scope(db: &dyn Db, file: File, range: TextRange) -> bool {
    let registry = blender_property_registry(db);
    registry.contains(file, range)
}

/// Looks up a dynamic Blender property type for a given class and attribute name.
/// Only searches within the register() function in the project root __init__.py
/// and all functions it transitively calls.
pub(crate) fn lookup_blender_dynamic_property<'db>(
    db: &'db dyn Db,
    class: StaticClassLiteral<'db>,
    name: &str,
) -> Option<Type<'db>> {
    // Only applies to classes defined in bpy.types modules
    let class_file = class.body_scope(db).file(db);
    let class_module = file_to_module(db, class_file)?;
    let module_name = class_module.name(db).as_str();
    if module_name != "bpy.types" && !module_name.starts_with("bpy.types.") {
        return None;
    }

    let class_name = class.name(db).as_str();

    // Build registry of properties reachable from register()
    let registry = blender_property_registry(db);
    let locations = registry.get(class_name, name)?;

    // Use the first matching location to resolve the type
    for location in locations {
        let file = location.file;
        let parsed = parsed_module(db, file).load(db);

        // Find the matching assignment in the AST to get the target expression
        let target_expr = find_assignment_target_at_range(parsed.suite(), location.target_range);
        let Some(target) = target_expr else {
            continue;
        };

        // Resolve the type via the semantic index
        let index = semantic_index(db, file);
        for scope_id in index.scope_ids() {
            let table = place_table(db, scope_id);
            let Some(place_expr) = PlaceExpr::try_from_expr(target) else {
                continue;
            };
            let Some(place_id) = table.place_id(&place_expr) else {
                continue;
            };

            let result = place_by_id(
                db,
                scope_id,
                place_id,
                RequiresExplicitReExport::No,
                ConsideredDefinitions::EndOfScope,
            );

            if let Some(ty) = result.place.ignore_possibly_undefined() {
                return Some(ty);
            }
        }
    }

    None
}

/// Looks up a dynamic Blender property definition for a given class and attribute name.
/// Returns the Definition for IDE features (hover, go-to-definition).
/// Only searches within the register() scope.
pub(crate) fn lookup_blender_dynamic_property_definition<'db>(
    db: &'db dyn Db,
    class: StaticClassLiteral<'db>,
    name: &str,
) -> Option<Definition<'db>> {
    // Only applies to classes defined in bpy.types modules
    let class_file = class.body_scope(db).file(db);
    let class_module = file_to_module(db, class_file)?;
    let module_name = class_module.name(db).as_str();
    if module_name != "bpy.types" && !module_name.starts_with("bpy.types.") {
        return None;
    }

    let class_name = class.name(db).as_str();

    // Build registry of properties reachable from register()
    let registry = blender_property_registry(db);
    let locations = registry.get(class_name, name)?;

    // Use the first matching location to resolve the definition
    for location in locations {
        let file = location.file;
        let parsed = parsed_module(db, file).load(db);

        let target_expr = find_assignment_target_at_range(parsed.suite(), location.target_range);
        let Some(target) = target_expr else {
            continue;
        };

        let index = semantic_index(db, file);
        for scope_id in index.scope_ids() {
            let table = place_table(db, scope_id);
            let Some(place_expr) = PlaceExpr::try_from_expr(target) else {
                continue;
            };
            let Some(place_id) = table.place_id(&place_expr) else {
                continue;
            };

            let use_def = use_def_map(db, scope_id);
            for binding in use_def.end_of_scope_bindings(place_id) {
                if let Some(def) = binding.binding.definition() {
                    return Some(def);
                }
            }
        }
    }

    None
}

/// Recursively searches through statements to find an assignment target expression
/// at a specific text range.
fn find_assignment_target_at_range(stmts: &[ast::Stmt], range: TextRange) -> Option<&Expr> {
    for stmt in stmts {
        match stmt {
            ast::Stmt::Assign(assign) => {
                for target in &assign.targets {
                    if target.range() == range {
                        return Some(target);
                    }
                }
            }
            ast::Stmt::FunctionDef(func_def) => {
                if let Some(found) = find_assignment_target_at_range(&func_def.body, range) {
                    return Some(found);
                }
            }
            ast::Stmt::ClassDef(class_def) => {
                if let Some(found) = find_assignment_target_at_range(&class_def.body, range) {
                    return Some(found);
                }
            }
            ast::Stmt::If(if_stmt) => {
                if let Some(found) = find_assignment_target_at_range(&if_stmt.body, range) {
                    return Some(found);
                }
                for elif in &if_stmt.elif_else_clauses {
                    if let Some(found) = find_assignment_target_at_range(&elif.body, range) {
                        return Some(found);
                    }
                }
            }
            ast::Stmt::With(with_stmt) => {
                if let Some(found) = find_assignment_target_at_range(&with_stmt.body, range) {
                    return Some(found);
                }
            }
            ast::Stmt::Match(match_stmt) => {
                for case in &match_stmt.cases {
                    if let Some(found) = find_assignment_target_at_range(&case.body, range) {
                        return Some(found);
                    }
                }
            }
            ast::Stmt::For(for_stmt) => {
                if let Some(found) = find_assignment_target_at_range(&for_stmt.body, range) {
                    return Some(found);
                }
                if let Some(found) = find_assignment_target_at_range(&for_stmt.orelse, range) {
                    return Some(found);
                }
            }
            ast::Stmt::While(while_stmt) => {
                if let Some(found) = find_assignment_target_at_range(&while_stmt.body, range) {
                    return Some(found);
                }
                if let Some(found) = find_assignment_target_at_range(&while_stmt.orelse, range) {
                    return Some(found);
                }
            }
            ast::Stmt::Try(try_stmt) => {
                if let Some(found) = find_assignment_target_at_range(&try_stmt.body, range) {
                    return Some(found);
                }
                for handler in &try_stmt.handlers {
                    let ast::ExceptHandler::ExceptHandler(handler_inner) = handler;
                    if let Some(found) = find_assignment_target_at_range(&handler_inner.body, range)
                    {
                        return Some(found);
                    }
                }
                if let Some(found) = find_assignment_target_at_range(&try_stmt.orelse, range) {
                    return Some(found);
                }
                if let Some(found) = find_assignment_target_at_range(&try_stmt.finalbody, range) {
                    return Some(found);
                }
            }
            _ => {}
        }
    }
    None
}
