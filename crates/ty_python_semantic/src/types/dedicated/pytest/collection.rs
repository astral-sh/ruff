//! Models the functions that pytest collects as tests.

use ruff_db::parsed::parsed_module;
use ruff_text_size::Ranged;
use ty_module_resolver::file_to_module;
use ty_python_core::definition::{Definition, DefinitionKind};
use ty_python_core::scope::ScopeKind;
use ty_python_core::{ProgramFile, semantic_index};

use crate::Db;
use crate::place::Place;
use crate::types::infer::original_class_type;
use crate::types::{
    ClassBase, ClassLiteral, KnownClass, MemberLookupPolicy, ProgramEnvironment,
    StaticClassLiteral, Type, definition_expression_type,
};

/// Returns the pytest test represented by `function` under the default collection conventions.
///
/// This recognizes test functions, pytest-style test methods, and `unittest.TestCase` methods. It
/// returns `None` for functions that pytest would not collect, including fixture declarations and
/// methods on pytest-style classes with custom constructors.
#[salsa::tracked(returns(copy))]
pub(crate) fn pytest_test_for_function<'db>(
    db: &'db dyn Db,
    function: Definition<'db>,
) -> Option<PytestTest<'db>> {
    if !is_default_pytest_test_file(db, function.program_file(db)) {
        return None;
    }

    let DefinitionKind::Function(function_ref) = function.kind(db) else {
        return None;
    };

    let module = parsed_module(db, function.python_file(db)).load(db);
    if !function_ref.node(&module).name.as_str().starts_with("test") {
        return None;
    }

    let parent = collection_parent(db, function)?;
    if super::fixture_declaration(db, function).is_some() {
        return None;
    }

    let (kind, enclosing_class) = match parent {
        CollectionParent::Module => (PytestTestKind::Pytest, None),
        CollectionParent::Class(class) => (pytest_test_class_kind(db, class)?, Some(class)),
    };

    Some(PytestTest {
        function,
        kind,
        enclosing_class,
    })
}

/// A function that pytest collects as a test.
#[derive(Debug, Clone, Copy, Eq, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub struct PytestTest<'db> {
    function: Definition<'db>,
    kind: PytestTestKind,
    enclosing_class: Option<Definition<'db>>,
}

impl<'db> PytestTest<'db> {
    /// Returns the collected function definition.
    pub fn function(self) -> Definition<'db> {
        self.function
    }

    /// Returns the collection mechanism responsible for this test.
    pub fn kind(self) -> PytestTestKind {
        self.kind
    }

    /// Returns the class that directly contains this test method, if any.
    pub fn enclosing_class(self) -> Option<Definition<'db>> {
        self.enclosing_class
    }
}

/// The collection mechanism responsible for a test.
#[derive(Debug, Clone, Copy, Eq, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub enum PytestTestKind {
    /// A function or method collected according to pytest's naming and class conventions.
    Pytest,
    /// A method collected because its class inherits from `unittest.TestCase`.
    Unittest,
}

/// Returns the tests that pytest collects from `file` under the default collection conventions.
#[salsa::tracked(returns(deref), heap_size=ruff_memory_usage::heap_size)]
pub fn pytest_tests_in_file<'db>(
    db: &'db dyn Db,
    file: ProgramFile<'db>,
) -> Box<[PytestTest<'db>]> {
    if !is_default_pytest_test_file(db, file) {
        return Box::default();
    }

    let index = semantic_index(db, file);
    let module = parsed_module(db, file.python_file(db)).load(db);
    let mut tests = index
        .scope_ids()
        .filter_map(|scope| {
            let function_ref = scope.node(db).as_function()?;
            pytest_test_for_function(db, index.expect_single_definition(function_ref))
        })
        .collect::<Vec<_>>();
    tests.sort_unstable_by_key(|test| test.function.focus_range(db, &module).start());
    tests.into_boxed_slice()
}

#[derive(Debug, Clone, Copy)]
enum CollectionParent<'db> {
    Module,
    Class(Definition<'db>),
}

fn collection_parent<'db>(
    db: &'db dyn Db,
    definition: Definition<'db>,
) -> Option<CollectionParent<'db>> {
    let file = definition.program_file(db);
    let index = semantic_index(db, file);
    let mut scope = definition.scope(db).file_scope_id(db);
    if index.scope(scope).kind() == ScopeKind::TypeParams {
        scope = index.parent_scope_id(scope)?;
    }

    match index.scope(scope).kind() {
        ScopeKind::Module => Some(CollectionParent::Module),
        ScopeKind::Class => {
            let class_ref = index.scope(scope).node().as_class()?;
            Some(CollectionParent::Class(
                index.expect_single_definition(class_ref),
            ))
        }
        _ => None,
    }
}

