use ruff_python_ast::PySourceType;
use ruff_python_parser::python_version::PyVersion;
use ruff_python_stdlib::builtins::is_python_builtin;

pub(super) fn shadows_builtin(
    name: &str,
    source_type: PySourceType,
    ignorelist: &[String],
    python_version: PyVersion,
) -> bool {
    if is_python_builtin(name, python_version.minor(), source_type.is_ipynb()) {
        ignorelist.iter().all(|ignore| ignore != name)
    } else {
        false
    }
}
