//! Rules from [flake8-copyright](https://pypi.org/project/flake8-copyright/).
pub(crate) mod rules;

pub mod settings;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::registry::Rule;
    use crate::settings::Settings;
    use crate::test::test_path;
    use crate::test::test_snippet;
    use crate::{assert_messages, settings};

    #[test_case(Rule::MissingCopyrightNotice, Path::new("CPY001.py"))]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.noqa_code(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_copyright").join(path).as_path(),
            &Settings::for_rule(rule_code),
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test]
    fn notice() {
        let diagnostics = test_snippet(
            r#"
# Copyright 2023

import os
"#
            .trim(),
            &settings::Settings::for_rules(vec![Rule::MissingCopyrightNotice]),
        );
        assert_messages!(diagnostics);
    }

    #[test]
    fn notice_with_c() {
        let diagnostics = test_snippet(
            r#"
# Copyright (C) 2023

import os
"#
            .trim(),
            &settings::Settings::for_rules(vec![Rule::MissingCopyrightNotice]),
        );
        assert_messages!(diagnostics);
    }

    #[test]
    fn notice_with_caps() {
        let diagnostics = test_snippet(
            r#"
# COPYRIGHT (C) 2023

import os
"#
            .trim(),
            &settings::Settings::for_rules(vec![Rule::MissingCopyrightNotice]),
        );
        assert_messages!(diagnostics);
    }

    #[test]
    fn notice_with_range() {
        let diagnostics = test_snippet(
            r#"
# Copyright (C) 2021-2023

import os
"#
            .trim(),
            &settings::Settings::for_rules(vec![Rule::MissingCopyrightNotice]),
        );
        assert_messages!(diagnostics);
    }

    #[test]
    fn valid_author() {
        let diagnostics = test_snippet(
            r#"
# Copyright (C) 2023 Ruff

import os
"#
            .trim(),
            &settings::Settings {
                flake8_copyright: super::settings::Settings {
                    author: Some("Ruff".to_string()),
                    ..super::settings::Settings::default()
                },
                ..settings::Settings::for_rules(vec![Rule::MissingCopyrightNotice])
            },
        );
        assert_messages!(diagnostics);
    }

    #[test]
    fn invalid_author() {
        let diagnostics = test_snippet(
            r#"
# Copyright (C) 2023 Some Author

import os
"#
            .trim(),
            &settings::Settings {
                flake8_copyright: super::settings::Settings {
                    author: Some("Ruff".to_string()),
                    ..super::settings::Settings::default()
                },
                ..settings::Settings::for_rules(vec![Rule::MissingCopyrightNotice])
            },
        );
        assert_messages!(diagnostics);
    }

    #[test]
    fn small_file() {
        let diagnostics = test_snippet(
            r#"
import os
"#
            .trim(),
            &settings::Settings {
                flake8_copyright: super::settings::Settings {
                    min_file_size: 256,
                    ..super::settings::Settings::default()
                },
                ..settings::Settings::for_rules(vec![Rule::MissingCopyrightNotice])
            },
        );
        assert_messages!(diagnostics);
    }

    #[test]
    fn late_notice() {
        let diagnostics = test_snippet(
            r#"
# Content Content Content Content Content Content Content Content Content Content
# Content Content Content Content Content Content Content Content Content Content
# Content Content Content Content Content Content Content Content Content Content
# Content Content Content Content Content Content Content Content Content Content
# Content Content Content Content Content Content Content Content Content Content
# Content Content Content Content Content Content Content Content Content Content
# Content Content Content Content Content Content Content Content Content Content
# Content Content Content Content Content Content Content Content Content Content
# Content Content Content Content Content Content Content Content Content Content
# Content Content Content Content Content Content Content Content Content Content
# Content Content Content Content Content Content Content Content Content Content
# Content Content Content Content Content Content Content Content Content Content
# Content Content Content Content Content Content Content Content Content Content
# Content Content Content Content Content Content Content Content Content Content
# Content Content Content Content Content Content Content Content Content Content
# Content Content Content Content Content Content Content Content Content Content
# Content Content Content Content Content Content Content Content Content Content
# Content Content Content Content Content Content Content Content Content Content
# Content Content Content Content Content Content Content Content Content Content
# Content Content Content Content Content Content Content Content Content Content

# Copyright 2023
"#
            .trim(),
            &settings::Settings::for_rules(vec![Rule::MissingCopyrightNotice]),
        );
        assert_messages!(diagnostics);
    }
}
