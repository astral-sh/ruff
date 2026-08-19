//! Models the semantic relationships that pytest creates between fixtures and parameters.
//!
//! Pytest injects fixture values by matching parameter names to fixture providers. Ordinary Python
//! name resolution does not represent that relationship: the parameter is a local definition, and
//! the fixture function may be defined in another provider scope. This module therefore overlays
//! the pytest relationship on top of the parameter's normal Python definition (which is preserved).
//!
//! The model distinguishes four concepts:
//!
//! - A [`FixtureDeclaration`] is a function decorated with pytest's canonical `fixture` or
//!   `yield_fixture` decorator.
//! - A [`FixtureExposure`] is the name under which a provider makes that declaration available.
//!   The decorator's `name` argument can make this differ from the Python binding name.
//! - A [`FixtureRequest`] is an eligible parameter in a collected test or another fixture function.
//! - A [`FixtureBinding`] links a request to the declaration selected by static fixture provider lookup.
//!
//! For example:
//!
//! ```py
//! import pytest
//!
//! @pytest.fixture(name="database")  # Exposure: the public fixture name is `database`.
//! def make_database():              # Declaration: this decorated function is the fixture identity.
//!     return object()
//!
//! # Request: the parameter asks for the fixture exposed as `database`.
//! # Binding: provider lookup connects the request to the `make_database` declaration.
//! def test_query(database):
//!     assert database is not None
//! ```
//!
//! [`fixture_bindings_for_parameter`] provides the public interface to the model. Given a parameter
//! definition, it classifies the parameter as a possible request, searches providers in pytest
//! precedence order, and returns every equally viable declaration in the first matching provider
//! layer. Language server and type-inference features can consume this data without changing
//! general definition, reference or rename behavior for the parameter.

use std::cmp::Ordering;

use itertools::Either;
use ruff_db::parsed::{ParsedModuleRef, parsed_module};
use ruff_python_ast::{self as ast, name::Name};
use rustc_hash::FxHashSet;
use ty_module_resolver::{KnownModule, file_to_module};
use ty_python_core::definition::{Definition, DefinitionKind, ParameterDefinitionNodeKind};
use ty_python_core::scope::{FileScopeId, ScopeId, ScopeKind};
use ty_python_core::{ProgramFile, global_scope, place_table, semantic_index, use_def_map};

use crate::Db;
use crate::types::function::{FunctionType, KnownFunction};
use crate::types::infer::{function_known_decorators, infer_definition_types, original_class_type};
use crate::types::signatures::Parameter as SignatureParameter;
use crate::types::{
    ClassBase, ClassLiteral, ProgramEnvironment, Type, definition_expression_type,
    extract_fixed_length_iterable_element_types,
};

/// Resolves pytest fixtures requested by `parameter`.
///
/// This function can be used to resolve either a fixture requested by a test
/// function (`consumer` in context "A" below) or a fixture requested by another
/// fixture (`dependency` in context "B" below):
///
/// ```py
/// import pytest
///
/// def test_consumer(consumer): ...  # A
///
/// @pytest.fixture
/// def dependency(): ...
///
/// @pytest.fixture
/// def consumer(dependency): ...  # B
/// ```
///
/// The resolution implemented here matches what pytest will actually resolve at
/// runtime for context A, but not necessarily for context B. To see why,
/// consider this example:
///
/// ```py
/// import pytest
///
/// def test_consumer(consumer): ...  # A
///
/// class TestOverride:
///     @pytest.fixture
///     def dependency(self): ...
///
///     def test_consumer(self, consumer): ...  # C
///
/// @pytest.fixture
/// def dependency(): ...
///
/// @pytest.fixture
/// def consumer(dependency): ...  # B
/// ```
///
/// There is no single correct answer for the resolution at B in this example.
/// Rather, it depends on the test that requests `consumer`: if it is requested
/// at A then the correct answer is the global `dependency` fixture, but if it
/// is requested at C then the correct answer is `TestOverride.dependency`. We
/// might eventually return all statically reachable definitions of a fixture
/// named `dependency`, but for now we just resolve both A and B with the same
/// approach (a search through lexical scopes). That always resolves that to the
/// global `dependency` fixture at B even though that result is incomplete.
#[salsa::tracked(returns(deref), heap_size=ruff_memory_usage::heap_size)]
pub fn fixture_bindings_for_parameter<'db>(
    db: &'db dyn Db,
    parameter: Definition<'db>,
) -> Box<[FixtureBinding<'db>]> {
    let Some(request) = FixtureRequest::from_parameter(db, parameter) else {
        return Box::default();
    };

    let containing_scope = request.function_definition.scope(db);
    let class_scope = containing_scope
        .node(db)
        .as_class()
        .map(|_| containing_scope.file_scope_id(db));
    if let Some(class_scope) = class_scope {
        let file = parameter.program_file(db);
        let index = semantic_index(db, file);
        for class_ref in std::iter::successors(Some(class_scope), |scope| {
            non_type_parameter_parent(index, *scope)
        })
        .map_while(|scope| index.scope(scope).node().as_class())
        {
            let class_definition = index.expect_single_definition(class_ref);
            let Some(class) = original_class_type(db, class_definition) else {
                return Box::default();
            };
            let bindings = bindings_in_provider(db, &request, FixtureProvider::Class(class));
            if !bindings.is_empty() {
                return bindings;
            }
        }
    }

    let module_scope = global_scope(db, parameter.program_file(db));
    bindings_in_provider(db, &request, FixtureProvider::Scope(module_scope))
}

/// A pytest fixture request and the declaration selected by static provider lookup.
#[derive(Debug, Clone, Copy, Eq, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub struct FixtureBinding<'db> {
    request: Definition<'db>,
    fixture: Definition<'db>,
}

impl<'db> FixtureBinding<'db> {
    /// Returns the parameter definition that requests the fixture.
    pub fn request(self) -> Definition<'db> {
        self.request
    }

    /// Returns the decorated function that declares the fixture.
    pub fn fixture(self) -> Definition<'db> {
        self.fixture
    }
}