#[salsa::tracked(returns(copy))]
fn pytest_test_class_kind<'db>(
    db: &'db dyn Db,
    definition: Definition<'db>,
) -> Option<PytestTestKind> {
    let DefinitionKind::Class(class_ref) = definition.kind(db) else {
        return None;
    };

    match collection_parent(db, definition)? {
        CollectionParent::Module => {}
        CollectionParent::Class(parent) => {
            if pytest_test_class_kind(db, parent) != Some(PytestTestKind::Pytest) {
                return None;
            }
        }
    }

    let class = original_class_type(db, definition)?;
    if is_unittest_test_case(db, class) {
        return Some(PytestTestKind::Unittest);
    }

    let module = parsed_module(db, definition.python_file(db)).load(db);
    if !class_ref.node(&module).name.as_str().starts_with("Test") {
        return None;
    }

    has_default_pytest_constructors(db, class).then_some(PytestTestKind::Pytest)
}

/// Returns whether `class` has the default `object` constructors that pytest requires.
fn has_default_pytest_constructors(db: &dyn Db, class: ClassLiteral<'_>) -> bool {
    let Some(class) = class.as_static() else {
        return false;
    };
    let env = ProgramEnvironment::from_file(class.program_file(db));
    let Some(object) = KnownClass::Object.try_to_class_literal(db, &env) else {
        return false;
    };

    ["__init__", "__new__"].into_iter().all(|name| {
        let actual = ClassLiteral::Static(class)
            .class_member(db, &env, name, MemberLookupPolicy::default())
            .place;
        let expected = ClassLiteral::Static(object)
            .class_member(db, &env, name, MemberLookupPolicy::default())
            .place;
        let (Place::Defined(actual), Place::Defined(expected)) = (actual, expected) else {
            return false;
        };

        // Pytest compares each constructor with `object`'s by identity. Their inferred types can
        // differ after assignment, so compare the defining place and then recognize an explicit
        // assignment that restores the `object` constructor.
        if actual.provenance == expected.provenance {
            true
        } else if let Some(definition) = actual.provenance.definition() {
            restores_object_constructor(db, definition, object, name)
        } else {
            false
        }
    })
}

fn restores_object_constructor(
    db: &dyn Db,
    definition: Definition<'_>,
    object: StaticClassLiteral<'_>,
    name: &str,
) -> bool {
    let module = parsed_module(db, definition.python_file(db)).load(db);
    let value = match definition.kind(db) {
        DefinitionKind::Assignment(assignment) => assignment.value(&module),
        DefinitionKind::AnnotatedAssignment(assignment) => {
            let Some(value) = assignment.value(&module) else {
                return false;
            };
            value
        }
        _ => return false,
    };
    let Some(attribute) = value.as_attribute_expr() else {
        return false;
    };

    attribute.attr.as_str() == name
        && definition_expression_type(db, definition, &attribute.value)
            == Type::ClassLiteral(ClassLiteral::Static(object))
}

/// Returns whether the class inherits from the canonical `unittest.TestCase`.
fn is_unittest_test_case(db: &dyn Db, class: ClassLiteral<'_>) -> bool {
    class.iter_mro(db).any(|ancestor| {
        let ClassBase::Class(ancestor) = ancestor else {
            return false;
        };
        let Some((ancestor, _)) = ancestor.static_class_literal(db) else {
            return false;
        };
        ancestor.name(db) == "TestCase"
            && file_to_module(db, ancestor.program_file(db).resolver_file(db))
                .is_some_and(|module| module.name(db).as_str() == "unittest.case")
    })
}

#[salsa::tracked(returns(copy))]
fn is_default_pytest_test_file(db: &dyn Db, file: ProgramFile<'_>) -> bool {
    let Some(file_name) = file
        .file(db)
        .path(db)
        .as_system_path()
        .and_then(|path| path.file_name())
    else {
        return false;
    };
    let Some(stem) = file_name.strip_suffix(".py") else {
        return false;
    };
    stem.starts_with("test_") || stem.ends_with("_test")
}

#[cfg(test)]
mod tests {
    use ruff_db::files::system_path_to_file;
    use ruff_db::parsed::parsed_module;
    use ruff_python_ast as ast;
    use ty_python_core::ProgramFile;
    use ty_python_core::definition::{Definition, DefinitionKind};
    use ty_python_core::semantic_index;

    use super::{PytestTestKind, pytest_test_for_function, pytest_tests_in_file};
    use crate::Db;
    use crate::db::tests::{TestDb, TestDbBuilder};

