use crate::Db;
use ruff_db::files::File;
use ruff_db::source::source_text;
use ruff_python_ast::{Expr, ExprCall};
use ruff_text_size::Ranged;

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