/// An eligible fixture parameter and the context needed to resolve its request.
#[derive(Debug)]
struct FixtureRequest<'db> {
    parameter_definition: Definition<'db>,
    function_definition: Definition<'db>,
    name: Name,
}

impl<'db> FixtureRequest<'db> {
    fn from_parameter(db: &'db dyn Db, definition: Definition<'db>) -> Option<Self> {
        let DefinitionKind::Parameter(ParameterDefinitionNodeKind::Parameter(_)) =
            definition.kind(db)
        else {
            return None;
        };

        let file = definition.program_file(db);
        let index = semantic_index(db, file);
        let function_scope = definition.scope(db).file_scope_id(db);
        let function_ref = index.scope(function_scope).node().as_function()?;
        let function_definition = index.expect_single_definition(function_ref);
        let function_type =
            infer_definition_types(db, function_definition).function_type(function_definition)?;
        let signature_parameters = function_type.last_definition_signature(db).parameters();
        let parameter = signature_parameters
            .iter()
            .find(|parameter| parameter.definition() == Some(definition))?;
        let parameter_name = parameter.keyword_name()?;

        // Match pytest's logic for only injecting fixtures for required
        // parameters and by keyword:
        // https://docs.pytest.org/en/9.0.x/how-to/fixtures.html#requesting-fixtures
        // https://github.com/pytest-dev/pytest/blob/9.0.1/src/_pytest/compat.py#L145-L153
        if parameter.has_default() {
            return None;
        }

        let parent_scope = non_type_parameter_parent(index, function_scope)?;
        let parent_kind = index.scope(parent_scope).kind();
        if !matches!(parent_kind, ScopeKind::Module | ScopeKind::Class) {
            return None;
        }

        let class_scope = (parent_kind == ScopeKind::Class).then_some(parent_scope);
        if function_type.has_implicit_receiver(db)
            && signature_parameters
                .get_positional(0)
                .is_some_and(|parameter| parameter.definition() == Some(definition))
        {
            return None;
        }

        let module = parsed_module(db, file.python_file(db)).load(db);
        let function = function_ref.node(&module);
        if is_mock_patch_parameter(db, function_definition, function_type, function, definition) {
            return None;
        }

        // Check whether the fixture request itself appears in a fixture declaration e.g.
        //
        // ```py
        // @pytest.fixture
        // def database(): ...
        //
        // @pytest.fixture
        // def service(database): ...  # `database` is a fixture request.
        // ```
        let is_fixture_dependency = fixture_declaration(db, function_definition).is_some();
        if !is_fixture_dependency
            && (!is_collected_test(db, file, function, class_scope, &module)
                || class_scope
                    .is_some_and(|class_scope| is_unittest_test_case(db, file, class_scope)))
        {
            return None;
        }

        if !is_fixture_dependency
            && directly_parametrized(
                db,
                function_definition,
                function,
                class_scope,
                &module,
                index,
                parameter_name.as_str(),
            )
        {
            return None;
        }

        Some(Self {
            parameter_definition: definition,
            function_definition,
            name: parameter_name.clone(),
        })
    }
}

/// Returns whether `parameter` is supplied by `unittest.mock.patch`.
fn is_mock_patch_parameter<'db>(
    db: &'db dyn Db,
    function_definition: Definition<'db>,
    function_type: FunctionType<'db>,
    function: &ast::StmtFunctionDef,
    parameter_definition: Definition<'db>,
) -> bool {
    let decorators = function_known_decorators(db, function_definition);
    let patch_count = function
        .decorator_list
        .iter()
        .filter(|decorator| {
            let Some(call) = decorator.expression.as_call_expr() else {
                return false;
            };
            let new_position = if is_known_class_instance(
                db,
                function_definition,
                decorators.expression_type(&call.func),
                "_patcher",
                &[KnownModule::UnittestMock],
            ) {
                1
            } else if let Some(attribute) = call.func.as_attribute_expr()
                && attribute.attr.as_str() == "object"
                && is_known_class_instance(
                    db,
                    function_definition,
                    decorators.expression_type(&attribute.value),
                    "_patcher",
                    &[KnownModule::UnittestMock],
                )
            {
                2
            } else {
                return false;
            };

            call.arguments
                .find_argument_value("new", new_position)
                .is_none_or(|new| {
                    // Typeshed exposes `DEFAULT` as `Any`, so any dynamic value may enable
                    // positional injection.
                    matches!(decorators.expression_type(new), Some(Type::Dynamic(_)))
                })
        })
        .count();

    let signature = function_type.last_definition_signature(db);
    let parameters = signature.parameters();
    let is_source_keyword_parameter = |parameter: &SignatureParameter<'db>| {
        parameter.keyword_name().is_some()
            // ty applies PEP 484's legacy positional-only convention to leading `__name`
            // parameters, but Python and pytest still inspect them as positional-or-keyword.
            || (function.parameters.posonlyargs.is_empty() && parameter.is_positional_only())
    };
    let skips_receiver = function_type.has_implicit_receiver(db)
        && parameters
            .get_positional(0)
            .is_some_and(is_source_keyword_parameter);

    parameters
        .iter()
        .filter(|parameter| is_source_keyword_parameter(parameter) && !parameter.has_default())
        .skip(usize::from(skips_receiver))
        .take(patch_count)
        .any(|candidate| candidate.definition() == Some(parameter_definition))
}

/// A decorated fixture function.
#[derive(Debug, Clone, Eq, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
struct FixtureDeclaration<'db> {
    // The definition for the fixture function.
    definition: Definition<'db>,
    // The way in which the fixture exposes a name.
    name: FixtureName,
}

/// A fixture declaration made available under a name in a provider scope.
#[derive(Debug, Eq, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
struct FixtureExposure<'db> {
    name: Name,
    declaration: FixtureDeclaration<'db>,
}

impl<'db> FixtureExposure<'db> {
    /// Exposes a declaration under its explicit fixture name or local Python binding name.
    fn new(symbol_name: &Name, declaration: FixtureDeclaration<'db>) -> Option<Self> {
        let name = match &declaration.name {
            FixtureName::Default => symbol_name.clone(),
            FixtureName::Explicit(name) => name.clone(),
            FixtureName::Unknown => return None,
        };
        Some(Self { name, declaration })
    }
}

