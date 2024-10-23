use crate::settings::types::PythonVersion;
use ruff_python_ast::PySourceType;
use ruff_python_stdlib::builtins::is_python_builtin;

pub(super) fn shadows_builtin(
    name: &str,
    source_type: PySourceType,
    ignorelist: &[String],
    python_version: PythonVersion,
) -> bool {
    if is_python_builtin(name, python_version.minor(), source_type.is_ipynb()) {
        ignorelist.iter().all(|ignore| ignore != name)
    } else {
        false
    }
}
