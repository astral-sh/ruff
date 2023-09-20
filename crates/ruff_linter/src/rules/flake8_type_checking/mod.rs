//! Rules from [flake8-type-checking](https://pypi.org/project/flake8-type-checking/).
pub(crate) mod helpers;
pub(crate) mod rules;
pub mod settings;

#[cfg(test)]
mod tests {
    use std::convert::AsRef;
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::registry::{Linter, Rule};
    use crate::test::{test_path, test_snippet};
    use crate::{assert_messages, settings};

    #[test_case(Rule::EmptyTypeCheckingBlock, Path::new("TCH005.py"))]
    #[test_case(Rule::RuntimeImportInTypeCheckingBlock, Path::new("TCH004_1.py"))]
    #[test_case(Rule::RuntimeImportInTypeCheckingBlock, Path::new("TCH004_10.py"))]
    #[test_case(Rule::RuntimeImportInTypeCheckingBlock, Path::new("TCH004_11.py"))]
    #[test_case(Rule::RuntimeImportInTypeCheckingBlock, Path::new("TCH004_12.py"))]
    #[test_case(Rule::RuntimeImportInTypeCheckingBlock, Path::new("TCH004_13.py"))]
    #[test_case(Rule::RuntimeImportInTypeCheckingBlock, Path::new("TCH004_14.pyi"))]
    #[test_case(Rule::RuntimeImportInTypeCheckingBlock, Path::new("TCH004_2.py"))]
    #[test_case(Rule::RuntimeImportInTypeCheckingBlock, Path::new("TCH004_3.py"))]
    #[test_case(Rule::RuntimeImportInTypeCheckingBlock, Path::new("TCH004_4.py"))]
    #[test_case(Rule::RuntimeImportInTypeCheckingBlock, Path::new("TCH004_5.py"))]
    #[test_case(Rule::RuntimeImportInTypeCheckingBlock, Path::new("TCH004_6.py"))]
    #[test_case(Rule::RuntimeImportInTypeCheckingBlock, Path::new("TCH004_7.py"))]
    #[test_case(Rule::RuntimeImportInTypeCheckingBlock, Path::new("TCH004_8.py"))]
    #[test_case(Rule::RuntimeImportInTypeCheckingBlock, Path::new("TCH004_9.py"))]
    #[test_case(Rule::TypingOnlyFirstPartyImport, Path::new("TCH001.py"))]
    #[test_case(Rule::TypingOnlyStandardLibraryImport, Path::new("TCH003.py"))]
    #[test_case(Rule::TypingOnlyStandardLibraryImport, Path::new("snapshot.py"))]
    #[test_case(Rule::TypingOnlyThirdPartyImport, Path::new("TCH002.py"))]
    #[test_case(Rule::TypingOnlyThirdPartyImport, Path::new("strict.py"))]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.as_ref(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_type_checking").join(path).as_path(),
            &settings::Settings::for_rule(rule_code),
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Rule::TypingOnlyThirdPartyImport, Path::new("strict.py"))]
    fn strict(rule_code: Rule, path: &Path) -> Result<()> {
        let diagnostics = test_path(
            Path::new("flake8_type_checking").join(path).as_path(),
            &settings::Settings {
                flake8_type_checking: super::settings::Settings {
                    strict: true,
                    ..Default::default()
                },
                ..settings::Settings::for_rule(rule_code)
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test_case(Rule::TypingOnlyThirdPartyImport, Path::new("exempt_modules.py"))]
    fn exempt_modules(rule_code: Rule, path: &Path) -> Result<()> {
        let diagnostics = test_path(
            Path::new("flake8_type_checking").join(path).as_path(),
            &settings::Settings {
                flake8_type_checking: super::settings::Settings {
                    exempt_modules: vec!["pandas".to_string()],
                    ..Default::default()
                },
                ..settings::Settings::for_rule(rule_code)
            },
        )?;
        assert_messages!(diagnostics);
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
    fn runtime_evaluated_base_classes(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.as_ref(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_type_checking").join(path).as_path(),
            &settings::Settings {
                flake8_type_checking: super::settings::Settings {
                    runtime_evaluated_base_classes: vec![
                        "pydantic.BaseModel".to_string(),
                        "sqlalchemy.orm.DeclarativeBase".to_string(),
                    ],
                    ..Default::default()
                },
                ..settings::Settings::for_rule(rule_code)
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
            &settings::Settings {
                flake8_type_checking: super::settings::Settings {
                    runtime_evaluated_decorators: vec![
                        "attrs.define".to_string(),
                        "attrs.frozen".to_string(),
                    ],
                    ..Default::default()
                },
                ..settings::Settings::for_rule(rule_code)
            },
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(
        r#"
        from __future__ import annotations

        import pandas as pd

        def f(x: pd.DataFrame):
            pass
    "#,
        "no_typing_import"
    )]
    #[test_case(
        r#"
        from __future__ import annotations

        from typing import TYPE_CHECKING

        import pandas as pd

        def f(x: pd.DataFrame):
            pass
    "#,
        "typing_import_before_package_import"
    )]
    #[test_case(
        r#"
        from __future__ import annotations

        import pandas as pd

        from typing import TYPE_CHECKING

        def f(x: pd.DataFrame):
            pass
    "#,
        "typing_import_after_package_import"
    )]
    #[test_case(
        r#"
        from __future__ import annotations

        import pandas as pd

        def f(x: pd.DataFrame):
            pass

        from typing import TYPE_CHECKING
    "#,
        "typing_import_after_usage"
    )]
    #[test_case(
        r#"
        from __future__ import annotations

        from typing import TYPE_CHECKING

        import pandas as pd

        if TYPE_CHECKING:
            import os

        def f(x: pd.DataFrame):
            pass
    "#,
        "type_checking_block_own_line"
    )]
    #[test_case(
        r#"
        from __future__ import annotations

        from typing import TYPE_CHECKING

        import pandas as pd

        if TYPE_CHECKING: import os

        def f(x: pd.DataFrame):
            pass
    "#,
        "type_checking_block_inline"
    )]
    #[test_case(
        r#"
        from __future__ import annotations

        from typing import TYPE_CHECKING

        import pandas as pd

        if TYPE_CHECKING:
            # This is a comment.
            import os

        def f(x: pd.DataFrame):
            pass
    "#,
        "type_checking_block_comment"
    )]
    #[test_case(
        r#"
        from __future__ import annotations

        from typing import TYPE_CHECKING

        import pandas as pd

        def f(x: pd.DataFrame):
            pass

        if TYPE_CHECKING:
            import os
    "#,
        "type_checking_block_after_usage"
    )]
    #[test_case(
        r#"
        from __future__ import annotations

        from pandas import (
            DataFrame,  # DataFrame
            Series,  # Series
        )

        def f(x: DataFrame):
            pass
    "#,
        "import_from"
    )]
    #[test_case(
        r#"
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
    "#,
        "import_from_type_checking_block"
    )]
    #[test_case(
        r#"
        from __future__ import annotations

        from typing import TYPE_CHECKING

        from pandas import (
            DataFrame,  # DataFrame
            Series,  # Series
        )

        def f(x: DataFrame, y: Series):
            pass
    "#,
        "multiple_members"
    )]
    #[test_case(
        r#"
        from __future__ import annotations

        from typing import TYPE_CHECKING

        import os, sys

        def f(x: os, y: sys):
            pass
    "#,
        "multiple_modules_same_type"
    )]
    #[test_case(
        r#"
        from __future__ import annotations

        from typing import TYPE_CHECKING

        import os, pandas

        def f(x: os, y: pandas):
            pass
    "#,
        "multiple_modules_different_types"
    )]
    fn contents(contents: &str, snapshot: &str) {
        let diagnostics = test_snippet(
            contents,
            &settings::Settings::for_rules(Linter::Flake8TypeChecking.rules()),
        );
        assert_messages!(snapshot, diagnostics);
    }
}
