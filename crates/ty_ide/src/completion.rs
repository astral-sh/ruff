use ruff_db::files::File;
use ruff_db::parsed::{ParsedModule, parsed_module};
use ruff_python_parser::TokenAt;
use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::Db;
use crate::find_node::{CoveringNode, covering_node};

#[derive(Debug, Clone)]
pub struct Completion {
    pub label: String,
}

pub fn completion(db: &dyn Db, file: File, offset: TextSize) -> Vec<Completion> {
    let parsed = parsed_module(db.upcast(), file);

    let Some(target) = find_target(parsed, offset) else {
        return vec![];
    };

    let model = ty_python_semantic::SemanticModel::new(db.upcast(), file);
    let mut completions = model.completions(target.node());
    completions.sort();
    completions.dedup();
    completions
        .into_iter()
        .map(|name| Completion { label: name.into() })
        .collect()
}

fn find_target(parsed: &ParsedModule, offset: TextSize) -> Option<CoveringNode> {
    let offset = match parsed.tokens().at_offset(offset) {
        TokenAt::None => {
            return Some(covering_node(
                parsed.syntax().into(),
                TextRange::empty(offset),
            ));
        }
        TokenAt::Single(tok) => tok.end(),
        TokenAt::Between(_, tok) => tok.start(),
    };
    let before = parsed.tokens().before(offset);
    let last = before.last()?;
    let covering_node = covering_node(parsed.syntax().into(), last.range());
    Some(covering_node)
}

#[cfg(test)]
mod tests {
    use insta::assert_snapshot;

    use crate::completion;
    use crate::tests::{CursorTest, cursor_test};

    // At time of writing (2025-05-22), the tests below show some of the
    // naivete of our completions. That is, we don't even take what has been
    // typed into account. We just kind return all possible completions
    // regardless of what has been typed and rely on the client to do filtering
    // based on prefixes and what not.
    //
    // In the future, we might consider using "text edits,"[1] which will let
    // us have more control over which completions are shown to the end user.
    // But that will require us to at least do some kind of filtering based on
    // what has been typed.
    //
    // [1]: https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#textDocument_completion

    #[test]
    fn empty() {
        let test = cursor_test(
            "\
<CURSOR>
",
        );

        assert_snapshot!(test.completions(), @"<No completions found>");
    }

    #[test]
    fn imports1() {
        let test = cursor_test(
            "\
import re

<CURSOR>
",
        );

        assert_snapshot!(test.completions(), @"re");
    }

    #[test]
    fn imports2() {
        let test = cursor_test(
            "\
from os import path

<CURSOR>
",
        );

        assert_snapshot!(test.completions(), @"path");
    }

    // N.B. We don't currently explore module APIs. This
    // is still just emitting symbols from the detected scope.
    #[test]
    fn module_api() {
        let test = cursor_test(
            "\
import re

re.<CURSOR>
",
        );

        assert_snapshot!(test.completions(), @"re");
    }

