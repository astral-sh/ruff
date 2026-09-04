//! Models test collection for fixture resolution, parametrization, and editor test discovery.
//!
//! The model applies pytest's [default naming and class rules][test-discovery] to function
//! bindings in a file, including functions exposed through assignments and imports:
//!
//! - Files must be named `test_*.py` or `*_test.py`.
//! - Modules and classes with a falsy `__test__` flag are excluded, as are abstract test classes.
//! - Function and method names must start with `test`. Fixtures and known property descriptors are
//!   excluded even when their names match.
//! - Subclasses of the standard-library [`unittest.TestCase`][unittest-tests] are eligible regardless
//!   of their class names or constructors. Their methods are classified as
//!   [`PytestTestKind::StdlibUnittest`], allowing consumers to distinguish them from tests that
//!   support pytest fixture injection. A `runTest` method is collected when there are no callable
//!   methods whose names start with `test`, including inherited methods.
//! - Classes that do not inherit from `unittest.TestCase` must have names starting with `Test` and
//!   inherit both `__init__` and `__new__` from `object`.
//!
//! Test functions must be bound at module scope or in an eligible class. Test classes must be
//! defined at module scope or nested inside an eligible class that does not inherit from
//! `unittest.TestCase`. Definitions inside functions are excluded.
//!
//! For example, in `test_example.py`:
//!
//! ```py
//! import unittest
//!
//! def test_function(): ...  # recognized as a pytest test
//! def helper(): ...  # not recognized because the name does not start with test
//! test_alias = helper  # recognized as a pytest test
//!
//! class TestGroup:
//!     def test_method(self): ...  # recognized as a pytest test
//!
//! class Example(unittest.TestCase):
//!     def test_method(self): ...  # recognized as a unittest test
//!
//! class Group:
//!     def test_method(self): ...  # not recognized because the class name lacks the Test prefix
//!
//! class TestWithCustomInit:
//!     def __init__(self): ...
//!     def test_method(self): ...  # not recognized because the class defines __init__
//! ```
//!
//! There are two entry points:
//!
//! - [`pytest_test_for_binding`] classifies one binding definition, returning `None` when it does
//!   not satisfy the collection rules.
//! - [`pytest_tests_in_file`] applies the same classification to bindings that are still available
//!   at the end of their module or class scope, excluding bindings overwritten or deleted later in
//!   that scope. It returns the collected tests in source order.
//!
//! Each [`PytestTest`] records the binding that exposes the test, its underlying function,
//! collection kind, and directly enclosing class, if any. Consequently:
//!
//! - Inheriting a test method does not produce another result for the subclass.
//! - A parametrized test function produces one result, regardless of how many parameter
//!   combinations pytest would execute.
//! - Two aliases of the same function produce separate results under their bound names.
//!
//! [test-discovery]: https://docs.pytest.org/en/stable/explanation/goodpractices.html#conventions-for-python-test-discovery
//! [unittest-tests]: https://docs.pytest.org/en/stable/how-to/unittest.html

use ruff_db::parsed::parsed_module;
use ruff_text_size::Ranged;
use ty_python_core::definition::{Definition, DefinitionKind};
use ty_python_core::scope::ScopeKind;
use ty_python_core::{ProgramFile, global_scope, place_table, semantic_index, use_def_map};

use crate::Db;
use crate::place::definitions::DefinitionResolution;
use crate::place::{ConsideredDefinitions, Place, symbol};
use crate::types::function::FunctionType;
use crate::types::infer::{function_known_decorators, original_class_type};
use crate::types::{
    ClassBase, ClassLiteral, KnownClass, MemberLookupPolicy, ProgramEnvironment, Type,
    binding_type, definition_expression_type,
};

