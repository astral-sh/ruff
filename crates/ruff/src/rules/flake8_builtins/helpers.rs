use ruff_python_stdlib::builtins::is_builtin;

pub(super) fn shadows_builtin(name: &str, ignorelist: &[String]) -> bool {
    is_builtin(name) && ignorelist.iter().all(|ignore| ignore != name)
}
