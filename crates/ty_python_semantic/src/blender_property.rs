use crate::Db;
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
use ruff_text_size::Ranged;
use ty_module_resolver::{all_modules, file_to_module};

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

/// Recursively collects all assignment statements from a statement list,
/// including those nested inside function and class definitions.
fn collect_all_assignments<'a>(stmts: &'a [ast::Stmt]) -> Vec<&'a ast::StmtAssign> {
    let mut assignments = Vec::new();

    for stmt in stmts {
        match stmt {
            ast::Stmt::Assign(assign) => {
                assignments.push(assign);
            }
            ast::Stmt::FunctionDef(func_def) => {
                // Recursively search function bodies
                assignments.extend(collect_all_assignments(&func_def.body));
            }
            ast::Stmt::ClassDef(class_def) => {
                // Recursively search class bodies
                assignments.extend(collect_all_assignments(&class_def.body));
            }
            ast::Stmt::If(if_stmt) => {
                // Search if/elif/else branches
                assignments.extend(collect_all_assignments(&if_stmt.body));
                for elif in &if_stmt.elif_else_clauses {
                    assignments.extend(collect_all_assignments(&elif.body));
                }
            }
            ast::Stmt::With(with_stmt) => {
                assignments.extend(collect_all_assignments(&with_stmt.body));
            }
            ast::Stmt::Match(match_stmt) => {
                for case in &match_stmt.cases {
                    assignments.extend(collect_all_assignments(&case.body));
                }
            }
            ast::Stmt::For(for_stmt) => {
                assignments.extend(collect_all_assignments(&for_stmt.body));
                assignments.extend(collect_all_assignments(&for_stmt.orelse));
            }
            ast::Stmt::While(while_stmt) => {
                assignments.extend(collect_all_assignments(&while_stmt.body));
                assignments.extend(collect_all_assignments(&while_stmt.orelse));
            }
            ast::Stmt::Try(try_stmt) => {
                assignments.extend(collect_all_assignments(&try_stmt.body));
                for handler in &try_stmt.handlers {
                    let ast::ExceptHandler::ExceptHandler(handler_inner) = handler;
                    assignments.extend(collect_all_assignments(&handler_inner.body));
                }
                assignments.extend(collect_all_assignments(&try_stmt.orelse));
                assignments.extend(collect_all_assignments(&try_stmt.finalbody));
            }
            _ => {}
        }
    }

    assignments
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

/// Looks up a dynamic Blender property type for a given class and attribute name.
/// Scans all project modules for assignment statements matching the pattern
/// `<root>.types.<ClassName>.<name> = <BlenderPropertyCall>(...)`.
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

    // Scan all modules for matching dynamic property assignments
    for module in all_modules(db) {
        let Some(file) = module.file(db) else {
            continue;
        };

        let parsed = parsed_module(db, file).load(db);
        let all_assignments = collect_all_assignments(parsed.suite());

        for assign in all_assignments {

            // Only single-target assignments
            if assign.targets.len() != 1 {
                continue;
            }

            let target = &assign.targets[0];

            // Check if target matches the pattern <root>.types.<ClassName>.<prop_name>
            let Some((target_class, target_prop)) = parse_dynamic_blender_property_target(target)
            else {
                continue;
            };

            if target_class != class_name || target_prop != name {
                continue;
            }

            // Check if value is a Blender property call
            if as_blender_property(&assign.value).is_none() {
                continue;
            }

            // Resolve the type via the semantic index
            // Try all scopes in the file since the assignment could be in any scope
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
    }

    None
}

/// Looks up a dynamic Blender property definition for a given class and attribute name.
/// Returns the Definition for IDE features (hover, go-to-definition).
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

    // Scan all modules for matching dynamic property assignments
    for module in all_modules(db) {
        let Some(file) = module.file(db) else {
            continue;
        };

        let parsed = parsed_module(db, file).load(db);
        let all_assignments = collect_all_assignments(parsed.suite());

        for assign in all_assignments {

            if assign.targets.len() != 1 {
                continue;
            }

            let target = &assign.targets[0];

            let Some((target_class, target_prop)) = parse_dynamic_blender_property_target(target)
            else {
                continue;
            };

            if target_class != class_name || target_prop != name {
                continue;
            }

            if as_blender_property(&assign.value).is_none() {
                continue;
            }

            // Get the Definition from the use-def map
            // Try all scopes in the file since the assignment could be in any scope
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
    }

    None
}
