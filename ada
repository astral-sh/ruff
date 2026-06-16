[1mdiff --git a/crates/ruff_linter/src/rules/flake8_executable/rules/shebang_leading_whitespace.rs b/crates/ruff_linter/src/rules/flake8_executable/rules/shebang_leading_whitespace.rs[m
[1mindex 8285746933..424d218934 100644[m
[1m--- a/crates/ruff_linter/src/rules/flake8_executable/rules/shebang_leading_whitespace.rs[m
[1m+++ b/crates/ruff_linter/src/rules/flake8_executable/rules/shebang_leading_whitespace.rs[m
[36m@@ -28,6 +28,14 @@[m [muse crate::{AlwaysFixableViolation, Edit, Fix};[m
 /// #!/usr/bin/env python3[m
 /// ```[m
 ///[m
[32m+[m[32m/// ## Fix safety[m
[32m+[m[32m/// This rule's fix is marked as unsafe when the whitespace before the shebang[m
[32m+[m[32m/// contains a newline. Deleting the newline shifts the following lines up,[m
[32m+[m[32m/// which can move an encoding declaration onto the second line, where Python[m
[32m+[m[32m/// honors it as a magic encoding comment (PEP 263) and may change how the file[m
[32m+[m[32m/// is decoded. When the whitespace contains no newline, the shebang is already[m
[32m+[m[32m/// on the first line and the fix is safe.[m
[32m+[m[32m///[m
 /// ## References[m
 /// - [Python documentation: Executable Python Scripts](https://docs.python.org/3/tutorial/appendix.html#executable-python-scripts)[m
 #[derive(ViolationMetadata)][m
[36m@@ -69,6 +77,21 @@[m [mpub(crate) fn shebang_leading_whitespace([m
     if let Some(mut diagnostic) =[m
         context.report_diagnostic_if_enabled(ShebangLeadingWhitespace, prefix)[m
     {[m
[31m-        diagnostic.set_fix(Fix::safe_edit(Edit::range_deletion(prefix)));[m
[32m+[m[32m        // The fix is only unsafe when the leading whitespace contains a newline:[m
[32m+[m[32m        // deleting it shifts the following lines up, which can move an encoding[m
[32m+[m[32m        // declaration onto the second line, where Python honors it as a magic[m
[32m+[m[32m        // encoding comment (PEP 263) and may change how the file is decoded.[m
[32m+[m[32m        // Without a newline the shebang is already on the first line and the fix[m
[32m+[m[32m        // moves no other lines, so it is safe.[m
[32m+[m[32m        let fix = if locator[m
[32m+[m[32m            .up_to(range.start())[m
[32m+[m[32m            .chars()[m
[32m+[m[32m            .any(|c| matches!(c, '\r' | '\n'))[m
[32m+[m[32m        {[m
[32m+[m[32m            Fix::unsafe_edit(Edit::range_deletion(prefix))[m
[32m+[m[32m        } else {[m
[32m+[m[32m            Fix::safe_edit(Edit::range_deletion(prefix))[m
[32m+[m[32m        };[m
[32m+[m[32m        diagnostic.set_fix(fix);[m
     }[m
 }[m