/// A name and any fixture exposures that it contributes to a provider scope.
///
/// Bound names in class scopes are retained even without fixture exposures because they can
/// shadow fixtures inherited from another class in the provider's MRO.
#[derive(Debug, Eq, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
struct FixtureProviderName<'db> {
    name: Name,
    exposures: Box<[FixtureExposure<'db>]>,
}

/// How a fixture decorator determines the fixture's public name.
#[derive(Debug, Clone, Eq, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
enum FixtureName {
    /// Uses the Python binding name at the exposure site.
    Default,
    /// Uses a statically known explicit name.
    Explicit(Name),
    /// Represents a public name that ty cannot determine statically, such as a
    /// dynamically-typed expression or a non-literal `str`.
    Unknown,
}

/// A fixture provider layer in which to resolve a request.
#[derive(Clone, Copy)]
enum FixtureProvider<'db> {
    /// Uses a class and its statically known ancestors.
    Class(ClassLiteral<'db>),
    /// Uses a single provider scope.
    Scope(ScopeId<'db>),
}

/// Returns the names that participate in fixture lookup for one provider scope.
///
/// Separating this summary from request resolution lets Salsa reuse scope-specific fixture
/// discovery across parameters.
#[salsa::tracked(returns(deref), heap_size=ruff_memory_usage::heap_size)]
fn fixture_provider_names<'db>(
    db: &'db dyn Db,
    scope: ScopeId<'db>,
) -> Box<[FixtureProviderName<'db>]> {
    let is_class_scope = scope.node(db).scope_kind() == ScopeKind::Class;
    let table = place_table(db, scope);
    use_def_map(db, scope)
        .all_end_of_scope_symbol_bindings()
        .filter_map(|(symbol_id, definitions)| {
            let symbol = table.symbol(symbol_id);
            let name = symbol.name().clone();
            let is_bound_class_attribute = is_class_scope && symbol.is_bound();
            let exposures: Box<[_]> = definitions
                .filter_map(|binding| binding.binding.definition())
                .filter_map(|definition| fixture_declaration(db, definition).clone())
                .filter_map(|declaration| FixtureExposure::new(&name, declaration))
                .collect();
            // Reject names that neither expose a fixture nor bind a class attribute that can
            // shadow an inherited fixture.
            if exposures.is_empty() && !is_bound_class_attribute {
                return None;
            }
            Some(FixtureProviderName { name, exposures })
        })
        .collect()
}

/// Resolves a request against the fixture exposures in `provider`.
fn bindings_in_provider<'db>(
    db: &'db dyn Db,
    request: &FixtureRequest<'db>,
    provider: FixtureProvider<'db>,
) -> Box<[FixtureBinding<'db>]> {
    let provider_scopes = match provider {
        FixtureProvider::Class(class) => Either::Left(
            class
                .iter_mro(db)
                .filter_map(ClassBase::into_class)
                .filter(|ancestor| !ancestor.is_object(db))
                .filter_map(|ancestor| ancestor.static_class_literal(db))
                .map(|(ancestor, _)| ancestor.body_scope(db)),
        ),
        FixtureProvider::Scope(scope) => Either::Right(std::iter::once(scope)),
    };

    let mut seen_names = FxHashSet::default();
    let mut winning_name: Option<Name> = None;
    let mut bindings = Vec::new();

    for provider_scope in provider_scopes {
        for provider_name in fixture_provider_names(db, provider_scope) {
            let symbol_name = &provider_name.name;
            // A name supplied by an earlier scope shadows the same name here.
            if !seen_names.insert(symbol_name.clone()) {
                continue;
            }

            for exposure in &provider_name.exposures {
                // Request must match public name of the fixture
                if request.name != exposure.name
                    // A fixture definition cannot fulfill a request for itself
                    || request.function_definition == exposure.declaration.definition
                {
                    continue;
                }

                // Semantic-index traversal is unordered. Pytest registers fixture attributes in
                // sorted `dir()` order and selects the last registration, so retain bindings for
                // the lexicographically last matching attribute. Thus, if `first_provider` and
                // `second_provider` both expose `resource`, `second_provider` wins.
                //
                // `dir()` ordering: https://docs.python.org/3/library/functions.html#dir
                // Fixture discovery: https://github.com/pytest-dev/pytest/blob/9.0.1/src/_pytest/fixtures.py#L1852-L1880
                // Registration order: https://github.com/pytest-dev/pytest/blob/9.0.1/src/_pytest/fixtures.py#L1788-L1797
                // Fixture selection: https://github.com/pytest-dev/pytest/blob/9.0.1/src/_pytest/fixtures.py#L583-L599
                match winning_name.as_ref().map(|winner| winner.cmp(symbol_name)) {
                    Some(Ordering::Greater) => continue,
                    Some(Ordering::Less) | None => {
                        winning_name = Some(symbol_name.clone());
                        bindings.clear();
                    }
                    Some(Ordering::Equal) => {}
                }
                bindings.push(FixtureBinding {
                    request: request.parameter_definition,
                    fixture: exposure.declaration.definition,
                });
            }
        }
    }

    bindings.into_boxed_slice()
}

/// Returns a fixture declaration for a function with a canonical pytest fixture decorator.
#[salsa::tracked(returns(ref), heap_size=ruff_memory_usage::heap_size)]
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
    let expression = &function.decorator_list.first()?.expression;
    let (callee, arguments) = match expression {
        ast::Expr::Call(call) => (call.func.as_ref(), Some(&call.arguments)),
        expression => (expression, None),
    };
    let Type::FunctionLiteral(decorator) = inference.expression_type(callee)? else {
        return None;
    };
    if !matches!(
        decorator.known(db),
        Some(KnownFunction::PytestFixture | KnownFunction::PytestYieldFixture)
    ) {
        return None;
    }

    let name = arguments.map_or(FixtureName::Default, |arguments| {
        fixture_name_from_arguments(db, arguments, &|expression| {
            inference.expression_type(expression)
        })
    });
    Some(FixtureDeclaration { definition, name })
}

