use super::builder::TypeInferenceBuilder;
use crate::db::tests::{TestDb, setup_db};
use crate::place::symbol;
use crate::place::{ConsideredDefinitions, Place, global_symbol};
use crate::semantic_index::definition::Definition;
use crate::semantic_index::scope::FileScopeId;
use crate::semantic_index::{global_scope, place_table, semantic_index, use_def_map};
use crate::types::{KnownClass, KnownInstanceType, check_types};
use ruff_db::diagnostic::Diagnostic;
use ruff_db::files::{File, system_path_to_file};
use ruff_db::system::DbWithWritableSystem as _;
use ruff_db::testing::{assert_function_query_was_not_run, assert_function_query_was_run};

use super::*;

#[track_caller]
fn get_symbol<'db>(
    db: &'db TestDb,
    file_name: &str,
    scopes: &[&str],
    symbol_name: &str,
) -> Place<'db> {
    let file = system_path_to_file(db, file_name).expect("file to exist");
    let module = parsed_module(db, file).load(db);
    let index = semantic_index(db, file);
    let mut file_scope_id = FileScopeId::global();
    let mut scope = file_scope_id.to_scope_id(db, file);
    for expected_scope_name in scopes {
        file_scope_id = index
            .child_scopes(file_scope_id)
            .next()
            .unwrap_or_else(|| panic!("scope of {expected_scope_name}"))
            .0;
        scope = file_scope_id.to_scope_id(db, file);
        assert_eq!(scope.name(db, &module), *expected_scope_name);
    }

    symbol(db, scope, symbol_name, ConsideredDefinitions::EndOfScope).place
}

#[track_caller]
fn assert_diagnostic_messages(diagnostics: &[Diagnostic], expected: &[&str]) {
    let messages: Vec<&str> = diagnostics
        .iter()
        .map(Diagnostic::primary_message)
        .collect();
    assert_eq!(&messages, expected);
}

#[track_caller]
fn assert_file_diagnostics(db: &TestDb, filename: &str, expected: &[&str]) {
    let file = system_path_to_file(db, filename).unwrap();
    let diagnostics = check_types(db, file);

    assert_diagnostic_messages(&diagnostics, expected);
}

#[test]
fn not_literal_string() -> anyhow::Result<()> {
    let mut db = setup_db();
    let content = format!(
        r#"
            from typing_extensions import Literal, assert_type

            assert_type(not "{y}", bool)
            assert_type(not 10*"{y}", bool)
            assert_type(not "{y}"*10, bool)
            assert_type(not 0*"{y}", Literal[True])
            assert_type(not (-100)*"{y}", Literal[True])
            "#,
        y = "a".repeat(TypeInferenceBuilder::MAX_STRING_LITERAL_SIZE + 1),
    );
    db.write_dedented("src/a.py", &content)?;

    assert_file_diagnostics(&db, "src/a.py", &[]);

    Ok(())
}

#[test]
fn multiplied_string() -> anyhow::Result<()> {
    let mut db = setup_db();
    let content = format!(
        r#"
            from typing_extensions import Literal, LiteralString, assert_type

            assert_type(2 * "hello", Literal["hellohello"])
            assert_type("goodbye" * 3, Literal["goodbyegoodbyegoodbye"])
            assert_type("a" * {y}, Literal["{a_repeated}"])
            assert_type({z} * "b", LiteralString)
            assert_type(0 * "hello", Literal[""])
            assert_type(-3 * "hello", Literal[""])
            "#,
        y = TypeInferenceBuilder::MAX_STRING_LITERAL_SIZE,
        z = TypeInferenceBuilder::MAX_STRING_LITERAL_SIZE + 1,
        a_repeated = "a".repeat(TypeInferenceBuilder::MAX_STRING_LITERAL_SIZE),
    );
    db.write_dedented("src/a.py", &content)?;

    assert_file_diagnostics(&db, "src/a.py", &[]);

    Ok(())
}

