use std::collections::{HashMap, HashSet};

use crate::Db;
use crate::declare_lint;
use crate::lint::{Level, LintStatus};
use crate::place::{ConsideredDefinitions, RequiresExplicitReExport, place_by_id};
use crate::semantic_index::definition::Definition;
use crate::semantic_index::place::{PlaceExpr, ScopedPlaceId};
use crate::semantic_index::scope::ScopeKind;
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
pub(crate) struct BlenderPropertyRegistry {
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

/// Information about a single property on a Blender operator.
#[derive(Debug, Clone, PartialEq, Eq)]
struct OperatorPropertyInfo {
    /// The name of the property (e.g., "x", "y").
    name: String,
}

/// Information about a registered Blender operator class.
#[derive(Debug, Clone, PartialEq, Eq)]
struct BlenderOperatorInfo {
    /// The ops module part from bl_idname (e.g., "wm" from "wm.mouse_position").
    ops_module: String,
    /// The operator name part from bl_idname (e.g., "mouse_position" from "wm.mouse_position").
    op_name: String,
    /// The file where the operator class was defined.
    file: File,
    /// The class name (e.g., "SimpleMouseOperator").
    class_name: String,
    /// The operator's properties extracted from class annotations.
    properties: Vec<OperatorPropertyInfo>,
}

/// Registry of all Blender operator registrations reachable from register().
/// Maps (ops_module, op_name) to operator info.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct BlenderOperatorRegistry {
    operators: HashMap<(String, String), BlenderOperatorInfo>,
    /// Set of all ops module names for quick lookup (e.g., {"wm", "mesh"}).
    ops_modules: HashSet<String>,
}

impl BlenderOperatorRegistry {
    fn new() -> Self {
        Self {
            operators: HashMap::new(),
            ops_modules: HashSet::new(),
        }
    }

    fn add(&mut self, info: BlenderOperatorInfo) {
        self.ops_modules.insert(info.ops_module.clone());
        self.operators
            .insert((info.ops_module.clone(), info.op_name.clone()), info);
    }

    fn get(&self, ops_module: &str, op_name: &str) -> Option<&BlenderOperatorInfo> {
        self.operators
            .get(&(ops_module.to_string(), op_name.to_string()))
    }

    fn has_module(&self, module_name: &str) -> bool {
        self.ops_modules.contains(module_name)
    }
}