/// Classifies the `name` argument to a fixture decorator.
fn fixture_name_from_arguments<'db>(
    db: &'db dyn Db,
    arguments: &ast::Arguments,
    expression_type: &impl Fn(&ast::Expr) -> Option<Type<'db>>,
) -> FixtureName {
    let Some(name_keyword) = arguments.find_keyword("name") else {
        return FixtureName::Default;
    };

    let Some(name_type) = expression_type(&name_keyword.value) else {
        return FixtureName::Unknown;
    };
    if name_type.is_none(db) {
        FixtureName::Default
    } else if let Some(string) = name_type.as_string_literal() {
        let name_keyword_value = string.value(db);
        if name_keyword_value.is_empty() {
            FixtureName::Default
        } else {
            FixtureName::Explicit(Name::new(name_keyword_value))
        }
    } else {
        FixtureName::Unknown
    }
}

/// Returns a scope's lexical parent, skipping an intervening type-parameter scope.
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

/// Returns whether a function matches pytest's default naming conventions for
/// [test discovery](https://docs.pytest.org/en/9.0.x/explanation/goodpractices.html#test-discovery).
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

    let index = semantic_index(db, file);
    let mut class_scope = class_scope;
    while let Some(scope) = class_scope {
        let Some(class_ref) = index.scope(scope).node().as_class() else {
            return false;
        };
        if !class_ref.node(module).name.as_str().starts_with("Test") {
            return false;
        }
        let Some(parent_scope) = non_type_parameter_parent(index, scope) else {
            return false;
        };
        match index.scope(parent_scope).kind() {
            ScopeKind::Class => class_scope = Some(parent_scope),
            ScopeKind::Module => class_scope = None,
            _ => return false,
        }
    }
    true
}

/// Returns whether the class inherits from the canonical `unittest.TestCase`.
///
/// Pytest does not inject fixture parameters into
/// [`unittest.TestCase` methods](https://docs.pytest.org/en/9.0.x/how-to/unittest.html#pytest-features-in-unittest-testcase-subclasses).
fn is_unittest_test_case(db: &dyn Db, file: ProgramFile<'_>, class_scope: FileScopeId) -> bool {
    let index = semantic_index(db, file);
    let class_ref = index.scope(class_scope).node().expect_class();
    let definition = index.expect_single_definition(class_ref);
    let Some(class) = original_class_type(db, definition) else {
        return false;
    };

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

/// Returns whether static parametrization on the function or an enclosing class prevents this
/// fixture request.
fn directly_parametrized<'db>(
    db: &'db dyn Db,
    function_definition: Definition<'db>,
    function: &ast::StmtFunctionDef,
    class_scope: Option<FileScopeId>,
    module: &ParsedModuleRef,
    index: &ty_python_core::SemanticIndex<'_>,
    parameter_name: &str,
) -> bool {
    let decorators = function_known_decorators(db, function_definition);
    if function.decorator_list.iter().any(|decorator| {
        mark_excludes_fixture(
            db,
            function_definition,
            &decorator.expression,
            parameter_name,
            |expression| decorators.expression_type(expression),
        )
    }) {
        return true;
    }

    std::iter::successors(class_scope, |class_scope| {
        let parent = non_type_parameter_parent(index, *class_scope)?;
        (index.scope(parent).kind() == ScopeKind::Class).then_some(parent)
    })
    .any(|class_scope| {
        let class_ref = index.scope(class_scope).node().expect_class();
        let definition = index.expect_single_definition(class_ref);
        class_ref
            .node(module)
            .decorator_list
            .iter()
            .any(|decorator| {
                mark_excludes_fixture(
                    db,
                    definition,
                    &decorator.expression,
                    parameter_name,
                    |expression| Some(definition_expression_type(db, definition, expression)),
                )
            })
    })
}

/// Returns whether a static mark supplies this parameter directly or cannot be interpreted.
fn mark_excludes_fixture<'db>(
    db: &'db dyn Db,
    definition: Definition<'db>,
    expression: &ast::Expr,
    parameter_name: &str,
    expression_type: impl Fn(&ast::Expr) -> Option<Type<'db>>,
) -> bool {
    let Some(call) = expression.as_call_expr() else {
        return false;
    };
    let Some(attribute) = call.func.as_attribute_expr() else {
        return false;
    };
    if attribute.attr.as_str() != "parametrize"
        || !is_known_class_instance(
            db,
            definition,
            expression_type(&attribute.value),
            "MarkGenerator",
            &[KnownModule::Pytest, KnownModule::PytestMarkStructures],
        )
    {
        return false;
    }

    let Some(names) = call
        .arguments
        .find_argument_value("argnames", 0)
        .and_then(|argnames| {
            statically_known_parametrize_names(db, definition, argnames, &expression_type)
        })
    else {
        return true;
    };
    if !names.contains(&parameter_name) {
        return false;
    }

    is_indirect(
        db,
        definition,
        &call.arguments,
        parameter_name,
        &expression_type,
    ) != Some(true)
}

/// Returns whether a type is an instance of `class_name` from one of `modules`.
fn is_known_class_instance(
    db: &dyn Db,
    definition: Definition<'_>,
    ty: Option<Type<'_>>,
    class_name: &str,
    modules: &[KnownModule],
) -> bool {
    let Some(Type::NominalInstance(instance)) = ty else {
        return false;
    };
    let environment = ProgramEnvironment::from_file(definition.program_file(db));
    let Some(class) = instance
        .class(db, &environment)
        .class_literal(db)
        .as_static()
    else {
        return false;
    };

    class.name(db) == class_name
        && file_to_module(db, class.program_file(db).resolver_file(db))
            .and_then(|module| module.known(db))
            .is_some_and(|module| modules.contains(&module))
}

/// Returns how `parameter_name` is configured by the `indirect` argument.
///
/// `Some(true)` means the parameter is definitely indirect, `Some(false)` means it is definitely
/// direct, and `None` preserves uncertainty when the argument cannot be interpreted statically.
fn is_indirect<'db>(
    db: &'db dyn Db,
    definition: Definition<'db>,
    arguments: &ast::Arguments,
    parameter_name: &str,
    expression_type: &impl Fn(&ast::Expr) -> Option<Type<'db>>,
) -> Option<bool> {
    let Some(expression) = arguments.find_argument_value("indirect", 2) else {
        return Some(false);
    };
    let ty = expression_type(expression)?;
    if ty == Type::bool_literal(true) {
        return Some(true);
    }
    if ty == Type::bool_literal(false) {
        return Some(false);
    }
    statically_known_parametrize_names(db, definition, expression, expression_type)
        .map(|names| names.contains(&parameter_name))
}

