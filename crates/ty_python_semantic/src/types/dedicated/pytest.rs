use ruff_db::parsed::{ParsedModuleRef, parsed_module};
use ruff_python_ast::{self as ast, name::Name};
use ruff_text_size::Ranged;
use ty_module_resolver::{KnownModule, file_to_module};
use ty_python_core::definition::{Definition, DefinitionKind};
use ty_python_core::scope::{FileScopeId, ScopeKind};
use ty_python_core::{ProgramFile, global_scope, place_table, semantic_index, use_def_map};

use crate::Db;
use crate::types::Type;
use crate::types::function::FunctionDecorators;
use crate::types::ide_support::resolve_definition_targets;
use crate::types::infer::{function_known_decorator_flags, function_known_decorators};

/// Resolve the same-file pytest fixtures requested by `parameter`.
///
/// This query models only fixtures declared directly in the parameter's class or module. Imported,
/// conftest, built-in, and plugin fixtures are added by later provider layers.
#[salsa::tracked(returns(ref), heap_size=ruff_memory_usage::heap_size)]
pub fn fixture_bindings_for_parameter<'db>(
    db: &'db dyn Db,
    parameter: Definition<'db>,
) -> Box<[FixtureBinding<'db>]> {
    let Some(request) = FixtureRequest::from_parameter(db, parameter) else {
        return Box::default();
    };

    if let Some(class_scope) = request.class_scope {
        let bindings = bindings_in_provider(db, &request, class_scope);
        if !bindings.is_empty() {
            return bindings;
        }
    }

    let module_scope = global_scope(db, parameter.program_file(db)).file_scope_id(db);
    bindings_in_provider(db, &request, module_scope)
}

/// A pytest fixture request and the fixture declaration that satisfies it.
#[derive(Debug, Clone, Copy, Eq, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub struct FixtureBinding<'db> {
    request: Definition<'db>,
    fixture: Definition<'db>,
}

impl<'db> FixtureBinding<'db> {
    /// Return the parameter definition that requests the fixture.
    pub fn request(self) -> Definition<'db> {
        self.request
    }

    /// Return the decorated function that declares the fixture.
    pub fn fixture(self) -> Definition<'db> {
        self.fixture
    }
}

#[derive(Debug)]
struct FixtureRequest<'db> {
    definition: Definition<'db>,
    owner: Definition<'db>,
    name: Name,
    class_scope: Option<FileScopeId>,
}

impl<'db> FixtureRequest<'db> {
    fn from_parameter(db: &'db dyn Db, definition: Definition<'db>) -> Option<Self> {
        if !matches!(definition.kind(db), DefinitionKind::Parameter(_)) {
            return None;
        }

        let file = definition.program_file(db);
        let module = parsed_module(db, file.python_file(db)).load(db);
        let index = semantic_index(db, file);
        let function_scope = definition.scope(db).file_scope_id(db);
        let function_ref = index.scope(function_scope).node().as_function()?;
        let function = function_ref.node(&module);
        let owner = index.expect_single_definition(function_ref);
        let parameter_range = definition.focus_range(db, &module).range();
        let parameter = function
            .parameters
            .iter_non_variadic_params()
            .find(|parameter| parameter.parameter.name.range() == parameter_range)?;

        if parameter.default.is_some()
            || function
                .parameters
                .posonlyargs
                .iter()
                .any(|candidate| candidate.range() == parameter.range())
        {
            return None;
        }

        let parent_scope = non_type_parameter_parent(index, function_scope)?;
        let parent_kind = index.scope(parent_scope).kind();
        if !matches!(parent_kind, ScopeKind::Module | ScopeKind::Class) {
            return None;
        }

        let class_scope = (parent_kind == ScopeKind::Class).then_some(parent_scope);
        if let Some(class_scope) = class_scope {
            let class_parent = non_type_parameter_parent(index, class_scope)?;
            if index.scope(class_parent).kind() != ScopeKind::Module {
                return None;
            }

            let is_receiver = function
                .parameters
                .args
                .first()
                .is_some_and(|first| first.range() == parameter.range())
                && !function_known_decorator_flags(db, owner)
                    .contains(FunctionDecorators::STATICMETHOD);
            if is_receiver {
                return None;
            }
        }

        let is_fixture = fixture_declaration(db, owner).is_some();
        if !is_fixture && !is_collected_test(db, file, function, class_scope, &module) {
            return None;
        }

        let name = parameter.name().id.clone();
        if directly_parametrized(db, owner, function, name.as_str()) {
            return None;
        }

        Some(Self {
            definition,
            owner,
            name,
            class_scope,
        })
    }
}

