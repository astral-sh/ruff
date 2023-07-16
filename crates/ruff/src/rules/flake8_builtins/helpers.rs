use ruff_python_stdlib::builtins::BUILTINS;

pub(super) fn shadows_builtin(name: &str, ignorelist: &[String]) -> bool {
    BUILTINS.contains(&name) && ignorelist.iter().all(|ignore| ignore != name)
}