    #[test]
    fn one_function_prefix() {
        let test = cursor_test(
            "\
def foo(): ...

f<CURSOR>
",
        );

        assert_snapshot!(test.completions(), @r"
        f
        foo
        ");
    }

    #[test]
    fn one_function_not_prefix() {
        let test = cursor_test(
            "\
def foo(): ...

g<CURSOR>
",
        );

        assert_snapshot!(test.completions(), @r"
        foo
        g
        ");
    }

    #[test]
    fn one_function_blank() {
        let test = cursor_test(
            "\
def foo(): ...

<CURSOR>
",
        );

        assert_snapshot!(test.completions(), @r"
        foo
        ");
    }

    #[test]
    fn nested_function_prefix() {
        let test = cursor_test(
            "\
def foo():
    def foofoo(): ...

f<CURSOR>
",
        );

        assert_snapshot!(test.completions(), @r"
        f
        foo
        ");
    }

    #[test]
    fn nested_function_blank() {
        let test = cursor_test(
            "\
def foo():
    def foofoo(): ...

<CURSOR>
",
        );

        assert_snapshot!(test.completions(), @r"
        foo
        ");
    }

    #[test]
    fn nested_function_not_in_global_scope_prefix() {
        let test = cursor_test(
            "\
def foo():
    def foofoo(): ...
    f<CURSOR>
",
        );

        assert_snapshot!(test.completions(), @r"
        f
        foo
        foofoo
        ");
    }

    #[test]
    fn nested_function_not_in_global_scope_blank() {
        let test = cursor_test(
            "\
def foo():
    def foofoo(): ...
    <CURSOR>
",
        );

        // FIXME: Should include `foofoo`.
        //
        // `foofoo` isn't included at present (2025-05-22). The problem
        // here is that the AST for `def foo():` doesn't encompass the
        // trailing indentation. So when the cursor position is in that
        // trailing indentation, we can't (easily) get a handle to the
        // right scope. And even if we could, the AST expressions for
        // `def foo():` and `def foofoo(): ...` end at precisely the
        // same point. So there is no AST we can hold after the end of
        // `foofoo` but before the end of `foo`. So at the moment, it's
        // not totally clear how to get the right scope.
        //
        // If we didn't want to change the ranges on the AST nodes,
        // another approach here would be to get the inner most scope,
        // and explore its ancestors until we get to a level that
        // matches the current cursor's indentation. This seems fraught
        // however. It's not clear to me that we can always assume a
        // correspondence between scopes and indentation level.
        assert_snapshot!(test.completions(), @r"
        foo
        ");
    }

    #[test]
    fn double_nested_function_not_in_global_scope_prefix1() {
        let test = cursor_test(
            "\
def foo():
    def foofoo():
        def foofoofoo(): ...
    f<CURSOR>
",
        );

        assert_snapshot!(test.completions(), @r"
        f
        foo
        foofoo
        ");
    }

    #[test]
    fn double_nested_function_not_in_global_scope_prefix2() {
        let test = cursor_test(
            "\
def foo():
    def foofoo():
        def foofoofoo(): ...
    f<CURSOR>",
        );

        assert_snapshot!(test.completions(), @r"
        f
        foo
        foofoo
        ");
    }

    #[test]
    fn double_nested_function_not_in_global_scope_prefix3() {
        let test = cursor_test(
            "\
def foo():
    def foofoo():
        def foofoofoo(): ...
    f<CURSOR>
def frob(): ...
",
        );

        assert_snapshot!(test.completions(), @r"
        f
        foo
        foofoo
        frob
        ");
    }

    #[test]
    fn double_nested_function_not_in_global_scope_prefix4() {
        let test = cursor_test(
            "\
def foo():
    def foofoo():
        def foofoofoo(): ...
f<CURSOR>
def frob(): ...
",
        );

        assert_snapshot!(test.completions(), @r"
        f
        foo
        frob
        ");
    }

    #[test]
    fn double_nested_function_not_in_global_scope_prefix5() {
        let test = cursor_test(
            "\
def foo():
    def foofoo():
        def foofoofoo(): ...
        f<CURSOR>
def frob(): ...
",
        );

        assert_snapshot!(test.completions(), @r"
        f
        foo
        foofoo
        foofoofoo
        frob
        ");
    }

    #[test]
    fn double_nested_function_not_in_global_scope_blank1() {
        let test = cursor_test(
            "\
def foo():
    def foofoo():
        def foofoofoo(): ...
    <CURSOR>
",
        );

        // FIXME: Should include `foofoo` (but not `foofoofoo`).
        //
        // The tests below fail for the same reason that
        // `nested_function_not_in_global_scope_blank` fails: there is no
        // space in the AST ranges after the end of `foofoofoo` but before
        // the end of `foofoo`. So either the AST needs to be tweaked to
        // account for the indented whitespace, or some other technique
        // needs to be used to get the scope containing `foofoo` but not
        // `foofoofoo`.
        assert_snapshot!(test.completions(), @r"
        foo
        ");
    }

    #[test]
    fn double_nested_function_not_in_global_scope_blank2() {
        let test = cursor_test(
            " \
def foo():
    def foofoo():
        def foofoofoo(): ...
    <CURSOR>",
        );

        // FIXME: Should include `foofoo` (but not `foofoofoo`).
        assert_snapshot!(test.completions(), @r"
        foo
        ");
    }

    #[test]
    fn double_nested_function_not_in_global_scope_blank3() {
        let test = cursor_test(
            "\
def foo():
    def foofoo():
        def foofoofoo(): ...
    <CURSOR>
def frob(): ...
            ",
        );

        // FIXME: Should include `foofoo` (but not `foofoofoo`).
        assert_snapshot!(test.completions(), @r"
        foo
        frob
        ");
    }

    #[test]
    fn double_nested_function_not_in_global_scope_blank4() {
        let test = cursor_test(
            "\
def foo():
    def foofoo():
        def foofoofoo(): ...
    <CURSOR>

def frob(): ...
",
        );

        // FIXME: Should include `foofoo` (but not `foofoofoo`).
        assert_snapshot!(test.completions(), @r"
        foo
        frob
        ");
    }

    #[test]
    fn double_nested_function_not_in_global_scope_blank5() {
        let test = cursor_test(
            "\
def foo():
    def foofoo():
        def foofoofoo(): ...

    <CURSOR>

def frob(): ...
",
        );

        // FIXME: Should include `foofoo` (but not `foofoofoo`).
        assert_snapshot!(test.completions(), @r"
        foo
        frob
        ");
    }

    #[test]
    fn list_comprehension1() {
        let test = cursor_test(
            "\
[<CURSOR> for bar in [1, 2, 3]]
",
        );

        // It's not totally clear why `for` shows up in the
        // symbol tables of the detected scopes here. My guess
        // is that there's perhaps some sub-optimal behavior
        // here because the list comprehension as written is not
        // valid.
        assert_snapshot!(test.completions(), @r"
        bar
        for
        ");
    }

    #[test]
    fn list_comprehension2() {
        let test = cursor_test(
            "\
[f<CURSOR> for foo in [1, 2, 3]]
",
        );

        assert_snapshot!(test.completions(), @r"
        f
        foo
        ");
    }

    #[test]
    fn lambda_prefix1() {
        let test = cursor_test(
            "\
(lambda foo: (1 + f<CURSOR> + 2))(2)
",
        );

        assert_snapshot!(test.completions(), @r"
        f
        foo
        ");
    }

    #[test]
    fn lambda_prefix2() {
        let test = cursor_test(
            "\
(lambda foo: f<CURSOR> + 1)(2)
",
        );

        assert_snapshot!(test.completions(), @r"
        f
        foo
        ");
    }

    #[test]
    fn lambda_prefix3() {
        let test = cursor_test(
            "\
(lambda foo: (f<CURSOR> + 1))(2)
",
        );

        assert_snapshot!(test.completions(), @r"
        f
        foo
        ");
    }

    #[test]
    fn lambda_prefix4() {
        let test = cursor_test(
            "\
(lambda foo: 1 + f<CURSOR>)(2)
",
        );

        assert_snapshot!(test.completions(), @r"
        f
        foo
        ");
    }

    #[test]
    fn lambda_blank1() {
        let test = cursor_test(
            "\
(lambda foo: 1 + <CURSOR> + 2)(2)
",
        );

        assert_snapshot!(test.completions(), @"foo");
    }

    #[test]
    fn lambda_blank2() {
        let test = cursor_test(
            "\
(lambda foo: <CURSOR> + 1)(2)
",
        );

        // FIXME: Should include `foo`.
        //
        // These fails for similar reasons as above: the body of the
        // lambda doesn't include the position of <CURSOR> because
        // <CURSOR> is inside leading or trailing whitespace. (Even
        // when enclosed in parentheses. Specifically, parentheses
        // aren't part of the node's range unless it's relevant e.g.,
        // tuples.)
        //
        // The `lambda_blank1` test works because there are expressions
        // on either side of <CURSOR>.
        assert_snapshot!(test.completions(), @"<No completions found>");
    }

    #[test]
    fn lambda_blank3() {
        let test = cursor_test(
            "\
(lambda foo: (<CURSOR> + 1))(2)
",
        );

        // FIXME: Should include `foo`.
        assert_snapshot!(test.completions(), @"<No completions found>");
    }

    #[test]
    fn lambda_blank4() {
        let test = cursor_test(
            "\
(lambda foo: 1 + <CURSOR>)(2)
",
        );

        // FIXME: Should include `foo`.
        assert_snapshot!(test.completions(), @"<No completions found>");
    }

    #[test]
    fn class_prefix1() {
        let test = cursor_test(
            "\
class Foo:
    bar = 1
    quux = b<CURSOR>
    frob = 3
",
        );

        assert_snapshot!(test.completions(), @r"
        Foo
        b
        bar
        frob
        quux
        ");
    }

    #[test]
    fn class_prefix2() {
        let test = cursor_test(
            "\
class Foo:
    bar = 1
    quux = b<CURSOR>
",
        );

        assert_snapshot!(test.completions(), @r"
        Foo
        b
        bar
        quux
        ");
    }

    #[test]
    fn class_blank1() {
        let test = cursor_test(
            "\
class Foo:
    bar = 1
    quux = <CURSOR>
    frob = 3
",
        );

        // FIXME: Should include `bar`, `quux` and `frob`.
        // (Unclear if `Foo` should be included, but a false
        // positive isn't the end of the world.)
        //
        // These don't work for similar reasons as other
        // tests above with the <CURSOR> inside of whitespace.
        assert_snapshot!(test.completions(), @r"
        Foo
        ");
    }

    #[test]
    fn class_blank2() {
        let test = cursor_test(
            "\
class Foo:
    bar = 1
    quux = <CURSOR>
    frob = 3
",
        );

        // FIXME: Should include `bar`, `quux` and `frob`.
        // (Unclear if `Foo` should be included, but a false
        // positive isn't the end of the world.)
        assert_snapshot!(test.completions(), @r"
        Foo
        ");
    }

    #[test]
    fn class_super1() {
        let test = cursor_test(
            "\
class Bar: ...

class Foo(<CURSOR>):
    bar = 1
",
        );

        assert_snapshot!(test.completions(), @r"
        Bar
        Foo
        ");
    }

    #[test]
    fn class_super2() {
        let test = cursor_test(
            "\
class Foo(<CURSOR>):
    bar = 1

class Bar: ...
",
        );

        assert_snapshot!(test.completions(), @r"
        Bar
        Foo
        ");
    }

    #[test]
    fn class_super3() {
        let test = cursor_test(
            "\
class Foo(<CURSOR>
    bar = 1

class Bar: ...
",
        );

        assert_snapshot!(test.completions(), @r"
        Bar
        Foo
        ");
    }

    #[test]
    fn class_super4() {
        let test = cursor_test(
            "\
class Bar: ...

class Foo(<CURSOR>",
        );

        assert_snapshot!(test.completions(), @r"
        Bar
        Foo
        ");
    }

    // We don't yet take function parameters into account.
    #[test]
    fn call_prefix1() {
        let test = cursor_test(
            "\
def bar(okay=None): ...

foo = 1

bar(o<CURSOR>
",
        );

        assert_snapshot!(test.completions(), @r"
        bar
        foo
        o
        ");
    }

    #[test]
    fn call_blank1() {
        let test = cursor_test(
            "\
def bar(okay=None): ...

foo = 1

bar(<CURSOR>
",
        );

        assert_snapshot!(test.completions(), @r"
        bar
        foo
        ");
    }

    #[test]
    fn duplicate1() {
        let test = cursor_test(
            "\
def foo(): ...

class C:
    def foo(self): ...
    def bar(self):
        f<CURSOR>
",
        );

        assert_snapshot!(test.completions(), @r"
        C
        bar
        f
        foo
        self
        ");
    }

    #[test]
    fn instance_methods_are_not_regular_functions1() {
        let test = cursor_test(
            "\
class C:
    def foo(self): ...

<CURSOR>
",
        );

        assert_snapshot!(test.completions(), @"C");
    }

    #[test]
    fn instance_methods_are_not_regular_functions2() {
        let test = cursor_test(
            "\
class C:
    def foo(self): ...
    def bar(self):
        f<CURSOR>
",
        );

        // FIXME: Should NOT include `foo` here, since
        // that is only a method that can be called on
        // `self`.
        assert_snapshot!(test.completions(), @r"
        C
        bar
        f
        foo
        self
        ");
    }

    #[test]
    fn identifier_keyword_clash1() {
        let test = cursor_test(
            "\
classy_variable_name = 1

class<CURSOR>
",
        );

        assert_snapshot!(test.completions(), @"classy_variable_name");
    }

    #[test]
    fn identifier_keyword_clash2() {
        let test = cursor_test(
            "\
some_symbol = 1

print(f\"{some<CURSOR>
",
        );

        assert_snapshot!(test.completions(), @r"
        print
        some
        some_symbol
        ");
    }

    impl CursorTest {
        fn completions(&self) -> String {
            let completions = completion(&self.db, self.file, self.cursor_offset);
            if completions.is_empty() {
                return "<No completions found>".to_string();
            }
            completions
                .into_iter()
                .map(|completion| completion.label)
                .collect::<Vec<String>>()
                .join("\n")
        }
    }
}
