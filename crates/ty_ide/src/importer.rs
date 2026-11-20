//! Tests for the shared importer using the IDE's cursor fixtures.

#[cfg(test)]
mod tests {
    use insta::assert_snapshot;

    use crate::tests::{CursorTest, cursor_test};
    use ruff_python_ast::find_node::covering_node;
    use ruff_text_size::{Ranged, TextRange};
    use ty_python_core::ProgramFile;
    use ty_python_semantic::importer::{ImportRequest, Importer};

    impl CursorTest {
        fn import(&self, module: &str, member: &str) -> String {
            self.add(ImportRequest::import(module, member))
        }

        fn import_from(&self, module: &str, member: &str) -> String {
            self.add(ImportRequest::import_from(module, member))
        }

        fn module(&self, module: &str) -> String {
            self.add(ImportRequest::module(module))
        }

        fn add(&self, request: ImportRequest<'_>) -> String {
            let node = covering_node(
                self.cursor.parsed.syntax().into(),
                TextRange::empty(self.cursor.offset),
            )
            .node();
            let importer = self.importer();
            let members = importer.members_in_scope_at(node, self.cursor.offset);
            let resp = importer.import(request, &members);

            // We attempt to emulate what an LSP client would
            // do here and "insert" the import into the original
            // source document. I'm not 100% sure this models
            // reality correctly, but in particular, we are
            // careful to insert the symbol name first since
            // it *should* come after the import.
            let mut source = self.cursor.source.to_string();
            source.insert_str(self.cursor.offset.to_usize(), resp.symbol_text());
            if let Some(edit) = resp.import() {
                assert!(
                    edit.range().start() <= self.cursor.offset,
                    "import edit must come at or before <CURSOR>, \
                     but <CURSOR> starts at {} and the import \
                     edit is at {}..{}",
                    self.cursor.offset.to_usize(),
                    edit.range().start().to_usize(),
                    edit.range().end().to_usize(),
                );
                source.replace_range(edit.range().to_std_range(), edit.content().unwrap_or(""));
            }
            source
        }

