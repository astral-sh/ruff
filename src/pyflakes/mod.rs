pub mod cformat;
pub mod checks;
pub mod fixes;
pub mod format;
pub mod plugins;

#[cfg(test)]
mod tests {
    use std::convert::AsRef;
    use std::path::Path;

    use anyhow::Result;
    use regex::Regex;
    use rustpython_parser::lexer::LexResult;
    use test_case::test_case;
    use textwrap::dedent;

    use crate::checks::CheckCode;
    use crate::checks_gen::CheckCodePrefix;
    use crate::linter::{check_path, test_path};
    use crate::settings::flags;
    use crate::source_code_locator::SourceCodeLocator;
    use crate::{directives, rustpython_helpers, settings};

    #[test_case(CheckCode::F401, Path::new("F401_0.py"); "F401_0")]
    #[test_case(CheckCode::F401, Path::new("F401_1.py"); "F401_1")]
    #[test_case(CheckCode::F401, Path::new("F401_2.py"); "F401_2")]
    #[test_case(CheckCode::F401, Path::new("F401_3.py"); "F401_3")]
    #[test_case(CheckCode::F401, Path::new("F401_4.py"); "F401_4")]
    #[test_case(CheckCode::F401, Path::new("F401_5.py"); "F401_5")]
    #[test_case(CheckCode::F401, Path::new("F401_6.py"); "F401_6")]
    #[test_case(CheckCode::F401, Path::new("F401_7.py"); "F401_7")]
    #[test_case(CheckCode::F402, Path::new("F402.py"); "F402")]
    #[test_case(CheckCode::F403, Path::new("F403.py"); "F403")]
    #[test_case(CheckCode::F404, Path::new("F404.py"); "F404")]
    #[test_case(CheckCode::F405, Path::new("F405.py"); "F405")]
    #[test_case(CheckCode::F406, Path::new("F406.py"); "F406")]
    #[test_case(CheckCode::F407, Path::new("F407.py"); "F407")]
    #[test_case(CheckCode::F501, Path::new("F50x.py"); "F501")]
    #[test_case(CheckCode::F502, Path::new("F502.py"); "F502_1")]
    #[test_case(CheckCode::F502, Path::new("F50x.py"); "F502_0")]
    #[test_case(CheckCode::F503, Path::new("F503.py"); "F503_1")]
    #[test_case(CheckCode::F503, Path::new("F50x.py"); "F503_0")]
    #[test_case(CheckCode::F504, Path::new("F504.py"); "F504_1")]
    #[test_case(CheckCode::F504, Path::new("F50x.py"); "F504_0")]
    #[test_case(CheckCode::F505, Path::new("F504.py"); "F505_1")]
    #[test_case(CheckCode::F505, Path::new("F50x.py"); "F505_0")]
    #[test_case(CheckCode::F506, Path::new("F50x.py"); "F506")]
    #[test_case(CheckCode::F507, Path::new("F50x.py"); "F507")]
    #[test_case(CheckCode::F508, Path::new("F50x.py"); "F508")]
    #[test_case(CheckCode::F509, Path::new("F50x.py"); "F509")]
    #[test_case(CheckCode::F521, Path::new("F521.py"); "F521")]
    #[test_case(CheckCode::F522, Path::new("F522.py"); "F522")]
    #[test_case(CheckCode::F523, Path::new("F523.py"); "F523")]
    #[test_case(CheckCode::F524, Path::new("F524.py"); "F524")]
    #[test_case(CheckCode::F525, Path::new("F525.py"); "F525")]
    #[test_case(CheckCode::F541, Path::new("F541.py"); "F541")]
    #[test_case(CheckCode::F601, Path::new("F601.py"); "F601")]
    #[test_case(CheckCode::F602, Path::new("F602.py"); "F602")]
    #[test_case(CheckCode::F622, Path::new("F622.py"); "F622")]
    #[test_case(CheckCode::F631, Path::new("F631.py"); "F631")]
    #[test_case(CheckCode::F632, Path::new("F632.py"); "F632")]
    #[test_case(CheckCode::F633, Path::new("F633.py"); "F633")]
    #[test_case(CheckCode::F634, Path::new("F634.py"); "F634")]
    #[test_case(CheckCode::F701, Path::new("F701.py"); "F701")]
    #[test_case(CheckCode::F702, Path::new("F702.py"); "F702")]
    #[test_case(CheckCode::F704, Path::new("F704.py"); "F704")]
    #[test_case(CheckCode::F706, Path::new("F706.py"); "F706")]
    #[test_case(CheckCode::F707, Path::new("F707.py"); "F707")]
    #[test_case(CheckCode::F722, Path::new("F722.py"); "F722")]
    #[test_case(CheckCode::F811, Path::new("F811_0.py"); "F811_0")]
    #[test_case(CheckCode::F811, Path::new("F811_1.py"); "F811_1")]
    #[test_case(CheckCode::F811, Path::new("F811_2.py"); "F811_2")]
    #[test_case(CheckCode::F811, Path::new("F811_3.py"); "F811_3")]
    #[test_case(CheckCode::F811, Path::new("F811_4.py"); "F811_4")]
    #[test_case(CheckCode::F811, Path::new("F811_5.py"); "F811_5")]
    #[test_case(CheckCode::F811, Path::new("F811_6.py"); "F811_6")]
    #[test_case(CheckCode::F811, Path::new("F811_7.py"); "F811_7")]
    #[test_case(CheckCode::F811, Path::new("F811_8.py"); "F811_8")]
    #[test_case(CheckCode::F811, Path::new("F811_9.py"); "F811_9")]
    #[test_case(CheckCode::F811, Path::new("F811_10.py"); "F811_10")]
    #[test_case(CheckCode::F811, Path::new("F811_11.py"); "F811_11")]
    #[test_case(CheckCode::F811, Path::new("F811_12.py"); "F811_12")]
    #[test_case(CheckCode::F811, Path::new("F811_13.py"); "F811_13")]
    #[test_case(CheckCode::F811, Path::new("F811_14.py"); "F811_14")]
    #[test_case(CheckCode::F811, Path::new("F811_15.py"); "F811_15")]
    #[test_case(CheckCode::F811, Path::new("F811_16.py"); "F811_16")]
    #[test_case(CheckCode::F811, Path::new("F811_17.py"); "F811_17")]
    #[test_case(CheckCode::F811, Path::new("F811_18.py"); "F811_18")]
    #[test_case(CheckCode::F811, Path::new("F811_19.py"); "F811_19")]
    #[test_case(CheckCode::F811, Path::new("F811_20.py"); "F811_20")]
    #[test_case(CheckCode::F821, Path::new("F821_0.py"); "F821_0")]
    #[test_case(CheckCode::F821, Path::new("F821_1.py"); "F821_1")]
    #[test_case(CheckCode::F821, Path::new("F821_2.py"); "F821_2")]
    #[test_case(CheckCode::F821, Path::new("F821_3.py"); "F821_3")]
    #[test_case(CheckCode::F821, Path::new("F821_4.py"); "F821_4")]
    #[test_case(CheckCode::F821, Path::new("F821_5.py"); "F821_5")]
    #[test_case(CheckCode::F821, Path::new("F821_6.py"); "F821_6")]
    #[test_case(CheckCode::F821, Path::new("F821_7.py"); "F821_7")]
    #[test_case(CheckCode::F822, Path::new("F822.py"); "F822")]
    #[test_case(CheckCode::F823, Path::new("F823.py"); "F823")]
    #[test_case(CheckCode::F831, Path::new("F831.py"); "F831")]
    #[test_case(CheckCode::F841, Path::new("F841_0.py"); "F841_0")]
    #[test_case(CheckCode::F841, Path::new("F841_1.py"); "F841_1")]
    #[test_case(CheckCode::F842, Path::new("F842.py"); "F842")]
    #[test_case(CheckCode::F901, Path::new("F901.py"); "F901")]
    fn checks(check_code: CheckCode, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", check_code.as_ref(), path.to_string_lossy());
        let mut checks = test_path(
            Path::new("./resources/test/fixtures/pyflakes")
                .join(path)
                .as_path(),
            &settings::Settings::for_rule(check_code),
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(snapshot, checks);
        Ok(())
    }

    #[test]
    fn f841_dummy_variable_rgx() -> Result<()> {
        let mut checks = test_path(
            Path::new("./resources/test/fixtures/pyflakes/F841_0.py"),
            &settings::Settings {
                dummy_variable_rgx: Regex::new(r"^z$").unwrap(),
                ..settings::Settings::for_rule(CheckCode::F841)
            },
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test]
    fn init() -> Result<()> {
        let mut checks = test_path(
            Path::new("./resources/test/fixtures/pyflakes/__init__.py"),
            &settings::Settings::for_rules(vec![CheckCode::F821, CheckCode::F822]),
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test]
    fn future_annotations() -> Result<()> {
        let mut checks = test_path(
            Path::new("./resources/test/fixtures/pyflakes/future_annotations.py"),
            &settings::Settings::for_rules(vec![CheckCode::F401, CheckCode::F821]),
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test]
    fn multi_statement_lines() -> Result<()> {
        let mut checks = test_path(
            Path::new("./resources/test/fixtures/pyflakes/multi_statement_lines.py"),
            &settings::Settings::for_rule(CheckCode::F401),
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    /// A re-implementation of the Pyflakes test runner.
    /// Note that all tests marked with `#[ignore]` should be considered TODOs.
    fn flakes(contents: &str, expected: &[CheckCode]) -> Result<()> {
        let contents = dedent(contents);
        let settings = settings::Settings::for_rules(CheckCodePrefix::F.codes());
        let tokens: Vec<LexResult> = rustpython_helpers::tokenize(&contents);
        let locator = SourceCodeLocator::new(&contents);
        let directives = directives::extract_directives(
            &tokens,
            &locator,
            directives::Flags::from_settings(&settings),
        );
        let mut checks = check_path(
            Path::new("<filename>"),
            None,
            &contents,
            tokens,
            &locator,
            &directives,
            &settings,
            flags::Autofix::Enabled,
            flags::Noqa::Enabled,
        )?;
        checks.sort_by_key(|check| check.location);
        let actual = checks
            .iter()
            .map(|check| check.kind.code().clone())
            .collect::<Vec<_>>();
        assert_eq!(actual, expected);
        Ok(())
    }

    /// See: <https://github.com/PyCQA/pyflakes/blob/04ecb0c324ef3b61124e2f80f9e1af6c3a4c7b26/pyflakes/test/test_undefined_names.py>
    #[test]
    fn undefined() -> Result<()> {
        flakes("bar", &[CheckCode::F821])?;
        Ok(())
    }

    #[test]
    fn defined_in_list_comp() -> Result<()> {
        flakes("[a for a in range(10) if a]", &[])?;
        Ok(())
    }

    #[test]
    fn undefined_in_list_comp() -> Result<()> {
        flakes(
            r#"
        [a for a in range(10)]
        a
        "#,
            &[CheckCode::F821],
        )
    }

    #[test]
    fn undefined_exception_name() -> Result<()> {
        // Exception names can't be used after the except: block.
        //
        // The exc variable is unused inside the exception handler.
        flakes(
            r#"
        try:
            raise ValueError('ve')
        except ValueError as exc:
            pass
        exc
        "#,
            &[CheckCode::F841, CheckCode::F821],
        )?;
        Ok(())
    }

    #[test]
    fn names_declared_in_except_blocks() -> Result<()> {
        // Locals declared in except: blocks can be used after the block.
        //
        // This shows the example in test_undefinedExceptionName is
        // different.
        flakes(
            r#"
        try:
            raise ValueError('ve')
        except ValueError as exc:
            e = exc
        e
        "#,
            &[],
        )?;
        Ok(())
    }

    #[test]
    fn undefined_exception_name_obscuring_local_variable2() -> Result<()> {
        // Exception names are unbound after the `except:` block.
        //
        // Last line will raise UnboundLocalError.
        // The exc variable is unused inside the exception handler.
        flakes(
            r#"
        try:
            raise ValueError('ve')
        except ValueError as exc:
            pass
        print(exc)
        exc = 'Original value'
        "#,
            &[CheckCode::F841, CheckCode::F821],
        )?;
        Ok(())
    }

    #[test]
    fn del_exception_in_except() -> Result<()> {
        // The exception name can be deleted in the except: block.
        flakes(
            r#"
        try:
            pass
        except Exception as exc:
            del exc
        "#,
            &[],
        )?;
        Ok(())
    }

    #[test]
    fn functions_need_global_scope() -> Result<()> {
        flakes(
            r#"
        class a:
            def b():
                fu
        fu = 1
        "#,
            &[],
        )?;
        Ok(())
    }

    #[test]
    fn builtins() -> Result<()> {
        flakes("range(10)", &[])?;
        Ok(())
    }

    #[test]
    fn builtin_windows_error() -> Result<()> {
        // C{WindowsError} is sometimes a builtin name, so no warning is emitted
        // for using it.
        flakes("WindowsError", &[])?;
        Ok(())
    }

    #[test]
    fn module_annotations() -> Result<()> {
        // Use of the C{__annotations__} in module scope should not emit
        // an undefined name warning when version is greater than or equal to 3.6.
        flakes("__annotations__", &[])?;
        Ok(())
    }

    #[test]
    fn magic_globals_file() -> Result<()> {
        // Use of the C{__file__} magic global should not emit an undefined name
        // warning.
        flakes("__file__", &[])?;
        Ok(())
    }

    #[test]
    fn magic_globals_builtins() -> Result<()> {
        // Use of the C{__builtins__} magic global should not emit an undefined
        // name warning.
        flakes("__builtins__", &[])?;
        Ok(())
    }

    #[test]
    fn magic_globals_name() -> Result<()> {
        // Use of the C{__name__} magic global should not emit an undefined name
        // warning.
        flakes("__name__", &[])?;
        Ok(())
    }

    #[test]
    fn magic_module_in_class_scope() -> Result<()> {
        // Use of the C{__module__} magic builtin should not emit an undefined
        // name warning if used in class scope.
        flakes("__module__", &[CheckCode::F821])?;
        flakes(
            r#"
        class Foo:
            __module__
        "#,
            &[],
        )?;
        flakes(
            r#"
        class Foo:
            def bar(self):
                __module__
        "#,
            &[CheckCode::F821],
        )?;
        Ok(())
    }

    #[test]
    fn magic_qualname_in_class_scope() -> Result<()> {
        // Use of the C{__qualname__} magic builtin should not emit an undefined
        // name warning if used in class scope.
        flakes("__qualname__", &[CheckCode::F821])?;
        flakes(
            r#"
        class Foo:
            __qualname__
        "#,
            &[],
        )?;
        flakes(
            r#"
        class Foo:
            def bar(self):
                __qualname__
        "#,
            &[CheckCode::F821],
        )?;
        Ok(())
    }

    #[test]
    fn global_import_star() -> Result<()> {
        // Can't find undefined names with import *.
        flakes("from fu import *; bar", &[CheckCode::F403, CheckCode::F405])?;
        Ok(())
    }

    #[test]
    fn defined_by_global() -> Result<()> {
        // "global" can make an otherwise undefined name in another function
        // defined.
        flakes(
            r#"
        def a(): global fu; fu = 1
        def b(): fu
        "#,
            &[],
        )?;
        flakes(
            r#"
        def c(): bar
        def b(): global bar; bar = 1
        "#,
            &[],
        )?;
        // TODO(charlie): Extract globals recursively (such that we don't raise F821).
        flakes(
            r#"
        def c(): bar
        def d():
            def b():
                global bar; bar = 1
        "#,
            &[CheckCode::F821],
        )?;
        Ok(())
    }

    #[test]
    fn defined_by_global_multiple_names() -> Result<()> {
        // "global" can accept multiple names.
        flakes(
            r#"
        def a(): global fu, bar; fu = 1; bar = 2
        def b(): fu; bar
        "#,
            &[],
        )?;
        Ok(())
    }

    #[test]
    fn global_in_global_scope() -> Result<()> {
        // A global statement in the global scope is ignored.
        flakes(
            r#"
        global x
        def foo():
            print(x)
        "#,
            &[CheckCode::F821],
        )?;
        Ok(())
    }

    #[test]
    fn global_reset_name_only() -> Result<()> {
        // A global statement does not prevent other names being undefined.
        flakes(
            r#"
        def f1():
            s

        def f2():
            global m
        "#,
            &[CheckCode::F821],
        )?;
        Ok(())
    }

    #[test]
    fn del() -> Result<()> {
        // Del deletes bindings.
        flakes("a = 1; del a; a", &[CheckCode::F821])?;
        Ok(())
    }

    #[test]
    fn del_global() -> Result<()> {
        // Del a global binding from a function.
        flakes(
            r#"
        a = 1
        def f():
            global a
            del a
        a
        "#,
            &[],
        )?;
        Ok(())
    }

    #[test]
    fn del_undefined() -> Result<()> {
        // Del an undefined name.
        flakes("del a", &[CheckCode::F821])?;
        Ok(())
    }

    #[test]
    fn del_conditional() -> Result<()> {
        // Ignores conditional bindings deletion.
        flakes(
            r#"
        context = None
        test = True
        if False:
            del(test)
        assert(test)
        "#,
            &[],
        )?;
        Ok(())
    }

    #[test]
    fn del_conditional_nested() -> Result<()> {
        // Ignored conditional bindings deletion even if they are nested in other
        // blocks.
        flakes(
            r#"
        context = None
        test = True
        if False:
            with context():
                del(test)
        assert(test)
        "#,
            &[],
        )?;
        Ok(())
    }

    #[test]
    fn del_while() -> Result<()> {
        // Ignore bindings deletion if called inside the body of a while
        // statement.
        flakes(
            r#"
        def test():
            foo = 'bar'
            while False:
                del foo
            assert(foo)
        "#,
            &[],
        )?;
        Ok(())
    }

    #[test]
    fn del_while_test_usage() -> Result<()> {
        // Ignore bindings deletion if called inside the body of a while
        // statement and name is used inside while's test part.
        flakes(
            r#"
        def _worker():
            o = True
            while o is not True:
                del o
                o = False
        "#,
            &[],
        )?;
        Ok(())
    }

    #[test]
    fn del_while_nested() -> Result<()> {
        // Ignore bindings deletions if node is part of while's test, even when
        // del is in a nested block.
        flakes(
            r#"
        context = None
        def _worker():
            o = True
            while o is not True:
                while True:
                    with context():
                        del o
                o = False
        "#,
            &[],
        )?;
        Ok(())
    }

    #[test]
    fn global_from_nested_scope() -> Result<()> {
        // Global names are available from nested scopes.
        flakes(
            r#"
        a = 1
        def b():
            def c():
                a
        "#,
            &[],
        )?;
        Ok(())
    }

    #[test]
    fn later_redefined_global_from_nested_scope() -> Result<()> {
        // Test that referencing a local name that shadows a global, before it is
        // defined, generates a warning.
        flakes(
            r#"
        a = 1
        def fun():
            a
            a = 2
            return a
        "#,
            &[CheckCode::F823],
        )?;
        Ok(())
    }

    #[test]
    fn later_redefined_global_from_nested_scope2() -> Result<()> {
        // Test that referencing a local name in a nested scope that shadows a
        // global declared in an enclosing scope, before it is defined, generates
        // a warning.
        flakes(
            r#"
            a = 1
            def fun():
                global a
                def fun2():
                    a
                    a = 2
                    return a
        "#,
            &[CheckCode::F823],
        )?;
        Ok(())
    }

    #[test]
    fn intermediate_class_scope_ignored() -> Result<()> {
        // If a name defined in an enclosing scope is shadowed by a local variable
        // and the name is used locally before it is bound, an unbound local
        // warning is emitted, even if there is a class scope between the enclosing
        // scope and the local scope.
        flakes(
            r#"
        def f():
            x = 1
            class g:
                def h(self):
                    a = x
                    x = None
                    print(x, a)
            print(x)
        "#,
            &[CheckCode::F823],
        )?;
        Ok(())
    }

    #[test]
    fn later_redefined_global_from_nested_scope3() -> Result<()> {
        // Test that referencing a local name in a nested scope that shadows a
        // global, before it is defined, generates a warning.
        flakes(
            r#"
            def fun():
                a = 1
                def fun2():
                    a
                    a = 1
                    return a
                return a
        "#,
            &[CheckCode::F823],
        )?;
        Ok(())
    }

    #[test]
    fn undefined_augmented_assignment() -> Result<()> {
        flakes(
            r#"
            def f(seq):
                a = 0
                seq[a] += 1
                seq[b] /= 2
                c[0] *= 2
                a -= 3
                d += 4
                e[any] = 5
            "#,
            &[
                CheckCode::F821,
                CheckCode::F821,
                CheckCode::F821,
                CheckCode::F841,
                CheckCode::F821,
            ],
        )?;
        Ok(())
    }

    #[test]
    fn nested_class() -> Result<()> {
        // Nested classes can access enclosing scope.
        flakes(
            r#"
        def f(foo):
            class C:
                bar = foo
                def f(self):
                    return foo
            return C()

        f(123).f()
        "#,
            &[],
        )?;
        Ok(())
    }

    #[test]
    fn bad_nested_class() -> Result<()> {
        // Free variables in nested classes must bind at class creation.
        flakes(
            r#"
        def f():
            class C:
                bar = foo
            foo = 456
            return foo
        f()
        "#,
            &[CheckCode::F821],
        )?;
        Ok(())
    }

    #[test]
    fn defined_as_star_args() -> Result<()> {
        // Star and double-star arg names are defined.
        flakes(
            r#"
        def f(a, *b, **c):
            print(a, b, c)
        "#,
            &[],
        )?;
        Ok(())
    }

    #[test]
    fn defined_as_star_unpack() -> Result<()> {
        // Star names in unpack are defined.
        flakes(
            r#"
        a, *b = range(10)
        print(a, b)
        "#,
            &[],
        )?;
        flakes(
            r#"
        *a, b = range(10)
        print(a, b)
        "#,
            &[],
        )?;
        flakes(
            r#"
        a, *b, c = range(10)
        print(a, b, c)
        "#,
            &[],
        )?;
        Ok(())
    }

    #[test]
    fn used_as_star_unpack() -> Result<()> {
        // Star names in unpack are used if RHS is not a tuple/list literal.
        flakes(
            r#"
        def f():
            a, *b = range(10)
        "#,
            &[],
        )?;
        flakes(
            r#"
        def f():
            (*a, b) = range(10)
        "#,
            &[],
        )?;
        flakes(
            r#"
        def f():
            [a, *b, c] = range(10)
        "#,
            &[],
        )?;
        Ok(())
    }

    #[test]
    fn keyword_only_args() -> Result<()> {
        // Keyword-only arg names are defined.
        flakes(
            r#"
        def f(*, a, b=None):
            print(a, b)
        "#,
            &[],
        )?;
        flakes(
            r#"
        import default_b
        def f(*, a, b=default_b):
            print(a, b)
        "#,
            &[],
        )?;
        Ok(())
    }

    #[test]
    fn keyword_only_args_undefined() -> Result<()> {
        // Typo in kwonly name.
        flakes(
            r#"
        def f(*, a, b=default_c):
            print(a, b)
        "#,
            &[CheckCode::F821],
        )?;
        Ok(())
    }

    #[test]
    fn annotation_undefined() -> Result<()> {
        // Undefined annotations.
        flakes(
            r#"
        from abc import note1, note2, note3, note4, note5
        def func(a: note1, *args: note2,
                 b: note3=12, **kw: note4) -> note5: pass
        "#,
            &[],
        )?;

        flakes(
            r#"
        def func():
            d = e = 42
            def func(a: {1, d}) -> (lambda c: e): pass
        "#,
            &[],
        )?;

        Ok(())
    }

    #[test]
    fn meta_class_undefined() -> Result<()> {
        flakes(
            r#"
        from abc import ABCMeta
        class A(metaclass=ABCMeta): pass
        "#,
            &[],
        )?;

        Ok(())
    }

    #[test]
    fn defined_in_gen_exp() -> Result<()> {
        // Using the loop variable of a generator expression results in no
        // warnings.
        flakes("(a for a in [1, 2, 3] if a)", &[])?;

        flakes("(b for b in (a for a in [1, 2, 3] if a) if b)", &[])?;

        Ok(())
    }

    #[test]
    fn undefined_in_gen_exp_nested() -> Result<()> {
        // The loop variables of generator expressions nested together are
        // not defined in the other generator.
        flakes(
            "(b for b in (a for a in [1, 2, 3] if b) if b)",
            &[CheckCode::F821],
        )?;

        flakes(
            "(b for b in (a for a in [1, 2, 3] if a) if a)",
            &[CheckCode::F821],
        )?;
        Ok(())
    }

    #[test]
    fn undefined_with_error_handler() -> Result<()> {
        // Some compatibility code checks explicitly for NameError.
        // It should not trigger warnings.
        flakes(
            r#"
        try:
            socket_map
        except NameError:
            socket_map = {}
        "#,
            &[],
        )?;
        flakes(
            r#"
        try:
            _memoryview.contiguous
        except (NameError, AttributeError):
            raise RuntimeError("Python >= 3.3 is required")
        "#,
            &[],
        )?;
        // If NameError is not explicitly handled, generate a warning.
        flakes(
            r#"
        try:
            socket_map
        except:
            socket_map = {}
        "#,
            &[CheckCode::F821],
        )?;
        flakes(
            r#"
        try:
            socket_map
        except Exception:
            socket_map = {}
        "#,
            &[CheckCode::F821],
        )?;
        Ok(())
    }

    #[test]
    fn defined_in_class() -> Result<()> {
        // Defined name for generator expressions and dict/set comprehension.
        flakes(
            r#"
        class A:
            T = range(10)

            Z = (x for x in T)
            L = [x for x in T]
            B = dict((i, str(i)) for i in T)
        "#,
            &[],
        )?;

        flakes(
            r#"
        class A:
            T = range(10)

            X = {x for x in T}
            Y = {x:x for x in T}
        "#,
            &[],
        )?;
        Ok(())
    }

    #[test]
    fn defined_in_class_nested() -> Result<()> {
        // Defined name for nested generator expressions in a class.
        flakes(
            r#"
        class A:
            T = range(10)

            Z = (x for x in (a for a in T))
        "#,
            &[],
        )?;
        Ok(())
    }

    #[test]
    fn undefined_in_loop() -> Result<()> {
        // The loop variable is defined after the expression is computed.
        flakes(
            r#"
        for i in range(i):
            print(i)
        "#,
            &[CheckCode::F821],
        )?;
        flakes(
            r#"
        [42 for i in range(i)]
        "#,
            &[CheckCode::F821],
        )?;
        flakes(
            r#"
        (42 for i in range(i))
        "#,
            &[CheckCode::F821],
        )?;
        Ok(())
    }

    #[test]
    fn defined_from_lambda_in_dictionary_comprehension() -> Result<()> {
        // Defined name referenced from a lambda function within a dict/set
        // comprehension.
        flakes(
            r#"
        {lambda: id(x) for x in range(10)}
        "#,
            &[],
        )?;
        Ok(())
    }

    #[test]
    fn defined_from_lambda_in_generator() -> Result<()> {
        // Defined name referenced from a lambda function within a generator
        // expression.
        flakes(
            r#"
        any(lambda: id(x) for x in range(10))
        "#,
            &[],
        )?;
        Ok(())
    }

    #[test]
    fn undefined_from_lambda_in_dictionary_comprehension() -> Result<()> {
        // Undefined name referenced from a lambda function within a dict/set
        // comprehension.
        flakes(
            r#"
        {lambda: id(y) for x in range(10)}
        "#,
            &[CheckCode::F821],
        )?;
        Ok(())
    }

    #[test]
    fn undefined_from_lambda_in_comprehension() -> Result<()> {
        // Undefined name referenced from a lambda function within a generator
        // expression.
        flakes(
            r#"
        any(lambda: id(y) for x in range(10))
        "#,
            &[CheckCode::F821],
        )?;
        Ok(())
    }

    #[test]
    fn dunder_class() -> Result<()> {
        flakes(
            r#"
        class Test(object):
            def __init__(self):
                print(__class__.__name__)
                self.x = 1

        t = Test()
        "#,
            &[],
        )?;
        Ok(())
    }

    /// See: <https://github.com/PyCQA/pyflakes/blob/04ecb0c324ef3b61124e2f80f9e1af6c3a4c7b26/pyflakes/test/test_imports.py>
    #[test]
    fn unused_import() -> Result<()> {
        flakes("import fu, bar", &[CheckCode::F401, CheckCode::F401])?;
        flakes(
            "from baz import fu, bar",
            &[CheckCode::F401, CheckCode::F401],
        )?;
        Ok(())
    }

    #[test]
    fn unused_import_relative() -> Result<()> {
        flakes("from . import fu", &[CheckCode::F401])?;
        flakes("from . import fu as baz", &[CheckCode::F401])?;
        flakes("from .. import fu", &[CheckCode::F401])?;
        flakes("from ... import fu", &[CheckCode::F401])?;
        flakes("from .. import fu as baz", &[CheckCode::F401])?;
        flakes("from .bar import fu", &[CheckCode::F401])?;
        flakes("from ..bar import fu", &[CheckCode::F401])?;
        flakes("from ...bar import fu", &[CheckCode::F401])?;
        flakes("from ...bar import fu as baz", &[CheckCode::F401])?;

        Ok(())
    }

    #[test]
    fn aliased_import() -> Result<()> {
        flakes(
            "import fu as FU, bar as FU",
            &[CheckCode::F401, CheckCode::F811, CheckCode::F401],
        )?;
        flakes(
            "from moo import fu as FU, bar as FU",
            &[CheckCode::F401, CheckCode::F811, CheckCode::F401],
        )?;

        Ok(())
    }

    #[test]
    fn aliased_import_shadow_module() -> Result<()> {
        // Imported aliases can shadow the source of the import.
        flakes("from moo import fu as moo; moo", &[])?;
        flakes("import fu as fu; fu", &[])?;
        flakes("import fu.bar as fu; fu", &[])?;

        Ok(())
    }

    #[test]
    fn used_import() -> Result<()> {
        flakes("import fu; print(fu)", &[])?;
        flakes("from baz import fu; print(fu)", &[])?;
        flakes("import fu; del fu", &[])?;

        Ok(())
    }

    #[test]
    fn used_import_relative() -> Result<()> {
        flakes("from . import fu; assert fu", &[])?;
        flakes("from .bar import fu; assert fu", &[])?;
        flakes("from .. import fu; assert fu", &[])?;
        flakes("from ..bar import fu as baz; assert baz", &[])?;

        Ok(())
    }

    #[test]
    fn redefined_while_unused() -> Result<()> {
        flakes("import fu; fu = 3", &[CheckCode::F401, CheckCode::F811])?;
        flakes(
            "import fu; fu, bar = 3",
            &[CheckCode::F401, CheckCode::F811],
        )?;
        flakes(
            "import fu; [fu, bar] = 3",
            &[CheckCode::F401, CheckCode::F811],
        )?;

        Ok(())
    }

    #[test]
    fn redefined_if() -> Result<()> {
        // Test that importing a module twice within an if
        // block does raise a warning.
        flakes(
            r#"
        i = 2
        if i==1:
            import os
            import os
        os.path
        "#,
            &[CheckCode::F401, CheckCode::F811],
        )?;

        Ok(())
    }

    #[test]
    fn redefined_if_else() -> Result<()> {
        // Test that importing a module twice in if
        // and else blocks does not raise a warning.
        flakes(
            r#"
        i = 2
        if i==1:
            import os
        else:
            import os
        os.path
        "#,
            &[],
        )?;

        Ok(())
    }

    #[test]
    fn redefined_try() -> Result<()> {
        // Test that importing a module twice in a try block
        // does raise a warning.
        flakes(
            r#"
        try:
            import os
            import os
        except:
            pass
        os.path
        "#,
            &[CheckCode::F401, CheckCode::F811],
        )?;

        Ok(())
    }

    #[test]
    fn redefined_try_except() -> Result<()> {
        // Test that importing a module twice in a try
        // and except block does not raise a warning.
        flakes(
            r#"
        try:
            import os
        except:
            import os
        os.path
        "#,
            &[],
        )?;

        Ok(())
    }

    #[test]
    fn redefined_try_nested() -> Result<()> {
        // Test that importing a module twice using a nested
        // try/except and if blocks does not issue a warning.
        flakes(
            r#"
        try:
            if True:
                if True:
                    import os
        except:
            import os
        os.path
        "#,
            &[],
        )?;

        Ok(())
    }

    #[test]
    fn redefined_try_except_multi() -> Result<()> {
        flakes(
            r#"
        try:
            from aa import mixer
        except AttributeError:
            from bb import mixer
        except RuntimeError:
            from cc import mixer
        except:
            from dd import mixer
        mixer(123)
        "#,
            &[],
        )?;

        Ok(())
    }

    #[test]
    fn redefined_try_else() -> Result<()> {
        flakes(
            r#"
        try:
            from aa import mixer
        except ImportError:
            pass
        else:
            from bb import mixer
        mixer(123)
        "#,
            &[CheckCode::F401, CheckCode::F811],
        )?;

        Ok(())
    }

    #[test]
    fn redefined_try_except_else() -> Result<()> {
        flakes(
            r#"
        try:
            import funca
        except ImportError:
            from bb import funca
            from bb import funcb
        else:
            from bbb import funcb
        print(funca, funcb)
        "#,
            &[],
        )?;

        Ok(())
    }

    #[test]
    fn redefined_try_except_finally() -> Result<()> {
        flakes(
            r#"
        try:
            from aa import a
        except ImportError:
            from bb import a
        finally:
            a = 42
        print(a)
        "#,
            &[],
        )?;

        Ok(())
    }

    #[test]
    fn redefined_try_except_else_finally() -> Result<()> {
        flakes(
            r#"
        try:
            import b
        except ImportError:
            b = Ellipsis
            from bb import a
        else:
            from aa import a
        finally:
            a = 42
        print(a, b)
        "#,
            &[],
        )?;

        Ok(())
    }

    #[test]
    fn redefined_by_function() -> Result<()> {
        flakes(
            r#"
        import fu
        def fu():
            pass
        "#,
            &[CheckCode::F401, CheckCode::F811],
        )?;

        Ok(())
    }

    #[test]
    fn redefined_in_nested_function() -> Result<()> {
        // Test that shadowing a global name with a nested function definition
        // generates a warning.
        flakes(
            r#"
        import fu
        def bar():
            def baz():
                def fu():
                    pass
        "#,
            &[CheckCode::F401, CheckCode::F811],
        )?;

        Ok(())
    }

    #[test]
    fn redefined_in_nested_function_twice() -> Result<()> {
        // Test that shadowing a global name with a nested function definition
        // generates a warning.
        flakes(
            r#"
        import fu
        def bar():
            import fu
            def baz():
                def fu():
                    pass
        "#,
            &[
                CheckCode::F401,
                CheckCode::F811,
                CheckCode::F401,
                CheckCode::F811,
            ],
        )?;

        Ok(())
    }

    #[test]
    fn redefined_but_used_later() -> Result<()> {
        // Test that a global import which is redefined locally,
        // but used later in another scope does not generate a warning.
        flakes(
            r#"
        import unittest, transport

        class GetTransportTestCase(unittest.TestCase):
            def test_get_transport(self):
                transport = 'transport'
                self.assertIsNotNone(transport)

        class TestTransportMethodArgs(unittest.TestCase):
            def test_send_defaults(self):
                transport.Transport()"#,
            &[],
        )?;

        Ok(())
    }

    #[test]
    fn redefined_by_class() -> Result<()> {
        flakes(
            r#"
        import fu
        class fu:
            pass
        "#,
            &[CheckCode::F401, CheckCode::F811],
        )?;

        Ok(())
    }

    #[test]
    fn redefined_by_subclass() -> Result<()> {
        // If an imported name is redefined by a class statement which also uses
        // that name in the bases list, no warning is emitted.
        flakes(
            r#"
        from fu import bar
        class bar(bar):
            pass
        "#,
            &[],
        )?;

        Ok(())
    }

    #[test]
    fn redefined_in_class() -> Result<()> {
        // Test that shadowing a global with a class attribute does not produce a
        // warning.
        flakes(
            r#"
        import fu
        class bar:
            fu = 1
        print(fu)
        "#,
            &[],
        )?;

        Ok(())
    }

    #[test]
    fn import_in_class() -> Result<()> {
        // Test that import within class is a locally scoped attribute.
        flakes(
            r#"
        class bar:
            import fu
        "#,
            &[],
        )?;

        flakes(
            r#"
        class bar:
            import fu

        fu
        "#,
            &[CheckCode::F821],
        )?;

        Ok(())
    }

    #[test]
    fn used_in_function() -> Result<()> {
        flakes(
            r#"
        import fu
        def fun():
            print(fu)
        "#,
            &[],
        )?;

        Ok(())
    }

    #[test]
    fn shadowed_by_parameter() -> Result<()> {
        flakes(
            r#"
        import fu
        def fun(fu):
            print(fu)
        "#,
            &[CheckCode::F401, CheckCode::F811],
        )?;

        flakes(
            r#"
        import fu
        def fun(fu):
            print(fu)
        print(fu)
        "#,
            &[],
        )?;

        Ok(())
    }

    #[test]
    fn new_assignment() -> Result<()> {
        flakes("fu = None", &[])?;

        Ok(())
    }

    #[test]
    fn used_in_getattr() -> Result<()> {
        flakes("import fu; fu.bar.baz", &[])?;
        flakes("import fu; \"bar\".fu.baz", &[CheckCode::F401])?;

        Ok(())
    }

    #[test]
    fn used_in_slice() -> Result<()> {
        flakes("import fu; print(fu.bar[1:])", &[])?;
        Ok(())
    }

    #[test]
    fn used_in_if_body() -> Result<()> {
        flakes(
            r#"
        import fu
        if True: print(fu)
        "#,
            &[],
        )?;

        Ok(())
    }

    #[test]
    fn used_in_if_conditional() -> Result<()> {
        flakes(
            r#"
        import fu
        if fu: pass
        "#,
            &[],
        )?;

        Ok(())
    }

    #[test]
    fn used_in_elif_conditional() -> Result<()> {
        flakes(
            r#"
        import fu
        if False: pass
        elif fu: pass
        "#,
            &[],
        )?;

        Ok(())
    }

    #[test]
    fn used_in_else() -> Result<()> {
        flakes(
            r#"
        import fu
        if False: pass
        else: print(fu)
        "#,
            &[],
        )?;

        Ok(())
    }

    #[test]
    fn used_in_call() -> Result<()> {
        flakes("import fu; fu.bar()", &[])?;

        Ok(())
    }

    #[test]
    fn used_in_class() -> Result<()> {
        flakes(
            r#"
        import fu
        class bar:
            bar = fu
        "#,
            &[],
        )?;

        Ok(())
    }

    #[test]
    fn used_in_class_base() -> Result<()> {
        flakes(
            r#"
        import fu
        class bar(object, fu.baz):
            pass
        "#,
            &[],
        )?;

        Ok(())
    }

    #[test]
    fn not_used_in_nested_scope() -> Result<()> {
        flakes(
            r#"
        import fu
        def bleh():
            pass
        print(fu)
        "#,
            &[],
        )?;

        Ok(())
    }

    #[test]
    fn used_in_for() -> Result<()> {
        flakes(
            r#"
        import fu
        for bar in range(9):
            print(fu)
        "#,
            &[],
        )?;

        Ok(())
    }

    #[test]
    fn used_in_for_else() -> Result<()> {
        flakes(
            r#"
        import fu
        for bar in range(10):
            pass
        else:
            print(fu)
        "#,
            &[],
        )?;

        Ok(())
    }

    #[test]
    fn redefined_by_for() -> Result<()> {
        flakes(
            r#"
        import fu
        for fu in range(2):
            pass
        "#,
            &[CheckCode::F401, CheckCode::F402],
        )?;

        Ok(())
    }

    #[test]
    fn shadowed_by_for() -> Result<()> {
        // Test that shadowing a global name with a for loop variable generates a
        // warning.
        flakes(
            r#"
        import fu
        fu.bar()
        for fu in ():
            pass
        "#,
            &[CheckCode::F402],
        )?;

        Ok(())
    }

    #[test]
    fn shadowed_by_for_deep() -> Result<()> {
        // Test that shadowing a global name with a for loop variable nested in a
        // tuple unpack generates a warning.
        flakes(
            r#"
        import fu
        fu.bar()
        for (x, y, z, (a, b, c, (fu,))) in ():
            pass
        "#,
            &[CheckCode::F402],
        )?;
        flakes(
            r#"
        import fu
        fu.bar()
        for [x, y, z, (a, b, c, (fu,))] in ():
            pass
        "#,
            &[CheckCode::F402],
        )?;

        Ok(())
    }

    #[test]
    fn used_in_return() -> Result<()> {
        flakes(
            r#"
        import fu
        def fun():
            return fu
        "#,
            &[],
        )?;

        Ok(())
    }

    #[test]
    fn used_in_operators() -> Result<()> {
        flakes("import fu; 3 + fu.bar", &[])?;
        flakes("import fu; 3 % fu.bar", &[])?;
        flakes("import fu; 3 - fu.bar", &[])?;
        flakes("import fu; 3 * fu.bar", &[])?;
        flakes("import fu; 3 ** fu.bar", &[])?;
        flakes("import fu; 3 / fu.bar", &[])?;
        flakes("import fu; 3 // fu.bar", &[])?;
        flakes("import fu; -fu.bar", &[])?;
        flakes("import fu; ~fu.bar", &[])?;
        flakes("import fu; 1 == fu.bar", &[])?;
        flakes("import fu; 1 | fu.bar", &[])?;
        flakes("import fu; 1 & fu.bar", &[])?;
        flakes("import fu; 1 ^ fu.bar", &[])?;
        flakes("import fu; 1 >> fu.bar", &[])?;
        flakes("import fu; 1 << fu.bar", &[])?;

        Ok(())
    }

    #[test]
    fn used_in_assert() -> Result<()> {
        flakes("import fu; assert fu.bar", &[])?;

        Ok(())
    }

    #[test]
    fn used_in_subscript() -> Result<()> {
        flakes("import fu; fu.bar[1]", &[])?;

        Ok(())
    }

    #[test]
    fn used_in_logic() -> Result<()> {
        flakes("import fu; fu and False", &[])?;
        flakes("import fu; fu or False", &[])?;
        flakes("import fu; not fu.bar", &[])?;

        Ok(())
    }

    #[test]
    fn used_in_list() -> Result<()> {
        flakes("import fu; [fu]", &[])?;

        Ok(())
    }

    #[test]
    fn used_in_tuple() -> Result<()> {
        flakes("import fu; (fu,)", &[])?;

        Ok(())
    }

    #[test]
    fn used_in_try() -> Result<()> {
        flakes(
            r#"
        import fu
        try: fu
        except: pass
        "#,
            &[],
        )?;

        Ok(())
    }

    #[test]
    fn used_in_except() -> Result<()> {
        flakes(
            r#"
        import fu
        try: fu
        except: pass
        "#,
            &[],
        )?;

        Ok(())
    }

    #[test]
    fn redefined_by_except() -> Result<()> {
        flakes(
            r#"
        import fu
        try: pass
        except Exception as fu: pass
        "#,
            &[CheckCode::F401, CheckCode::F811, CheckCode::F841],
        )?;

        Ok(())
    }

    #[test]
    fn used_in_raise() -> Result<()> {
        flakes(
            r#"
        import fu
        raise fu.bar
        "#,
            &[],
        )?;

        Ok(())
    }

    #[test]
    fn used_in_yield() -> Result<()> {
        flakes(
            r#"
        import fu
        def gen():
            yield fu
        "#,
            &[],
        )?;

        Ok(())
    }

    #[test]
    fn used_in_dict() -> Result<()> {
        flakes("import fu; {fu:None}", &[])?;
        flakes("import fu; {1:fu}", &[])?;

        Ok(())
    }

    #[test]
    fn used_in_parameter_default() -> Result<()> {
        flakes(
            r#"
        import fu
        def f(bar=fu):
            pass
        "#,
            &[],
        )?;

        Ok(())
    }

    #[test]
    fn used_in_attribute_assign() -> Result<()> {
        flakes("import fu; fu.bar = 1", &[])?;

        Ok(())
    }

    #[test]
    fn used_in_keyword_arg() -> Result<()> {
        flakes("import fu; fu.bar(stuff=fu)", &[])?;

        Ok(())
    }

    #[test]
    fn used_in_assignment() -> Result<()> {
        flakes("import fu; bar=fu", &[])?;
        flakes("import fu; n=0; n+=fu", &[])?;

        Ok(())
    }

    #[test]
    fn used_in_list_comp() -> Result<()> {
        flakes("import fu; [fu for _ in range(1)]", &[])?;
        flakes("import fu; [1 for _ in range(1) if fu]", &[])?;

        Ok(())
    }

    #[test]
    fn used_in_try_finally() -> Result<()> {
        flakes(
            r#"
        import fu
        try: pass
        finally: fu
        "#,
            &[],
        )?;

        flakes(
            r#"
        import fu
        try: fu
        finally: pass
        "#,
            &[],
        )?;

        Ok(())
    }

    #[test]
    fn used_in_while() -> Result<()> {
        flakes(
            r#"
        import fu
        while 0:
            fu
        "#,
            &[],
        )?;

        flakes(
            r#"
        import fu
        while fu: pass
        "#,
            &[],
        )?;

        Ok(())
    }

    #[test]
    fn used_in_global() -> Result<()> {
        // A 'global' statement shadowing an unused import should not prevent it
        // from being reported.
        flakes(
            r#"
        import fu
        def f(): global fu
        "#,
            &[CheckCode::F401],
        )?;

        Ok(())
    }

    #[test]
    fn used_and_global() -> Result<()> {
        // A 'global' statement shadowing a used import should not cause it to be
        // reported as unused.
        flakes(
            r#"
            import foo
            def f(): global foo
            def g(): foo.is_used()
        "#,
            &[],
        )?;

        Ok(())
    }

    #[test]
    fn assigned_to_global() -> Result<()> {
        // Binding an import to a declared global should not cause it to be
        // reported as unused.
        flakes(
            r#"
            def f(): global foo; import foo
            def g(): foo.is_used()
        "#,
            &[],
        )?;

        Ok(())
    }

    #[test]
    fn used_in_exec() -> Result<()> {
        flakes("import fu; exec('print(1)', fu.bar)", &[])?;

        Ok(())
    }

    #[test]
    fn used_in_lambda() -> Result<()> {
        flakes(
            r#"import fu;
        lambda: fu
        "#,
            &[],
        )?;

        Ok(())
    }

    #[test]
    fn shadowed_by_lambda() -> Result<()> {
        flakes(
            "import fu; lambda fu: fu",
            &[CheckCode::F401, CheckCode::F811],
        )?;
        flakes("import fu; lambda fu: fu\nfu()", &[])?;

        Ok(())
    }

    #[test]
    fn used_in_slice_obj() -> Result<()> {
        flakes(
            r#"import fu;
        "meow"[::fu]
        "#,
            &[],
        )?;

        Ok(())
    }

    #[test]
    fn unused_in_nested_scope() -> Result<()> {
        flakes(
            r#"
        def bar():
            import fu
        fu
        "#,
            &[CheckCode::F401, CheckCode::F821],
        )?;

        Ok(())
    }

    #[test]
    fn methods_dont_use_class_scope() -> Result<()> {
        flakes(
            r#"
        class bar:
            import fu
            def fun(self):
                fu
        "#,
            &[CheckCode::F821],
        )?;

        Ok(())
    }

    #[test]
    fn nested_functions_nest_scope() -> Result<()> {
        flakes(
            r#"
        def a():
            def b():
                fu
            import fu
        "#,
            &[],
        )?;

        Ok(())
    }

    #[test]
    fn nested_class_and_function_scope() -> Result<()> {
        flakes(
            r#"
        def a():
            import fu
            class b:
                def c(self):
                    print(fu)
        "#,
            &[],
        )?;

        Ok(())
    }

    #[test]
    fn package_import() -> Result<()> {
        // If a dotted name is imported and used, no warning is reported.
        flakes(
            r#"
        import fu.bar
        fu.bar
        "#,
            &[],
        )?;

        Ok(())
    }

    #[test]
    fn unused_package_import() -> Result<()> {
        // If a dotted name is imported and not used, an unused import warning is
        // reported.
        flakes("import fu.bar", &[CheckCode::F401])?;

        Ok(())
    }

    #[test]
    fn duplicate_submodule_import() -> Result<()> {
        // If a submodule of a package is imported twice, an unused import warning and a
        // redefined while unused warning are reported.
        flakes(
            r#"
        import fu.bar, fu.bar
        fu.bar
        "#,
            &[CheckCode::F401, CheckCode::F811],
        )?;
        flakes(
            r#"
        import fu.bar
        import fu.bar
        fu.bar
        "#,
            &[CheckCode::F401, CheckCode::F811],
        )?;

        Ok(())
    }

    #[test]
    fn different_submodule_import() -> Result<()> {
        // If two different submodules of a package are imported, no duplicate import
        // warning is reported for the package.
        flakes(
            r#"
        import fu.bar, fu.baz
        fu.bar, fu.baz
        "#,
            &[],
        )?;
        flakes(
            r#"
        import fu.bar
        import fu.baz
        fu.bar, fu.baz
        "#,
            &[],
        )?;

        Ok(())
    }

    #[test]
    fn used_package_with_submodule_import() -> Result<()> {
        // Usage of package marks submodule imports as used.
        flakes(
            r#"
        import fu
        import fu.bar
        fu.x
        "#,
            &[],
        )?;

        flakes(
            r#"
        import fu.bar
        import fu
        fu.x
        "#,
            &[],
        )?;

        Ok(())
    }

    #[test]
    fn used_package_with_submodule_import_of_alias() -> Result<()> {
        // Usage of package by alias marks submodule imports as used.
        flakes(
            r#"
        import foo as f
        import foo.bar
        f.bar.do_something()
        "#,
            &[],
        )?;

        flakes(
            r#"
        import foo as f
        import foo.bar.blah
        f.bar.blah.do_something()
        "#,
            &[],
        )?;

        Ok(())
    }

    #[test]
    fn unused_package_with_submodule_import() -> Result<()> {
        // When a package and its submodule are imported, only report once.
        flakes(
            r#"
        import fu
        import fu.bar
        "#,
            &[CheckCode::F401],
        )?;

        Ok(())
    }

    #[test]
    fn assign_rhs_first() -> Result<()> {
        flakes("import fu; fu = fu", &[])?;
        flakes("import fu; fu, bar = fu", &[])?;
        flakes("import fu; [fu, bar] = fu", &[])?;
        flakes("import fu; fu += fu", &[])?;

        Ok(())
    }

    #[test]
    fn trying_multiple_imports() -> Result<()> {
        flakes(
            r#"
        try:
            import fu
        except ImportError:
            import bar as fu
        fu
        "#,
            &[],
        )?;

        Ok(())
    }

    #[test]
    fn non_global_does_not_redefine() -> Result<()> {
        flakes(
            r#"
        import fu
        def a():
            fu = 3
            return fu
        fu
        "#,
            &[],
        )?;

        Ok(())
    }

    #[test]
    fn functions_run_later() -> Result<()> {
        flakes(
            r#"
        def a():
            fu
        import fu
        "#,
            &[],
        )?;

        Ok(())
    }

    #[test]
    fn function_names_are_bound_now() -> Result<()> {
        flakes(
            r#"
        import fu
        def fu():
            fu
        fu
        "#,
            &[CheckCode::F401, CheckCode::F811],
        )?;

        Ok(())
    }

    #[test]
    fn ignore_non_import_redefinitions() -> Result<()> {
        flakes("a = 1; a = 2", &[])?;

        Ok(())
    }

    #[test]
    fn imported_in_class() -> Result<()> {
        // Imports in class scope can be used through self.
        flakes(
            r#"
        class C:
            import i
            def __init__(self):
                self.i
        "#,
            &[],
        )?;

        Ok(())
    }

    #[test]
    fn import_used_in_method_definition() -> Result<()> {
        // Method named 'foo' with default args referring to module named 'foo'.
        flakes(
            r#"
        import foo

        class Thing(object):
            def foo(self, parser=foo.parse_foo):
                pass
        "#,
            &[],
        )?;

        Ok(())
    }

    #[test]
    fn future_import() -> Result<()> {
        // __future__ is special.
        flakes("from __future__ import division", &[])?;
        flakes(
            r#"
        "docstring is allowed before future import"
        from __future__ import division
        "#,
            &[],
        )?;

        Ok(())
    }

    #[test]
    fn future_import_first() -> Result<()> {
        // __future__ imports must come before anything else.
        flakes(
            r#"
        x = 5
        from __future__ import division
        "#,
            &[CheckCode::F404],
        )?;
        flakes(
            r#"
        from foo import bar
        from __future__ import division
        bar
        "#,
            &[CheckCode::F404],
        )?;

        Ok(())
    }

    #[test]
    fn future_import_used() -> Result<()> {
        // __future__ is special, but names are injected in the namespace.
        flakes(
            r#"
        from __future__ import division
        from __future__ import print_function

        assert print_function is not division
        "#,
            &[],
        )?;

        Ok(())
    }

    #[test]
    fn future_import_undefined() -> Result<()> {
        // Importing undefined names from __future__ fails.
        flakes(
            r#"
        from __future__ import print_statement
        "#,
            &[CheckCode::F407],
        )?;

        Ok(())
    }

    #[test]
    fn future_import_star() -> Result<()> {
        // Importing '*' from __future__ fails.
        flakes(
            r#"
        from __future__ import *
        "#,
            &[CheckCode::F407],
        )?;

        Ok(())
    }

    #[test]
    fn ignored_in_function() -> Result<()> {
        // An C{__all__} definition does not suppress unused import warnings in a
        // function scope.
        flakes(
            r#"
        def foo():
            import bar
            __all__ = ["bar"]
        "#,
            &[CheckCode::F401, CheckCode::F841],
        )?;

        Ok(())
    }

    #[test]
    fn ignored_in_class() -> Result<()> {
        // An C{__all__} definition in a class does not suppress unused import warnings.
        flakes(
            r#"
        import bar
        class foo:
            __all__ = ["bar"]
        "#,
            &[CheckCode::F401],
        )?;

        Ok(())
    }

    #[test]
    fn ignored_when_not_directly_assigned() -> Result<()> {
        flakes(
            r#"
        import bar
        (__all__,) = ("foo",)
        "#,
            &[CheckCode::F401],
        )?;

        Ok(())
    }

    #[test]
    fn warning_suppressed() -> Result<()> {
        // If a name is imported and unused but is named in C{__all__}, no warning
        // is reported.
        flakes(
            r#"
        import foo
        __all__ = ["foo"]
        "#,
            &[],
        )?;
        flakes(
            r#"
        import foo
        __all__ = ("foo",)
        "#,
            &[],
        )?;

        Ok(())
    }

    #[test]
    fn augmented_assignment() -> Result<()> {
        // The C{__all__} variable is defined incrementally.
        flakes(
            r#"
        import a
        import c
        __all__ = ['a']
        __all__ += ['b']
        if 1 < 3:
            __all__ += ['c', 'd']
        "#,
            &[CheckCode::F822, CheckCode::F822],
        )?;

        Ok(())
    }

    #[test]
    fn list_concatenation_assignment() -> Result<()> {
        // The C{__all__} variable is defined through list concatenation.
        flakes(
            r#"
        import sys
        __all__ = ['a'] + ['b'] + ['c']
        "#,
            &[
                CheckCode::F401,
                CheckCode::F822,
                CheckCode::F822,
                CheckCode::F822,
            ],
        )?;

        Ok(())
    }

    #[test]
    fn tuple_concatenation_assignment() -> Result<()> {
        // The C{__all__} variable is defined through tuple concatenation.
        flakes(
            r#"
        import sys
        __all__ = ('a',) + ('b',) + ('c',)
        "#,
            &[
                CheckCode::F401,
                CheckCode::F822,
                CheckCode::F822,
                CheckCode::F822,
            ],
        )?;

        Ok(())
    }

    #[test]
    fn all_with_attributes() -> Result<()> {
        flakes(
            r#"
        from foo import bar
        __all__ = [bar.__name__]
        "#,
            &[],
        )?;

        Ok(())
    }

    #[test]
    fn all_with_names() -> Result<()> {
        flakes(
            r#"
        from foo import bar
        __all__ = [bar]
        "#,
            &[],
        )?;

        Ok(())
    }

    #[test]
    fn all_with_attributes_added() -> Result<()> {
        flakes(
            r#"
        from foo import bar
        from bar import baz
        __all__ = [bar.__name__] + [baz.__name__]
        "#,
            &[],
        )?;

        Ok(())
    }

    #[test]
    fn all_mixed_attributes_and_strings() -> Result<()> {
        flakes(
            r#"
        from foo import bar
        from foo import baz
        __all__ = ['bar', baz.__name__]
        "#,
            &[],
        )?;

        Ok(())
    }

    #[test]
    fn unbound_exported() -> Result<()> {
        // If C{__all__} includes a name which is not bound, a warning is emitted.
        flakes(
            r#"
        __all__ = ["foo"]
        "#,
            &[CheckCode::F822],
        )?;

        Ok(())
    }

    #[test]
    fn import_star_exported() -> Result<()> {
        // Report undefined if import * is used
        flakes(
            r#"
        from math import *
        __all__ = ['sin', 'cos']
        csc(1)
        "#,
            &[
                CheckCode::F403,
                CheckCode::F405,
                CheckCode::F405,
                CheckCode::F405,
            ],
        )?;

        Ok(())
    }

    #[ignore]
    #[test]
    fn import_star_not_exported() -> Result<()> {
        // Report unused import when not needed to satisfy __all__.
        flakes(
            r#"
        from foolib import *
        a = 1
        __all__ = ['a']
        "#,
            &[CheckCode::F403, CheckCode::F401],
        )?;

        Ok(())
    }

    #[test]
    fn used_in_gen_exp() -> Result<()> {
        // Using a global in a generator expression results in no warnings.
        flakes("import fu; (fu for _ in range(1))", &[])?;
        flakes("import fu; (1 for _ in range(1) if fu)", &[])?;

        Ok(())
    }

    #[test]
    fn redefined_by_gen_exp() -> Result<()> {
        // Re-using a global name as the loop variable for a generator
        // expression results in a redefinition warning.
        flakes(
            "import fu; (1 for fu in range(1))",
            &[CheckCode::F401, CheckCode::F811],
        )?;

        Ok(())
    }

    #[test]
    fn used_as_decorator() -> Result<()> {
        // Using a global name in a decorator statement results in no warnings,
        // but using an undefined name in a decorator statement results in an
        // undefined name warning.
        flakes(
            r#"
        from interior import decorate
        @decorate
        def f():
            return "hello"
        "#,
            &[],
        )?;

        flakes(
            r#"
        from interior import decorate
        @decorate('value", &[])?;
        def f():
            return "hello"
        "#,
            &[],
        )?;

        flakes(
            r#"
        @decorate
        def f():
            return "hello"
        "#,
            &[CheckCode::F821],
        )?;

        Ok(())
    }

    #[test]
    fn used_as_class_decorator() -> Result<()> {
        // Using an imported name as a class decorator results in no warnings,
        // but using an undefined name as a class decorator results in an
        // undefined name warning.
        flakes(
            r#"
        from interior import decorate
        @decorate
        class foo:
            pass
        "#,
            &[],
        )?;

        flakes(
            r#"
        from interior import decorate
        @decorate("foo")
        class bar:
            pass
        "#,
            &[],
        )?;

        flakes(
            r#"
        @decorate
        class foo:
            pass
        "#,
            &[CheckCode::F821],
        )?;

        Ok(())
    }

    /// See: <https://github.com/PyCQA/pyflakes/blob/04ecb0c324ef3b61124e2f80f9e1af6c3a4c7b26/pyflakes/test/test_type_annotations.py>
    #[test]
    fn typing_overload() -> Result<()> {
        // Allow intentional redefinitions via @typing.overload.
        flakes(
            r#"
        import typing
        from typing import overload

        @overload
        def f(s: None) -> None:
            pass

        @overload
        def f(s: int) -> int:
            pass

        def f(s):
            return s

        @typing.overload
        def g(s: None) -> None:
            pass

        @typing.overload
        def g(s: int) -> int:
            pass

        def g(s):
            return s
        "#,
            &[],
        )?;
        Ok(())
    }

    #[test]
    fn typing_extensions_overload() -> Result<()> {
        // Allow intentional redefinitions via @typing_extensions.overload.
        flakes(
            r#"
        import typing_extensions
        from typing_extensions import overload

        @overload
        def f(s: None) -> None:
            pass

        @overload
        def f(s: int) -> int:
            pass

        def f(s):
            return s

        @typing_extensions.overload
        def g(s: None) -> None:
            pass

        @typing_extensions.overload
        def g(s: int) -> int:
            pass

        def g(s):
            return s
        "#,
            &[],
        )?;
        Ok(())
    }

    #[test]
    fn typing_overload_async() -> Result<()> {
        // Allow intentional redefinitions via @typing.overload (async).
        flakes(
            r#"
        from typing import overload

        @overload
        async def f(s: None) -> None:
            pass

        @overload
        async def f(s: int) -> int:
            pass

        async def f(s):
            return s
        "#,
            &[],
        )?;
        Ok(())
    }

    #[test]
    fn overload_with_multiple_decorators() -> Result<()> {
        flakes(
            r#"
            from typing import overload
            dec = lambda f: f

            @dec
            @overload
            def f(x: int) -> int:
                pass

            @dec
            @overload
            def f(x: str) -> str:
                pass

            @dec
            def f(x): return x
       "#,
            &[],
        )?;
        Ok(())
    }

    #[test]
    fn overload_in_class() -> Result<()> {
        flakes(
            r#"
        from typing import overload

        class C:
            @overload
            def f(self, x: int) -> int:
                pass

            @overload
            def f(self, x: str) -> str:
                pass

            def f(self, x): return x
        "#,
            &[],
        )?;
        Ok(())
    }

    #[test]
    fn aliased_typing_import() -> Result<()> {
        // Detect when typing is imported as another name.
        flakes(
            r#"
        import typing as t

        @t.overload
        def f(s: None) -> None:
            pass

        @t.overload
        def f(s: int) -> int:
            pass

        def f(s):
            return s
        "#,
            &[],
        )?;
        Ok(())
    }

    #[test]
    fn not_a_typing_overload() -> Result<()> {
        // regression test for @typing.overload detection bug in 2.1.0.
        flakes(
            r#"
            def foo(x):
                return x

            @foo
            def bar():
                pass

            def bar():
                pass
        "#,
            &[CheckCode::F811],
        )?;
        Ok(())
    }

    #[test]
    fn variable_annotations() -> Result<()> {
        flakes(
            r#"
        name: str
        age: int
        "#,
            &[],
        )?;
        flakes(
            r#"
        name: str = 'Bob'
        age: int = 18
        "#,
            &[],
        )?;
        flakes(
            r#"
        class C:
            name: str
            age: int
        "#,
            &[],
        )?;
        flakes(
            r#"
        class C:
            name: str = 'Bob'
            age: int = 18
        "#,
            &[],
        )?;
        flakes(
            r#"
        def f():
            name: str
            age: int
        "#,
            &[CheckCode::F842, CheckCode::F842],
        )?;
        flakes(
            r#"
        def f():
            name: str = 'Bob'
            age: int = 18
            foo: not_a_real_type = None
        "#,
            &[
                CheckCode::F841,
                CheckCode::F841,
                CheckCode::F841,
                CheckCode::F821,
            ],
        )?;
        flakes(
            r#"
        def f():
            name: str
            print(name)
        "#,
            &[CheckCode::F821],
        )?;
        flakes(
            r#"
        from typing import Any
        def f():
            a: Any
        "#,
            &[CheckCode::F842],
        )?;
        flakes(
            r#"
        foo: not_a_real_type
        "#,
            &[CheckCode::F821],
        )?;
        flakes(
            r#"
        foo: not_a_real_type = None
        "#,
            &[CheckCode::F821],
        )?;
        flakes(
            r#"
        class C:
            foo: not_a_real_type
        "#,
            &[CheckCode::F821],
        )?;
        flakes(
            r#"
        class C:
            foo: not_a_real_type = None
        "#,
            &[CheckCode::F821],
        )?;
        flakes(
            r#"
        def f():
            class C:
                foo: not_a_real_type
        "#,
            &[CheckCode::F821],
        )?;
        flakes(
            r#"
        def f():
            class C:
                foo: not_a_real_type = None
        "#,
            &[CheckCode::F821],
        )?;
        flakes(
            r#"
        from foo import Bar
        bar: Bar
        "#,
            &[],
        )?;
        flakes(
            r#"
        from foo import Bar
        bar: 'Bar'
        "#,
            &[],
        )?;
        flakes(
            r#"
        import foo
        bar: foo.Bar
        "#,
            &[],
        )?;
        flakes(
            r#"
        import foo
        bar: 'foo.Bar'
        "#,
            &[],
        )?;
        flakes(
            r#"
        from foo import Bar
        def f(bar: Bar): pass
        "#,
            &[],
        )?;
        flakes(
            r#"
        from foo import Bar
        def f(bar: 'Bar'): pass
        "#,
            &[],
        )?;
        flakes(
            r#"
        from foo import Bar
        def f(bar) -> Bar: return bar
        "#,
            &[],
        )?;
        flakes(
            r#"
        from foo import Bar
        def f(bar) -> 'Bar': return bar
        "#,
            &[],
        )?;
        flakes(
            r#"
        bar: 'Bar'
        "#,
            &[CheckCode::F821],
        )?;
        flakes(
            r#"
        bar: 'foo.Bar'
        "#,
            &[CheckCode::F821],
        )?;
        flakes(
            r#"
        from foo import Bar
        bar: str
        "#,
            &[CheckCode::F401],
        )?;
        flakes(
            r#"
        from foo import Bar
        def f(bar: str): pass
        "#,
            &[CheckCode::F401],
        )?;
        flakes(
            r#"
        def f(a: A) -> A: pass
        class A: pass
        "#,
            &[CheckCode::F821, CheckCode::F821],
        )?;
        flakes(
            r#"
        def f(a: 'A') -> 'A': return a
        class A: pass
        "#,
            &[],
        )?;
        flakes(
            r#"
        a: A
        class A: pass
        "#,
            &[CheckCode::F821],
        )?;
        flakes(
            r#"
        a: 'A'
        class A: pass
        "#,
            &[],
        )?;
        flakes(
            r#"
        T: object
        def f(t: T): pass
        "#,
            &[CheckCode::F821],
        )?;
        flakes(
            r#"
        T: object
        def g(t: 'T'): pass
        "#,
            &[],
        )?;
        flakes(
            r#"
        a: 'A B'
        "#,
            &[CheckCode::F722],
        )?;
        flakes(
            r#"
        a: 'A; B'
        "#,
            &[CheckCode::F722],
        )?;
        flakes(
            r#"
        a: '1 + 2'
        "#,
            &[],
        )?;
        flakes(
            r#"
        a: 'a: "A"'
        "#,
            &[CheckCode::F722],
        )?;
        Ok(())
    }

    #[test]
    fn variable_annotation_references_self_name_undefined() -> Result<()> {
        flakes(
            r#"
        x: int = x
        "#,
            &[CheckCode::F821],
        )?;
        Ok(())
    }

    #[test]
    fn type_alias_annotations() -> Result<()> {
        flakes(
            r#"
        from typing_extensions import TypeAlias
        from foo import Bar

        bar: TypeAlias = Bar
        "#,
            &[],
        )?;
        flakes(
            r#"
        from typing_extensions import TypeAlias
        from foo import Bar

        bar: TypeAlias = 'Bar'
        "#,
            &[],
        )?;
        flakes(
            r#"
        from typing_extensions import TypeAlias
        from foo import Bar

        class A:
            bar: TypeAlias = Bar
        "#,
            &[],
        )?;
        flakes(
            r#"
        from typing_extensions import TypeAlias
        from foo import Bar

        class A:
            bar: TypeAlias = 'Bar'
        "#,
            &[],
        )?;
        flakes(
            r#"
        from typing_extensions import TypeAlias

        bar: TypeAlias
        "#,
            &[],
        )?;
        flakes(
            r#"
        from typing_extensions import TypeAlias
        from foo import Bar

        bar: TypeAlias
        "#,
            &[CheckCode::F401],
        )?;
        Ok(())
    }

    #[test]
    fn annotating_an_import() -> Result<()> {
        flakes(
            r#"
            from a import b, c
            b: c
            print(b)
        "#,
            &[],
        )?;
        Ok(())
    }

    #[test]
    fn unused_annotation() -> Result<()> {
        // Unused annotations are fine in module and class scope.
        flakes(
            r#"
        x: int
        class Cls:
            y: int
        "#,
            &[],
        )?;
        flakes(
            r#"
        def f():
            x: int
        "#,
            &[CheckCode::F842],
        )?;
        // This should only print one UnusedVariable message.
        flakes(
            r#"
        def f():
            x: int
            x = 3
        "#,
            &[CheckCode::F841],
        )?;
        Ok(())
    }

    #[test]
    fn unassigned_annotation_is_undefined() -> Result<()> {
        flakes(
            r#"
        name: str
        print(name)
        "#,
            &[CheckCode::F821],
        )?;
        Ok(())
    }

    #[test]
    fn annotated_async_def() -> Result<()> {
        flakes(
            r#"
        class c: pass
        async def func(c: c) -> None: pass
        "#,
            &[],
        )?;
        Ok(())
    }

    #[test]
    fn postponed_annotations() -> Result<()> {
        flakes(
            r#"
        from __future__ import annotations
        def f(a: A) -> A: pass
        class A:
            b: B
        class B: pass
        "#,
            &[],
        )?;

        flakes(
            r#"
        from __future__ import annotations
        def f(a: A) -> A: pass
        class A:
            b: Undefined
        class B: pass
        "#,
            &[CheckCode::F821],
        )?;

        flakes(
            r#"
        from __future__ import annotations
        T: object
        def f(t: T): pass
        def g(t: 'T'): pass
        "#,
            &[],
        )?;
        Ok(())
    }

    #[test]
    fn type_annotation_clobbers_all() -> Result<()> {
        flakes(
            r#"
        from typing import TYPE_CHECKING, List

        from y import z

        if not TYPE_CHECKING:
            __all__ = ("z",)
        else:
            __all__: List[str]
        "#,
            &[],
        )?;
        Ok(())
    }

    #[test]
    fn return_annotation_is_class_scope_variable() -> Result<()> {
        flakes(
            r#"
        from typing import TypeVar
        class Test:
            Y = TypeVar('Y')

            def t(self, x: Y) -> Y:
                return x
        "#,
            &[],
        )?;
        Ok(())
    }

    #[test]
    fn return_annotation_is_function_body_variable() -> Result<()> {
        flakes(
            r#"
        class Test:
            def t(self) -> Y:
                Y = 2
                return Y
        "#,
            &[CheckCode::F821],
        )?;
        Ok(())
    }

    #[test]
    fn positional_only_argument_annotations() -> Result<()> {
        flakes(
            r#"
        from x import C

        def f(c: C, /): ...
        "#,
            &[],
        )?;
        Ok(())
    }

    #[test]
    fn partially_quoted_type_annotation() -> Result<()> {
        flakes(
            r#"
        from queue import Queue
        from typing import Optional

        def f() -> Optional['Queue[str]']:
            return None
        "#,
            &[],
        )?;
        Ok(())
    }

    #[test]
    fn partially_quoted_type_assignment() -> Result<()> {
        flakes(
            r#"
        from queue import Queue
        from typing import Optional

        MaybeQueue = Optional['Queue[str]']
        "#,
            &[],
        )?;
        Ok(())
    }

    #[test]
    fn nested_partially_quoted_type_assignment() -> Result<()> {
        flakes(
            r#"
        from queue import Queue
        from typing import Callable

        Func = Callable[['Queue[str]'], None]
        "#,
            &[],
        )?;
        Ok(())
    }

    #[test]
    fn quoted_type_cast() -> Result<()> {
        flakes(
            r#"
        from typing import cast, Optional

        maybe_int = cast('Optional[int]', 42)
        "#,
            &[],
        )?;
        Ok(())
    }

    #[test]
    fn type_cast_literal_str_to_str() -> Result<()> {
        // Checks that our handling of quoted type annotations in the first
        // argument to `cast` doesn't cause issues when (only) the _second_
        // argument is a literal str which looks a bit like a type annotation.
        flakes(
            r#"
        from typing import cast

        a_string = cast(str, 'Optional[int]')
        "#,
            &[],
        )?;
        Ok(())
    }

    #[test]
    fn quoted_type_cast_renamed_import() -> Result<()> {
        flakes(
            r#"
        from typing import cast as tsac, Optional as Maybe

        maybe_int = tsac('Maybe[int]', 42)
        "#,
            &[],
        )?;
        Ok(())
    }

    #[test]
    fn quoted_type_var_constraints() -> Result<()> {
        flakes(
            r#"
        from typing import TypeVar, Optional

        T = TypeVar('T', 'str', 'Optional[int]', bytes)
        "#,
            &[],
        )?;
        Ok(())
    }

    #[test]
    fn quoted_type_var_bound() -> Result<()> {
        flakes(
            r#"
        from typing import TypeVar, Optional, List

        T = TypeVar('T', bound='Optional[int]')
        S = TypeVar('S', int, bound='List[int]')
        "#,
            &[],
        )?;
        Ok(())
    }

    #[test]
    fn literal_type_typing() -> Result<()> {
        flakes(
            r#"
        from typing import Literal

        def f(x: Literal['some string']) -> None:
            return None
        "#,
            &[],
        )?;
        Ok(())
    }

    #[test]
    fn literal_type_typing_extensions() -> Result<()> {
        flakes(
            r#"
        from typing_extensions import Literal

        def f(x: Literal['some string']) -> None:
            return None
        "#,
            &[],
        )?;
        Ok(())
    }

    #[test]
    fn annotated_type_typing_missing_forward_type_multiple_args() -> Result<()> {
        flakes(
            r#"
        from typing import Annotated

        def f(x: Annotated['integer', 1]) -> None:
            return None
        "#,
            &[CheckCode::F821],
        )?;
        Ok(())
    }

    #[test]
    fn annotated_type_typing_with_string_args() -> Result<()> {
        flakes(
            r#"
        from typing import Annotated

        def f(x: Annotated[int, '> 0']) -> None:
            return None
        "#,
            &[],
        )?;
        Ok(())
    }

    #[test]
    fn annotated_type_typing_with_string_args_in_union() -> Result<()> {
        flakes(
            r#"
        from typing import Annotated, Union

        def f(x: Union[Annotated['int', '>0'], 'integer']) -> None:
            return None
        "#,
            &[CheckCode::F821],
        )?;
        Ok(())
    }

    // We err on the side of assuming strings are forward references.
    #[ignore]
    #[test]
    fn literal_type_some_other_module() -> Result<()> {
        // err on the side of false-negatives for types named Literal.
        flakes(
            r#"
        from my_module import compat
        from my_module.compat import Literal

        def f(x: compat.Literal['some string']) -> None:
            return None
        def g(x: Literal['some string']) -> None:
            return None
        "#,
            &[],
        )?;
        Ok(())
    }

    #[test]
    fn literal_union_type_typing() -> Result<()> {
        flakes(
            r#"
        from typing import Literal

        def f(x: Literal['some string', 'foo bar']) -> None:
            return None
        "#,
            &[],
        )?;
        Ok(())
    }

    // TODO(charlie): Support nested deferred string annotations.
    #[ignore]
    #[test]
    fn deferred_twice_annotation() -> Result<()> {
        flakes(
            r#"
            from queue import Queue
            from typing import Optional

            def f() -> "Optional['Queue[str]']":
                return None
        "#,
            &[],
        )?;
        Ok(())
    }

    #[test]
    fn partial_string_annotations_with_future_annotations() -> Result<()> {
        flakes(
            r#"
            from __future__ import annotations

            from queue import Queue
            from typing import Optional

            def f() -> Optional['Queue[str]']:
                return None
        "#,
            &[],
        )?;
        Ok(())
    }

    #[test]
    fn forward_annotations_for_classes_in_scope() -> Result<()> {
        flakes(
            r#"
        from typing import Optional

        def f():
            class C:
                a: "D"
                b: Optional["D"]
                c: "Optional[D]"

            class D: pass
        "#,
            &[],
        )?;
        Ok(())
    }

    #[test]
    fn idiomiatic_typing_guards() -> Result<()> {
        // typing.TYPE_CHECKING: python3.5.3+.
        flakes(
            r#"
            from typing import TYPE_CHECKING

            if TYPE_CHECKING:
                from t import T

            def f() -> T:
                pass
        "#,
            &[],
        )?;
        // False: the old, more-compatible approach.
        flakes(
            r#"
            if False:
                from t import T

            def f() -> T:
                pass
        "#,
            &[],
        )?;
        // Some choose to assign a constant and do it that way.
        flakes(
            r#"
            MYPY = False

            if MYPY:
                from t import T

            def f() -> T:
                pass
        "#,
            &[],
        )?;
        Ok(())
    }

    #[test]
    fn typing_guard_for_protocol() -> Result<()> {
        flakes(
            r#"
            from typing import TYPE_CHECKING

            if TYPE_CHECKING:
                from typing import Protocol
            else:
                Protocol = object

            class C(Protocol):
                def f() -> int:
                    pass
        "#,
            &[],
        )?;
        Ok(())
    }

    #[test]
    fn typed_names_correct_forward_ref() -> Result<()> {
        flakes(
            r#"
            from typing import TypedDict, List, NamedTuple

            List[TypedDict("x", {})]
            List[TypedDict("x", x=int)]
            List[NamedTuple("a", a=int)]
            List[NamedTuple("a", [("a", int)])]
        "#,
            &[],
        )?;
        flakes(
            r#"
            from typing import TypedDict, List, NamedTuple, TypeVar

            List[TypedDict("x", {"x": "Y"})]
            List[TypedDict("x", x="Y")]
            List[NamedTuple("a", [("a", "Y")])]
            List[NamedTuple("a", a="Y")]
            List[TypedDict("x", {"x": List["a"]})]
            List[TypeVar("A", bound="C")]
            List[TypeVar("A", List["C"])]
        "#,
            &[
                CheckCode::F821,
                CheckCode::F821,
                CheckCode::F821,
                CheckCode::F821,
                CheckCode::F821,
                CheckCode::F821,
                CheckCode::F821,
            ],
        )?;
        flakes(
            r#"
            from typing import NamedTuple, TypeVar, cast
            from t import A, B, C, D, E

            NamedTuple("A", [("a", A["C"])])
            TypeVar("A", bound=A["B"])
            TypeVar("A", A["D"])
            cast(A["E"], [])
        "#,
            &[],
        )?;
        Ok(())
    }

    #[test]
    fn named_types_classes() -> Result<()> {
        flakes(
            r#"
            from typing import TypedDict, NamedTuple
            class X(TypedDict):
                y: TypedDict("z", {"zz":int})

            class Y(NamedTuple):
                y: NamedTuple("v", [("vv", int)])
        "#,
            &[],
        )?;
        Ok(())
    }
}
