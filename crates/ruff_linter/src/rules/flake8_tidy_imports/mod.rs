//! Rules from [flake8-tidy-imports](https://pypi.org/project/flake8-tidy-imports/).
pub(crate) mod matchers;
pub(crate) mod rules;
pub mod settings;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use ruff_python_ast::PythonVersion;
    use ruff_python_trivia::textwrap::dedent;
    use rustc_hash::FxHashMap;

    use crate::assert_diagnostics;
    use crate::registry::Rule;
    use crate::rules::flake8_tidy_imports;
    use crate::rules::flake8_tidy_imports::settings::{
        AllImports, ApiBan, ImportSelection, ImportSelector, ImportSelectorSettings, Strictness,
    };
    use crate::settings::LinterSettings;
    use crate::source_kind::SourceKind;
    use crate::test::{test_contents, test_path};

    #[test]
    fn banned_api() -> Result<()> {
        let diagnostics = test_path(
            Path::new("flake8_tidy_imports/TID251.py"),
            &LinterSettings {
                flake8_tidy_imports: flake8_tidy_imports::settings::Settings {
                    banned_api: FxHashMap::from_iter([
                        (
                            "cgi".to_string(),
                            ApiBan {
                                msg: "The cgi module is deprecated.".to_string(),
                            },
                        ),
                        (
                            "typing.TypedDict".to_string(),
                            ApiBan {
                                msg: "Use typing_extensions.TypedDict instead.".to_string(),
                            },
                        ),
                    ]),
                    ..Default::default()
                },
                ..LinterSettings::for_rules(vec![Rule::BannedApi])
            },
        )?;
        assert_diagnostics!(diagnostics);
        Ok(())
    }

    #[test]
    fn banned_api_package() -> Result<()> {
        let diagnostics = test_path(
            Path::new("flake8_tidy_imports/TID/my_package/sublib/api/application.py"),
            &LinterSettings {
                flake8_tidy_imports: flake8_tidy_imports::settings::Settings {
                    banned_api: FxHashMap::from_iter([
                        (
                            "attrs".to_string(),
                            ApiBan {
                                msg: "The attrs module is deprecated.".to_string(),
                            },
                        ),
                        (
                            "my_package.sublib.protocol".to_string(),
                            ApiBan {
                                msg: "The protocol module is deprecated.".to_string(),
                            },
                        ),
                    ]),
                    ..Default::default()
                },
                namespace_packages: vec![Path::new("my_package").to_path_buf()],
                ..LinterSettings::for_rules(vec![Rule::BannedApi])
            },
        )?;
        assert_diagnostics!(diagnostics);
        Ok(())
    }

    #[test]
    fn ban_parent_imports() -> Result<()> {
        let diagnostics = test_path(
            Path::new("flake8_tidy_imports/TID252.py"),
            &LinterSettings {
                flake8_tidy_imports: flake8_tidy_imports::settings::Settings {
                    ban_relative_imports: Strictness::Parents,
                    ..Default::default()
                },
                ..LinterSettings::for_rules(vec![Rule::RelativeImports])
            },
        )?;
        assert_diagnostics!(diagnostics);
        Ok(())
    }

    #[test]
    fn ban_all_imports() -> Result<()> {
        let diagnostics = test_path(
            Path::new("flake8_tidy_imports/TID252.py"),
            &LinterSettings {
                flake8_tidy_imports: flake8_tidy_imports::settings::Settings {
                    ban_relative_imports: Strictness::All,
                    ..Default::default()
                },
                ..LinterSettings::for_rules(vec![Rule::RelativeImports])
            },
        )?;
        assert_diagnostics!(diagnostics);
        Ok(())
    }

    #[test]
    fn ban_parent_imports_package() -> Result<()> {
        let diagnostics = test_path(
            Path::new("flake8_tidy_imports/TID/my_package/sublib/api/application.py"),
            &LinterSettings {
                flake8_tidy_imports: flake8_tidy_imports::settings::Settings {
                    ban_relative_imports: Strictness::Parents,
                    ..Default::default()
                },
                namespace_packages: vec![Path::new("my_package").to_path_buf()],
                ..LinterSettings::for_rules(vec![Rule::RelativeImports])
            },
        )?;
        assert_diagnostics!(diagnostics);
        Ok(())
    }

    #[test]
    fn banned_module_level_imports() -> Result<()> {
        let diagnostics = test_path(
            Path::new("flake8_tidy_imports/TID253.py"),
            &LinterSettings {
                flake8_tidy_imports: flake8_tidy_imports::settings::Settings {
                    banned_module_level_imports: vec![
                        "torch".to_string(),
                        "tensorflow".to_string(),
                    ],
                    ..Default::default()
                },
                ..LinterSettings::for_rules(vec![Rule::BannedModuleLevelImports])
            },
        )?;
        assert_diagnostics!(diagnostics);
        Ok(())
    }

    #[test]
    fn preview_lazy_import_mismatch() -> Result<()> {
        let diagnostics = test_path(
            Path::new("flake8_tidy_imports/TID254.py"),
            &LinterSettings {
                flake8_tidy_imports: flake8_tidy_imports::settings::Settings {
                    require_lazy: ImportSelector::Selection(ImportSelection::Imports(vec![
                        "typing".to_string(),
                        "foo".to_string(),
                        "email".to_string(),
                        "bar".to_string(),
                        "starry".to_string(),
                        "collections".to_string(),
                        "pkg".to_string(),
                    ])),
                    ..Default::default()
                },
                ..LinterSettings::for_rule(Rule::LazyImportMismatch)
                    .with_preview_mode()
                    .with_target_version(PythonVersion::PY315)
            },
        )?;
        assert_diagnostics!(diagnostics);
        Ok(())
    }

    #[test]
    fn preview_lazy_import_mismatch_all() -> Result<()> {
        let diagnostics = test_path(
            Path::new("flake8_tidy_imports/TID254.py"),
            &LinterSettings {
                flake8_tidy_imports: flake8_tidy_imports::settings::Settings {
                    require_lazy: ImportSelector::Selection(ImportSelection::All(AllImports::All)),
                    ..Default::default()
                },
                ..LinterSettings::for_rule(Rule::LazyImportMismatch)
                    .with_preview_mode()
                    .with_target_version(PythonVersion::PY315)
            },
        )?;
        assert_diagnostics!(diagnostics);
        Ok(())
    }

    #[test]
    fn preview_lazy_import_mismatch_pre_py315() -> Result<()> {
        let diagnostics = test_path(
            Path::new("flake8_tidy_imports/TID254_py314.py"),
            &LinterSettings {
                flake8_tidy_imports: flake8_tidy_imports::settings::Settings {
                    require_lazy: ImportSelector::Selection(ImportSelection::All(AllImports::All)),
                    ..Default::default()
                },
                ..LinterSettings::for_rule(Rule::LazyImportMismatch)
                    .with_preview_mode()
                    .with_target_version(PythonVersion::PY314)
            },
        )?;
        assert!(diagnostics.is_empty());
        Ok(())
    }

    #[test]
    fn preview_lazy_import_immediately_resolved() -> Result<()> {
        let diagnostics = test_path(
            Path::new("flake8_tidy_imports/TID255.py"),
            &LinterSettings::for_rule(Rule::LazyImportImmediatelyResolved)
                .with_preview_mode()
                .with_target_version(PythonVersion::PY315),
        )?;
        assert_diagnostics!(diagnostics);
        Ok(())
    }

    #[test]
    fn preview_lazy_import_immediately_resolved_fix() {
        let source = dedent(
            r#"
            lazy  import foo
            lazy  from library import Component

            base = foo.Base
            component = Component
            "#,
        );
        let expected = dedent(
            r#"
            import foo
            from library import Component

            base = foo.Base
            component = Component
            "#,
        );

        let source_kind = SourceKind::Python {
            code: source.to_string(),
            is_stub: false,
        };

        let (diagnostics, fixed) = test_contents(
            &source_kind,
            Path::new("flake8_tidy_imports/TID255_fix.py"),
            &LinterSettings::for_rule(Rule::LazyImportImmediatelyResolved)
                .with_preview_mode()
                .with_target_version(PythonVersion::PY315),
        );

        assert_eq!(diagnostics.len(), 2);
        assert_eq!(fixed.source_code(), expected);
    }

    #[test]
    fn preview_lazy_import_mismatch_fix() {
        let source = dedent(
            r#"
            from __future__ import annotations

            import os

            if True:
                import email

            with manager():
                from foo import bar

            try:
                import collections
            except Exception:
                pass

            from starry import *

            def func():
                import fractions

            class Example:
                import decimal
            "#,
        );
        let expected = dedent(
            r#"
            from __future__ import annotations

            lazy import os

            if True:
                lazy import email

            with manager():
                lazy from foo import bar

            try:
                import collections
            except Exception:
                pass

            from starry import *

            def func():
                import fractions

            class Example:
                import decimal
            "#,
        );

        let source_kind = SourceKind::Python {
            code: source.to_string(),
            is_stub: false,
        };

        let (diagnostics, fixed) = test_contents(
            &source_kind,
            Path::new("flake8_tidy_imports/TID254_fix.py"),
            &LinterSettings {
                flake8_tidy_imports: flake8_tidy_imports::settings::Settings {
                    require_lazy: ImportSelector::Selection(ImportSelection::All(AllImports::All)),
                    ..Default::default()
                },
                ..LinterSettings::for_rule(Rule::LazyImportMismatch)
                    .with_preview_mode()
                    .with_target_version(PythonVersion::PY315)
            },
        );

        assert_eq!(diagnostics.len(), 3);
        assert_eq!(fixed.source_code(), expected);
    }

    #[test]
    fn preview_lazy_import_mismatch_dotted_module() {
        let source = dedent(
            r#"
            import foo
            import foo.bar

            from foo import bar
            from foo import baz
            "#,
        );
        let expected = dedent(
            r#"
            import foo
            lazy import foo.bar

            lazy from foo import bar
            from foo import baz
            "#,
        );

        let source_kind = SourceKind::Python {
            code: source.to_string(),
            is_stub: false,
        };

        let (diagnostics, fixed) = test_contents(
            &source_kind,
            Path::new("flake8_tidy_imports/TID254_dotted.py"),
            &LinterSettings {
                flake8_tidy_imports: flake8_tidy_imports::settings::Settings {
                    require_lazy: ImportSelector::Selection(ImportSelection::Imports(vec![
                        "foo.bar".to_string(),
                    ])),
                    ..Default::default()
                },
                ..LinterSettings::for_rule(Rule::LazyImportMismatch)
                    .with_preview_mode()
                    .with_target_version(PythonVersion::PY315)
            },
        );

        assert_eq!(diagnostics.len(), 2);
        assert_eq!(fixed.source_code(), expected);
    }

    #[test]
    fn preview_lazy_import_mismatch_exclude() {
        let source = dedent(
            r#"
            import sitecustomize
            import typing
            import typing_extensions
            "#,
        );
        let expected = dedent(
            r#"
            import sitecustomize
            lazy import typing
            lazy import typing_extensions
            "#,
        );

        let source_kind = SourceKind::Python {
            code: source.to_string(),
            is_stub: false,
        };

        let (diagnostics, fixed) = test_contents(
            &source_kind,
            Path::new("flake8_tidy_imports/TID254_exclude.py"),
            &LinterSettings {
                flake8_tidy_imports: flake8_tidy_imports::settings::Settings {
                    require_lazy: ImportSelector::Settings(ImportSelectorSettings {
                        include: ImportSelection::All(AllImports::All),
                        exclude: vec!["sitecustomize".to_string()],
                    }),
                    ..Default::default()
                },
                ..LinterSettings::for_rule(Rule::LazyImportMismatch)
                    .with_preview_mode()
                    .with_target_version(PythonVersion::PY315)
            },
        );

        assert_eq!(diagnostics.len(), 2);
        assert_eq!(fixed.source_code(), expected);
    }

    #[test]
    fn preview_lazy_import_mismatch_ban_lazy() {
        let source = dedent(
            r#"
            lazy import sitecustomize
            lazy import typing
            lazy from foo import bar
            "#,
        );
        let expected = dedent(
            r#"
            lazy import sitecustomize
            import typing
            from foo import bar
            "#,
        );

        let source_kind = SourceKind::Python {
            code: source.to_string(),
            is_stub: false,
        };

        let (diagnostics, fixed) = test_contents(
            &source_kind,
            Path::new("flake8_tidy_imports/TID254_ban_lazy.py"),
            &LinterSettings {
                flake8_tidy_imports: flake8_tidy_imports::settings::Settings {
                    ban_lazy: ImportSelector::Settings(ImportSelectorSettings {
                        include: ImportSelection::All(AllImports::All),
                        exclude: vec!["sitecustomize".to_string()],
                    }),
                    ..Default::default()
                },
                ..LinterSettings::for_rule(Rule::LazyImportMismatch)
                    .with_preview_mode()
                    .with_target_version(PythonVersion::PY315)
            },
        );

        assert_eq!(diagnostics.len(), 2);
        assert_eq!(fixed.source_code(), expected);
    }
}
