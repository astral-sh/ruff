mod builtin_modules;
mod known_stdlib;

pub use builtin_modules::is_builtin_module;
pub use known_stdlib::is_known_standard_library;

#[cfg(test)]
mod tests {
    use super::is_known_standard_library;

    #[test]
    fn python_315_stdlib_modules() {
        assert!(is_known_standard_library(15, "tomllib"));
        assert!(is_known_standard_library(15, "profiling"));
    }
}
