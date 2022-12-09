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
    use crate::source_code_locator::SourceCodeLocator;
    use crate::{directives, rustpython_helpers, settings};

    #[test_case(CheckCode::F401, Path::new("F401_0.py"); "F401_0")]
    #[test_case(CheckCode::F401, Path::new("F401_1.py"); "F401_1")]
    #[test_case(CheckCode::F401, Path::new("F401_2.py"); "F401_2")]
    #[test_case(CheckCode::F401, Path::new("F401_3.py"); "F401_3")]
    #[test_case(CheckCode::F401, Path::new("F401_4.py"); "F401_4")]
    #[test_case(CheckCode::F401, Path::new("F401_5.py"); "F401_5")]
    #[test_case(CheckCode::F401, Path::new("F401_6.py"); "F401_6")]
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
    #[test_case(CheckCode::F822, Path::new("F822.py"); "F822")]
    #[test_case(CheckCode::F823, Path::new("F823.py"); "F823")]
    #[test_case(CheckCode::F831, Path::new("F831.py"); "F831")]
    #[test_case(CheckCode::F841, Path::new("F841_0.py"); "F841_0")]
    #[test_case(CheckCode::F841, Path::new("F841_1.py"); "F841_1")]
    #[test_case(CheckCode::F901, Path::new("F901.py"); "F901")]
    fn checks(check_code: CheckCode, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", check_code.as_ref(), path.to_string_lossy());
        let mut checks = test_path(
            Path::new("./resources/test/fixtures/pyflakes")
                .join(path)
                .as_path(),
            &settings::Settings::for_rule(check_code),
            true,
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
            true,
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
            true,
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
            true,
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    /// A re-implementation of the Pyflakes test runner.
    /// See: <https://github.com/PyCQA/pyflakes/blob/04ecb0c324ef3b61124e2f80f9e1af6c3a4c7b26/pyflakes/test/test_undefined_names.py>
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
            &contents,
            tokens,
            &locator,
            &directives,
            &settings,
            true,
            false,
        )?;
        checks.sort_by_key(|check| check.location);
        let actual = checks
            .iter()
            .map(|check| check.kind.code().clone())
            .collect::<Vec<_>>();
        assert_eq!(actual, expected);
        Ok(())
    }

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
            raise ValueError('ve")
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

    // TODO(charlie): Bubble global assignments up to the module scope.
    #[ignore]
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
        Ok(())
    }

    // TODO(charlie): Bubble global assignments up to the module scope.
    #[ignore]
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
}
