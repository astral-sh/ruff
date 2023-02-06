use libcst_native::{Expression, NameOrAttribute};

fn compose_call_path_inner<'a>(expr: &'a Expression, parts: &mut Vec<&'a str>) {
    match &expr {
        Expression::Call(expr) => {
            compose_call_path_inner(&expr.func, parts);
        }
        Expression::Attribute(expr) => {
            compose_call_path_inner(&expr.value, parts);
            parts.push(expr.attr.value);
        }
        Expression::Name(expr) => {
            parts.push(expr.value);
        }
        _ => {}
    }
}

pub fn compose_call_path(expr: &Expression) -> Option<String> {
    let mut segments = vec![];
    compose_call_path_inner(expr, &mut segments);
    if segments.is_empty() {
        None
    } else {
        Some(segments.join("."))
    }
}

pub fn compose_module_path(module: &NameOrAttribute) -> String {
    match module {
        NameOrAttribute::N(name) => name.value.to_string(),
        NameOrAttribute::A(attr) => {
            let name = attr.attr.value;
            let prefix = compose_call_path(&attr.value);
            prefix.map_or_else(|| name.to_string(), |prefix| format!("{prefix}.{name}"))
        }
    }
}