#[test]
fn multiplied_literal_string() -> anyhow::Result<()> {
    let mut db = setup_db();
    let content = format!(
        r#"
            from typing_extensions import Literal, LiteralString, assert_type

            assert_type("{y}", LiteralString)
            assert_type(10*"{y}", LiteralString)
            assert_type("{y}"*10, LiteralString)
            assert_type(0*"{y}", Literal[""])
            assert_type((-100)*"{y}", Literal[""])
            "#,
        y = "a".repeat(TypeInferenceBuilder::MAX_STRING_LITERAL_SIZE + 1),
    );
    db.write_dedented("src/a.py", &content)?;

    assert_file_diagnostics(&db, "src/a.py", &[]);

    Ok(())
}

#[test]
fn truncated_string_literals_become_literal_string() -> anyhow::Result<()> {
    let mut db = setup_db();
    let content = format!(
        r#"
            from typing_extensions import LiteralString, assert_type

            assert_type("{y}", LiteralString)
            assert_type("a" + "{z}", LiteralString)
            "#,
        y = "a".repeat(TypeInferenceBuilder::MAX_STRING_LITERAL_SIZE + 1),
        z = "a".repeat(TypeInferenceBuilder::MAX_STRING_LITERAL_SIZE),
    );
    db.write_dedented("src/a.py", &content)?;

    assert_file_diagnostics(&db, "src/a.py", &[]);

    Ok(())
}

#[test]
fn adding_string_literals_and_literal_string() -> anyhow::Result<()> {
    let mut db = setup_db();
    let content = format!(
        r#"
            from typing_extensions import LiteralString, assert_type

            assert_type("{y}", LiteralString)
            assert_type("{y}" + "a", LiteralString)
            assert_type("a" + "{y}", LiteralString)
            assert_type("{y}" + "{y}", LiteralString)
            "#,
        y = "a".repeat(TypeInferenceBuilder::MAX_STRING_LITERAL_SIZE + 1),
    );
    db.write_dedented("src/a.py", &content)?;

    assert_file_diagnostics(&db, "src/a.py", &[]);

    Ok(())
}