/// Returns the tests that pytest collects from `file` under the default collection conventions.
#[salsa::tracked(returns(deref), heap_size=ruff_memory_usage::heap_size)]
fn pytest_tests_in_file<'db>(db: &'db dyn Db, file: ProgramFile<'db>) -> Box<[PytestTest<'db>]> {
    if !is_default_pytest_test_file(db, file) {
        return Box::default();
    }

    let index = semantic_index(db, file);
    let module = parsed_module(db, file.python_file(db)).load(db);
    let mut tests = Vec::new();
    for scope in index.scope_ids() {
        let scope = scope.file_scope_id(db);
        if !matches!(
            index.scope(scope).kind(),
            ScopeKind::Module | ScopeKind::Class
        ) {
            continue;
        }
        for (symbol, bindings) in index.use_def_map(scope).all_end_of_scope_symbol_bindings() {
            let name = index.place_table(scope).symbol(symbol).name();
            if !name.starts_with("test") && name != "runTest" {
                continue;
            }
            let resolution = DefinitionResolution::from_bindings(db, bindings);
            tests.extend(
                resolution
                    .definitions()
                    .iter()
                    .filter_map(|binding| pytest_test_for_binding(db, *binding).cloned()),
            );
        }
    }

    tests.sort_unstable_by_key(|test| test.binding.focus_range(db, &module).start());

    tests.into_boxed_slice()
}

/// A function that pytest collects as a test.
#[derive(Debug, Clone, Eq, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub(crate) struct PytestTest<'db> {
    binding: Definition<'db>,
    function: Definition<'db>,
    kind: PytestTestKind,
    enclosing_class: Option<Definition<'db>>,
}

impl PytestTest<'_> {
    /// Returns the collection mechanism responsible for this test.
    pub(crate) fn kind(&self) -> PytestTestKind {
        self.kind
    }
}

/// The collection mechanism responsible for a test.
#[derive(Debug, Clone, Copy, Eq, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub(crate) enum PytestTestKind {
    /// A function or method collected according to pytest's naming and class conventions.
    Pytest,
    /// A method collected because its class inherits from `unittest.TestCase`.
    StdlibUnittest,
}

/// Returns the pytest test exposed by `binding` under the default collection conventions.
///
/// The binding can be a function declaration, assignment, or import. Its name, file, and enclosing
/// scope determine collection eligibility; the underlying function may be defined elsewhere.
/// Returns `None` for unavailable bindings, values whose function cannot be identified, fixtures,
/// and bindings that fail the naming or enclosing-class rules.
#[salsa::tracked(returns(as_ref))]
pub(crate) fn pytest_test_for_binding<'db>(
    db: &'db dyn Db,
    binding: Definition<'db>,
) -> Option<PytestTest<'db>> {
    if !is_default_pytest_test_file(db, binding.program_file(db)) {
        return None;
    }

    let symbol = binding.place(db).as_symbol()?;
    let table = place_table(db, binding.scope(db));
    let name = table.symbol(symbol).name();
    if !name.starts_with("test") && name != "runTest" {
        return None;
    }

    let scope = enclosing_scope(db, binding)?;
    let (kind, enclosing_class) = match scope {
        EnclosingScope::Module => (PytestTestKind::Pytest, None),
        EnclosingScope::Class(class) => (pytest_test_class_kind(db, class)?, Some(class)),
    };
    if name == "runTest"
        && (kind != PytestTestKind::StdlibUnittest
            || has_unittest_test_methods(db, original_class_type(db, enclosing_class?)?))
    {
        return None;
    }
    if !super::is_available_definition(db, binding) {
        return None;
    }

    let function = if matches!(binding.kind(db), DefinitionKind::Function(_))
        && is_excluded_test_function(db, binding)
    {
        return None;
    } else if matches!(binding.kind(db), DefinitionKind::Function(_)) {
        binding
    } else {
        let function = match binding_value_type(db, binding) {
            Type::FunctionLiteral(function) => function,
            Type::BoundMethod(method) => method.function(db),
            _ => return None,
        };
        test_function_definition(db, function)
    };

    if super::fixtures::fixture_declaration(db, function).is_some() {
        return None;
    }

    Some(PytestTest {
        binding,
        function,
        kind,
        enclosing_class,
    })
}

/// Returns whether a property decorator excludes a function from collection.
fn is_excluded_test_function<'db>(db: &'db dyn Db, definition: Definition<'db>) -> bool {
    let DefinitionKind::Function(function) = definition.kind(db) else {
        return false;
    };

    let module = parsed_module(db, definition.python_file(db)).load(db);
    let decorators = &function.node(&module).decorator_list;
    if decorators.is_empty() {
        return false;
    }

    let inference = function_known_decorators(db, definition);
    for decorator in decorators {
        match inference.expression_type(&decorator.expression) {
            Some(Type::ClassLiteral(class)) if class.is_known(db, KnownClass::Property) => {
                return true;
            }
            Some(Type::ClassLiteral(class))
                if class.is_known(db, KnownClass::Staticmethod)
                    || class.is_known(db, KnownClass::Classmethod) => {}
            // Unknown outer decorators can transform the value. Only inspect beneath the
            // method wrappers that pytest itself unwraps during collection.
            _ => return false,
        }
    }

    false
}