#[derive(Debug, Clone)]
struct FixtureDeclaration<'db> {
    definition: Definition<'db>,
    name: FixtureName,
}

#[derive(Debug, Clone)]
struct FixtureExposure<'db> {
    name: Name,
    declaration: FixtureDeclaration<'db>,
}

#[derive(Debug, Clone)]
enum FixtureName {
    Default,
    Explicit(Name),
    Dynamic,
}

fn bindings_in_provider<'db>(
    db: &'db dyn Db,
    request: &FixtureRequest<'db>,
    provider: FileScopeId,
) -> Box<[FixtureBinding<'db>]> {
    let scope = provider.to_scope_id(db, request.definition.program_file(db));
    let table = place_table(db, scope);
    let use_def = use_def_map(db, scope);
    let mut bindings = Vec::new();

    for (symbol_id, definitions) in use_def.all_end_of_scope_symbol_bindings() {
        let symbol_name = table.symbol(symbol_id).name();
        for definition in definitions.filter_map(|binding| binding.binding.definition()) {
            for definition in resolve_definition_targets(db, definition, symbol_name) {
                let Some(declaration) = fixture_declaration(db, definition) else {
                    continue;
                };
                let Some(exposure) = fixture_exposure(symbol_name, declaration) else {
                    continue;
                };
                if exposure.name != request.name
                    || exposure.declaration.definition == request.owner
                    || bindings
                        .iter()
                        .any(|binding: &FixtureBinding<'db>| binding.fixture == definition)
                {
                    continue;
                }
                bindings.push(FixtureBinding {
                    request: request.definition,
                    fixture: definition,
                });
            }
        }
    }

    bindings.into_boxed_slice()
}

fn fixture_exposure<'db>(
    symbol_name: &Name,
    declaration: FixtureDeclaration<'db>,
) -> Option<FixtureExposure<'db>> {
    let name = match &declaration.name {
        FixtureName::Default => symbol_name.clone(),
        FixtureName::Explicit(name) => name.clone(),
        FixtureName::Dynamic => return None,
    };
    Some(FixtureExposure { name, declaration })
}

fn fixture_declaration<'db>(
    db: &'db dyn Db,
    definition: Definition<'db>,
) -> Option<FixtureDeclaration<'db>> {
    let DefinitionKind::Function(function_ref) = definition.kind(db) else {
        return None;
    };
    let module = parsed_module(db, definition.python_file(db)).load(db);
    let function = function_ref.node(&module);
    let inference = function_known_decorators(db, definition);

    for decorator in &function.decorator_list {
        let (callee, name) = match &decorator.expression {
            ast::Expr::Call(call) => (
                call.func.as_ref(),
                fixture_name_from_arguments(&call.arguments),
            ),
            expression => (expression, FixtureName::Default),
        };
        let Some(Type::FunctionLiteral(decorator_function)) = inference.expression_type(callee)
        else {
            continue;
        };
        let is_fixture_decorator =
            file_to_module(db, decorator_function.program_file(db).resolver_file(db))
                .is_some_and(|module| module.known(db) == Some(KnownModule::PytestFixtures))
                && matches!(
                    decorator_function.name(db).as_str(),
                    "fixture" | "yield_fixture"
                );
        if is_fixture_decorator {
            return Some(FixtureDeclaration { definition, name });
        }
    }

    None
}