#[test]
fn pep695_type_params() {
    let mut db = setup_db();

    db.write_dedented(
        "src/a.py",
        "
            def f[T, U: A, V: (A, B), W = A, X: A = A1, Y: (int,)]():
                pass

            class A: ...
            class B: ...
            class A1(A): ...
            ",
    )
    .unwrap();

    let check_typevar = |var: &'static str,
                         display: &'static str,
                         upper_bound: Option<&'static str>,
                         constraints: Option<&[&'static str]>,
                         default: Option<&'static str>| {
        let var_ty = get_symbol(&db, "src/a.py", &["f"], var).expect_type();
        assert_eq!(var_ty.display(&db).to_string(), display);

        let expected_name_ty = format!(r#"Literal["{var}"]"#);
        let name_ty = var_ty.member(&db, "__name__").place.expect_type();
        assert_eq!(name_ty.display(&db).to_string(), expected_name_ty);

        let Type::KnownInstance(KnownInstanceType::TypeVar(typevar)) = var_ty else {
            panic!("expected TypeVar");
        };

        assert_eq!(
            typevar
                .upper_bound(&db)
                .map(|ty| ty.display(&db).to_string()),
            upper_bound.map(std::borrow::ToOwned::to_owned)
        );
        assert_eq!(
            typevar.constraints(&db).map(|tys| tys
                .iter()
                .map(|ty| ty.display(&db).to_string())
                .collect::<Vec<_>>()),
            constraints.map(|strings| strings
                .iter()
                .map(std::string::ToString::to_string)
                .collect::<Vec<_>>())
        );
        assert_eq!(
            typevar
                .default_type(&db)
                .map(|ty| ty.display(&db).to_string()),
            default.map(std::borrow::ToOwned::to_owned)
        );
    };

    check_typevar("T", "TypeVar", None, None, None);
    check_typevar("U", "TypeVar", Some("A"), None, None);
    check_typevar("V", "TypeVar", None, Some(&["A", "B"]), None);
    check_typevar("W", "TypeVar", None, None, Some("A"));
    check_typevar("X", "TypeVar", Some("A"), None, Some("A1"));

    // a typevar with less than two constraints is treated as unconstrained
    check_typevar("Y", "TypeVar", None, None, None);
}

/// Test that a symbol known to be unbound in a scope does not still trigger cycle-causing
/// reachability-constraint checks in that scope.
#[test]
fn unbound_symbol_no_reachability_constraint_check() {
    let mut db = setup_db();

    // First, type-check a random other file so that we cache a result for the `module_type_symbols`
    // query (which often encounters cycles due to `types.pyi` importing `typing_extensions` and
    // `typing_extensions.pyi` importing `types`). Clear the events afterwards so that unrelated
    // cycles from that query don't interfere with our test.
    db.write_dedented("src/wherever.py", "print(x)").unwrap();
    assert_file_diagnostics(&db, "src/wherever.py", &["Name `x` used when not defined"]);
    db.clear_salsa_events();

    // If the bug we are testing for is not fixed, what happens is that when inferring the
    // `flag: bool = True` definitions, we look up `bool` as a deferred name (thus from end of
    // scope), and because of the early return its "unbound" binding has a reachability
    // constraint of `~flag`, which we evaluate, meaning we have to evaluate the definition of
    // `flag` -- and we are in a cycle. With the fix, we short-circuit evaluating reachability
    // constraints on "unbound" if a symbol is otherwise not bound.
    db.write_dedented(
        "src/a.py",
        "
            from __future__ import annotations

            def f():
                flag: bool = True
                if flag:
                    return True
            ",
    )
    .unwrap();

    db.clear_salsa_events();
    assert_file_diagnostics(&db, "src/a.py", &[]);
    let events = db.take_salsa_events();
    let cycles = salsa::attach(&db, || {
        events
            .iter()
            .filter_map(|event| {
                if let salsa::EventKind::WillIterateCycle { database_key, .. } = event.kind {
                    Some(format!("{database_key:?}"))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
    });
    let expected: Vec<String> = vec![];
    assert_eq!(cycles, expected);
}

// Incremental inference tests
#[track_caller]
fn first_public_binding<'db>(db: &'db TestDb, file: File, name: &str) -> Definition<'db> {
    let scope = global_scope(db, file);
    use_def_map(db, scope)
        .end_of_scope_symbol_bindings(place_table(db, scope).symbol_id(name).unwrap())
        .find_map(|b| b.binding.definition())
        .expect("no binding found")
}

#[test]
fn dependency_public_symbol_type_change() -> anyhow::Result<()> {
    let mut db = setup_db();

    db.write_files([
        ("/src/a.py", "from foo import x"),
        ("/src/foo.py", "x: int = 10\ndef foo(): ..."),
    ])?;

    let a = system_path_to_file(&db, "/src/a.py").unwrap();
    let x_ty = global_symbol(&db, a, "x").place.expect_type();

    assert_eq!(x_ty.display(&db).to_string(), "int");

    // Change `x` to a different value
    db.write_file("/src/foo.py", "x: bool = True\ndef foo(): ...")?;

    let a = system_path_to_file(&db, "/src/a.py").unwrap();

    let x_ty_2 = global_symbol(&db, a, "x").place.expect_type();

    assert_eq!(x_ty_2.display(&db).to_string(), "bool");

    Ok(())
}

#[test]
fn dependency_internal_symbol_change() -> anyhow::Result<()> {
    let mut db = setup_db();

    db.write_files([
        ("/src/a.py", "from foo import x"),
        ("/src/foo.py", "x: int = 10\ndef foo(): y = 1"),
    ])?;

    let a = system_path_to_file(&db, "/src/a.py").unwrap();
    let x_ty = global_symbol(&db, a, "x").place.expect_type();

    assert_eq!(x_ty.display(&db).to_string(), "int");

    db.write_file("/src/foo.py", "x: int = 10\ndef foo(): pass")?;

    let a = system_path_to_file(&db, "/src/a.py").unwrap();

    db.clear_salsa_events();

    let x_ty_2 = global_symbol(&db, a, "x").place.expect_type();

    assert_eq!(x_ty_2.display(&db).to_string(), "int");

    let events = db.take_salsa_events();

    assert_function_query_was_not_run(
        &db,
        infer_definition_types,
        first_public_binding(&db, a, "x"),
        &events,
    );

    Ok(())
}

#[test]
fn dependency_unrelated_symbol() -> anyhow::Result<()> {
    let mut db = setup_db();

    db.write_files([
        ("/src/a.py", "from foo import x"),
        ("/src/foo.py", "x: int = 10\ny: bool = True"),
    ])?;

    let a = system_path_to_file(&db, "/src/a.py").unwrap();
    let x_ty = global_symbol(&db, a, "x").place.expect_type();

    assert_eq!(x_ty.display(&db).to_string(), "int");

    db.write_file("/src/foo.py", "x: int = 10\ny: bool = False")?;

    let a = system_path_to_file(&db, "/src/a.py").unwrap();

    db.clear_salsa_events();

    let x_ty_2 = global_symbol(&db, a, "x").place.expect_type();

    assert_eq!(x_ty_2.display(&db).to_string(), "int");

    let events = db.take_salsa_events();

    assert_function_query_was_not_run(
        &db,
        infer_definition_types,
        first_public_binding(&db, a, "x"),
        &events,
    );
    Ok(())
}

#[test]
fn dependency_implicit_instance_attribute() -> anyhow::Result<()> {
    fn x_rhs_expression(db: &TestDb) -> Expression<'_> {
        let file_main = system_path_to_file(db, "/src/main.py").unwrap();
        let ast = parsed_module(db, file_main).load(db);
        // Get the second statement in `main.py` (x = …) and extract the expression
        // node on the right-hand side:
        let x_rhs_node = &ast.syntax().body[1].as_assign_stmt().unwrap().value;

        let index = semantic_index(db, file_main);
        index.expression(x_rhs_node.as_ref())
    }

    let mut db = setup_db();

    db.write_dedented(
        "/src/mod.py",
        r#"
        class C:
            def f(self):
                self.attr: int | None = None
        "#,
    )?;
    db.write_dedented(
        "/src/main.py",
        r#"
        from mod import C
        # multiple targets ensures RHS is a standalone expression, relied on by this test
        x = y = C().attr
        "#,
    )?;

    let file_main = system_path_to_file(&db, "/src/main.py").unwrap();
    let attr_ty = global_symbol(&db, file_main, "x").place.expect_type();
    assert_eq!(attr_ty.display(&db).to_string(), "int | None");

    // Change the type of `attr` to `str | None`; this should trigger the type of `x` to be re-inferred
    db.write_dedented(
        "/src/mod.py",
        r#"
        class C:
            def f(self):
                self.attr: str | None = None
        "#,
    )?;

    let events = {
        db.clear_salsa_events();
        let attr_ty = global_symbol(&db, file_main, "x").place.expect_type();
        assert_eq!(attr_ty.display(&db).to_string(), "str | None");
        db.take_salsa_events()
    };
    assert_function_query_was_run(
        &db,
        infer_expression_types_impl,
        InferExpression::Bare(x_rhs_expression(&db)),
        &events,
    );

    // Add a comment; this should not trigger the type of `x` to be re-inferred
    db.write_dedented(
        "/src/mod.py",
        r#"
        class C:
            def f(self):
                # a comment!
                self.attr: str | None = None
        "#,
    )?;

    let events = {
        db.clear_salsa_events();
        let attr_ty = global_symbol(&db, file_main, "x").place.expect_type();
        assert_eq!(attr_ty.display(&db).to_string(), "str | None");
        db.take_salsa_events()
    };

    assert_function_query_was_not_run(
        &db,
        infer_expression_types_impl,
        InferExpression::Bare(x_rhs_expression(&db)),
        &events,
    );

    Ok(())
}

/// This test verifies that changing a class's declaration in a non-meaningful way (e.g. by adding a comment)
/// doesn't trigger type inference for expressions that depend on the class's members.
#[test]
fn dependency_own_instance_member() -> anyhow::Result<()> {
    fn x_rhs_expression(db: &TestDb) -> Expression<'_> {
        let file_main = system_path_to_file(db, "/src/main.py").unwrap();
        let ast = parsed_module(db, file_main).load(db);
        // Get the second statement in `main.py` (x = …) and extract the expression
        // node on the right-hand side:
        let x_rhs_node = &ast.syntax().body[1].as_assign_stmt().unwrap().value;

        let index = semantic_index(db, file_main);
        index.expression(x_rhs_node.as_ref())
    }

    let mut db = setup_db();

    db.write_dedented(
        "/src/mod.py",
        r#"
        class C:
            if random.choice([True, False]):
                attr: int = 42
            else:
                attr: None = None
        "#,
    )?;
    db.write_dedented(
        "/src/main.py",
        r#"
        from mod import C
        # multiple targets ensures RHS is a standalone expression, relied on by this test
        x = y = C().attr
        "#,
    )?;

    let file_main = system_path_to_file(&db, "/src/main.py").unwrap();
    let attr_ty = global_symbol(&db, file_main, "x").place.expect_type();
    assert_eq!(attr_ty.display(&db).to_string(), "int | None");

    // Change the type of `attr` to `str | None`; this should trigger the type of `x` to be re-inferred
    db.write_dedented(
        "/src/mod.py",
        r#"
        class C:
            if random.choice([True, False]):
                attr: str = "42"
            else:
                attr: None = None
        "#,
    )?;

    let events = {
        db.clear_salsa_events();
        let attr_ty = global_symbol(&db, file_main, "x").place.expect_type();
        assert_eq!(attr_ty.display(&db).to_string(), "str | None");
        db.take_salsa_events()
    };
    assert_function_query_was_run(
        &db,
        infer_expression_types_impl,
        InferExpression::Bare(x_rhs_expression(&db)),
        &events,
    );

    // Add a comment; this should not trigger the type of `x` to be re-inferred
    db.write_dedented(
        "/src/mod.py",
        r#"
        class C:
            # comment
            if random.choice([True, False]):
                attr: str = "42"
            else:
                attr: None = None
        "#,
    )?;

    let events = {
        db.clear_salsa_events();
        let attr_ty = global_symbol(&db, file_main, "x").place.expect_type();
        assert_eq!(attr_ty.display(&db).to_string(), "str | None");
        db.take_salsa_events()
    };

    assert_function_query_was_not_run(
        &db,
        infer_expression_types_impl,
        InferExpression::Bare(x_rhs_expression(&db)),
        &events,
    );

    Ok(())
}