/// Returns statically known pytest parametrization names from a string or fixed-length iterable.
fn statically_known_parametrize_names<'db>(
    db: &'db dyn Db,
    definition: Definition<'db>,
    expression: &ast::Expr,
    expression_type: &impl Fn(&ast::Expr) -> Option<Type<'db>>,
) -> Option<Vec<&'db str>> {
    let ty = expression_type(expression)?;
    if let Some(string) = ty.as_string_literal() {
        return Some(
            string
                .value(db)
                .split(|character: char| character == ',' || character.is_whitespace())
                .filter(|name| !name.is_empty())
                .collect(),
        );
    }

    let environment = ProgramEnvironment::from_file(definition.program_file(db));
    extract_fixed_length_iterable_element_types(db, &environment, expression, |element| {
        expression_type(element).unwrap_or_else(Type::unknown)
    })?
    .iter()
    .map(|element| element.as_string_literal().map(|string| string.value(db)))
    .collect()
}

#[cfg(test)]
mod tests {
    use insta::assert_snapshot;
    use ruff_db::diagnostic::{
        Annotation, Diagnostic, DiagnosticId, DisplayDiagnosticConfig, DisplayDiagnostics,
        Severity, SubDiagnostic, SubDiagnosticSeverity,
    };
    use ruff_db::files::system_path_to_file;
    use ruff_db::parsed::parsed_module;
    use ruff_python_ast as ast;
    use ty_python_core::definition::Definition;
    use ty_python_core::semantic_index;

    use super::fixture_bindings_for_parameter;
    use crate::Db as _;
    use crate::db::tests::{TestDb, TestDbBuilder};