    #[test]
    fn collects_pytest_and_unittest_functions() {
        let test = CollectionTest::new(
            "/src/test_example.py",
            r#"
import unittest

import pytest

def test_module(): ...
def helper(): ...

class TestClass:
    def test_method(self): ...

    class TestNested:
        def test_nested(self): ...

class Example:
    def test_not_collected(self): ...

class UnitCase(unittest.TestCase):
    def test_unit(self): ...

def outer():
    def test_local(): ...

@pytest.fixture
def test_fixture(): ...
"#,
        );

        assert_eq!(
            test.collected_tests(),
            vec![
                ("test_method".to_owned(), PytestTestKind::Pytest),
                ("test_module".to_owned(), PytestTestKind::Pytest),
                ("test_nested".to_owned(), PytestTestKind::Pytest),
                ("test_unit".to_owned(), PytestTestKind::Unittest),
            ]
        );
        assert_eq!(
            pytest_test_for_function(&test.db, test.function("helper")),
            None
        );
        assert_eq!(
            pytest_test_for_function(&test.db, test.function("Example.test_not_collected")),
            None
        );
        assert_eq!(
            pytest_test_for_function(&test.db, test.function("test_fixture")),
            None
        );
        assert!(
            pytest_test_for_function(&test.db, test.function("test_module"))
                .expect("module test should be collected")
                .enclosing_class()
                .is_none()
        );
        assert!(
            pytest_test_for_function(&test.db, test.function("TestClass.test_method"))
                .expect("method should be collected")
                .enclosing_class()
                .is_some()
        );
    }

    #[test]
    fn requires_a_default_test_module_name() {
        let test = CollectionTest::new(
            "/src/example.py",
            r#"
def test_example(): ...
"#,
        );

        assert!(pytest_tests_in_file(&test.db, test.program_file()).is_empty());
        assert_eq!(
            pytest_test_for_function(&test.db, test.function("test_example")),
            None
        );
    }

    #[test]
    fn rejects_custom_constructors_and_accepts_restored_defaults() {
        let test = CollectionTest::new(
            "/src/test_example.py",
            r#"
class InitBase:
    def __init__(self): ...

class NewBase:
    def __new__(cls): ...

class TestOwnInit:
    def __init__(self): ...
    def test_own_init(self): ...

class TestInheritedInit(InitBase):
    def test_inherited_init(self): ...

class TestOwnNew:
    def __new__(cls): ...
    def test_own_new(self): ...

class TestInheritedNew(NewBase):
    def test_inherited_new(self): ...

class TestRestoredInit(InitBase):
    __init__ = object.__init__
    def test_restored_init(self): ...

class TestRestoredNew(NewBase):
    __new__ = object.__new__
    def test_restored_new(self): ...
"#,
        );

        assert_eq!(
            test.collected_tests(),
            vec![
                ("test_restored_init".to_owned(), PytestTestKind::Pytest),
                ("test_restored_new".to_owned(), PytestTestKind::Pytest),
            ]
        );
    }

    struct CollectionTest {
        db: TestDb,
        path: &'static str,
    }

    impl CollectionTest {
        fn new(path: &'static str, source: &'static str) -> Self {
            Self {
                db: pytest_db(path, source),
                path,
            }
        }

        fn program_file(&self) -> ProgramFile<'_> {
            let file = system_path_to_file(&self.db, self.path).expect("test file should exist");
            self.db.program_file(file)
        }

        fn function<'db>(&'db self, selector: &str) -> Definition<'db> {
            let file = self.program_file();
            let module = parsed_module(&self.db, file.python_file(&self.db)).load(&self.db);
            let function = find_function(module.suite(), selector).expect("test function exists");
            semantic_index(&self.db, file).expect_single_definition(function)
        }

        fn collected_tests(&self) -> Vec<(String, PytestTestKind)> {
            let mut tests = pytest_tests_in_file(&self.db, self.program_file())
                .iter()
                .filter_map(|test| {
                    let definition = test.function();
                    let module =
                        parsed_module(&self.db, definition.python_file(&self.db)).load(&self.db);
                    let DefinitionKind::Function(function) = definition.kind(&self.db) else {
                        return None;
                    };
                    Some((function.node(&module).name.to_string(), test.kind()))
                })
                .collect::<Vec<_>>();
            tests.sort_unstable_by(|left, right| left.0.cmp(&right.0));
            tests
        }
    }

    fn find_function<'ast>(
        statements: &'ast [ast::Stmt],
        selector: &str,
    ) -> Option<&'ast ast::StmtFunctionDef> {
        if let Some((class_name, nested)) = selector.split_once('.') {
            return statements.iter().find_map(|statement| {
                let class = statement.as_class_def_stmt()?;
                (class.name.as_str() == class_name)
                    .then(|| find_function(&class.body, nested))
                    .flatten()
            });
        }

        statements.iter().find_map(|statement| {
            statement
                .as_function_def_stmt()
                .filter(|function| function.name.as_str() == selector)
        })
    }

    fn pytest_db(path: &'static str, source: &'static str) -> TestDb {
        TestDbBuilder::new()
            .with_third_party_packages()
            .with_file(
                "/.venv/lib/python3.13/site-packages/_pytest/__init__.pyi",
                r#"
"#,
            )
            .with_file(
                "/.venv/lib/python3.13/site-packages/_pytest/fixtures.pyi",
                r#"
from typing import Any, Callable

def fixture(function: Callable[..., Any] | None = ...) -> Any: ...
"#,
            )
            .with_file(
                "/.venv/lib/python3.13/site-packages/pytest/__init__.pyi",
                r#"
from _pytest.fixtures import fixture as fixture
"#,
            )
            .with_file(path, source)
            .build()
            .expect("valid pytest test database")
    }
}