fn fixture_name_from_arguments(arguments: &ast::Arguments) -> FixtureName {
    let Some(value) = arguments
        .keywords
        .iter()
        .find(|keyword| keyword.arg.as_ref().is_some_and(|arg| arg == "name"))
        .map(|keyword| &keyword.value)
    else {
        return FixtureName::Default;
    };

    if value.is_none_literal_expr() {
        FixtureName::Default
    } else if let Some(string) = value.as_string_literal_expr() {
        let value = string.value.to_str();
        if value.is_empty() {
            FixtureName::Default
        } else {
            FixtureName::Explicit(Name::new(value))
        }
    } else {
        FixtureName::Dynamic
    }
}

fn non_type_parameter_parent(
    index: &ty_python_core::SemanticIndex<'_>,
    scope: FileScopeId,
) -> Option<FileScopeId> {
    let parent = index.parent_scope_id(scope)?;
    if index.scope(parent).kind() == ScopeKind::TypeParams {
        index.parent_scope_id(parent)
    } else {
        Some(parent)
    }
}

fn is_collected_test(
    db: &dyn Db,
    file: ProgramFile<'_>,
    function: &ast::StmtFunctionDef,
    class_scope: Option<FileScopeId>,
    module: &ParsedModuleRef,
) -> bool {
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
    if !(stem.starts_with("test_") || stem.ends_with("_test"))
        || !function.name.as_str().starts_with("test")
    {
        return false;
    }

    class_scope.is_none_or(|class_scope| {
        semantic_index(db, file)
            .scope(class_scope)
            .node()
            .as_class()
            .is_some_and(|class_ref| class_ref.node(module).name.as_str().starts_with("Test"))
    })
}

fn directly_parametrized(
    db: &dyn Db,
    owner: Definition<'_>,
    function: &ast::StmtFunctionDef,
    parameter_name: &str,
) -> bool {
    let inference = function_known_decorators(db, owner);
    function.decorator_list.iter().any(|decorator| {
        let Some(call) = decorator.expression.as_call_expr() else {
            return false;
        };
        let Some(mark_attribute) = call
            .func
            .as_attribute_expr()
            .filter(|attribute| attribute.attr.as_str() == "parametrize")
            .and_then(|attribute| attribute.value.as_attribute_expr())
            .filter(|attribute| attribute.attr.as_str() == "mark")
        else {
            return false;
        };
        let Some(Type::ModuleLiteral(module)) =
            inference.expression_type(mark_attribute.value.as_ref())
        else {
            return false;
        };
        if module.module(db).known(db) != Some(KnownModule::Pytest) {
            return false;
        }

        parametrized_names(&call.arguments).is_some_and(|names| names.contains(&parameter_name))
            && is_indirect(&call.arguments, parameter_name) == Some(false)
    })
}

fn parametrized_names(arguments: &ast::Arguments) -> Option<Vec<&str>> {
    let expression = arguments.args.first().or_else(|| {
        arguments
            .keywords
            .iter()
            .find(|keyword| keyword.arg.as_ref().is_some_and(|arg| arg == "argnames"))
            .map(|keyword| &keyword.value)
    })?;
    static_string_list(expression)
}

fn is_indirect(arguments: &ast::Arguments, parameter_name: &str) -> Option<bool> {
    let Some(expression) = arguments
        .keywords
        .iter()
        .find(|keyword| keyword.arg.as_ref().is_some_and(|arg| arg == "indirect"))
        .map(|keyword| &keyword.value)
    else {
        return Some(false);
    };
    if let Some(boolean) = expression.as_boolean_literal_expr() {
        return Some(boolean.value);
    }
    static_string_list(expression).map(|names| names.contains(&parameter_name))
}

