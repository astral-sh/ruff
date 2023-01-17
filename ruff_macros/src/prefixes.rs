// Longer prefixes should come first so that you can find an origin for a code
// by simply picking the first entry that starts with the given prefix.

pub const PREFIX_TO_ORIGIN: &[(&str, &str)] = &[
    ("ANN", "Flake8Annotations"),
    ("ARG", "Flake8UnusedArguments"),
    ("A", "Flake8Builtins"),
    ("BLE", "Flake8BlindExcept"),
    ("B", "Flake8Bugbear"),
    ("C4", "Flake8Comprehensions"),
    ("C9", "McCabe"),
    ("COM", "Flake8Commas"),
    ("DTZ", "Flake8Datetimez"),
    ("D", "Pydocstyle"),
    ("ERA", "Eradicate"),
    ("EM", "Flake8ErrMsg"),
    ("E", "Pycodestyle"),
    ("FBT", "Flake8BooleanTrap"),
    ("F", "Pyflakes"),
    ("ICN", "Flake8ImportConventions"),
    ("ISC", "Flake8ImplicitStrConcat"),
    ("I", "Isort"),
    ("N", "PEP8Naming"),
    ("PD", "PandasVet"),
    ("PGH", "PygrepHooks"),
    ("PL", "Pylint"),
    ("PT", "Flake8PytestStyle"),
    ("Q", "Flake8Quotes"),
    ("RET", "Flake8Return"),
    ("SIM", "Flake8Simplify"),
    ("S", "Flake8Bandit"),
    ("T10", "Flake8Debugger"),
    ("T20", "Flake8Print"),
    ("TID", "Flake8TidyImports"),
    ("UP", "Pyupgrade"),
    ("W", "Pycodestyle"),
    ("YTT", "Flake82020"),
    ("PIE", "Flake8Pie"),
    ("RUF", "Ruff"),
];

#[cfg(test)]
mod tests {
    use super::PREFIX_TO_ORIGIN;

    #[test]
    fn order() {
        for (idx, (prefix, _)) in PREFIX_TO_ORIGIN.iter().enumerate() {
            for (prior_prefix, _) in PREFIX_TO_ORIGIN[..idx].iter() {
                assert!(!prefix.starts_with(prior_prefix));
            }
        }
    }
}