/// Returns the assigned target's type before assignment error recovery.
///
/// For `class Test: __test__ = False`, class-member lookup exposes `bool`, but reading the
/// assignment target preserves `Literal[False]`, allowing collection to recognize the opt-out.
///
/// An inherited `__test__` binding may belong to another file. Tracking keeps that file's AST and
/// semantic-index dependencies on the defining binding, so subclasses share the result and
/// unrelated edits to the defining file need not invalidate their collection queries.
#[salsa::tracked(returns(copy))]
fn binding_value_type<'db>(db: &'db dyn Db, binding: Definition<'db>) -> Type<'db> {
    let module = parsed_module(db, binding.python_file(db)).load(db);
    let target = match binding.kind(db) {
        DefinitionKind::Assignment(assignment) => Some(assignment.target(&module)),
        DefinitionKind::AnnotatedAssignment(assignment) if assignment.has_value() => {
            Some(assignment.target(&module))
        }
        _ => None,
    };
    target.map_or_else(
        || binding_type(db, binding),
        |target| definition_expression_type(db, binding, target),
    )
}

/// Uses surviving bindings' assigned values when lookup returns a declared or widened type.
fn place_value_type<'db>(db: &'db dyn Db, place: Place<'db>) -> Option<Type<'db>> {
    let Place::Defined(place) = place else {
        return None;
    };
    let Some(definition) = place.provenance.definition() else {
        return Some(place.ty);
    };
    let resolution = DefinitionResolution::from_bindings(
        db,
        use_def_map(db, definition.scope(db)).end_of_scope_bindings(definition.place(db)),
    );
    match resolution.definitions() {
        [] => None,
        [binding] => Some(binding_value_type(db, *binding)),
        _ => Some(place.ty),
    }
}

/// Returns the definition of a function that may be imported from another file.
///
/// This is tracked because `FunctionType::definition` reads the function's semantic index.
/// Without this boundary, unrelated edits to that file could make importing bindings' collection
/// queries rerun. Tracking lets Salsa stop that invalidation when the returned definition is unchanged.
#[salsa::tracked(returns(copy))]
fn test_function_definition<'db>(db: &'db dyn Db, function: FunctionType<'db>) -> Definition<'db> {
    function.definition(db)
}

#[derive(Debug, Clone, Copy)]
enum EnclosingScope<'db> {
    Module,
    Class(Definition<'db>),
}

fn enclosing_scope<'db>(
    db: &'db dyn Db,
    definition: Definition<'db>,
) -> Option<EnclosingScope<'db>> {
    let file = definition.program_file(db);
    let index = semantic_index(db, file);
    let scope = definition.file_scope(db);

    match index.scope(scope).kind() {
        ScopeKind::Module => Some(EnclosingScope::Module),
        ScopeKind::Class => {
            let class_ref = index.scope(scope).node().as_class()?;
            Some(EnclosingScope::Class(
                index.expect_single_definition(class_ref),
            ))
        }
        _ => None,
    }
}