    #[test]
    fn resolves_same_file_fixture_declarations_and_dependencies() {
        let test = PytestTestCase::new(
            "/src/test_example.py",
            r#"
import pytest
from pytest import fixture as make_fixture, yield_fixture

@pytest.fixture
def database(): ...

@make_fixture()
@pytest.mark.parametrize("database", [1])
def service(database): ...

@yield_fixture()
def legacy_cache(): ...

def test_use(database, service, legacy_cache): ...

def wrapper(function): return lambda: function()

@wrapper
@pytest.fixture
def wrapped(): ...

def test_wrapped(wrapped): ...
"#,
        );

        let service = test.function("service");
        let test_use = test.function("test_use");
        let test_wrapped = test.function("test_wrapped");

        assert_snapshot!(service.fixture_resolution("database"), @"
        info[pytest-fixture]: Resolve fixture for parameter
          --> src/test_example.py:10:13
           |
        10 | def service(database): ...
           |             ^^^^^^^^ fixture requested here
        info: Found 1 fixture
         --> src/test_example.py:6:5
          |
        6 | def database(): ...
          |     --------
        ");

        assert_snapshot!(test_use.fixture_resolution("database"), @"
        info[pytest-fixture]: Resolve fixture for parameter
          --> src/test_example.py:15:14
           |
        15 | def test_use(database, service, legacy_cache): ...
           |              ^^^^^^^^ fixture requested here
        info: Found 1 fixture
         --> src/test_example.py:6:5
          |
        6 | def database(): ...
          |     --------
        ");

        assert_snapshot!(test_use.fixture_resolution("service"), @"
        info[pytest-fixture]: Resolve fixture for parameter
          --> src/test_example.py:15:24
           |
        15 | def test_use(database, service, legacy_cache): ...
           |                        ^^^^^^^ fixture requested here
        info: Found 1 fixture
          --> src/test_example.py:10:5
           |
        10 | def service(database): ...
           |     -------
        ");

        assert_snapshot!(test_use.fixture_resolution("legacy_cache"), @"
        info[pytest-fixture]: Resolve fixture for parameter
          --> src/test_example.py:15:33
           |
        15 | def test_use(database, service, legacy_cache): ...
           |                                 ^^^^^^^^^^^^ fixture requested here
        info: Found 1 fixture
          --> src/test_example.py:13:5
           |
        13 | def legacy_cache(): ...
           |     ------------
        ");

        assert_snapshot!(test_wrapped.fixture_resolution("wrapped"), @"No fixture resolved for parameter `wrapped`");
    }

    #[test]
    fn honors_static_names_and_ignores_dynamic_names() {
        let test = PytestTestCase::new(
            "/src/test_example.py",
            r#"
import pytest

def fixture_name() -> str: ...

@pytest.fixture(name="public_name")
def implementation(): ...

@pytest.fixture(name="public_" + "name")
def later_implementation(): ...

@pytest.fixture(name=fixture_name())
def dynamic_implementation(): ...

def test_use(public_name, implementation, dynamic): ...
"#,
        );

        let test_use = test.function("test_use");

        assert_snapshot!(test_use.fixture_resolution("public_name"), @"
        info[pytest-fixture]: Resolve fixture for parameter
          --> src/test_example.py:15:14
           |
        15 | def test_use(public_name, implementation, dynamic): ...
           |              ^^^^^^^^^^^ fixture requested here
        info: Found 1 fixture
          --> src/test_example.py:10:5
           |
        10 | def later_implementation(): ...
           |     --------------------
        ");

        assert_snapshot!(test_use.fixture_resolution("implementation"), @"No fixture resolved for parameter `implementation`");
        assert_snapshot!(test_use.fixture_resolution("dynamic"), @"No fixture resolved for parameter `dynamic`");
    }

    #[test]
    fn prefers_class_fixtures_and_skips_method_receivers() {
        let test = PytestTestCase::new(
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
        );

        let test_use = test.function("TestExample.test_use");
        let dependent = test.function("TestExample.dependent");

        assert_snapshot!(test_use.fixture_resolution("value"), @"
        info[pytest-fixture]: Resolve fixture for parameter
          --> src/test_example.py:14:24
           |
        14 |     def test_use(self, value, dependent): ...
           |                        ^^^^^ fixture requested here
        info: Found 1 fixture
         --> src/test_example.py:9:9
          |
        9 |     def value(self): ...
          |         -----
        ");

        assert_snapshot!(dependent.fixture_resolution("value"), @"
        info[pytest-fixture]: Resolve fixture for parameter
          --> src/test_example.py:12:25
           |
        12 |     def dependent(self, value): ...
           |                         ^^^^^ fixture requested here
        info: Found 1 fixture
         --> src/test_example.py:9:9
          |
        9 |     def value(self): ...
          |         -----
        ");

        assert_snapshot!(test_use.fixture_resolution("self"), @"No fixture resolved for parameter `self`");
    }

    #[test]
    fn uses_module_fixture_for_same_name_class_override() {
        let test = PytestTestCase::new(
            "/src/test_example.py",
            r#"
import pytest

@pytest.fixture
def value(): ...

class TestExample:
    @pytest.fixture
    def value(self, value): ...
"#,
        );

        let class_fixture = test.function("TestExample.value");

        assert_snapshot!(class_fixture.fixture_resolution("value"), @"
        info[pytest-fixture]: Resolve fixture for parameter
         --> src/test_example.py:9:21
          |
        9 |     def value(self, value): ...
          |                     ^^^^^ fixture requested here
        info: Found 1 fixture
         --> src/test_example.py:5:5
          |
        5 | def value(): ...
          |     -----
        ");
    }

    #[test]
    fn uses_lexical_context_for_fixture_dependencies() {
        let test = PytestTestCase::new(
            "/src/test_example.py",
            r#"
import pytest

@pytest.fixture
def dependency(): ...

@pytest.fixture
def consumer(dependency): ...

class TestExample:
    @pytest.fixture
    def dependency(self): ...

    def test_use(self, consumer): ...
"#,
        );

        let consumer = test.function("consumer");

        assert_snapshot!(consumer.fixture_resolution("dependency"), @"
        info[pytest-fixture]: Resolve fixture for parameter
         --> src/test_example.py:8:14
          |
        8 | def consumer(dependency): ...
          |              ^^^^^^^^^^ fixture requested here
        info: Found 1 fixture
         --> src/test_example.py:5:5
          |
        5 | def dependency(): ...
          |     ----------
        ");
    }

    #[test]
    fn resolves_fixtures_in_test_class_bases() {
        let test = PytestTestCase::new(
            "/src/test_example.py",
            r#"
import pytest

class Base:
    @pytest.fixture
    def inherited(self): ...

class TestExample(Base):
    def test_use(self, inherited): ...

class TestShadowed(Base):
    inherited = None
    def test_use(self, inherited): ...

class TestAnnotated(Base):
    inherited: object
    def test_use(self, inherited): ...
"#,
        );

        let test_use = test.function("TestExample.test_use");
        let shadowed = test.function("TestShadowed.test_use");
        let annotated = test.function("TestAnnotated.test_use");

        assert_snapshot!(test_use.fixture_resolution("inherited"), @"
        info[pytest-fixture]: Resolve fixture for parameter
         --> src/test_example.py:9:24
          |
        9 |     def test_use(self, inherited): ...
          |                        ^^^^^^^^^ fixture requested here
        info: Found 1 fixture
         --> src/test_example.py:6:9
          |
        6 |     def inherited(self): ...
          |         ---------
        ");

        assert_snapshot!(shadowed.fixture_resolution("inherited"), @"No fixture resolved for parameter `inherited`");

        assert_snapshot!(annotated.fixture_resolution("inherited"), @"
        info[pytest-fixture]: Resolve fixture for parameter
          --> src/test_example.py:17:24
           |
        17 |     def test_use(self, inherited): ...
           |                        ^^^^^^^^^ fixture requested here
        info: Found 1 fixture
         --> src/test_example.py:6:9
          |
        6 |     def inherited(self): ...
          |         ---------
        ");
    }

    #[test]
    fn follows_test_class_mro() {
        let test = PytestTestCase::new(
            "/src/test_example.py",
            r#"
import pytest

class First:
    @pytest.fixture(name="resource")
    def first_provider(self): ...

class Second:
    @pytest.fixture(name="resource")
    def second_provider(self): ...

class TestExample(First, Second):
    def test_use(self, resource): ...
"#,
        );

        let test_use = test.function("TestExample.test_use");

        assert_snapshot!(test_use.fixture_resolution("resource"), @"
        info[pytest-fixture]: Resolve fixture for parameter
          --> src/test_example.py:13:24
           |
        13 |     def test_use(self, resource): ...
           |                        ^^^^^^^^ fixture requested here
        info: Found 1 fixture
          --> src/test_example.py:10:9
           |
        10 |     def second_provider(self): ...
           |         ---------------
        ");
    }

    #[test]
    fn classifies_only_supported_fixture_requests() {
        let test = PytestTestCase::new(
            "/src/test_example.py",
            r#"
import pytest

@pytest.fixture
def value(): ...

def helper(value): ...

def test_defaults(positional_only, /, value=None, *args, **kwargs): ...

class Example:
    def test_method(value): ...
"#,
        );

        let helper = test.function("helper");
        let test_defaults = test.function("test_defaults");
        let example_method = test.function("Example.test_method");

        assert_snapshot!(helper.fixture_resolution("value"), @"No fixture resolved for parameter `value`");
        assert_snapshot!(test_defaults.fixture_resolution("positional_only"), @"No fixture resolved for parameter `positional_only`");
        assert_snapshot!(test_defaults.fixture_resolution("value"), @"No fixture resolved for parameter `value`");
        assert_snapshot!(test_defaults.fixture_resolution("args"), @"No fixture resolved for parameter `args`");
        assert_snapshot!(test_defaults.fixture_resolution("kwargs"), @"No fixture resolved for parameter `kwargs`");
        assert_snapshot!(example_method.fixture_resolution("value"), @"No fixture resolved for parameter `value`");
    }

    #[test]
    fn excludes_mock_patch_and_unittest_parameters() {
        let test = PytestTestCase::new(
            "/src/test_example.py",
            r#"
import unittest
from unittest import mock

import pytest

@pytest.fixture
def patched(): ...

@pytest.fixture
def value(): ...

@mock.patch("module.target")
def test_patched(patched, value): ...

class TestUnit(unittest.TestCase):
    def test_method(self, value): ...

@mock.patch.multiple("module", value=mock.DEFAULT)
def test_patch_multiple(value): ...

@mock.patch("module.target")
def test_legacy_patch(__patched, value): ...

@mock.patch.object(object, "attribute")
def test_patch_object(patched, value): ...

@mock.patch("module.target", new=mock.DEFAULT)
def test_explicit_default(patched, value): ...
"#,
        );

        let patched = test.function("test_patched");
        let unittest_method = test.function("TestUnit.test_method");
        let patch_multiple = test.function("test_patch_multiple");
        let legacy_patch = test.function("test_legacy_patch");
        let patch_object = test.function("test_patch_object");
        let explicit_default = test.function("test_explicit_default");

        assert_snapshot!(patched.fixture_resolution("patched"), @"No fixture resolved for parameter `patched`");
        assert_snapshot!(patch_object.fixture_resolution("patched"), @"No fixture resolved for parameter `patched`");
        assert_snapshot!(explicit_default.fixture_resolution("patched"), @"No fixture resolved for parameter `patched`");

        assert_snapshot!(patched.fixture_resolution("value"), @"
        info[pytest-fixture]: Resolve fixture for parameter
          --> src/test_example.py:14:27
           |
        14 | def test_patched(patched, value): ...
           |                           ^^^^^ fixture requested here
        info: Found 1 fixture
          --> src/test_example.py:11:5
           |
        11 | def value(): ...
           |     -----
        ");

        assert_snapshot!(unittest_method.fixture_resolution("value"), @"No fixture resolved for parameter `value`");

        assert_snapshot!(patch_multiple.fixture_resolution("value"), @"
        info[pytest-fixture]: Resolve fixture for parameter
          --> src/test_example.py:20:25
           |
        20 | def test_patch_multiple(value): ...
           |                         ^^^^^ fixture requested here
        info: Found 1 fixture
          --> src/test_example.py:11:5
           |
        11 | def value(): ...
           |     -----
        ");

        assert_snapshot!(legacy_patch.fixture_resolution("value"), @"
        info[pytest-fixture]: Resolve fixture for parameter
          --> src/test_example.py:23:34
           |
        23 | def test_legacy_patch(__patched, value): ...
           |                                  ^^^^^ fixture requested here
        info: Found 1 fixture
          --> src/test_example.py:11:5
           |
        11 | def value(): ...
           |     -----
        ");
    }

    #[test]
    fn resolves_fixtures_for_nested_test_classes() {
        let test = PytestTestCase::new(
            "/src/test_example.py",
            r#"
import pytest

@pytest.fixture
def value(): ...

class TestOuter:
    @pytest.fixture
    def outer(self): ...

    class TestInner:
        def test_method(self, value, outer): ...
"#,
        );

        let nested_method = test.function("TestOuter.TestInner.test_method");

        assert_snapshot!(nested_method.fixture_resolution("value"), @"
        info[pytest-fixture]: Resolve fixture for parameter
          --> src/test_example.py:12:31
           |
        12 |         def test_method(self, value, outer): ...
           |                               ^^^^^ fixture requested here
        info: Found 1 fixture
         --> src/test_example.py:5:5
          |
        5 | def value(): ...
          |     -----
        ");

        assert_snapshot!(nested_method.fixture_resolution("outer"), @"
        info[pytest-fixture]: Resolve fixture for parameter
          --> src/test_example.py:12:38
           |
        12 |         def test_method(self, value, outer): ...
           |                                      ^^^^^ fixture requested here
        info: Found 1 fixture
         --> src/test_example.py:9:9
          |
        9 |     def outer(self): ...
          |         -----
        ");
    }

    #[test]
    fn resolves_fixture_after_positional_only_method_receiver() {
        let test = PytestTestCase::new(
            "/src/test_example.py",
            r#"
import pytest

@pytest.fixture
def value(): ...

class TestExample:
    def test_method(self, /, value): ...
"#,
        );

        let test_method = test.function("TestExample.test_method");

        assert_snapshot!(test_method.fixture_resolution("value"), @"
        info[pytest-fixture]: Resolve fixture for parameter
         --> src/test_example.py:8:30
          |
        8 |     def test_method(self, /, value): ...
          |                              ^^^^^ fixture requested here
        info: Found 1 fixture
         --> src/test_example.py:5:5
          |
        5 | def value(): ...
          |     -----
        ");
    }

    #[test]
    fn excludes_direct_parameters_and_keeps_indirect_parameters() {
        let test = PytestTestCase::new(
            "/src/test_example.py",
            r#"
import pytest
from pytest import mark as aliased_mark

@pytest.fixture
def value(): ...

@pytest.fixture
def other(): ...

@pytest.mark.parametrize("value", [1])
def test_direct(value): ...

@pytest.mark.parametrize("value", [1], True)
def test_indirect(value): ...

@pytest.mark.parametrize("value, other", [(1, 2)], indirect=["value"])
def test_mixed(value, other): ...

@aliased_mark.parametrize("value", [1])
def test_aliased_direct(value): ...

@aliased_mark.parametrize("value", [1], indirect=True)
def test_aliased_indirect(value): ...

@pytest.mark.parametrize("value", [1])
class TestParametrized:
    def test_value(self, value): ...

@pytest.mark.parametrize("value", [1])
class TestOuter:
    class TestInner:
        def test_value(self, value): ...
"#,
        );

        let test_direct = test.function("test_direct");
        let test_indirect = test.function("test_indirect");
        let test_mixed = test.function("test_mixed");
        let test_aliased_direct = test.function("test_aliased_direct");
        let test_aliased_indirect = test.function("test_aliased_indirect");
        let test_class_parametrized = test.function("TestParametrized.test_value");
        let test_outer_class_parametrized = test.function("TestOuter.TestInner.test_value");

        assert_snapshot!(test_direct.fixture_resolution("value"), @"No fixture resolved for parameter `value`");

        assert_snapshot!(test_indirect.fixture_resolution("value"), @"
        info[pytest-fixture]: Resolve fixture for parameter
          --> src/test_example.py:15:19
           |
        15 | def test_indirect(value): ...
           |                   ^^^^^ fixture requested here
        info: Found 1 fixture
         --> src/test_example.py:6:5
          |
        6 | def value(): ...
          |     -----
        ");

        assert_snapshot!(test_mixed.fixture_resolution("value"), @"
        info[pytest-fixture]: Resolve fixture for parameter
          --> src/test_example.py:18:16
           |
        18 | def test_mixed(value, other): ...
           |                ^^^^^ fixture requested here
        info: Found 1 fixture
         --> src/test_example.py:6:5
          |
        6 | def value(): ...
          |     -----
        ");

        assert_snapshot!(test_mixed.fixture_resolution("other"), @"No fixture resolved for parameter `other`");
        assert_snapshot!(test_aliased_direct.fixture_resolution("value"), @"No fixture resolved for parameter `value`");

        assert_snapshot!(test_aliased_indirect.fixture_resolution("value"), @"
        info[pytest-fixture]: Resolve fixture for parameter
          --> src/test_example.py:24:27
           |
        24 | def test_aliased_indirect(value): ...
           |                           ^^^^^ fixture requested here
        info: Found 1 fixture
         --> src/test_example.py:6:5
          |
        6 | def value(): ...
          |     -----
        ");

        assert_snapshot!(test_class_parametrized.fixture_resolution("value"), @"No fixture resolved for parameter `value`");
        assert_snapshot!(test_outer_class_parametrized.fixture_resolution("value"), @"No fixture resolved for parameter `value`");
    }

    #[test]
    fn requires_a_default_pytest_test_module_name() {
        let test = PytestTestCase::new(
            "/src/example.py",
            r#"
import pytest

@pytest.fixture
def value(): ...

def test_use(value): ...
"#,
        );

        let test_use = test.function("test_use");

        assert_snapshot!(test_use.fixture_resolution("value"), @"No fixture resolved for parameter `value`");
    }

    struct PytestTestCase {
        db: TestDb,
        path: &'static str,
    }

    impl PytestTestCase {
        fn new(path: &'static str, source: &'static str) -> Self {
            Self {
                db: pytest_db(path, source),
                path,
            }
        }

        fn function<'test>(&'test self, name: &str) -> PytestTestFunction<'test> {
            PytestTestFunction {
                test: self,
                name: name.to_owned(),
            }
        }
    }

    struct PytestTestFunction<'test> {
        test: &'test PytestTestCase,
        name: String,
    }

    impl PytestTestFunction<'_> {
        fn fixture_resolution(&self, parameter_name: &str) -> String {
            let db = &self.test.db;
            let parameter = self.parameter_definition(parameter_name);
            let fixtures = fixture_bindings_for_parameter(db, parameter);
            if fixtures.is_empty() {
                return format!("No fixture resolved for parameter `{parameter_name}`");
            }

            let parameter_module = parsed_module(db, parameter.python_file(db)).load(db);
            let mut diagnostic = Diagnostic::new(
                DiagnosticId::lint("pytest-fixture"),
                Severity::Info,
                "Resolve fixture for parameter",
            );
            diagnostic.annotate(
                Annotation::primary(parameter.focus_range(db, &parameter_module).into())
                    .message("fixture requested here"),
            );

            let mut resolved = SubDiagnostic::new(
                SubDiagnosticSeverity::Info,
                format_args!(
                    "Found {} fixture{}",
                    fixtures.len(),
                    if fixtures.len() == 1 { "" } else { "s" }
                ),
            );
            for binding in fixtures {
                let fixture = binding.fixture();
                let module = parsed_module(db, fixture.python_file(db)).load(db);
                resolved.annotate(Annotation::secondary(
                    fixture.focus_range(db, &module).into(),
                ));
            }
            diagnostic.sub(resolved);

            DisplayDiagnostics::new(
                db,
                &DisplayDiagnosticConfig::new("ty").context(0),
                &[diagnostic],
            )
            .to_string()
            .replace('\\', "/")
        }

        fn parameter_definition<'db>(&'db self, parameter_name: &str) -> Definition<'db> {
            let db = &self.test.db;
            let file = system_path_to_file(db, self.test.path).expect("test file exists");
            let file = db.program_file(file);
            let module = parsed_module(db, file.python_file(db)).load(db);
            let function = find_function(module.suite(), &self.name).expect("test function exists");
            let index = semantic_index(db, file);
            let parameter = function
                .parameters
                .iter()
                .find(|candidate| candidate.name().as_str() == parameter_name)
                .expect("test parameter exists");
            match parameter {
                ast::AnyParameterRef::Variadic(parameter) => {
                    index.expect_single_definition(parameter)
                }
                ast::AnyParameterRef::NonVariadic(parameter) => {
                    index.expect_single_definition(parameter)
                }
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

    fn pytest_db(path: &'static str, source: &'static str) -> TestDb {
        TestDbBuilder::new()
            .with_third_party_packages()
            .with_file(
                "/.venv/lib/python3.13/site-packages/_pytest/__init__.pyi",
                "",
            )
            .with_file(
                "/.venv/lib/python3.13/site-packages/_pytest/mark/__init__.pyi",
                "",
            )
            .with_file(
                "/.venv/lib/python3.13/site-packages/_pytest/mark/structures.pyi",
                r#"
class MarkDecorator:
    def __call__(self, *args: object, **kwargs: object) -> object: ...

class MarkGenerator:
    parametrize: MarkDecorator
"#,
            )
            .with_file(
                "/.venv/lib/python3.13/site-packages/_pytest/fixtures.pyi",
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
                "/.venv/lib/python3.13/site-packages/pytest/__init__.pyi",
                r#"
from _pytest.fixtures import fixture as fixture, yield_fixture as yield_fixture
from _pytest.mark.structures import MarkGenerator

mark: MarkGenerator
"#,
            )
            .with_file(path, source)
            .build()
            .expect("valid pytest test database")
    }
}