#[test]
fn dependency_implicit_class_member() -> anyhow::Result<()> {
    fn x_rhs_expression(db: &TestDb) -> Expression<'_> {
        let file_main = system_path_to_file(db, "/src/main.py").unwrap();
        let ast = parsed_module(db, file_main).load(db);
        // Get the third statement in `main.py` (x = …) and extract the expression
        // node on the right-hand side:
        let x_rhs_node = &ast.syntax().body[2].as_assign_stmt().unwrap().value;

        let index = semantic_index(db, file_main);
        index.expression(x_rhs_node.as_ref())
    }

    let mut db = setup_db();

    db.write_dedented(
        "/src/mod.py",
        r#"
        class C:
            def __init__(self):
                self.instance_attr: str = "24"

            @classmethod
            def method(cls):
                cls.class_attr: int = 42
        "#,
    )?;
    db.write_dedented(
        "/src/main.py",
        r#"
        from mod import C
        C.method()
        # multiple targets ensures RHS is a standalone expression, relied on by this test
        x = y = C().class_attr
        "#,
    )?;

    let file_main = system_path_to_file(&db, "/src/main.py").unwrap();
    let attr_ty = global_symbol(&db, file_main, "x").place.expect_type();
    assert_eq!(attr_ty.display(&db).to_string(), "int");

    // Change the type of `class_attr` to `str`; this should trigger the type of `x` to be re-inferred
    db.write_dedented(
        "/src/mod.py",
        r#"
        class C:
            def __init__(self):
                self.instance_attr: str = "24"

            @classmethod
            def method(cls):
                cls.class_attr: str = "42"
        "#,
    )?;

    let events = {
        db.clear_salsa_events();
        let attr_ty = global_symbol(&db, file_main, "x").place.expect_type();
        assert_eq!(attr_ty.display(&db).to_string(), "str");
        db.take_salsa_events()
    };
    assert_function_query_was_run(
        &db,
        infer_expression_types_impl,
        InferExpression::Bare(x_rhs_expression(&db)),
        &events,
    );

    // Add a comment; this should not trigger the type of `x` to be re-inferred
    db.write_dedented(
        "/src/mod.py",
        r#"
        class C:
            def __init__(self):
                self.instance_attr: str = "24"

            @classmethod
            def method(cls):
                # comment
                cls.class_attr: str = "42"
        "#,
    )?;

    let events = {
        db.clear_salsa_events();
        let attr_ty = global_symbol(&db, file_main, "x").place.expect_type();
        assert_eq!(attr_ty.display(&db).to_string(), "str");
        db.take_salsa_events()
    };

    assert_function_query_was_not_run(
        &db,
        infer_expression_types_impl,
        InferExpression::Bare(x_rhs_expression(&db)),
        &events,
    );

    Ok(())
}

