use ruff_python_ast::PySourceType;
use ruff_python_stdlib::builtins::{is_ipython_builtin, is_python_builtin};

pub(super) fn shadows_builtin(
    name: &str,
    ignorelist: &[String],
    source_type: PySourceType,
) -> bool {
    if is_python_builtin(name) || source_type.is_ipynb() && is_ipython_builtin(name) {
        ignorelist.iter().all(|ignore| ignore != name)
    } else {
        false
    }
}