fn static_string_list(expression: &ast::Expr) -> Option<Vec<&str>> {
    if let Some(string) = expression.as_string_literal_expr() {
        return Some(
            string
                .value
                .to_str()
                .split(|character: char| character == ',' || character.is_whitespace())
                .filter(|name| !name.is_empty())
                .collect(),
        );
    }

    let elements = expression
        .as_list_expr()
        .map(|list| list.elts.as_slice())
        .or_else(|| {
            expression
                .as_tuple_expr()
                .map(|tuple| tuple.elts.as_slice())
        })?;
    elements
        .iter()
        .map(|element| {
            element
                .as_string_literal_expr()
                .map(|string| string.value.to_str())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use ruff_db::files::system_path_to_file;
    use ruff_db::parsed::parsed_module;
    use ruff_python_ast as ast;
    use ty_python_core::definition::Definition;
    use ty_python_core::scope::ScopeKind;
    use ty_python_core::semantic_index;

    use super::fixture_bindings_for_parameter;
    use crate::Db as _;
    use crate::db::tests::{TestDb, TestDbBuilder};

    #[test]
    fn resolves_same_file_fixture_declarations_and_dependencies() -> Result<()> {
        let db = pytest_db(
            "/src/test_example.py",
            r#"
import pytest
from pytest import fixture as make_fixture, yield_fixture

@pytest.fixture
def first(): ...

@make_fixture()
def second(first): ...

@yield_fixture()
def legacy(): ...

def test_use(first, second, legacy): ...
"#,
        )?;

        assert_eq!(fixture_names(&db, "second", "first"), ["first"]);
        assert_eq!(fixture_names(&db, "test_use", "first"), ["first"]);
        assert_eq!(fixture_names(&db, "test_use", "second"), ["second"]);
        assert_eq!(fixture_names(&db, "test_use", "legacy"), ["legacy"]);
        Ok(())
    }

    #[test]
    fn honors_explicit_names_and_ignores_dynamic_names() -> Result<()> {
        let db = pytest_db(
            "/src/test_example.py",
            r#"
import pytest

fixture_name = "dynamic"

@pytest.fixture(name="public_name")
def implementation(): ...

@pytest.fixture(name=fixture_name)
def dynamic_implementation(): ...

def test_use(public_name, implementation, dynamic): ...
"#,
        )?;

        assert_eq!(
            fixture_names(&db, "test_use", "public_name"),
            ["implementation"]
        );
        assert!(fixture_names(&db, "test_use", "implementation").is_empty());
        assert!(fixture_names(&db, "test_use", "dynamic").is_empty());
        Ok(())
    }

    #[test]
    fn prefers_class_fixtures_and_skips_method_receivers() -> Result<()> {
        let db = pytest_db(
            "/src/test_example.py",
            r#"
import pytest

@pytest.fixture
def value(): ...

class TestExample:
    @pytest.fixture
    def value(self): ...

    @pytest.fixture
    def dependent(self, value): ...

    def test_use(self, value, dependent): ...
"#,
        )?;

        assert_eq!(
            fixture_names(&db, "TestExample.test_use", "value"),
            ["value"]
        );
        assert_eq!(
            fixture_provider_scopes(&db, "TestExample.test_use", "value"),
            [ScopeKind::Class]
        );
        assert_eq!(
            fixture_names(&db, "TestExample.dependent", "value"),
            ["value"]
        );
        assert!(fixture_names(&db, "TestExample.test_use", "self").is_empty());
        Ok(())
    }

    #[test]
    fn uses_module_fixture_for_same_name_class_override() -> Result<()> {
        let db = pytest_db(
            "/src/test_example.py",
            r#"
import pytest

@pytest.fixture
def value(): ...

class TestExample:
    @pytest.fixture
    def value(self, value): ...
"#,
        )?;

        assert_eq!(
            fixture_provider_scopes(&db, "TestExample.value", "value"),
            [ScopeKind::Module]
        );
        Ok(())
    }

    #[test]
    fn classifies_only_supported_fixture_requests() -> Result<()> {
        let db = pytest_db(
            "/src/test_example.py",
            r#"
import pytest

@pytest.fixture
def value(): ...

def helper(value): ...

def test_defaults(positional_only, /, value=None, *args, **kwargs): ...

class Example:
    def test_method(value): ...

class TestNested:
    class TestInner:
        def test_method(self, value): ...
"#,
        )?;

        assert!(fixture_names(&db, "helper", "value").is_empty());
        assert!(fixture_names(&db, "test_defaults", "positional_only").is_empty());
        assert!(fixture_names(&db, "test_defaults", "value").is_empty());
        assert!(fixture_names(&db, "test_defaults", "args").is_empty());
        assert!(fixture_names(&db, "test_defaults", "kwargs").is_empty());
        assert!(fixture_names(&db, "Example.test_method", "value").is_empty());
        assert!(fixture_names(&db, "TestNested.TestInner.test_method", "value").is_empty());
        Ok(())
    }

    #[test]
    fn excludes_direct_parameters_and_keeps_indirect_parameters() -> Result<()> {
        let db = pytest_db(
            "/src/test_example.py",
            r#"
import pytest

@pytest.fixture
def value(): ...

@pytest.fixture
def other(): ...

@pytest.mark.parametrize("value", [1])
def test_direct(value): ...

@pytest.mark.parametrize("value", [1], indirect=True)
def test_indirect(value): ...

@pytest.mark.parametrize("value, other", [(1, 2)], indirect=["value"])
def test_mixed(value, other): ...
"#,
        )?;

        assert!(fixture_names(&db, "test_direct", "value").is_empty());
        assert_eq!(fixture_names(&db, "test_indirect", "value"), ["value"]);
        assert_eq!(fixture_names(&db, "test_mixed", "value"), ["value"]);
        assert!(fixture_names(&db, "test_mixed", "other").is_empty());
        Ok(())
    }

    #[test]
    fn requires_a_default_pytest_module_name() -> Result<()> {
        let db = pytest_db(
            "/src/example.py",
            r#"
import pytest

@pytest.fixture
def value(): ...

def test_use(value): ...
"#,
        )?;

        assert!(fixture_names(&db, "test_use", "value").is_empty());
        Ok(())
    }

    #[test]
    fn resolves_imported_fixture_exposures() -> Result<()> {
        let db = pytest_db_with_files(&[
            (
                "/src/fixtures.py",
                r#"
import pytest

@pytest.fixture
def resource(): ...

@pytest.fixture(name="public_name")
def implementation(): ...
"#,
            ),
            (
                "/src/reexports.py",
                r#"
from fixtures import resource as middle
"#,
            ),
            (
                "/src/star_fixtures.py",
                r#"
import pytest

@pytest.fixture
def star_fixture(): ...
"#,
            ),
            (
                "/src/test_example.py",
                r#"
from fixtures import resource as direct_alias, implementation
from reexports import middle as chained
from star_fixtures import *

def test_use(
    direct_alias,
    chained,
    public_name,
    implementation,
    resource,
    star_fixture,
): ...
"#,
            ),
        ])?;

        assert_eq!(fixture_names(&db, "test_use", "direct_alias"), ["resource"]);
        assert_eq!(fixture_names(&db, "test_use", "chained"), ["resource"]);
        assert_eq!(
            fixture_names(&db, "test_use", "public_name"),
            ["implementation"]
        );
        assert_eq!(
            fixture_names(&db, "test_use", "star_fixture"),
            ["star_fixture"]
        );
        assert!(fixture_names(&db, "test_use", "implementation").is_empty());
        assert!(fixture_names(&db, "test_use", "resource").is_empty());
        Ok(())
    }

    #[test]
    fn ignores_overwritten_imported_fixture_exposures() -> Result<()> {
        let db = pytest_db_with_files(&[
            (
                "/src/fixtures.py",
                r#"
import pytest

@pytest.fixture
def resource(): ...
"#,
            ),
            (
                "/src/test_example.py",
                r#"
from fixtures import resource

resource = object()

def test_use(resource): ...
"#,
            ),
        ])?;

        assert!(fixture_names(&db, "test_use", "resource").is_empty());
        Ok(())
    }

    #[test]
    fn preserves_conditional_imported_fixture_definitions() -> Result<()> {
        let db = pytest_db_with_files(&[
            (
                "/src/first.py",
                r#"
import pytest

@pytest.fixture
def first(): ...
"#,
            ),
            (
                "/src/second.py",
                r#"
import pytest

@pytest.fixture
def second(): ...
"#,
            ),
            (
                "/src/test_example.py",
                r#"
flag: bool

if flag:
    from first import first as resource
else:
    from second import second as resource

def test_use(resource): ...
"#,
            ),
        ])?;

        let mut fixtures = fixture_names(&db, "test_use", "resource");
        fixtures.sort();
        assert_eq!(fixtures, ["first", "second"]);
        Ok(())
    }

    fn fixture_names(db: &TestDb, function: &str, parameter: &str) -> Vec<String> {
        let parameter = parameter_definition(db, function, parameter);
        fixture_bindings_for_parameter(db, parameter)
            .iter()
            .map(|binding| {
                binding
                    .fixture()
                    .name(db)
                    .expect("fixture is a named function")
            })
            .collect()
    }

    fn fixture_provider_scopes(db: &TestDb, function: &str, parameter: &str) -> Vec<ScopeKind> {
        let parameter = parameter_definition(db, function, parameter);
        fixture_bindings_for_parameter(db, parameter)
            .iter()
            .map(|binding| binding.fixture().scope(db).scope(db).kind())
            .collect()
    }

    fn parameter_definition<'db>(
        db: &'db TestDb,
        function: &str,
        parameter: &str,
    ) -> Definition<'db> {
        let file = system_path_to_file(db, "/src/test_example.py")
            .or_else(|_| system_path_to_file(db, "/src/example.py"))
            .expect("test file exists");
        let file = db.program_file(file);
        let module = parsed_module(db, file.python_file(db)).load(db);
        let function = find_function(module.suite(), function).expect("function exists");
        let index = semantic_index(db, file);
        let parameter = function
            .parameters
            .iter()
            .find(|candidate| candidate.name().as_str() == parameter)
            .expect("parameter exists");
        match parameter {
            ast::AnyParameterRef::Variadic(parameter) => index.expect_single_definition(parameter),
            ast::AnyParameterRef::NonVariadic(parameter) => {
                index.expect_single_definition(parameter)
            }
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

    fn pytest_db(path: &'static str, source: &'static str) -> Result<TestDb> {
        pytest_db_with_files(&[(path, source)])
    }

    fn pytest_db_with_files(files: &[(&'static str, &'static str)]) -> Result<TestDb> {
        let mut builder = TestDbBuilder::new()
            .with_site_packages()
            .with_file("/site-packages/_pytest/__init__.pyi", "")
            .with_file(
                "/site-packages/_pytest/fixtures.pyi",
                r#"
from typing import Any, Callable

def fixture(
    function: Callable[..., Any] | None = ...,
    *,
    name: str | None = ...,
) -> Any: ...

def yield_fixture(
    function: Callable[..., Any] | None = ...,
    *,
    name: str | None = ...,
) -> Any: ...
"#,
            )
            .with_file(
                "/site-packages/pytest/__init__.pyi",
                r#"
from _pytest.fixtures import fixture as fixture, yield_fixture as yield_fixture

class MarkDecorator:
    def __call__(self, *args: object, **kwargs: object) -> object: ...

class MarkGenerator:
    parametrize: MarkDecorator

mark: MarkGenerator
"#,
            );
        for (path, source) in files {
            builder = builder.with_file(*path, source);
        }
        builder.build()
    }
}
