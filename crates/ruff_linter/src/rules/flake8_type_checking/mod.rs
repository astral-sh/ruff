//! Rules from [flake8-type-checking](https://pypi.org/project/flake8-type-checking/).
pub(crate) mod helpers;
mod imports;
pub(crate) mod rules;
pub mod settings;

#[cfg(test)]
mod tests {
    use std::convert::AsRef;
    use std::path::Path;

    use anyhow::Result;
    use ruff_python_ast::PythonVersion;
    use test_case::test_case;

    use crate::registry::{Linter, Rule};
    use crate::test::{test_path, test_snippet};
    use crate::{assert_messages, settings};

    #[test_case(Rule::EmptyTypeCheckingBlock, Path::new("TC005.py"))]
    #[test_case(Rule::RuntimeCastValue, Path::new("TC006.py"))]
    #[test_case(Rule::RuntimeImportInTypeCheckingBlock, Path::new("TC004_1.py"))]
    #[test_case(Rule::RuntimeImportInTypeCheckingBlock, Path::new("TC004_10.py"))]
    #[test_case(Rule::RuntimeImportInTypeCheckingBlock, Path::new("TC004_11.py"))]
    #[test_case(Rule::RuntimeImportInTypeCheckingBlock, Path::new("TC004_12.py"))]
    #[test_case(Rule::RuntimeImportInTypeCheckingBlock, Path::new("TC004_13.py"))]
    #[test_case(Rule::RuntimeImportInTypeCheckingBlock, Path::new("TC004_14.pyi"))]
    #[test_case(Rule::RuntimeImportInTypeCheckingBlock, Path::new("TC004_15.py"))]
    #[test_case(Rule::RuntimeImportInTypeCheckingBlock, Path::new("TC004_16.py"))]
    #[test_case(Rule::RuntimeImportInTypeCheckingBlock, Path::new("TC004_17.py"))]
    #[test_case(Rule::RuntimeImportInTypeCheckingBlock, Path::new("TC004_2.py"))]
    #[test_case(Rule::RuntimeImportInTypeCheckingBlock, Path::new("TC004_3.py"))]
    #[test_case(Rule::RuntimeImportInTypeCheckingBlock, Path::new("TC004_4.py"))]
    #[test_case(Rule::RuntimeImportInTypeCheckingBlock, Path::new("TC004_5.py"))]
    #[test_case(Rule::RuntimeImportInTypeCheckingBlock, Path::new("TC004_6.py"))]
    #[test_case(Rule::RuntimeImportInTypeCheckingBlock, Path::new("TC004_7.py"))]
    #[test_case(Rule::RuntimeImportInTypeCheckingBlock, Path::new("TC004_8.py"))]
    #[test_case(Rule::RuntimeImportInTypeCheckingBlock, Path::new("TC004_9.py"))]
    #[test_case(Rule::RuntimeImportInTypeCheckingBlock, Path::new("quote.py"))]
    #[test_case(Rule::RuntimeStringUnion, Path::new("TC010_1.py"))]
    #[test_case(Rule::RuntimeStringUnion, Path::new("TC010_2.py"))]
    #[test_case(Rule::TypingOnlyFirstPartyImport, Path::new("TC001.py"))]
    #[test_case(Rule::TypingOnlyStandardLibraryImport, Path::new("TC003.py"))]
    #[test_case(Rule::TypingOnlyStandardLibraryImport, Path::new("init_var.py"))]
    #[test_case(Rule::TypingOnlyStandardLibraryImport, Path::new("kw_only.py"))]
    #[test_case(Rule::TypingOnlyStandardLibraryImport, Path::new("snapshot.py"))]
    #[test_case(
        Rule::TypingOnlyStandardLibraryImport,
        Path::new("singledispatchmethod.py")
    )]
    #[test_case(Rule::TypingOnlyThirdPartyImport, Path::new("TC002.py"))]
    #[test_case(Rule::TypingOnlyThirdPartyImport, Path::new("quote.py"))]
    #[test_case(Rule::TypingOnlyThirdPartyImport, Path::new("singledispatch.py"))]
    #[test_case(Rule::TypingOnlyThirdPartyImport, Path::new("strict.py"))]
    #[test_case(Rule::TypingOnlyThirdPartyImport, Path::new("typing_modules_1.py"))]
    #[test_case(Rule::TypingOnlyThirdPartyImport, Path::new("typing_modules_2.py"))]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.as_ref(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_type_checking").join(path).as_path(),
            &settings::LinterSettings::for_rule(rule_code),
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    // we test these rules as a pair, since they're opposites of one another
    // so we want to make sure their fixes are not going around in circles.
    #[test_case(Rule::UnquotedTypeAlias, Path::new("TC007.py"))]
    #[test_case(Rule::QuotedTypeAlias, Path::new("TC008.py"))]
    #[test_case(Rule::QuotedTypeAlias, Path::new("TC008_typing_execution_context.py"))]
    fn type_alias_rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.as_ref(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_type_checking").join(path).as_path(),
            &settings::LinterSettings::for_rules(vec![
                Rule::UnquotedTypeAlias,
                Rule::QuotedTypeAlias,
            ]),
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Rule::QuotedTypeAlias, Path::new("TC008_union_syntax_pre_py310.py"))]
    fn type_alias_rules_pre_py310(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!(
            "pre_py310_{}_{}",
            rule_code.as_ref(),
            path.to_string_lossy()
        );
        let diagnostics = test_path(
            Path::new("flake8_type_checking").join(path).as_path(),
            &settings::LinterSettings {
                unresolved_target_version: PythonVersion::PY39,
                ..settings::LinterSettings::for_rule(rule_code)
            },
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Rule::RuntimeImportInTypeCheckingBlock, Path::new("quote.py"))]
    #[test_case(Rule::TypingOnlyThirdPartyImport, Path::new("quote.py"))]
    #[test_case(Rule::RuntimeImportInTypeCheckingBlock, Path::new("quote2.py"))]
    #[test_case(Rule::TypingOnlyThirdPartyImport, Path::new("quote2.py"))]
    #[test_case(Rule::RuntimeImportInTypeCheckingBlock, Path::new("quote3.py"))]
    #[test_case(Rule::TypingOnlyThirdPartyImport, Path::new("quote3.py"))]
    fn quote(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("quote_{}_{}", rule_code.as_ref(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_type_checking").join(path).as_path(),
            &settings::LinterSettings {
                flake8_type_checking: super::settings::Settings {
                    quote_annotations: true,
                    ..Default::default()
                },
                ..settings::LinterSettings::for_rule(rule_code)
            },
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Rule::TypingOnlyThirdPartyImport, Path::new("strict.py"))]
    #[test_case(Rule::TypingOnlyStandardLibraryImport, Path::new("init_var.py"))]
    #[test_case(Rule::TypingOnlyStandardLibraryImport, Path::new("kw_only.py"))]
    fn strict(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("strict_{}_{}", rule_code.as_ref(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_type_checking").join(path).as_path(),
            &settings::LinterSettings {
                flake8_type_checking: super::settings::Settings {
                    strict: true,
                    ..Default::default()
                },
                ..settings::LinterSettings::for_rule(rule_code)
            },
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Rule::TypingOnlyThirdPartyImport, Path::new("exempt_modules.py"))]
    fn exempt_modules(rule_code: Rule, path: &Path) -> Result<()> {
        let diagnostics = test_path(
            Path::new("flake8_type_checking").join(path).as_path(),
            &settings::LinterSettings {
                flake8_type_checking: super::settings::Settings {
                    exempt_modules: vec!["pandas".to_string()],
                    ..Default::default()
                },
                ..settings::LinterSettings::for_rule(rule_code)
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test_case(
        Rule::TypingOnlyStandardLibraryImport,
        Path::new("exempt_type_checking_1.py")
    )]
    #[test_case(
        Rule::TypingOnlyStandardLibraryImport,
        Path::new("exempt_type_checking_2.py")
    )]
    #[test_case(
        Rule::TypingOnlyStandardLibraryImport,
        Path::new("exempt_type_checking_3.py")
    )]
    fn exempt_type_checking(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.as_ref(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_type_checking").join(path).as_path(),
            &settings::LinterSettings {
                flake8_type_checking: super::settings::Settings {
                    exempt_modules: vec![],
                    strict: true,
                    ..Default::default()
                },
                ..settings::LinterSettings::for_rule(rule_code)
            },
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(
        Rule::RuntimeImportInTypeCheckingBlock,
        Path::new("runtime_evaluated_base_classes_1.py")
    )]
    #[test_case(
        Rule::TypingOnlyThirdPartyImport,
        Path::new("runtime_evaluated_base_classes_2.py")
    )]
    #[test_case(
        Rule::TypingOnlyStandardLibraryImport,
        Path::new("runtime_evaluated_base_classes_3.py")
    )]
    #[test_case(
        Rule::TypingOnlyStandardLibraryImport,
        Path::new("runtime_evaluated_base_classes_4.py")
    )]
    #[test_case(
        Rule::TypingOnlyThirdPartyImport,
        Path::new("runtime_evaluated_base_classes_5.py")
    )]
    fn runtime_evaluated_base_classes(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.as_ref(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_type_checking").join(path).as_path(),
            &settings::LinterSettings {
                flake8_type_checking: super::settings::Settings {
                    runtime_required_base_classes: vec![
                        "pydantic.BaseModel".to_string(),
                        "sqlalchemy.orm.DeclarativeBase".to_string(),
                    ],
                    ..Default::default()
                },
                ..settings::LinterSettings::for_rule(rule_code)
            },
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(
        Rule::RuntimeImportInTypeCheckingBlock,
        Path::new("runtime_evaluated_decorators_1.py")
    )]
    #[test_case(
        Rule::TypingOnlyThirdPartyImport,
        Path::new("runtime_evaluated_decorators_2.py")
    )]
    #[test_case(
        Rule::TypingOnlyStandardLibraryImport,
        Path::new("runtime_evaluated_decorators_3.py")
    )]
    fn runtime_evaluated_decorators(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.as_ref(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_type_checking").join(path).as_path(),
            &settings::LinterSettings {
                flake8_type_checking: super::settings::Settings {
                    runtime_required_decorators: vec![
                        "attrs.define".to_string(),
                        "attrs.frozen".to_string(),
                        "pydantic.validate_call".to_string(),
                    ],
                    ..Default::default()
                },
                ..settings::LinterSettings::for_rule(rule_code)
            },
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Rule::TypingOnlyStandardLibraryImport, Path::new("module/direct.py"))]
    #[test_case(Rule::TypingOnlyStandardLibraryImport, Path::new("module/import.py"))]
    #[test_case(
        Rule::TypingOnlyStandardLibraryImport,
        Path::new("module/undefined.py")
    )]
    fn base_class_same_file(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.as_ref(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_type_checking").join(path).as_path(),
            &settings::LinterSettings {
                flake8_type_checking: super::settings::Settings {
                    runtime_required_base_classes: vec!["module.direct.MyBaseClass".to_string()],
                    ..Default::default()
                },
                ..settings::LinterSettings::for_rule(rule_code)
            },
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Rule::RuntimeImportInTypeCheckingBlock, Path::new("module/app.py"))]
    #[test_case(Rule::TypingOnlyStandardLibraryImport, Path::new("module/routes.py"))]
    fn decorator_same_file(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.as_ref(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_type_checking").join(path).as_path(),
            &settings::LinterSettings {
                flake8_type_checking: super::settings::Settings {
                    runtime_required_decorators: vec![
                        "fastapi.FastAPI.get".to_string(),
                        "fastapi.FastAPI.put".to_string(),
                        "module.app.AppContainer.app.get".to_string(),
                        "module.app.AppContainer.app.put".to_string(),
                        "module.app.app.get".to_string(),
                        "module.app.app.put".to_string(),
                        "module.app.app_container.app.get".to_string(),
                        "module.app.app_container.app.put".to_string(),
                    ],
                    ..Default::default()
                },
                ..settings::LinterSettings::for_rule(rule_code)
            },
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(
        r"
        from __future__ import annotations

        import pandas as pd

        def f(x: pd.DataFrame):
            pass
    ",
        "no_typing_import"
    )]
    #[test_case(
        r"
        from __future__ import annotations

        from typing import TYPE_CHECKING

        import pandas as pd

        def f(x: pd.DataFrame):
            pass
    ",
        "typing_import_before_package_import"
    )]
    #[test_case(
        r"
        from __future__ import annotations

        import pandas as pd

        from typing import TYPE_CHECKING

        def f(x: pd.DataFrame):
            pass
    ",
        "typing_import_after_package_import"
    )]
    #[test_case(
        r"
        from __future__ import annotations

        import pandas as pd

        def f(x: pd.DataFrame):
            pass

        from typing import TYPE_CHECKING
    ",
        "typing_import_after_usage"
    )]
    #[test_case(
        r"
        from __future__ import annotations

        from typing import TYPE_CHECKING

        import pandas as pd

        if TYPE_CHECKING:
            import os

        def f(x: pd.DataFrame):
            pass
    ",
        "type_checking_block_own_line"
    )]
    #[test_case(
        r"
        from __future__ import annotations

        from typing import TYPE_CHECKING

        import pandas as pd

        if TYPE_CHECKING: import os

        def f(x: pd.DataFrame):
            pass
    ",
        "type_checking_block_inline"
    )]
    #[test_case(
        r"
        from __future__ import annotations

        from typing import TYPE_CHECKING

        import pandas as pd

        if TYPE_CHECKING:
            # This is a comment.
            import os

        def f(x: pd.DataFrame):
            pass
    ",
        "type_checking_block_comment"
    )]
    #[test_case(
        r"
        from __future__ import annotations

        from typing import TYPE_CHECKING

        import pandas as pd

        def f(x: pd.DataFrame):
            pass

        if TYPE_CHECKING:
            import os
    ",
        "type_checking_block_after_usage"
    )]
    #[test_case(
        r"
        from __future__ import annotations

        from pandas import (
            DataFrame,  # DataFrame
            Series,  # Series
        )

        def f(x: DataFrame):
            pass
    ",
        "import_from"
    )]
    #[test_case(
        r"
        from __future__ import annotations

        from typing import TYPE_CHECKING

        from pandas import (
            DataFrame,  # DataFrame
            Series,  # Series
        )

        if TYPE_CHECKING:
            import os

        def f(x: DataFrame):
            pass
    ",
        "import_from_type_checking_block"
    )]
    #[test_case(
        r"
        from __future__ import annotations

        from typing import TYPE_CHECKING

        from pandas import (
            DataFrame,  # DataFrame
            Series,  # Series
        )

        def f(x: DataFrame, y: Series):
            pass
    ",
        "multiple_members"
    )]
    #[test_case(
        r"
        from __future__ import annotations

        from typing import TYPE_CHECKING

        import os, sys

        def f(x: os, y: sys):
            pass
    ",
        "multiple_modules_same_type"
    )]
    #[test_case(
        r"
        from __future__ import annotations

        from typing import TYPE_CHECKING

        import os, pandas

        def f(x: os, y: pandas):
            pass
    ",
        "multiple_modules_different_types"
    )]
    #[test_case(
        r"
        from __future__ import annotations

        from typing import TYPE_CHECKING, TypeAlias
        if TYPE_CHECKING:
            from foo import Foo  # TC004

        a: TypeAlias = Foo | None  # OK
    ",
        "tc004_precedence_over_tc007"
    )]
    #[test_case(
        r"
        from __future__ import annotations

        from typing import TypeAlias

        a: TypeAlias = 'int | None'  # TC008
        b: TypeAlias = 'int' | None  # TC010
    ",
        "tc010_precedence_over_tc008"
    )]
    fn contents(contents: &str, snapshot: &str) {
        let diagnostics = test_snippet(
            contents,
            &settings::LinterSettings::for_rules(Linter::Flake8TypeChecking.rules()),
        );
        assert_messages!(snapshot, diagnostics);
    }

    #[test_case(
        r"
        from __future__ import annotations

        TYPE_CHECKING = False
        if TYPE_CHECKING:
            from types import TracebackType

        def foo(tb: TracebackType): ...
    ",
        "github_issue_15681_regression_test"
    )]
    #[test_case(
        r"
        from __future__ import annotations

        import pathlib  # TC003

        TYPE_CHECKING = False
        if TYPE_CHECKING:
            from types import TracebackType

        def foo(tb: TracebackType) -> pathlib.Path: ...
    ",
        "github_issue_15681_fix_test"
    )]
    #[test_case(
        r"
        from __future__ import annotations

        TYPE_CHECKING = False
        if TYPE_CHECKING:
            from typing import Any, Literal, Never, Self
        else:
            def __getattr__(name: str):
                pass

        __all__ = ['TYPE_CHECKING', 'Any', 'Literal', 'Never', 'Self']
    ",
        "github_issue_16045"
    )]
    fn contents_preview(contents: &str, snapshot: &str) {
        let diagnostics = test_snippet(
            contents,
            &settings::LinterSettings {
                preview: settings::types::PreviewMode::Enabled,
                ..settings::LinterSettings::for_rules(Linter::Flake8TypeChecking.rules())
            },
        );
        assert_messages!(snapshot, diagnostics);
    }
}
