use ruff_python_ast::PySourceType;
use ruff_python_ast::PythonVersion;
use ruff_python_stdlib::builtins::is_python_builtin;

pub(super) fn shadows_builtin(
    name: &str,
    source_type: PySourceType,
    ignorelist: &[String],
    python_version: Option<PythonVersion>,
) -> bool {
    let python_version = python_version.unwrap_or_else(PythonVersion::latest);
    if is_python_builtin(name, python_version.minor, source_type.is_ipynb()) {
        ignorelist.iter().all(|ignore| ignore != name)
    } else {
        false
    }
}