        fn importer(&self) -> Importer<'_> {
            Importer::new(
                &self.db,
                ProgramFile::new(
                    &self.db,
                    self.cursor.file,
                    self.db.program_environment().program(&self.db),
                ),
                &self.cursor.parsed,
            )
        }
    }

    #[test]
    fn empty_source_qualified() {
        let test = cursor_test("<CURSOR>");
        assert_snapshot!(
            test.import("collections", "defaultdict"), @"
        import collections
        collections.defaultdict
        ");
    }

    #[test]
    fn empty_source_unqualified() {
        let test = cursor_test("<CURSOR>");
        assert_snapshot!(
            test.import_from("collections", "defaultdict"), @"
        from collections import defaultdict
        defaultdict
        ");
    }

    #[test]
    fn import_exists_qualified() {
        let test = cursor_test(
            "\
import collections
<CURSOR>
        ",
        );
        assert_snapshot!(
            test.import("collections", "defaultdict"), @"
        import collections
        collections.defaultdict
        ");
    }

    #[test]
    fn import_exists_unqualified() {
        let test = cursor_test(
            "\
from collections import defaultdict
<CURSOR>
        ",
        );
        assert_snapshot!(
            test.import("collections", "defaultdict"), @"
        from collections import defaultdict
        defaultdict
        ");
    }

    #[test]
    fn import_exists_glob() {
        let test = cursor_test(
            "\
from collections import *
<CURSOR>
        ",
        );
        assert_snapshot!(
            test.import("collections", "defaultdict"), @"
        from collections import *
        defaultdict
        ");
    }

    #[test]
    fn import_exists_qualified_aliased() {
        let test = cursor_test(
            "\
import collections as c
<CURSOR>
        ",
        );
        assert_snapshot!(
            test.import("collections", "defaultdict"), @"
        import collections as c
        c.defaultdict
        ");
    }

    #[test]
    fn import_exists_unqualified_aliased() {
        let test = cursor_test(
            "\
from collections import defaultdict as ddict
<CURSOR>
        ",
        );
        assert_snapshot!(
            test.import("collections", "defaultdict"), @"
        from collections import defaultdict as ddict
        ddict
        ");
    }

    #[test]
    fn import_partially_exists_single() {
        let test = cursor_test(
            "\
from collections import Counter
<CURSOR>
        ",
        );
        assert_snapshot!(
            test.import("collections", "defaultdict"), @"
        from collections import Counter, defaultdict
        defaultdict
        ");
    }

    #[test]
    fn import_partially_exists_aliased_single() {
        let test = cursor_test(
            "\
from collections import Counter as C
<CURSOR>
        ",
        );
        assert_snapshot!(
            test.import("collections", "defaultdict"), @"
        from collections import Counter as C, defaultdict
        defaultdict
        ");
    }

    #[test]
    fn import_partially_exists_multi() {
        let test = cursor_test(
            "\
from collections import Counter, OrderedDict
<CURSOR>
        ",
        );
        assert_snapshot!(
            test.import("collections", "defaultdict"), @"
        from collections import Counter, OrderedDict, defaultdict
        defaultdict
        ");
    }

    #[test]
    fn import_partially_exists_aliased_multi() {
        let test = cursor_test(
            "\
from collections import Counter as C, OrderedDict as OD
<CURSOR>
        ",
        );
        assert_snapshot!(
            test.import("collections", "defaultdict"), @"
        from collections import Counter as C, OrderedDict as OD, defaultdict
        defaultdict
        ");
    }

    #[test]
    fn import_partially_exists_semi_colon() {
        let test = cursor_test(
            "\
from collections import Counter;
<CURSOR>
        ",
        );
        assert_snapshot!(
            test.import("collections", "defaultdict"), @"
        from collections import Counter, defaultdict;
        defaultdict
        ");
    }

    #[test]
    fn import_partially_exists_continuation() {
        let test = cursor_test(
            "\
from collections import Counter, \\
  OrderedDict
<CURSOR>
        ",
        );
        assert_snapshot!(
            test.import("collections", "defaultdict"), @r"
        from collections import Counter, \
          OrderedDict, defaultdict
        defaultdict
        ");
    }

    #[test]
    fn import_partially_exists_parentheses_single() {
        let test = cursor_test(
            "\
from collections import (Counter)
<CURSOR>
        ",
        );
        assert_snapshot!(
            test.import("collections", "defaultdict"), @"
        from collections import (Counter, defaultdict)
        defaultdict
        ");
    }

    #[test]
    fn import_partially_exists_parentheses_trailing_comma() {
        let test = cursor_test(
            "\
from collections import (Counter,)
<CURSOR>
        ",
        );
        assert_snapshot!(
            test.import("collections", "defaultdict"), @"
        from collections import (Counter, defaultdict,)
        defaultdict
        ");
    }

    #[test]
    fn import_partially_exists_parentheses_multi_line_trailing_comma() {
        let test = cursor_test(
            "\
from collections import (
    Counter,
    OrderedDict,
)
<CURSOR>
        ",
        );
        assert_snapshot!(
            test.import("collections", "defaultdict"), @"
        from collections import (
            Counter,
            OrderedDict, defaultdict,
        )
        defaultdict
        ");
    }

    #[test]
    fn import_partially_exists_parentheses_multi_line_no_trailing_comma() {
        let test = cursor_test(
            "\
from collections import (
    Counter,
    OrderedDict
)
<CURSOR>
        ",
        );
        assert_snapshot!(
            test.import("collections", "defaultdict"), @"
        from collections import (
            Counter,
            OrderedDict, defaultdict
        )
        defaultdict
        ");
    }

    #[test]
    fn import_partially_exists_relative() {
        let test = CursorTest::builder()
            .source("package/__init__.py", "")
            .source("package/foo.py", "Foo = 1\nBar = 2\n")
            .source(
                "package/sub1/sub2/quux.py",
                "from ...foo import Foo\n<CURSOR>\n",
            )
            .build();
        assert_snapshot!(
            test.import("package.foo", "Bar"), @"
        from ...foo import Foo, Bar
        Bar
        ");
    }

    #[test]
    fn import_partially_exists_incomplete() {
        let test = cursor_test(
            "\
from collections import
<CURSOR>
        ",
        );
        assert_snapshot!(
            test.import("collections", "defaultdict"), @"
        from collections import defaultdict
        defaultdict
        ");
    }

    #[test]
    fn import_partially_exists_incomplete_parentheses1() {
        let test = cursor_test(
            "\
from collections import ()
<CURSOR>
        ",
        );
        // In this case, because of the `()` being an
        // invalid AST, our importer gives up and just
        // adds a new line. We could add more heuristics
        // to make this case work, but I think there will
        // always be some cases like this that won't make
        // sense.
        assert_snapshot!(
            test.import("collections", "defaultdict"), @"
        from collections import ()
        from collections import defaultdict
        defaultdict
        ");
    }

    #[test]
    fn priority_unqualified_over_unqualified() {
        let test = cursor_test(
            "\
from collections import defaultdict
import re
from collections import defaultdict
<CURSOR>
        ",
        );
        assert_snapshot!(
            test.import("collections", "defaultdict"), @"
        from collections import defaultdict
        import re
        from collections import defaultdict
        defaultdict
        ");
    }

    #[test]
    fn priority_unqualified_over_unqualified_between() {
        let test = cursor_test(
            "\
from collections import defaultdict
import re
<CURSOR>
from collections import defaultdict
        ",
        );
        assert_snapshot!(
            test.import("collections", "defaultdict"), @"
        from collections import defaultdict
        import re
        defaultdict
        from collections import defaultdict
        ");
    }

    #[test]
    fn priority_unqualified_over_qualified() {
        let test = cursor_test(
            "\
import collections
from collections import defaultdict
<CURSOR>
        ",
        );
        assert_snapshot!(
            test.import("collections", "defaultdict"), @"
        import collections
        from collections import defaultdict
        defaultdict
        ");
    }

    #[test]
    fn priority_unqualified_over_partial() {
        let test = cursor_test(
            "\
from collections import OrderedDict
from collections import defaultdict
<CURSOR>
        ",
        );
        assert_snapshot!(
            test.import("collections", "defaultdict"), @"
        from collections import OrderedDict
        from collections import defaultdict
        defaultdict
        ");
    }

    #[test]
    fn priority_qualified_over_partial() {
        let test = cursor_test(
            "\
from collections import OrderedDict
import collections
<CURSOR>
        ",
        );
        assert_snapshot!(
            test.import("collections", "defaultdict"), @"
        from collections import OrderedDict, defaultdict
        import collections
        defaultdict
        ");
    }

    #[test]
    fn out_of_scope_ordering_top_level() {
        let test = cursor_test(
            "\
<CURSOR>
from collections import defaultdict
        ",
        );
        // Since the import came after the cursor,
        // we add another import at the top-level
        // of the module.
        assert_snapshot!(
            test.import("collections", "defaultdict"), @"
        import collections
        collections.defaultdict
        from collections import defaultdict
        ");
        assert_snapshot!(
            test.import_from("collections", "defaultdict"), @"
        from collections import defaultdict
        defaultdict
        from collections import defaultdict
        ");
    }

    #[test]
    fn out_of_scope_ordering_within_function_add_import() {
        let test = cursor_test(
            "\
def foo():
    <CURSOR>
from collections import defaultdict
        ",
        );
        // Since the import came after the cursor,
        // we add another import at the top-level
        // of the module.
        assert_snapshot!(
            test.import("collections", "defaultdict"), @"
        import collections
        def foo():
            collections.defaultdict
        from collections import defaultdict
        ");
    }

    #[test]
    fn in_scope_ordering_within_function() {
        let test = cursor_test(
            "\
from collections import defaultdict

def foo():
    <CURSOR>
        ",
        );
        assert_snapshot!(
            test.import("collections", "defaultdict"), @"
        from collections import defaultdict

        def foo():
            defaultdict
        ");
    }

    #[test]
    fn existing_future_import() {
        let test = cursor_test(
            "\
from __future__ import annotations

<CURSOR>
        ",
        );
        assert_snapshot!(
            test.import("typing", "TypeVar"), @"
        from __future__ import annotations
        import typing

        typing.TypeVar
        ");
    }

    #[test]
    fn existing_future_import_after_docstring() {
        let test = cursor_test(
            r#"
"This is a module level docstring"
from __future__ import annotations

<CURSOR>
        "#,
        );
        assert_snapshot!(
            test.import("typing", "TypeVar"), @r#"

        "This is a module level docstring"
        from __future__ import annotations
        import typing

        typing.TypeVar
        "#);
    }

    #[test]
    fn lazy_future_import_is_not_special() {
        // Lazy `__future__` imports must not act like real future-import anchors for insertion.
        let test = cursor_test(
            "\
lazy from __future__ import annotations

<CURSOR>
        ",
        );
        assert_snapshot!(
            test.import("typing", "TypeVar"), @"
        import typing
        lazy from __future__ import annotations

        typing.TypeVar
        ");
    }

    #[test]
    fn qualify_symbol_to_avoid_overwriting_other_symbol_in_scope() {
        let test = cursor_test(
            "\
defaultdict = 1
(<CURSOR>)
        ",
        );

        assert_snapshot!(
            test.import("collections", "defaultdict"), @"
        import collections
        defaultdict = 1
        (collections.defaultdict)
        ");
        assert_snapshot!(
            test.import_from("collections", "defaultdict"), @"
        import collections
        defaultdict = 1
        (collections.defaultdict)
        ");
    }

    #[test]
    fn unqualify_symbol_to_avoid_overwriting_other_symbol_in_scope() {
        let test = cursor_test(
            "\
collections = 1
(<CURSOR>)
        ",
        );

        assert_snapshot!(
            test.import("collections", "defaultdict"), @"
        from collections import defaultdict
        collections = 1
        (defaultdict)
        ");
        assert_snapshot!(
            test.import_from("collections", "defaultdict"), @"
        from collections import defaultdict
        collections = 1
        (defaultdict)
        ");
    }

    /// Tests a failure scenario where both the module
    /// name and the member name are in scope and defined
    /// as something other than a module. In this case,
    /// it's very difficult to auto-insert an import in a
    /// way that is correct.
    ///
    /// At time of writing (2025-09-15), we just insert a
    /// qualified import anyway, even though this will result
    /// in what is likely incorrect code. This seems better
    /// than some alternatives:
    ///
    /// 1. Silently do nothing.
    /// 2. Silently omit the symbol from completions.
    /// 3. Come up with an alias for the symbol.
    ///
    /// I think it would perhaps be ideal if we could somehow
    /// prompt the user for what they want to do. But I think
    /// this is okay for now. ---AG
    #[test]
    fn import_results_in_conflict() {
        let test = cursor_test(
            "\
collections = 1
defaultdict = 2
(<CURSOR>)
        ",
        );

        assert_snapshot!(
            test.import("collections", "defaultdict"), @"
        import collections
        collections = 1
        defaultdict = 2
        (collections.defaultdict)
        ");
        assert_snapshot!(
            test.import_from("collections", "defaultdict"), @"
        import collections
        collections = 1
        defaultdict = 2
        (collections.defaultdict)
        ");
    }

    #[test]
    fn within_function_definition_simple() {
        let test = cursor_test(
            "\
def foo():
    (<CURSOR>)
        ",
        );

        assert_snapshot!(
            test.import("collections", "defaultdict"), @"
        import collections
        def foo():
            (collections.defaultdict)
        ");
        assert_snapshot!(
            test.import_from("collections", "defaultdict"), @"
        from collections import defaultdict
        def foo():
            (defaultdict)
        ");
    }

    #[test]
    fn within_function_definition_member_conflict() {
        let test = cursor_test(
            "\
def defaultdict():
    (<CURSOR>)
        ",
        );

        assert_snapshot!(
            test.import("collections", "defaultdict"), @"
        import collections
        def defaultdict():
            (collections.defaultdict)
        ");
        assert_snapshot!(
            test.import_from("collections", "defaultdict"), @"
        import collections
        def defaultdict():
            (collections.defaultdict)
        ");
    }

    #[test]
    fn within_function_definition_module_conflict() {
        let test = cursor_test(
            "\
def collections():
    (<CURSOR>)
        ",
        );

        assert_snapshot!(
            test.import("collections", "defaultdict"), @"
        from collections import defaultdict
        def collections():
            (defaultdict)
        ");
        assert_snapshot!(
            test.import_from("collections", "defaultdict"), @"
        from collections import defaultdict
        def collections():
            (defaultdict)
        ");
    }

    #[test]
    fn member_conflict_with_other_import() {
        let test = cursor_test(
            "\
import defaultdict

(<CURSOR>)
        ",
        );

        assert_snapshot!(
            test.import("collections", "defaultdict"), @"
        import collections
        import defaultdict

        (collections.defaultdict)
        ");
        assert_snapshot!(
            test.import_from("collections", "defaultdict"), @"
        import collections
        import defaultdict

        (collections.defaultdict)
        ");
    }

    #[test]
    fn module_conflict_with_other_import() {
        let test = cursor_test(
            "\
from foo import collections

(<CURSOR>)
        ",
        );

        assert_snapshot!(
            test.import("collections", "defaultdict"), @"
        from collections import defaultdict
        from foo import collections

        (defaultdict)
        ");
        assert_snapshot!(
            test.import_from("collections", "defaultdict"), @"
        from collections import defaultdict
        from foo import collections

        (defaultdict)
        ");
    }

    #[test]
    fn member_conflict_with_other_member_import() {
        let test = cursor_test(
            "\
from othermodule import defaultdict

(<CURSOR>)
        ",
        );

        assert_snapshot!(
            test.import("collections", "defaultdict"), @"
        import collections
        from othermodule import defaultdict

        (collections.defaultdict)
        ");
        assert_snapshot!(
            test.import_from("collections", "defaultdict"), @"
        import collections
        from othermodule import defaultdict

        (collections.defaultdict)
        ");
    }

    #[test]
    fn member_conflict_with_other_module_import_alias() {
        let test = cursor_test(
            "\
import defaultdict as ddict

(<CURSOR>)
        ",
        );

        assert_snapshot!(
            test.import("collections", "defaultdict"), @"
        import collections
        import defaultdict as ddict

        (collections.defaultdict)
        ");
        assert_snapshot!(
            test.import_from("collections", "defaultdict"), @"
        from collections import defaultdict
        import defaultdict as ddict

        (defaultdict)
        ");
    }

    #[test]
    fn member_conflict_with_other_member_import_alias() {
        let test = cursor_test(
            "\
from othermodule import something as defaultdict

(<CURSOR>)
        ",
        );

        assert_snapshot!(
            test.import("collections", "defaultdict"), @"
        import collections
        from othermodule import something as defaultdict

        (collections.defaultdict)
        ");
        assert_snapshot!(
            test.import_from("collections", "defaultdict"), @"
        import collections
        from othermodule import something as defaultdict

        (collections.defaultdict)
        ");
    }

    #[test]
    fn no_conflict_alias_module() {
        let test = cursor_test(
            "\
import defaultdict as ddict

(<CURSOR>)
        ",
        );

        assert_snapshot!(
            test.import("collections", "defaultdict"), @"
        import collections
        import defaultdict as ddict

        (collections.defaultdict)
        ");
        assert_snapshot!(
            test.import_from("collections", "defaultdict"), @"
        from collections import defaultdict
        import defaultdict as ddict

        (defaultdict)
        ");
    }

    #[test]
    fn no_conflict_alias_member() {
        let test = cursor_test(
            "\
from foo import defaultdict as ddict

(<CURSOR>)
        ",
        );

        assert_snapshot!(
            test.import("collections", "defaultdict"), @"
        import collections
        from foo import defaultdict as ddict

        (collections.defaultdict)
        ");
        assert_snapshot!(
            test.import_from("collections", "defaultdict"), @"
        from collections import defaultdict
        from foo import defaultdict as ddict

        (defaultdict)
        ");
    }

    #[test]
    fn multiple_import_blocks_std() {
        let test = cursor_test(
            "\
import json
import re

from whenever import ZonedDateTime
import numpy as np

(<CURSOR>)
        ",
        );

        assert_snapshot!(
            test.import("collections", "defaultdict"), @"
        import collections
        import json
        import re

        from whenever import ZonedDateTime
        import numpy as np

        (collections.defaultdict)
        ");
        assert_snapshot!(
            test.import_from("collections", "defaultdict"), @"
        from collections import defaultdict
        import json
        import re

        from whenever import ZonedDateTime
        import numpy as np

        (defaultdict)
        ");
    }

    #[test]
    fn multiple_import_blocks_other() {
        let test = CursorTest::builder()
            .source("foo.py", "Foo = 1\nBar = 2\n")
            .source(
                "main.py",
                "\
import json
import re

from whenever import ZonedDateTime
import numpy as np

(<CURSOR>)
        ",
            )
            .build();

        assert_snapshot!(
            test.import("foo", "Bar"), @"
        import foo
        import json
        import re

        from whenever import ZonedDateTime
        import numpy as np

        (foo.Bar)
        ");
        assert_snapshot!(
            test.import_from("foo", "Bar"), @"
        from foo import Bar
        import json
        import re

        from whenever import ZonedDateTime
        import numpy as np

        (Bar)
        ");
    }

    #[test]
    fn conditional_imports_new_import() {
        let test = CursorTest::builder()
            .source("foo.py", "MAGIC = 1")
            .source("bar.py", "MAGIC = 2")
            .source("quux.py", "MAGIC = 3")
            .source(
                "main.py",
                "\
if os.getenv(\"WHATEVER\"):
    from foo import MAGIC
else:
    from bar import MAGIC

(<CURSOR>)
        ",
            )
            .build();

        assert_snapshot!(
            test.import("quux", "MAGIC"), @r#"
        import quux
        if os.getenv("WHATEVER"):
            from foo import MAGIC
        else:
            from bar import MAGIC

        (quux.MAGIC)
        "#);
        assert_snapshot!(
            test.import_from("quux", "MAGIC"), @r#"
        import quux
        if os.getenv("WHATEVER"):
            from foo import MAGIC
        else:
            from bar import MAGIC

        (quux.MAGIC)
        "#);
    }

    // FIXME: This test (and the one below it) aren't
    // quite right. Namely, because we aren't handling
    // multiple binding sites correctly, we don't see the
    // existing `MAGIC` symbol.
    #[test]
    fn conditional_imports_existing_import1() {
        let test = CursorTest::builder()
            .source("foo.py", "MAGIC = 1")
            .source("bar.py", "MAGIC = 2")
            .source("quux.py", "MAGIC = 3")
            .source(
                "main.py",
                "\
if os.getenv(\"WHATEVER\"):
    from foo import MAGIC
else:
    from bar import MAGIC

(<CURSOR>)
        ",
            )
            .build();

        assert_snapshot!(
            test.import("foo", "MAGIC"), @r#"
        import foo
        if os.getenv("WHATEVER"):
            from foo import MAGIC
        else:
            from bar import MAGIC

        (foo.MAGIC)
        "#);
        assert_snapshot!(
            test.import_from("foo", "MAGIC"), @r#"
        from foo import MAGIC
        if os.getenv("WHATEVER"):
            from foo import MAGIC
        else:
            from bar import MAGIC

        (MAGIC)
        "#);
    }

    #[test]
    fn conditional_imports_existing_import2() {
        let test = CursorTest::builder()
            .source("foo.py", "MAGIC = 1")
            .source("bar.py", "MAGIC = 2")
            .source("quux.py", "MAGIC = 3")
            .source(
                "main.py",
                "\
if os.getenv(\"WHATEVER\"):
    from foo import MAGIC
else:
    from bar import MAGIC

(<CURSOR>)
        ",
            )
            .build();

        assert_snapshot!(
            test.import("bar", "MAGIC"), @r#"
        import bar
        if os.getenv("WHATEVER"):
            from foo import MAGIC
        else:
            from bar import MAGIC

        (bar.MAGIC)
        "#);
        assert_snapshot!(
            test.import_from("bar", "MAGIC"), @r#"
        import bar
        if os.getenv("WHATEVER"):
            from foo import MAGIC
        else:
            from bar import MAGIC

        (bar.MAGIC)
        "#);
    }

    // FIXME: This test (and the one below it) aren't quite right. We
    // don't recognize the multiple declaration sites for `fubar`.
    //
    // In this case, it's not totally clear what we should do. Since we
    // are trying to import `MAGIC` from `foo`, we could add a `from
    // foo import MAGIC` within the first `if` block. Or we could try
    // and "infer" something about the code assuming that we know
    // `MAGIC` is in both `foo` and `bar`.
    #[test]
    fn conditional_imports_existing_module1() {
        let test = CursorTest::builder()
            .source("foo.py", "MAGIC = 1")
            .source("bar.py", "MAGIC = 2")
            .source("quux.py", "MAGIC = 3")
            .source(
                "main.py",
                "\
if os.getenv(\"WHATEVER\"):
    import foo as fubar
else:
    import bar as fubar

(<CURSOR>)
        ",
            )
            .build();

        assert_snapshot!(
            test.import("foo", "MAGIC"), @r#"
        import foo
        if os.getenv("WHATEVER"):
            import foo as fubar
        else:
            import bar as fubar

        (foo.MAGIC)
        "#);
        assert_snapshot!(
            test.import_from("foo", "MAGIC"), @r#"
        from foo import MAGIC
        if os.getenv("WHATEVER"):
            import foo as fubar
        else:
            import bar as fubar

        (MAGIC)
        "#);
    }

    #[test]
    fn conditional_imports_existing_module2() {
        let test = CursorTest::builder()
            .source("foo.py", "MAGIC = 1")
            .source("bar.py", "MAGIC = 2")
            .source("quux.py", "MAGIC = 3")
            .source(
                "main.py",
                "\
if os.getenv(\"WHATEVER\"):
    import foo as fubar
else:
    import bar as fubar

(<CURSOR>)
        ",
            )
            .build();

        assert_snapshot!(
            test.import("bar", "MAGIC"), @r#"
        import bar
        if os.getenv("WHATEVER"):
            import foo as fubar
        else:
            import bar as fubar

        (bar.MAGIC)
        "#);
        assert_snapshot!(
            test.import_from("bar", "MAGIC"), @r#"
        from bar import MAGIC
        if os.getenv("WHATEVER"):
            import foo as fubar
        else:
            import bar as fubar

        (MAGIC)
        "#);
    }

    #[test]
    fn try_imports_new_import() {
        let test = CursorTest::builder()
            .source("foo.py", "MAGIC = 1")
            .source("bar.py", "MAGIC = 2")
            .source("quux.py", "MAGIC = 3")
            .source(
                "main.py",
                "\
try:
    from foo import MAGIC
except ImportError:
    from bar import MAGIC

(<CURSOR>)
        ",
            )
            .build();

        assert_snapshot!(
            test.import("quux", "MAGIC"), @"
        import quux
        try:
            from foo import MAGIC
        except ImportError:
            from bar import MAGIC

        (quux.MAGIC)
        ");
        assert_snapshot!(
            test.import_from("quux", "MAGIC"), @"
        import quux
        try:
            from foo import MAGIC
        except ImportError:
            from bar import MAGIC

        (quux.MAGIC)
        ");
    }

    // FIXME: This test (and the one below it) aren't
    // quite right. Namely, because we aren't handling
    // multiple binding sites correctly, we don't see the
    // existing `MAGIC` symbol.
    #[test]
    fn try_imports_existing_import1() {
        let test = CursorTest::builder()
            .source("foo.py", "MAGIC = 1")
            .source("bar.py", "MAGIC = 2")
            .source("quux.py", "MAGIC = 3")
            .source(
                "main.py",
                "\
try:
    from foo import MAGIC
except ImportError:
    from bar import MAGIC

(<CURSOR>)
        ",
            )
            .build();

        assert_snapshot!(
            test.import("foo", "MAGIC"), @"
        import foo
        try:
            from foo import MAGIC
        except ImportError:
            from bar import MAGIC

        (foo.MAGIC)
        ");
        assert_snapshot!(
            test.import_from("foo", "MAGIC"), @"
        from foo import MAGIC
        try:
            from foo import MAGIC
        except ImportError:
            from bar import MAGIC

        (MAGIC)
        ");
    }

    #[test]
    fn try_imports_existing_import2() {
        let test = CursorTest::builder()
            .source("foo.py", "MAGIC = 1")
            .source("bar.py", "MAGIC = 2")
            .source("quux.py", "MAGIC = 3")
            .source(
                "main.py",
                "\
try:
    from foo import MAGIC
except ImportError:
    from bar import MAGIC

(<CURSOR>)
        ",
            )
            .build();

        assert_snapshot!(
            test.import("bar", "MAGIC"), @"
        import bar
        try:
            from foo import MAGIC
        except ImportError:
            from bar import MAGIC

        (bar.MAGIC)
        ");
        assert_snapshot!(
            test.import_from("bar", "MAGIC"), @"
        import bar
        try:
            from foo import MAGIC
        except ImportError:
            from bar import MAGIC

        (bar.MAGIC)
        ");
    }

    #[test]
    fn import_module_blank() {
        let test = cursor_test(
            "\
<CURSOR>
        ",
        );
        assert_snapshot!(
            test.module("collections"), @"
        import collections
        collections
        ");
    }

    #[test]
    fn import_module_exists() {
        let test = cursor_test(
            "\
import collections
<CURSOR>
        ",
        );
        assert_snapshot!(
            test.module("collections"), @"
        import collections
        collections
        ");
    }

    #[test]
    fn import_module_from_exists() {
        let test = cursor_test(
            "\
from collections import defaultdict
<CURSOR>
        ",
        );
        assert_snapshot!(
            test.module("collections"), @"
        import collections
        from collections import defaultdict
        collections
        ");
    }

    // This test is working as intended. That is,
    // `abc` is already in scope, so requesting an
    // import for `collections.abc` could feasibly
    // reuse the import and rewrite the symbol text
    // to just `abc`. But for now it seems better
    // to respect what has been written and add the
    // `import collections.abc`. This behavior could
    // plausibly be changed.
    #[test]
    fn import_module_from_via_member_exists() {
        let test = cursor_test(
            "\
from collections import abc
<CURSOR>
        ",
        );
        assert_snapshot!(
            test.module("collections.abc"), @"
        import collections.abc
        from collections import abc
        collections.abc
        ");
    }
}