/// Returns the collection kind for a class under pytest's default conventions.
///
/// Returns [`PytestTestKind::Pytest`] for classes whose names start with `Test` and whose
/// constructors are inherited from `object`, or [`PytestTestKind::StdlibUnittest`] for
/// `unittest.TestCase` subclasses. In either case, the class must be at module scope or nested in an
/// eligible class that does not inherit from `unittest.TestCase`. Abstract classes and classes
/// with a falsy `__test__` flag (including an inherited flag) are excluded.
///
/// Returns `None` when `definition` is not a class, the class fails those collection rules, or
/// its type or constructors cannot be resolved. This does not check the module's filename or
/// whether the class contains any test methods.
fn pytest_test_class_kind<'db>(
    db: &'db dyn Db,
    definition: Definition<'db>,
) -> Option<PytestTestKind> {
    let DefinitionKind::Class(class_ref) = definition.kind(db) else {
        return None;
    };
    if !super::is_available_definition(db, definition) {
        return None;
    }

    match enclosing_scope(db, definition)? {
        EnclosingScope::Module => {}
        EnclosingScope::Class(parent) => {
            if pytest_test_class_kind(db, parent) != Some(PytestTestKind::Pytest) {
                return None;
            }
        }
    }

    let class = original_class_type(db, definition)?;
    let env = ProgramEnvironment::from_file(definition.program_file(db));
    if place_value_type(
        db,
        class
            .class_member(db, &env, "__test__", MemberLookupPolicy::default())
            .place,
    )
    .is_some_and(|flag| flag.bool(db, &env).is_always_false())
    {
        return None;
    }

    // The subtype check models `isinstance(cls, ABCMeta)`. Inheriting from ABC supplies this
    // metaclass; @abstractmethod alone does not make a class abstract at runtime.
    // Exclude classes with explicitly abstract members that have not been concretely overridden.
    // Implicit abstract methods in protocols affect only type checking and are ignored here.
    // Custom metaclass behavior is approximated.
    if Type::ClassLiteral(class).is_subtype_of(db, &env, KnownClass::ABCMeta.to_instance(db, &env))
        && class
            .identity_specialization(db)
            .abstract_methods(db)
            .values()
            .any(|method| method.kind.is_explicit())
    {
        return None;
    }

    if is_unittest_test_case(db, class) {
        return Some(PytestTestKind::StdlibUnittest);
    }

    let module = parsed_module(db, definition.python_file(db)).load(db);
    if !class_ref.node(&module).name.as_str().starts_with("Test") {
        return None;
    }

    has_default_pytest_constructors(db, class).then_some(PytestTestKind::Pytest)
}

/// Returns whether `class` inherits both constructors from `object`.
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

        actual.provenance == expected.provenance
    })
}

/// Returns whether the class inherits from the canonical `unittest.TestCase`.
fn is_unittest_test_case(db: &dyn Db, class: ClassLiteral<'_>) -> bool {
    class
        .iter_mro(db)
        .filter_map(ClassBase::into_class)
        .any(|ancestor| ancestor.is_known(db, KnownClass::UnittestTestCase))
}

/// Unittest uses `runTest` only when no named test methods are available, including inherited ones.
fn has_unittest_test_methods<'db>(db: &'db dyn Db, class: ClassLiteral<'db>) -> bool {
    let env = ProgramEnvironment::from_file(class.program_file(db));
    class
        .iter_mro(db)
        .filter_map(ClassBase::into_class)
        .filter_map(|ancestor| ancestor.static_class_literal(db))
        .any(|(ancestor, _)| {
            place_table(db, ancestor.body_scope(db))
                .symbols()
                .any(|symbol| {
                    if !symbol.name().starts_with("test") {
                        return false;
                    }
                    let member = class
                        .class_member(db, &env, symbol.name(), MemberLookupPolicy::default())
                        .place;
                    place_value_type(db, member)
                        .is_some_and(|ty| ty.try_upcast_to_callable(db, &env).is_some())
                })
        })
}

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

    if !(stem.starts_with("test_") || stem.ends_with("_test")) {
        return false;
    }

    let env = ProgramEnvironment::from_file(file);
    !place_value_type(
        db,
        symbol(
            db,
            global_scope(db, file),
            "__test__",
            ConsideredDefinitions::EndOfScope,
        )
        .place,
    )
    .is_some_and(|flag| flag.bool(db, &env).is_always_false())
}

#[cfg(test)]
mod tests {
    use insta::assert_snapshot;
    use ruff_db::diagnostic::{
        Annotation, Diagnostic, DiagnosticId, DisplayDiagnosticConfig, DisplayDiagnostics, Severity,
    };
    use ruff_db::files::{FileRange, system_path_to_file};
    use ruff_db::parsed::parsed_module;
    use ruff_python_ast as ast;
    use ruff_text_size::Ranged;
    use ty_python_core::ProgramFile;
    use ty_python_core::definition::{Definition, DefinitionKind};
    use ty_python_core::semantic_index;

    use super::{PytestTestKind, pytest_test_for_binding, pytest_tests_in_file};
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
async def test_async(): ...
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