/// Inferring the result of a call-expression shouldn't need to re-run after
/// a trivial change to the function's file (e.g. by adding a docstring to the function).
#[test]
fn call_type_doesnt_rerun_when_only_callee_changed() -> anyhow::Result<()> {
    let mut db = setup_db();

    db.write_dedented(
        "src/foo.py",
        r#"
        def foo() -> int:
            return 5
    "#,
    )?;
    db.write_dedented(
        "src/bar.py",
        r#"
        from foo import foo

        # multiple targets ensures RHS is a standalone expression, relied on by this test
        a = b = foo()
        "#,
    )?;

    let bar = system_path_to_file(&db, "src/bar.py")?;
    let a = global_symbol(&db, bar, "a").place;

    assert_eq!(a.expect_type(), KnownClass::Int.to_instance(&db));
    let events = db.take_salsa_events();

    let module = parsed_module(&db, bar).load(&db);
    let call = &*module.syntax().body[1].as_assign_stmt().unwrap().value;
    let foo_call = semantic_index(&db, bar).expression(call);

    assert_function_query_was_run(
        &db,
        infer_expression_types_impl,
        InferExpression::Bare(foo_call),
        &events,
    );

    // Add a docstring to foo to trigger a re-run.
    // The bar-call site of foo should not be re-run because of that
    db.write_dedented(
        "src/foo.py",
        r#"
        def foo() -> int:
            "Computes a value"
            return 5
        "#,
    )?;
    db.clear_salsa_events();

    let a = global_symbol(&db, bar, "a").place;

    assert_eq!(a.expect_type(), KnownClass::Int.to_instance(&db));
    let events = db.take_salsa_events();

    let module = parsed_module(&db, bar).load(&db);
    let call = &*module.syntax().body[1].as_assign_stmt().unwrap().value;
    let foo_call = semantic_index(&db, bar).expression(call);

    assert_function_query_was_not_run(
        &db,
        infer_expression_types_impl,
        InferExpression::Bare(foo_call),
        &events,
    );

    Ok(())
}