/// Combined registries for both dynamic properties and operators,
/// built from a single walk of the register() function.
#[derive(Debug, PartialEq, Eq)]
struct BlenderRegistries {
    properties: BlenderPropertyRegistry,
    operators: BlenderOperatorRegistry,
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

/// Collects all qualified/attribute function calls from a statement list.
/// Collects calls like `module.func()` and returns (qualifier, func_name) pairs.
fn collect_qualified_calls_in_body(stmts: &[ast::Stmt]) -> Vec<(String, String)> {
    let mut calls = Vec::new();

    for stmt in stmts {
        match stmt {
            ast::Stmt::Expr(expr_stmt) => {
                if let Some(call) = expr_stmt.value.as_call_expr() {
                    if let Some(attr) = call.func.as_attribute_expr() {
                        if let Some(name) = attr.value.as_name_expr() {
                            calls.push((name.id.to_string(), attr.attr.to_string()));
                        }
                    }
                }
            }
            ast::Stmt::If(if_stmt) => {
                calls.extend(collect_qualified_calls_in_body(&if_stmt.body));
                for elif in &if_stmt.elif_else_clauses {
                    calls.extend(collect_qualified_calls_in_body(&elif.body));
                }
            }
            ast::Stmt::With(with_stmt) => {
                calls.extend(collect_qualified_calls_in_body(&with_stmt.body));
            }
            ast::Stmt::Match(match_stmt) => {
                for case in &match_stmt.cases {
                    calls.extend(collect_qualified_calls_in_body(&case.body));
                }
            }
            ast::Stmt::For(for_stmt) => {
                calls.extend(collect_qualified_calls_in_body(&for_stmt.body));
                calls.extend(collect_qualified_calls_in_body(&for_stmt.orelse));
            }
            ast::Stmt::While(while_stmt) => {
                calls.extend(collect_qualified_calls_in_body(&while_stmt.body));
                calls.extend(collect_qualified_calls_in_body(&while_stmt.orelse));
            }
            ast::Stmt::Try(try_stmt) => {
                calls.extend(collect_qualified_calls_in_body(&try_stmt.body));
                for handler in &try_stmt.handlers {
                    let ast::ExceptHandler::ExceptHandler(handler_inner) = handler;
                    calls.extend(collect_qualified_calls_in_body(&handler_inner.body));
                }
                calls.extend(collect_qualified_calls_in_body(&try_stmt.orelse));
                calls.extend(collect_qualified_calls_in_body(&try_stmt.finalbody));
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

/// Resolves a relative import to an absolute module name string.
/// For `from .helpers import func` (level=1, module=Some("helpers")), returns the absolute
/// module name like "my_addon.helpers".
/// For `from . import helpers` (level=1, module=None), returns the package name like "my_addon".
fn resolve_relative_module_name(
    db: &dyn Db,
    file: File,
    level: u32,
    import_module: Option<&str>,
) -> Option<String> {
    if level == 0 {
        return import_module.map(|m| m.to_string());
    }

    let current_module = file_to_module(db, file)?;
    let current_name = current_module.name(db);

    // For __init__.py (Package), the current package is the module itself.
    // For regular files (Module), the current package is the parent.
    let mut base = if current_module.kind(db).is_package() {
        Some(current_name.clone())
    } else {
        current_name.parent()
    };

    // Go up (level - 1) more levels
    for _ in 1..level {
        base = base?.parent();
    }

    match (base, import_module) {
        (Some(base_name), Some(module)) => {
            let mut result = base_name.as_str().to_string();
            result.push('.');
            result.push_str(module);
            Some(result)
        }
        (Some(base_name), None) => Some(base_name.as_str().to_string()),
        (None, Some(module)) => Some(module.to_string()),
        (None, None) => None,
    }
}

/// Builds a map of imported names to their source (module_name, func_name) from a file's
/// `from X import Y` statements, including relative imports like `from .X import Y`.
fn build_import_map(
    db: &dyn Db,
    file: File,
    stmts: &[ast::Stmt],
) -> HashMap<String, (String, String)> {
    let mut imports = HashMap::new();

    for stmt in stmts {
        if let ast::Stmt::ImportFrom(import_from) = stmt {
            let resolved_module = if import_from.level > 0 {
                // Relative import: resolve to absolute module name
                resolve_relative_module_name(
                    db,
                    file,
                    import_from.level,
                    import_from.module.as_ref().map(|m| m.as_str()),
                )
            } else {
                import_from.module.as_ref().map(|m| m.to_string())
            };

            if let Some(module_name) = resolved_module {
                // Only process imports that have a module part (e.g., `from .helpers import func`,
                // not `from . import helpers` which imports modules, handled by
                // build_module_import_map).
                if import_from.module.is_some() || import_from.level == 0 {
                    for alias in &import_from.names {
                        let local_name =
                            alias.asname.as_ref().unwrap_or(&alias.name).to_string();
                        let original_name = alias.name.to_string();
                        imports.insert(local_name, (module_name.clone(), original_name));
                    }
                }
            }
        }
    }

    imports
}

/// Builds a map of module aliases to their module names from `import X`, `import X as Y`,
/// and `from . import X` statements. Maps local_name -> module_name.
fn build_module_import_map(
    db: &dyn Db,
    file: File,
    stmts: &[ast::Stmt],
) -> HashMap<String, String> {
    let mut imports = HashMap::new();

    for stmt in stmts {
        match stmt {
            ast::Stmt::Import(import_stmt) => {
                for alias in &import_stmt.names {
                    let local_name = alias.asname.as_ref().unwrap_or(&alias.name).to_string();
                    let module_name = alias.name.to_string();
                    imports.insert(local_name, module_name);
                }
            }
            ast::Stmt::ImportFrom(import_from) => {
                // Handle `from . import X` (level > 0, no module name) which imports submodules
                if import_from.level > 0 && import_from.module.is_none() {
                    for alias in &import_from.names {
                        let local_name =
                            alias.asname.as_ref().unwrap_or(&alias.name).to_string();
                        // Resolve the full module path for each imported name
                        if let Some(resolved) = resolve_relative_module_name(
                            db,
                            file,
                            import_from.level,
                            Some(alias.name.as_str()),
                        ) {
                            imports.insert(local_name, resolved);
                        }
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

/// Finds a top-level class definition by name in a list of statements.
fn find_class_def<'a>(stmts: &'a [ast::Stmt], name: &str) -> Option<&'a ast::StmtClassDef> {
    for stmt in stmts {
        if let ast::Stmt::ClassDef(class_def) = stmt {
            if class_def.name.as_str() == name {
                return Some(class_def);
            }
        }
    }
    None
}

/// Collects class names passed to `bpy.utils.register_class(ClassName)` calls
/// within a statement body, handling nested control flow.
fn collect_register_class_calls_in_body<'a>(stmts: &'a [ast::Stmt]) -> Vec<&'a str> {
    let mut class_names = Vec::new();

    for stmt in stmts {
        match stmt {
            ast::Stmt::Expr(expr_stmt) => {
                if let Some(call) = expr_stmt.value.as_call_expr() {
                    if is_register_class_call(&call.func) {
                        if let Some(first_arg) = call.arguments.args.first() {
                            if let Some(name) = first_arg.as_name_expr() {
                                class_names.push(name.id.as_str());
                            }
                        }
                    }
                }
            }
            ast::Stmt::If(if_stmt) => {
                class_names.extend(collect_register_class_calls_in_body(&if_stmt.body));
                for elif in &if_stmt.elif_else_clauses {
                    class_names.extend(collect_register_class_calls_in_body(&elif.body));
                }
            }
            ast::Stmt::With(with_stmt) => {
                class_names.extend(collect_register_class_calls_in_body(&with_stmt.body));
            }
            ast::Stmt::Match(match_stmt) => {
                for case in &match_stmt.cases {
                    class_names.extend(collect_register_class_calls_in_body(&case.body));
                }
            }
            ast::Stmt::For(for_stmt) => {
                class_names.extend(collect_register_class_calls_in_body(&for_stmt.body));
                class_names.extend(collect_register_class_calls_in_body(&for_stmt.orelse));
            }
            ast::Stmt::While(while_stmt) => {
                class_names.extend(collect_register_class_calls_in_body(&while_stmt.body));
                class_names.extend(collect_register_class_calls_in_body(&while_stmt.orelse));
            }
            ast::Stmt::Try(try_stmt) => {
                class_names.extend(collect_register_class_calls_in_body(&try_stmt.body));
                for handler in &try_stmt.handlers {
                    let ast::ExceptHandler::ExceptHandler(handler_inner) = handler;
                    class_names.extend(collect_register_class_calls_in_body(&handler_inner.body));
                }
                class_names.extend(collect_register_class_calls_in_body(&try_stmt.orelse));
                class_names.extend(collect_register_class_calls_in_body(&try_stmt.finalbody));
            }
            _ => {}
        }
    }

    class_names
}

/// Checks if a call expression matches the `bpy.utils.register_class` or
/// `<alias>.utils.register_class` pattern.
fn is_register_class_call(func: &Expr) -> bool {
    // Check for *.utils.register_class pattern
    if let Some(outer_attr) = func.as_attribute_expr() {
        if outer_attr.attr.as_str() != "register_class" {
            return false;
        }
        if let Some(inner_attr) = outer_attr.value.as_attribute_expr() {
            if inner_attr.attr.as_str() == "utils" && inner_attr.value.is_name_expr() {
                return true;
            }
        }
    }
    // Also check for simple register_class(X) calls (when imported)
    if let Some(name) = func.as_name_expr() {
        if name.id.as_str() == "register_class" {
            return true;
        }
    }
    false
}

/// Extracts operator info from a class definition if it has a `bl_idname` attribute.
/// Returns None if the class doesn't have a valid bl_idname.
fn extract_operator_info(
    class_def: &ast::StmtClassDef,
    file: File,
) -> Option<BlenderOperatorInfo> {
    let mut bl_idname: Option<String> = None;
    let mut properties = Vec::new();

    for stmt in &class_def.body {
        match stmt {
            // Check for bl_idname = "module.op_name"
            ast::Stmt::Assign(assign) => {
                if assign.targets.len() == 1 {
                    if let Some(name_expr) = assign.targets[0].as_name_expr() {
                        if name_expr.id.as_str() == "bl_idname" {
                            if let Some(string_lit) = assign.value.as_string_literal_expr() {
                                bl_idname = Some(string_lit.value.to_str().to_string());
                            }
                        }
                    }
                }
            }
            // Check for annotated properties: x: bpy.props.IntProperty()
            ast::Stmt::AnnAssign(ann_assign) => {
                if let Some(name_expr) = ann_assign.target.as_name_expr() {
                    let prop_name = name_expr.id.as_str();
                    // Check bl_idname as annotated assignment: bl_idname: str = "module.op_name"
                    if prop_name == "bl_idname" {
                        if let Some(ref value) = ann_assign.value {
                            if let Some(string_lit) = value.as_string_literal_expr() {
                                bl_idname = Some(string_lit.value.to_str().to_string());
                            }
                        }
                        continue;
                    }
                    // Skip known Blender class-level attributes
                    if matches!(
                        prop_name,
                        "bl_label" | "bl_description" | "bl_options" | "bl_translation_context"
                    ) {
                        continue;
                    }
                    // Check if annotation is a Blender property call
                    if as_blender_property(&ann_assign.annotation).is_some() {
                        properties.push(OperatorPropertyInfo {
                            name: prop_name.to_string(),
                        });
                    }
                }
            }
            _ => {}
        }
    }

    // Parse bl_idname: "wm.mouse_position" -> ("wm", "mouse_position")
    let bl_idname = bl_idname?;
    let (ops_module, op_name) = bl_idname.split_once('.')?;

    Some(BlenderOperatorInfo {
        ops_module: ops_module.to_string(),
        op_name: op_name.to_string(),
        file,
        class_name: class_def.name.to_string(),
        properties,
    })
}

/// Builds both the property and operator registries by walking from register()
/// in the root __init__.py and transitively following function calls.
/// Cached by Salsa — only re-evaluated when the root init file or called functions change.
#[salsa::tracked(returns(ref), no_eq)]
fn blender_registries(db: &dyn Db) -> BlenderRegistries {
    let mut registries = BlenderRegistries {
        properties: BlenderPropertyRegistry::new(),
        operators: BlenderOperatorRegistry::new(),
    };

    // Find the root __init__.py (first-party package with no dots in module name)
    let root_file = find_root_init_file(db);
    let Some(root_file) = root_file else {
        return registries;
    };

    let parsed = parsed_module(db, root_file).load(db);
    let stmts = parsed.suite();

    // Find register() function
    let Some(register_func) = find_function_def(stmts, "register") else {
        return registries;
    };

    // Walk register() and all functions it calls
    let mut visited = HashSet::new();
    walk_function_for_properties(
        db,
        root_file,
        stmts,
        &register_func.body,
        &mut registries,
        &mut visited,
    );

    registries
}

/// Returns the property registry (delegating to the combined registries).
pub(crate) fn blender_property_registry(db: &dyn Db) -> &BlenderPropertyRegistry {
    &blender_registries(db).properties
}

/// Returns the operator registry.
pub(crate) fn blender_operator_registry(db: &dyn Db) -> &BlenderOperatorRegistry {
    &blender_registries(db).operators
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
/// Blender property assignments and operator registrations into the registries.
fn walk_function_for_properties(
    db: &dyn Db,
    file: File,
    file_stmts: &[ast::Stmt],
    body: &[ast::Stmt],
    registries: &mut BlenderRegistries,
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
            registries
                .properties
                .add(class_name, prop_name, file, target.range());
        }
    }

    // Collect register_class calls and extract operator info
    let register_calls = collect_register_class_calls_in_body(body);
    for class_name in &register_calls {
        // Look up the class definition in the current file
        if let Some(class_def) = find_class_def(file_stmts, class_name) {
            if let Some(op_info) = extract_operator_info(class_def, file) {
                registries.operators.add(op_info);
            }
        }
    }

    // Collect function calls and follow them
    let calls = collect_function_calls_in_body(body);
    let import_map = build_import_map(db, file, file_stmts);

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
                    registries,
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
                        registries,
                        visited,
                    );
                }
            }
        }
    }

    // Collect qualified calls like `module.func()` and follow them
    let qualified_calls = collect_qualified_calls_in_body(body);
    let module_import_map = build_module_import_map(db, file, file_stmts);

    for (qualifier, func_name) in &qualified_calls {
        if let Some(module_name_str) = module_import_map.get(qualifier) {
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
                func_name: func_name.clone(),
            };
            if visited.insert(target) {
                let target_parsed = parsed_module(db, target_file).load(db);
                let target_stmts = target_parsed.suite();
                if let Some(func_def) = find_function_def(target_stmts, func_name) {
                    walk_function_for_properties(
                        db,
                        target_file,
                        target_stmts,
                        &func_def.body,
                        registries,
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

/// Resolves the type of a property defined in a Blender operator class body
/// by looking up the declared type through the semantic index.
/// This delegates type inference to the stub files (e.g., `bpy.props.IntProperty() -> int`).
fn resolve_operator_property_type<'db>(
    db: &'db dyn Db,
    file: File,
    class_name: &str,
    prop_name: &str,
) -> Option<Type<'db>> {
    let parsed = parsed_module(db, file).load(db);
    let index = semantic_index(db, file);

    // Find the class body scope by iterating file scopes and matching the class name
    for scope_id in index.scope_ids() {
        let file_scope_id = scope_id.file_scope_id(db);
        let scope = index.scope(file_scope_id);

        if scope.kind() != ScopeKind::Class {
            continue;
        }

        // Check if this is the right class by comparing names
        let Some(class_ref) = scope.node().as_class() else {
            continue;
        };
        if class_ref.node(&parsed).name.as_str() != class_name {
            continue;
        }

        // Look up the property symbol in the class body scope
        let table = place_table(db, scope_id);
        let Some(symbol_id) = table.symbol_id(prop_name) else {
            continue;
        };

        let place_id = ScopedPlaceId::Symbol(symbol_id);
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

    None
}

/// Synthesizes a callable type for a Blender operator.
/// The signature is:
///   (execution_context: int | str | None = None, undo: bool | None = None, /,
///    *, prop1: T1 | None = None, ...) -> set[str]
/// Property types are inferred from the property function return type via the semantic index,
/// delegating to the stub file annotations (e.g., `IntProperty() -> int`).
fn synthesize_operator_callable_type<'db>(
    db: &'db dyn Db,
    info: &BlenderOperatorInfo,
) -> Type<'db> {
    use crate::types::{CallableType, KnownClass, Parameter, Parameters, Signature, UnionType};
    use ruff_python_ast::name::Name;

    let none_ty = Type::none(db);

    // Positional-only: execution_context: int | str | None = None
    let exec_ctx_type = UnionType::from_elements(db, [
        KnownClass::Int.to_instance(db),
        KnownClass::Str.to_instance(db),
        none_ty,
    ]);

    // Positional-only: undo: bool | None = None
    let undo_type = UnionType::from_elements(db, [
        KnownClass::Bool.to_instance(db),
        none_ty,
    ]);

    let mut params = vec![
        Parameter::positional_only(Some(Name::new_static("execution_context")))
            .with_annotated_type(exec_ctx_type)
            .with_default_type(none_ty),
        Parameter::positional_only(Some(Name::new_static("undo")))
            .with_annotated_type(undo_type)
            .with_default_type(none_ty),
    ];

    // Keyword-only parameters from operator properties
    for prop in &info.properties {
        let prop_type = resolve_operator_property_type(db, info.file, &info.class_name, &prop.name)
            .unwrap_or(Type::unknown());
        let param_type = UnionType::from_elements(db, [prop_type, none_ty]);
        params.push(
            Parameter::keyword_only(Name::new(&prop.name))
                .with_annotated_type(param_type)
                .with_default_type(none_ty),
        );
    }

    // Return type: set[str]
    let return_type = KnownClass::Set.to_specialized_instance(db, &[
        KnownClass::Str.to_instance(db),
    ]);

    let parameters = Parameters::new(db, params);
    let signature = Signature::new(parameters, return_type);
    Type::Callable(CallableType::single(db, signature))
}

/// Looks up a Blender operator by ops module and name, returning its synthesized callable type.
pub(crate) fn lookup_blender_operator<'db>(
    db: &'db dyn Db,
    ops_module: &str,
    op_name: &str,
) -> Option<Type<'db>> {
    let registry = blender_operator_registry(db);
    let info = registry.get(ops_module, op_name)?;
    Some(synthesize_operator_callable_type(db, info))
}

/// Checks if there are any registered Blender operators for the given ops module name.
pub(crate) fn has_blender_ops_module(db: &dyn Db, module_name: &str) -> bool {
    let registry = blender_operator_registry(db);
    registry.has_module(module_name)
}