        assert_snapshot!(test.collected_tests(), @"
        info[pytest-collection]: Collected pytest test
         --> src/test_example.py:6:5
          |
        6 | def test_module(): ...
          |     ^^^^^^^^^^^

        info[pytest-collection]: Collected pytest test
         --> src/test_example.py:7:11
          |
        7 | async def test_async(): ...
          |           ^^^^^^^^^^

        info[pytest-collection]: Collected pytest test
          --> src/test_example.py:11:9
           |
        11 |     def test_method(self): ...
           |         ^^^^^^^^^^^

        info[pytest-collection]: Collected pytest test
          --> src/test_example.py:14:13
           |
        14 |         def test_nested(self): ...
           |             ^^^^^^^^^^^

        info[pytest-collection]: Collected unittest test
          --> src/test_example.py:20:9
           |
        20 |     def test_unit(self): ...
           |         ^^^^^^^^^
        ");
        assert!(
            pytest_test_for_binding(&test.db, test.function("test_module"))
                .expect("module test should be collected")
                .enclosing_class
                .is_none()
        );
        assert!(
            pytest_test_for_binding(&test.db, test.function("TestClass.test_method"))
                .expect("method should be collected")
                .enclosing_class
                .is_some()
        );
    }

    #[test]
    fn matches_test_function_prefix_case_sensitively() {
        let test = CollectionTest::new(
            "/src/test_example.py",
            r#"
def testFunction(): ...
def Test_function(): ...
def TEST_FUNCTION(): ...

class TestClass:
    def testMethod(self): ...
    def Test_method(self): ...
    def TEST_METHOD(self): ...
"#,
        );

        assert_snapshot!(test.collected_tests(), @"
        info[pytest-collection]: Collected pytest test
         --> src/test_example.py:2:5
          |
        2 | def testFunction(): ...
          |     ^^^^^^^^^^^^

        info[pytest-collection]: Collected pytest test
         --> src/test_example.py:7:9
          |
        7 |     def testMethod(self): ...
          |         ^^^^^^^^^^
        ");
        // File-wide collection skips this name before checking individual bindings.
        assert!(pytest_test_for_binding(&test.db, test.function("Test_function")).is_none());
    }

    #[test]
    fn collects_generic_functions_and_classes() {
        let test = CollectionTest::new(
            "/src/test_example.py",
            r#"
def test_generic[T](): ...

class TestGeneric[T]:
    def test_generic_method[U](self): ...

    class TestNested[V]:
        def test_nested_generic[W](self): ...

def outer[T]():
    def test_local[U](): ...
"#,
        );

        assert_snapshot!(test.collected_tests(), @"
        info[pytest-collection]: Collected pytest test
         --> src/test_example.py:2:5
          |
        2 | def test_generic[T](): ...
          |     ^^^^^^^^^^^^

        info[pytest-collection]: Collected pytest test
         --> src/test_example.py:5:9
          |
        5 |     def test_generic_method[U](self): ...
          |         ^^^^^^^^^^^^^^^^^^^

        info[pytest-collection]: Collected pytest test
         --> src/test_example.py:8:13
          |
        8 |         def test_nested_generic[W](self): ...
          |             ^^^^^^^^^^^^^^^^^^^
        ");
    }

    #[test]
    fn collects_parametrized_generic_function_once() {
        let test = CollectionTest::new(
            "/src/test_example.py",
            r#"
import pytest

@pytest.mark.parametrize("value", [1, "foo"])
def test_generic[T](value: T) -> None: ...
"#,
        );

        assert_snapshot!(test.collected_tests(), @"
        info[pytest-collection]: Collected pytest test
         --> src/test_example.py:5:5
          |
        5 | def test_generic[T](value: T) -> None: ...
          |     ^^^^^^^^^^^^
        ");
    }

    #[test]
    fn treats_project_defined_unittest_test_case_as_an_ordinary_base() {
        // The project-local `unittest` package shadows the standard library. Inheriting from its
        // `unittest.case.TestCase` does not grant unittest collection: `Example` is excluded, while
        // `TestExample` is collected by name because it starts with `Test`.
        let db = pytest_db_with_files(&[
            (
                "/src/unittest/__init__.py",
                r#"
from .case import TestCase
"#,
            ),
            (
                "/src/unittest/case.py",
                r#"
class TestCase: ...
"#,
            ),
            (
                "/src/test_example.py",
                r#"
from unittest import TestCase

class Example(TestCase):
    def test_not_collected(self): ...

class TestExample(TestCase):
    def test_pytest(self): ...
"#,
            ),
        ]);

        assert_snapshot!(collected_tests(&db, "/src/test_example.py"), @"
        info[pytest-collection]: Collected pytest test
         --> src/test_example.py:8:9
          |
        8 |     def test_pytest(self): ...
          |         ^^^^^^^^^^^
        ");
    }

    #[test]
    fn requires_a_default_test_module_name() {
        let test = CollectionTest::new(
            "/src/example.py",
            r#"
def test_example(): ...
"#,
        );

        assert_snapshot!(test.collected_tests(), @"No tests collected");
        // File-wide collection returns early without checking individual bindings.
        assert_eq!(
            pytest_test_for_binding(&test.db, test.function("test_example")),
            None
        );
    }

    #[test]
    fn rejects_custom_constructors() {
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
"#,
        );

        assert_snapshot!(test.collected_tests(), @"No tests collected");
    }

    #[test]
    fn collects_only_remaining_bindings() {
        let test = CollectionTest::new(
            "/src/test_example.py",
            r#"
def test_redefined(): ...
test_original = original_alias = test_redefined
def test_redefined(): ...

def test_overwritten(): ...
test_overwritten = None

def test_unpacked_overwrite(): ...
test_unpacked_overwrite, other = None, 0

def test_deleted(): ...
del test_deleted

class TestOverwritten:
    def test_hidden(self): ...
TestOverwritten = None

class TestMethods:
    def test_redefined_method(self): ...
    def test_redefined_method(self): ...

if False:
    def test_unreachable(): ...
"#,
        );

        assert_snapshot!(test.collected_tests(), @"
        info[pytest-collection]: Collected pytest test
         --> src/test_example.py:3:1
          |
        2 | def test_redefined(): ...
          |     --------------
        3 | test_original = original_alias = test_redefined
          | ^^^^^^^^^^^^^

        info[pytest-collection]: Collected pytest test
         --> src/test_example.py:4:5
          |
        4 | def test_redefined(): ...
          |     ^^^^^^^^^^^^^^

        info[pytest-collection]: Collected pytest test
          --> src/test_example.py:21:9
           |
        21 |     def test_redefined_method(self): ...
           |         ^^^^^^^^^^^^^^^^^^^^^
        ");
        // Check the original definition, which file-wide collection never visits.
        assert!(pytest_test_for_binding(&test.db, test.function("test_redefined")).is_none());
    }

    #[test]
    fn collects_function_aliases_and_imports() {
        let db = pytest_db_with_files(&[
            (
                "/src/helpers.py",
                r#"
def external_function(): ...
"#,
            ),
            (
                "/src/reexport.py",
                r#"
from helpers import external_function as forwarded
"#,
            ),
            (
                "/src/test_example.py",
                r#"
from helpers import external_function as test_imported
from reexport import forwarded as test_reexported

def local_function(): ...
test_unpacked_alias, _ = local_function, 0
test_annotated_alias: object = local_function
local_function = None

class TestMethods:
    test_imported_method = test_imported
"#,
            ),
        ]);

        assert_snapshot!(collected_tests(&db, "/src/test_example.py"), @"
        info[pytest-collection]: Collected pytest test
         --> src/test_example.py:2:42
          |
        2 | from helpers import external_function as test_imported
          |                                          ^^^^^^^^^^^^^
          |
         ::: src/helpers.py:2:5
          |
        2 | def external_function(): ...
          |     -----------------

        info[pytest-collection]: Collected pytest test
         --> src/test_example.py:3:35
          |
        3 | from reexport import forwarded as test_reexported
          |                                   ^^^^^^^^^^^^^^^
          |
         ::: src/helpers.py:2:5
          |
        2 | def external_function(): ...
          |     -----------------

        info[pytest-collection]: Collected pytest test
         --> src/test_example.py:6:1
          |
        5 | def local_function(): ...
          |     --------------
        6 | test_unpacked_alias, _ = local_function, 0
          | ^^^^^^^^^^^^^^^^^^^

        info[pytest-collection]: Collected pytest test
         --> src/test_example.py:7:1
          |
        5 | def local_function(): ...
          |     --------------
        6 | test_unpacked_alias, _ = local_function, 0
        7 | test_annotated_alias: object = local_function
          | ^^^^^^^^^^^^^^^^^^^^

        info[pytest-collection]: Collected pytest test
          --> src/test_example.py:11:5
           |
        11 |     test_imported_method = test_imported
           |     ^^^^^^^^^^^^^^^^^^^^
           |
          ::: src/helpers.py:2:5
           |
         2 | def external_function(): ...
           |     -----------------
        ");
    }

    #[test]
    fn honors_module_opt_out() {
        let test = CollectionTest::new(
            "/src/test_example.py",
            r#"
__test__: bool = True
__test__ = False
def test_example(): ...
"#,
        );
        assert_snapshot!(test.collected_tests(), @"No tests collected");
        // File-wide collection returns early without checking individual bindings.
        assert!(pytest_test_for_binding(&test.db, test.function("test_example")).is_none());
    }

    #[test]
    fn honors_inherited_class_opt_outs() {
        let test = CollectionTest::new(
            "/src/test_example.py",
            r#"
import unittest

class TestDisabled:
    __test__ = False
    def test_disabled(self): ...

class TestInherited(TestDisabled):
    def test_inherited(self): ...

class TestEnabled(TestDisabled):
    __test__ = True
    def test_enabled(self): ...

class DisabledUnit(unittest.TestCase):
    __test__ = False
    def test_disabled_unit(self): ...
"#,
        );
        assert_snapshot!(test.collected_tests(), @"
        info[pytest-collection]: Collected pytest test
          --> src/test_example.py:13:9
           |
        13 |     def test_enabled(self): ...
           |         ^^^^^^^^^^^^
        ");
    }

    #[test]
    fn excludes_abstract_test_classes() {
        let test = CollectionTest::new(
            "/src/test_example.py",
            r#"
from abc import ABC, abstractmethod
import unittest

class TestAbstract(ABC):
    @abstractmethod
    def value(self): ...
    def test_abstract(self): ...

class TestConcrete(TestAbstract):
    def value(self): return 1
    def test_concrete(self): ...

class AbstractUnit(unittest.TestCase, TestAbstract):
    def test_abstract_unit(self): ...

class TestABCWithoutAbstractMethods(ABC):
    def test_concrete_abc(self): ...

class TestWithoutABCMeta:
    @abstractmethod
    def test_without_abcmeta(self): ...
"#,
        );
        assert_snapshot!(test.collected_tests(), @"
        info[pytest-collection]: Collected pytest test
          --> src/test_example.py:12:9
           |
        12 |     def test_concrete(self): ...
           |         ^^^^^^^^^^^^^

        info[pytest-collection]: Collected pytest test
          --> src/test_example.py:18:9
           |
        18 |     def test_concrete_abc(self): ...
           |         ^^^^^^^^^^^^^^^^^

        info[pytest-collection]: Collected pytest test
          --> src/test_example.py:22:9
           |
        22 |     def test_without_abcmeta(self): ...
           |         ^^^^^^^^^^^^^^^^^^^^
        ");
    }

    #[test]
    fn excludes_known_property_decorators() {
        let test = CollectionTest::new(
            "/src/test_example.py",
            r#"
from builtins import property as descriptor
import unittest

@descriptor
def helper(self): ...
test_getter = helper.fget
test_property = helper

class TestDescriptors:
    @descriptor
    def test_data(self): return 42

    @staticmethod
    @descriptor
    def test_wrapped_data(self): return 42

    @staticmethod
    def test_staticmethod(): ...

    @classmethod
    def test_classmethod(cls): ...

class UnitDescriptors(unittest.TestCase):
    @descriptor
    def test_unit_data(self): return 42

def property[F](function: F) -> F:
    return function

@property
def test_custom_property_decorator(): ...
"#,
        );
        assert_snapshot!(test.collected_tests(), @"
        info[pytest-collection]: Collected pytest test
         --> src/test_example.py:7:1
          |
        6 | def helper(self): ...
          |     ------
        7 | test_getter = helper.fget
          | ^^^^^^^^^^^

        info[pytest-collection]: Collected pytest test
          --> src/test_example.py:19:9
           |
        19 |     def test_staticmethod(): ...
           |         ^^^^^^^^^^^^^^^^^

        info[pytest-collection]: Collected pytest test
          --> src/test_example.py:22:9
           |
        22 |     def test_classmethod(cls): ...
           |         ^^^^^^^^^^^^^^^^

        info[pytest-collection]: Collected pytest test
          --> src/test_example.py:32:5
           |
        32 | def test_custom_property_decorator(): ...
           |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
        ");
    }

    #[test]
    fn collects_unittest_run_test_only_without_named_tests() {
        let test = CollectionTest::new(
            "/src/test_example.py",
            r#"
import unittest

def runTest(): ...
class TestOrdinary:
    def runTest(self): ...

class Fallback(unittest.TestCase):
    def runTest(self): ...

class Named(unittest.TestCase):
    def test_named(self): ...
    def runTest(self): ...

class Inherited(Named):
    def runTest(self): ...

class NonCallable(unittest.TestCase):
    test_data = None
    @property
    def test_property(self): return 1
    def runTest(self): ...
"#,
        );
        assert_snapshot!(test.collected_tests(), @"
        info[pytest-collection]: Collected unittest test
         --> src/test_example.py:9:9
          |
        9 |     def runTest(self): ...
          |         ^^^^^^^

        info[pytest-collection]: Collected unittest test
          --> src/test_example.py:12:9
           |
        12 |     def test_named(self): ...
           |         ^^^^^^^^^^

        info[pytest-collection]: Collected unittest test
          --> src/test_example.py:22:9
           |
        22 |     def runTest(self): ...
           |         ^^^^^^^
        ");
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

        fn collected_tests(&self) -> String {
            collected_tests(&self.db, self.path)
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
        pytest_db_with_files(&[(path, source)])
    }

    fn pytest_db_with_files(files: &[(&'static str, &'static str)]) -> TestDb {
        let builder = TestDbBuilder::new()
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
from typing import Callable
from _pytest.fixtures import fixture as fixture

class MarkGenerator:
    def parametrize[F](self, argnames: str, argvalues: object) -> Callable[[F], F]: ...

mark: MarkGenerator
"#,
            );
        files
            .iter()
            .fold(builder, |builder, (path, source)| {
                builder.with_file(path, source)
            })
            .build()
            .expect("valid pytest test database")
    }

    fn collected_tests(db: &TestDb, path: &str) -> String {
        let file = program_file(db, path);
        let tests = pytest_tests_in_file(db, file);
        if tests.is_empty() {
            return "No tests collected".to_owned();
        }

        let module = parsed_module(db, file.python_file(db)).load(db);
        // Render one diagnostic per result so the snapshot preserves collection order.
        let diagnostics = tests
            .iter()
            .map(|test| {
                let kind = match test.kind {
                    PytestTestKind::Pytest => "pytest",
                    PytestTestKind::StdlibUnittest => "unittest",
                };
                let mut diagnostic = Diagnostic::new(
                    DiagnosticId::lint("pytest-collection"),
                    Severity::Info,
                    format_args!("Collected {kind} test"),
                );
                let range = match test.binding.kind(db) {
                    DefinitionKind::ImportFrom(import) => {
                        let alias = import.alias(&module);
                        FileRange::new(
                            test.binding.file(db),
                            alias.asname.as_ref().unwrap_or(&alias.name).range(),
                        )
                    }
                    _ => test.binding.focus_range(db, &module),
                };
                diagnostic.annotate(Annotation::primary(range.into()));
                if test.binding != test.function {
                    let module = parsed_module(db, test.function.python_file(db)).load(db);
                    diagnostic.annotate(Annotation::secondary(
                        test.function.focus_range(db, &module).into(),
                    ));
                }
                diagnostic
            })
            .collect::<Vec<_>>();

        DisplayDiagnostics::new(
            db,
            &DisplayDiagnosticConfig::new("ty").context(0),
            &diagnostics,
        )
        .to_string()
        .replace('\\', "/")
    }

    fn program_file<'db>(db: &'db TestDb, path: &str) -> ProgramFile<'db> {
        let file = system_path_to_file(db, path).expect("test file should exist");
        db.program_file(file)
    }
}
