//! Models the semantic relationships that pytest creates between fixtures and parameters.
//!
//! Pytest injects fixture values by matching parameter names to fixtures available in a particular
//! search scope. Ordinary Python name resolution does not represent that relationship: the
//! parameter is a local definition, and the fixture function may be defined in another scope.
//! This module therefore overlays the pytest relationship on top of the parameter's normal
//! Python definition (which is preserved).
//!
//! The model distinguishes four concepts:
//!
//! - A [`FixtureDeclaration`] is a function decorated with pytest's canonical `fixture` or
//!   `yield_fixture` decorator.
//! - A [`FixtureExposure`] is the name under which that declaration is available during fixture
//!   lookup. The decorator's `name` argument can make this differ from the Python binding name.
//! - A [`FixtureRequest`] is an eligible parameter in a collected test or another fixture function.
//! - A [`FixtureBinding`] links a request to the selected declaration and the exposures through
//!   which it was found.
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
//! # Binding: fixture lookup connects the request to the `make_database` declaration.
//! def test_query(database):
//!     assert database is not None
//! ```
//!
//! We provide two entry points to this model:
//!
//! - [`fixture_bindings_for_parameter`]: Given a parameter definition, it classifies the parameter as
//!   a possible request, inspects fixture search scopes in pytest precedence order, and returns every
//!   equally viable declaration in the first matching scope. Language server and type-inference
//!   features can consume this data without changing general definition, reference or rename behavior
//!   for the parameter.
//! - [`fixture_exposures_for_definition`]: Given a definition, it returns the fixture exposures made
//!   available by that definition, including those reached through imports. Each exposure records the
//!   fixture's public name, canonical declaration, and local and source bindings.

use std::cmp::Ordering;

use itertools::Either;
use ruff_db::files::FileRange;
use ruff_db::files::system_path_to_file;
use ruff_db::parsed::{ParsedModuleRef, parsed_module};
use ruff_python_ast::{self as ast, name::Name};
use ruff_text_size::TextRange;
use rustc_hash::FxHashSet;
use ty_module_resolver::{
    ImportingFile, KnownModule, ModuleName, file_to_module, resolve_module_for_import_from,
    resolve_real_module_confident, stub_file_to_real_module,
};
use ty_python_core::ast_node_ref::AstNodeRef;
use ty_python_core::definition::{Definition, DefinitionKind, ParameterDefinitionNodeKind};
use ty_python_core::scope::{FileScopeId, ScopeId, ScopeKind};
use ty_python_core::{
    Program, ProgramFile, global_scope, place_table, semantic_index, use_def_map,
};

use super::collection::{PytestTestKind, pytest_test_for_binding};
use super::is_available_definition;
use crate::lexical_name_path::lexical_name_path_for_definition;
use crate::place::definitions::DefinitionResolution;
use crate::types::function::{FunctionType, KnownFunction};
use crate::types::infer::{function_known_decorators, infer_definition_types, original_class_type};
use crate::types::signatures::Parameter as SignatureParameter;
use crate::types::{
    ClassBase, ClassLiteral, KnownClass, ProgramEnvironment, Type, definition_expression_type,
    extract_fixed_length_iterable_element_types, may_exist_at_runtime,
};
use crate::{Db, FxIndexMap, FxIndexSet};

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
/// At present, we search the parameter's class hierarchy, module, enclosing
/// conftest hierarchy, and installed core pytest plugins. Fixtures from other
/// plugins are not yet supported.
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
    let Some(request) = fixture_request_for_parameter(db, parameter) else {
        return Box::default();
    };

    // pytest creates the special `request` fixture on demand, so there is no decorated fixture
    // declaration to return as a `FixtureBinding`.
    // https://docs.pytest.org/en/stable/reference/reference.html#request
    if request.name == "request" {
        return Box::default();
    }

    // First, resolve bindings from the containing class scope, if any.
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
            let bindings = bindings_in_search_scope(db, &request, FixtureSearchScope::Class(class));
            if !bindings.is_empty() {
                return bindings;
            }
        }
    }

    // Second, resolve bindings from the module scope.
    let request_file = parameter.program_file(db);
    let bindings = bindings_in_search_scope(
        db,
        &request,
        FixtureSearchScope::Scope(global_scope(db, request_file)),
    );
    if !bindings.is_empty() {
        return bindings;
    }

    // Third, resolve bindings from the conftest hierarchy.
    for conftest in conftest_files(db, request_file) {
        let bindings = bindings_in_search_scope(
            db,
            &request,
            FixtureSearchScope::Scope(global_scope(db, conftest)),
        );
        if !bindings.is_empty() {
            return bindings;
        }
    }

    // Finally, search installed core plugins in reverse registration order. The legacy
    // temporary-directory plugin is registered after the static core plugins, so it comes first.
    if let Some(plugin) = pytest_legacy_tmpdir_plugin(db, request_file.program(db)) {
        let bindings = bindings_in_search_scope(db, &request, FixtureSearchScope::Class(plugin));
        if !bindings.is_empty() {
            return bindings;
        }
    }
    for plugin in pytest_global_plugin_files(db, request_file.program(db))
        .iter()
        .rev()
    {
        let bindings = bindings_in_search_scope(
            db,
            &request,
            FixtureSearchScope::Scope(global_scope(db, *plugin)),
        );
        if !bindings.is_empty() {
            return bindings;
        }
    }

    Box::default()
}

/// A pytest fixture request and the fixture selected by static fixture lookup.
#[derive(Debug, Eq, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub struct FixtureBinding<'db> {
    request: Definition<'db>,
    fixture: Definition<'db>,
    exposures: Box<[FixtureExposure<'db>]>,
}

impl<'db> FixtureBinding<'db> {
    /// Returns the decorated function that declares the fixture.
    pub fn fixture(&self) -> Definition<'db> {
        self.fixture
    }

    /// Returns the equally viable exposures through which the request reaches the fixture.
    pub fn exposures(&self) -> &[FixtureExposure<'db>] {
        &self.exposures
    }
}

/// Returns the available pytest fixture exposures contributed by `definition`.
///
/// A decorated function contributes an exposure directly:
///
/// ```python
/// # fixtures.py
/// import pytest
///
/// @pytest.fixture
/// def resource(): ...
/// ```
///
/// Querying the definition of `resource` returns one exposure, schematically:
///
/// ```text
/// FixtureExposure {
///     name: "resource",
///     local_binding: Definition(fixtures.resource),
///     fixture: Definition(fixtures.resource),
///     source_binding: None,
/// }
/// ```
///
/// An import contributes the exposures reachable through that import:
///
/// ```python
/// # fixtures.py
/// import pytest
///
/// @pytest.fixture
/// def resource(): ...
///
/// # plugin.py
/// from fixtures import resource as helper
/// ```
///
/// Querying the import definition of `helper` returns:
///
/// ```text
/// FixtureExposure {
///     name: "helper",
///     local_binding: Definition(plugin.helper),
///     fixture: Definition(fixtures.resource),
///     source_binding: Some(Definition(fixtures.resource)),
/// }
/// ```
pub fn fixture_exposures_for_definition<'db>(
    db: &'db dyn Db,
    definition: Definition<'db>,
) -> Vec<FixtureExposure<'db>> {
    let index = semantic_index(db, definition.program_file(db));
    let definition_scope = definition.file_scope(db);
    if !is_available_fixture_search_scope(db, index, definition_scope)
        || !is_available_definition(db, definition)
    {
        return Vec::new();
    }

    let Some(symbol) = definition.place(db).as_symbol() else {
        return Vec::new();
    };
    let name = place_table(db, definition.scope(db)).symbol(symbol).name();

    exposures_contributed_by_definition(db, definition, name)
}

/// Returns the installed core pytest plugin files in registration order.
pub fn pytest_global_plugin_files<'db>(
    db: &'db dyn Db,
    program: Program<'db>,
) -> &'db [ProgramFile<'db>] {
    let Some(config_module) = resolve_real_module_confident(
        db,
        program.resolver_environment(db),
        &KnownModule::PytestConfig.name(),
    ) else {
        return &[];
    };
    if !config_module.is_known(db, KnownModule::PytestConfig) {
        return &[];
    }
    let Some(config_file) = config_module.file(db) else {
        return &[];
    };

    pytest_global_plugin_files_from_config(db, program.program_file(db, config_file))
}

/// Reads the installed core pytest plugin files from the resolved configuration module.
#[salsa::tracked(returns(deref), heap_size=ruff_memory_usage::heap_size)]
fn pytest_global_plugin_files_from_config<'db>(
    db: &'db dyn Db,
    config_file: ProgramFile<'db>,
) -> Box<[ProgramFile<'db>]> {
    let Some(plugin_names) =
        static_string_sequence_for_module_symbol(db, config_file, "default_plugins")
    else {
        return Box::default();
    };

    let Some(config_module) = file_to_module(db, config_file.resolver_file(db)) else {
        return Box::default();
    };

    let config_search_path = config_module.search_path(db);
    let resolver_environment = config_file.resolver_environment(db);
    let mut seen = FxHashSet::default();
    let mut plugins = Vec::new();

    for plugin_name in plugin_names {
        let qualified_name = if plugin_name.starts_with("_pytest.") {
            plugin_name
        } else {
            format!("_pytest.{plugin_name}")
        };
        if !seen.insert(qualified_name.clone()) {
            continue;
        }
        let Some(module_name) = ModuleName::new(&qualified_name) else {
            tracing::debug!(
                plugin_name = qualified_name,
                "Skipping invalid pytest core plugin name"
            );
            continue;
        };
        let Some(module) = resolve_real_module_confident(db, resolver_environment, &module_name)
        else {
            continue;
        };
        if module.search_path(db) != config_search_path {
            continue;
        }
        if let Some(file) = module.file(db) {
            plugins.push(ProgramFile::new(db, file, config_file.program(db)));
        }
    }

    plugins.into_boxed_slice()
}

/// Returns pytest's dynamically registered legacy temporary-directory plugin class.
#[salsa::tracked(returns(copy), heap_size=ruff_memory_usage::heap_size)]
fn pytest_legacy_tmpdir_plugin<'db>(
    db: &'db dyn Db,
    program: Program<'db>,
) -> Option<ClassLiteral<'db>> {
    let mut legacypath_file = None;
    let mut has_tmpdir = false;

    for file in pytest_global_plugin_files(db, program) {
        let module = file_to_module(db, file.resolver_file(db))?;
        match module.name(db).as_str() {
            "_pytest.legacypath" => legacypath_file = Some(*file),
            "_pytest.tmpdir" => has_tmpdir = true,
            _ => {}
        }
    }

    // pytest registers this class during `pytest_configure` only when both plugins are active.
    // https://github.com/pytest-dev/pytest/blob/9.1.1/src/_pytest/legacypath.py#L439-L459
    if has_tmpdir && let Some(file) = legacypath_file {
        original_class_type(db, end_of_scope_definition(db, file, "LegacyTmpdirPlugin")?)
    } else {
        None
    }
}

/// An eligible fixture parameter and the context needed to resolve its request.
#[derive(Debug)]
struct FixtureRequest<'db> {
    parameter_definition: Definition<'db>,
    function_definition: Definition<'db>,
    name: Name,
}

/// Function-level information shared by all possible fixture requests in a signature.
struct FixtureRequestContext<'db, 'ast> {
    function_definition: Definition<'db>,
    function_type: FunctionType<'db>,
    function: &'ast ast::StmtFunctionDef,
    module: &'ast ParsedModuleRef,
    index: &'db ty_python_core::SemanticIndex<'db>,
    class_scope: Option<FileScopeId>,
    is_fixture_dependency: bool,
    mock_patch_count: usize,
}

impl<'db, 'ast> FixtureRequestContext<'db, 'ast> {
    /// Constructs a new context representing a function that can receive fixtures
    /// through one or more parameters.
    fn new(
        db: &'db dyn Db,
        function_ref: &'db AstNodeRef<ast::StmtFunctionDef>,
        class_scope: Option<FileScopeId>,
        module: &'ast ParsedModuleRef,
        index: &'db ty_python_core::SemanticIndex<'db>,
    ) -> Option<Self> {
        let function_definition = index.expect_single_definition(function_ref);
        let function = function_ref.node(module);

        // Parameters on fixture declarations request their values from other fixtures:
        //
        // ```py
        // @pytest.fixture
        // def database(): ...
        //
        // @pytest.fixture
        // def service(database): ...  # `database` is a fixture request.
        // ```
        let is_fixture_dependency = !function.decorator_list.is_empty()
            && fixture_declaration(db, function_definition).is_some();

        // Pytest collects `unittest.TestCase` methods but does not inject fixtures into them.
        // https://docs.pytest.org/en/9.0.x/how-to/unittest.html#pytest-features-in-unittest-testcase-subclasses
        if !is_fixture_dependency
            && pytest_test_for_binding(db, function_definition)
                .is_none_or(|test| test.kind() != PytestTestKind::Pytest)
        {
            return None;
        }

        let function_type =
            infer_definition_types(db, function_definition).function_type(function_definition)?;

        Some(Self {
            function_definition,
            function_type,
            function,
            module,
            index,
            class_scope,
            is_fixture_dependency,
            mock_patch_count: mock_patch_count(db, function_definition, function),
        })
    }

    /// Classifies the nearest non-type-parameter parent of a fixture-request function.
    fn parent_scope(
        index: &ty_python_core::SemanticIndex<'db>,
        function_scope: FileScopeId,
    ) -> Option<FixtureRequestParentScope> {
        let parent_scope = non_type_parameter_parent(index, function_scope)?;
        match index.scope(parent_scope).kind() {
            ScopeKind::Module => Some(FixtureRequestParentScope::Module),
            ScopeKind::Class => Some(FixtureRequestParentScope::Class(parent_scope)),
            _ => None,
        }
    }

    /// Returns the fixture request represented by `definition`, if it is eligible for injection.
    fn fixture_request_for_parameter(
        &self,
        db: &'db dyn Db,
        definition: Definition<'db>,
    ) -> Option<FixtureRequest<'db>> {
        let signature_parameters = self
            .function_type
            .last_definition_signature(db)
            .parameters();
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

        if self.function_type.has_implicit_receiver(db)
            && signature_parameters
                .get_positional(0)
                .is_some_and(|parameter| parameter.definition() == Some(definition))
        {
            return None;
        }

        if self.is_mock_patch_parameter(db, definition) {
            return None;
        }

        if !self.is_fixture_dependency && self.directly_parametrized(db, parameter_name.as_str()) {
            return None;
        }

        Some(FixtureRequest {
            parameter_definition: definition,
            function_definition: self.function_definition,
            name: parameter_name.clone(),
        })
    }

    /// Returns whether `parameter` is supplied by `unittest.mock.patch`.
    fn is_mock_patch_parameter(
        &self,
        db: &'db dyn Db,
        parameter_definition: Definition<'db>,
    ) -> bool {
        if self.mock_patch_count == 0 {
            return false;
        }

        let signature = self.function_type.last_definition_signature(db);
        let parameters = signature.parameters();
        let is_source_keyword_parameter = |parameter: &SignatureParameter<'db>| {
            parameter.keyword_name().is_some()
                // ty applies PEP 484's legacy positional-only convention to leading `__name`
                // parameters, but Python and pytest still inspect them as positional-or-keyword.
                || (self.function.parameters.posonlyargs.is_empty()
                    && parameter.is_positional_only())
        };
        let skips_receiver = self.function_type.has_implicit_receiver(db)
            && parameters
                .get_positional(0)
                .is_some_and(is_source_keyword_parameter);

        parameters
            .iter()
            .filter(|parameter| is_source_keyword_parameter(parameter) && !parameter.has_default())
            .skip(usize::from(skips_receiver))
            .take(self.mock_patch_count)
            .any(|candidate| candidate.definition() == Some(parameter_definition))
    }

    /// Returns whether static parametrization on the function or an enclosing class prevents this
    /// fixture request.
    fn directly_parametrized(&self, db: &'db dyn Db, parameter_name: &str) -> bool {
        if !self.function.decorator_list.is_empty() {
            let decorators = function_known_decorators(db, self.function_definition);
            if self.function.decorator_list.iter().any(|decorator| {
                mark_excludes_fixture(
                    db,
                    self.function_definition,
                    &decorator.expression,
                    parameter_name,
                    |expression| decorators.expression_type(expression),
                )
            }) {
                return true;
            }
        }

        std::iter::successors(self.class_scope, |class_scope| {
            let parent = non_type_parameter_parent(self.index, *class_scope)?;
            (self.index.scope(parent).kind() == ScopeKind::Class).then_some(parent)
        })
        .any(|class_scope| {
            let class_ref = self.index.scope(class_scope).node().expect_class();
            let definition = self.index.expect_single_definition(class_ref);
            class_ref
                .node(self.module)
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
}

enum FixtureRequestParentScope {
    Module,
    Class(FileScopeId),
}

impl FixtureRequestParentScope {
    fn class_scope(self) -> Option<FileScopeId> {
        match self {
            Self::Module => None,
            Self::Class(scope) => Some(scope),
        }
    }
}

/// Returns the fixture request represented by `definition`, if it is eligible for injection.
fn fixture_request_for_parameter<'db>(
    db: &'db dyn Db,
    definition: Definition<'db>,
) -> Option<FixtureRequest<'db>> {
    let DefinitionKind::Parameter(ParameterDefinitionNodeKind::Parameter(_)) = definition.kind(db)
    else {
        return None;
    };

    let file = definition.program_file(db);
    let index = semantic_index(db, file);
    let function_scope = definition.scope(db).file_scope_id(db);
    let function_ref = index.scope(function_scope).node().as_function()?;
    let class_scope = FixtureRequestContext::parent_scope(index, function_scope)?.class_scope();
    let module = parsed_module(db, file.python_file(db)).load(db);
    let context = FixtureRequestContext::new(db, function_ref, class_scope, &module, index)?;

    context.fixture_request_for_parameter(db, definition)
}

/// Returns the number of parameters supplied by `unittest.mock.patch`.
fn mock_patch_count<'db>(
    db: &'db dyn Db,
    function_definition: Definition<'db>,
    function: &ast::StmtFunctionDef,
) -> usize {
    if function.decorator_list.is_empty() {
        return 0;
    }

    let decorators = function_known_decorators(db, function_definition);
    function
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
        .count()
}

/// A decorated fixture function.
#[derive(Debug, Eq, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub(super) struct FixtureDeclaration<'db> {
    // The definition for the fixture function.
    definition: Definition<'db>,
    // The way in which the fixture exposes a name.
    name: FixtureName,
}

/// A fixture made available through one Python binding.
///
/// For example, consider a fixture re-exported through two aliases:
///
/// ```python
/// # fixtures.py
/// import pytest
///
/// @pytest.fixture
/// def resource(): ...  # fixture
///
/// # reexports.py
/// from fixtures import resource as helper  # source_binding
///
/// # plugin.py
/// from reexports import helper as test_resource  # local_binding; name = "test_resource"
/// ```
///
/// The exposure contributed by `test_resource` points to `helper` as its immediate source and to
/// `resource` as the canonical fixture declaration.
#[derive(Debug, Clone, Eq, Hash, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub struct FixtureExposure<'db> {
    /// The name used to request this fixture (`"test_resource"` in the example above).
    name: Name,
    /// The local Python binding that exposes the fixture (`test_resource` in the example above).
    local_binding: Definition<'db>,
    /// The decorated function that declares the fixture (`resource` in the example above).
    fixture: Definition<'db>,
    /// The immediately preceding binding (`helper` in the example above), if any.
    ///
    /// This is `None` for a direct fixture declaration. For a stub backed by a runtime
    /// implementation, it is also `None` so the two definitions have separate reference families.
    source_binding: Option<Definition<'db>>,
}

impl<'db> FixtureExposure<'db> {
    /// Exposes a declaration under its explicit fixture name or local Python binding name.
    fn new(
        symbol_name: &Name,
        local_binding: Definition<'db>,
        declaration: &FixtureDeclaration<'db>,
        source_binding: Option<Definition<'db>>,
    ) -> Option<Self> {
        let name = match &declaration.name {
            FixtureName::Default => symbol_name.clone(),
            FixtureName::Explicit { name, .. } => name.clone(),
            FixtureName::Unknown => return None,
        };
        Some(Self {
            name,
            local_binding,
            fixture: declaration.definition,
            source_binding,
        })
    }

    /// Returns the public name that pytest uses to request this exposure.
    pub fn name(&self) -> &Name {
        &self.name
    }

    /// Returns the local Python binding through which this fixture is exposed.
    pub fn local_binding(&self) -> Definition<'db> {
        self.local_binding
    }

    /// Returns the decorated function that declares the fixture.
    pub fn fixture(&self) -> Definition<'db> {
        self.fixture
    }

    /// Returns the binding from which this exposure was imported, if any.
    pub fn source_binding(&self) -> Option<Definition<'db>> {
        self.source_binding
    }

    /// Returns the binding or decorator from which this exposure gets its public name.
    pub fn name_source(&self, db: &'db dyn Db) -> FixtureNameSource<'db> {
        let Some(declaration) = fixture_declaration(db, self.fixture) else {
            return FixtureNameSource::Binding(self.local_binding);
        };

        match &declaration.name {
            FixtureName::Explicit { range, .. } => FixtureNameSource::Explicit {
                fixture: self.fixture,
                declaration: range.map(|range| FileRange::new(self.fixture.file(db), range)),
            },
            FixtureName::Default | FixtureName::Unknown => {
                FixtureNameSource::Binding(self.local_binding)
            }
        }
    }
}

/// The source from which a fixture obtains its public name.
#[derive(Debug, Clone, Copy, Eq, Hash, PartialEq)]
pub enum FixtureNameSource<'db> {
    /// The Python binding that supplies the fixture name.
    Binding(Definition<'db>),
    /// An explicit fixture name supplied by the decorated function.
    Explicit {
        /// The decorated function that declares the fixture.
        fixture: Definition<'db>,
        /// The fixture-name literal's file and content range when it is one string literal.
        declaration: Option<FileRange>,
    },
}

/// A possible fixture name and the exposures it contributes to a fixture search scope.
///
/// Bound names in class scopes are retained even without fixture exposures because they can
/// shadow fixtures inherited from another class in the searched class's MRO.
#[derive(Debug, Eq, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
struct FixtureNameCandidate<'db> {
    name: Name,
    exposures: Box<[FixtureExposure<'db>]>,
}

/// How a fixture decorator determines the fixture's public name.
#[derive(Debug, Eq, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
enum FixtureName {
    /// Uses the Python binding name at the exposure site.
    Default,
    /// Uses a statically known explicit name.
    Explicit {
        name: Name,
        range: Option<TextRange>,
    },
    /// Represents a public name that ty cannot determine statically, such as a
    /// dynamically-typed expression or a non-literal `str`.
    Unknown,
}

/// Returns whether `scope` and each enclosing class remain available from their parent scopes.
fn is_available_fixture_search_scope<'db>(
    db: &'db dyn Db,
    index: &ty_python_core::SemanticIndex<'db>,
    scope: FileScopeId,
) -> bool {
    match index.scope(scope).kind() {
        ScopeKind::Module => true,
        ScopeKind::Class => non_type_parameter_parent(index, scope).is_some_and(|parent| {
            if !is_available_fixture_search_scope(db, index, parent) {
                return false;
            }

            let class_ref = index.scope(scope).node().expect_class();
            let definition = index.expect_single_definition(class_ref);
            is_available_definition(db, definition)
        }),
        _ => false,
    }
}

/// A class hierarchy or scope searched when resolving a fixture request.
#[derive(Clone, Copy)]
enum FixtureSearchScope<'db> {
    /// Searches a class and its statically known ancestors.
    Class(ClassLiteral<'db>),
    /// Searches a single scope.
    Scope(ScopeId<'db>),
}

/// Returns the names that may participate in fixture lookup for one fixture search scope.
///
/// Separating this summary from request resolution lets Salsa reuse scope-specific fixture
/// discovery across parameters.
#[salsa::tracked(returns(deref), heap_size=ruff_memory_usage::heap_size)]
fn fixture_name_candidates<'db>(
    db: &'db dyn Db,
    scope: ScopeId<'db>,
) -> Box<[FixtureNameCandidate<'db>]> {
    let is_class_scope = scope.node(db).scope_kind() == ScopeKind::Class;
    let table = place_table(db, scope);
    let mut name_candidates = Vec::new();

    for (symbol_id, bindings) in use_def_map(db, scope).all_end_of_scope_symbol_bindings() {
        let symbol = table.symbol(symbol_id);
        let name = symbol.name();
        let resolution = DefinitionResolution::from_bindings(db, bindings);
        let exposures =
            fixture_exposures_from_symbol_definitions(db, name, resolution.definitions());

        // Reject names that neither expose a fixture nor bind a runtime class attribute that
        // can shadow an inherited fixture.
        if exposures.is_empty()
            && !(is_class_scope
                && symbol.is_bound()
                && class_attribute_exists_at_runtime(db, resolution.definitions()))
        {
            continue;
        }
        name_candidates.push(FixtureNameCandidate {
            name: name.clone(),
            exposures: exposures.into_boxed_slice(),
        });
    }

    name_candidates.into_boxed_slice()
}

/// Returns fixture exposures reachable from the resolved definitions for one symbol.
fn fixture_exposures_from_symbol_definitions<'db>(
    db: &'db dyn Db,
    name: &Name,
    definitions: &[Definition<'db>],
) -> Vec<FixtureExposure<'db>> {
    let mut exposures = FxIndexSet::default();

    for definition in definitions.iter().copied() {
        exposures.extend(exposures_contributed_by_definition(db, definition, name));
    }

    exposures.into_iter().collect()
}

/// Returns whether a class attribute has any reachable runtime binding.
fn class_attribute_exists_at_runtime<'db>(
    db: &'db dyn Db,
    definitions: &[Definition<'db>],
) -> bool {
    definitions
        .iter()
        .any(|definition| may_exist_at_runtime(db, *definition))
}

/// Resolves a request against the fixture exposures in `search_scope`.
fn bindings_in_search_scope<'db>(
    db: &'db dyn Db,
    request: &FixtureRequest<'db>,
    search_scope: FixtureSearchScope<'db>,
) -> Box<[FixtureBinding<'db>]> {
    let search_scopes = match search_scope {
        FixtureSearchScope::Class(class) => Either::Left(
            class
                .iter_mro(db)
                .filter_map(ClassBase::into_class)
                .filter(|ancestor| !ancestor.is_object(db))
                .filter_map(|ancestor| ancestor.static_class_literal(db))
                .map(|(ancestor, _)| ancestor.body_scope(db)),
        ),
        FixtureSearchScope::Scope(scope) => Either::Right(std::iter::once(scope)),
    };

    let mut seen_names = FxHashSet::default();
    let mut winning_name: Option<&Name> = None;
    let mut fixtures: FxIndexMap<Definition<'db>, Vec<FixtureExposure<'db>>> =
        FxIndexMap::default();

    for scope in search_scopes {
        for name_candidate in fixture_name_candidates(db, scope) {
            let symbol_name = &name_candidate.name;
            // A name supplied by an earlier scope shadows the same name here.
            if !seen_names.insert(symbol_name) {
                continue;
            }

            for exposure in &name_candidate.exposures {
                // Request must match public name of the fixture
                if request.name != exposure.name
                    // A fixture definition cannot fulfill a request for itself
                    || request.function_definition == exposure.fixture
                {
                    continue;
                }

                // Semantic-index traversal is unordered. Pytest registers fixture attributes in
                // sorted `dir()` order and selects the last registration, so retain bindings for
                // the lexicographically last matching attribute. Thus, if `first_fixture` and
                // `second_fixture` both expose `resource`, `second_fixture` wins.
                //
                // `dir()` ordering: https://docs.python.org/3/library/functions.html#dir
                // Fixture discovery: https://github.com/pytest-dev/pytest/blob/9.0.1/src/_pytest/fixtures.py#L1852-L1880
                // Registration order: https://github.com/pytest-dev/pytest/blob/9.0.1/src/_pytest/fixtures.py#L1788-L1797
                // Fixture selection: https://github.com/pytest-dev/pytest/blob/9.0.1/src/_pytest/fixtures.py#L583-L599
                match winning_name.map(|winner| winner.cmp(symbol_name)) {
                    Some(Ordering::Greater) => continue,
                    Some(Ordering::Less) | None => {
                        winning_name = Some(symbol_name);
                        fixtures.clear();
                    }
                    Some(Ordering::Equal) => {}
                }
                let exposures = fixtures.entry(exposure.fixture).or_default();
                if !exposures.contains(exposure) {
                    exposures.push(exposure.clone());
                }
            }
        }
    }

    fixtures
        .into_iter()
        .map(|(fixture, exposures)| FixtureBinding {
            request: request.parameter_definition,
            fixture,
            exposures: exposures.into_boxed_slice(),
        })
        .collect()
}

/// Returns applicable `conftest.py` files from nearest to outermost.
fn conftest_files<'db>(db: &'db dyn Db, request_file: ProgramFile<'db>) -> Vec<ProgramFile<'db>> {
    let Some(path) = request_file.file(db).path(db).as_system_path() else {
        return Vec::new();
    };

    let program = request_file.program(db);
    let Some(root) = program
        .search_paths(db)
        .first_party_roots()
        .filter(|root| path.starts_with(*root))
        .min_by_key(|root| root.components().count())
    else {
        return Vec::new();
    };
    let Some(request_directory) = path.parent() else {
        return Vec::new();
    };

    let start_directory = if path.file_name() == Some("conftest.py") {
        // The caller already searched the request file as a module search scope.
        // Start in its parent when it is itself a conftest to avoid searching
        // the same search scope twice.
        request_directory.parent()
    } else {
        Some(request_directory)
    };
    let Some(start_directory) = start_directory else {
        return Vec::new();
    };

    start_directory
        .ancestors()
        .take_while(|directory| directory.starts_with(root))
        .filter_map(|directory| system_path_to_file(db, directory.join("conftest.py")).ok())
        .map(|file| ProgramFile::new(db, file, program))
        .collect()
}

/// Returns a fixture declaration for a function with a canonical pytest fixture decorator.
#[salsa::tracked(returns(ref), heap_size=ruff_memory_usage::heap_size)]
pub(super) fn fixture_declaration<'db>(
    db: &'db dyn Db,
    definition: Definition<'db>,
) -> Option<FixtureDeclaration<'db>> {
    let DefinitionKind::Function(function_ref) = definition.kind(db) else {
        return None;
    };
    let module = parsed_module(db, definition.python_file(db)).load(db);
    let function = function_ref.node(&module);
    let first_decorator = &function.decorator_list.first()?.expression;
    let inference = function_known_decorators(db, definition);
    let expression = if definition.scope(db).node(db).scope_kind() == ScopeKind::Class
        && matches!(
            inference.expression_type(first_decorator),
            Some(Type::ClassLiteral(class)) if class.is_known(db, KnownClass::Staticmethod)
        ) {
        // Pytest discovers fixtures on plugin classes through class attribute access. For
        // example:
        //
        // ```py
        // class LegacyTmpdirPlugin:
        //     @staticmethod
        //     @fixture
        //     def tmpdir(): ...
        // ```
        //
        // Accessing `LegacyTmpdirPlugin.tmpdir` invokes the `@staticmethod` descriptor and exposes
        // the fixture wrapper beneath it. Inspect the inner decorator to match that lookup.
        let fixture_decorator = function.decorator_list.get(1)?;
        &fixture_decorator.expression
    } else {
        first_decorator
    };
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

/// Returns fixture exposures contributed by a symbol definition.
fn exposures_contributed_by_definition<'db>(
    db: &'db dyn Db,
    definition: Definition<'db>,
    symbol_name: &Name,
) -> Vec<FixtureExposure<'db>> {
    if definition.file(db).is_stub(db)
        && let Some(source_file) =
            stub_file_to_real_module(db, definition.program_file(db).resolver_file(db))
                .and_then(|module| module.file(db))
    {
        let source_file = ProgramFile::new(db, source_file, definition.program(db));
        let source_exposures = if definition.scope(db).node(db).scope_kind() == ScopeKind::Class {
            fixture_exposures_from_stub_class_definition(db, definition, source_file)
        } else {
            fixture_exposures_for_name_in_scope(
                db,
                global_scope(db, source_file),
                symbol_name.clone(),
            )
            .to_vec()
        };

        // The stub is the binding visible to callers. Keep the runtime fixture as the canonical
        // declaration, but don't link the stub's exposure to the runtime binding, so their
        // references remain in separate families. This matches reference behavior for ordinary
        // Python symbols.
        return source_exposures
            .into_iter()
            .map(|exposure| FixtureExposure {
                local_binding: definition,
                source_binding: None,
                ..exposure
            })
            .collect();
    }

    let kind = definition.kind(db);
    if !matches!(
        &kind,
        DefinitionKind::Function(_) | DefinitionKind::ImportFrom(_) | DefinitionKind::StarImport(_)
    ) {
        return Vec::new();
    }
    if !may_exist_at_runtime(db, definition) {
        return Vec::new();
    }

    match kind {
        DefinitionKind::Function(_) => {
            let Some(declaration) = fixture_declaration(db, definition) else {
                return Vec::new();
            };
            let Some(exposure) = FixtureExposure::new(symbol_name, definition, declaration, None)
            else {
                return Vec::new();
            };
            vec![exposure]
        }
        DefinitionKind::ImportFrom(import) => {
            let parsed = parsed_module(db, definition.python_file(db)).load(db);
            fixture_exposures_from_import(
                db,
                definition,
                import.import(&parsed),
                import.alias(&parsed).name.id(),
                symbol_name,
            )
        }
        DefinitionKind::StarImport(import) => {
            let parsed = parsed_module(db, definition.python_file(db)).load(db);
            fixture_exposures_from_import(
                db,
                definition,
                import.import(&parsed),
                symbol_name,
                symbol_name,
            )
        }
        _ => Vec::new(),
    }
}

/// Returns fixture exposures for a stub class member from its runtime source class.
///
/// For example, given these corresponding files:
///
/// ```python
/// # plugin.pyi
/// class Plugin:
///     def resource(self): ...
///
/// # plugin.py
/// class Plugin:
///     @pytest.fixture
///     def resource(self): ...
/// ```
///
/// The stub definition yields the lexical path `["Plugin", "resource"]`. This function finds the
/// `Plugin` scope in the source file, then resolves the visible `resource` definitions in that scope.
fn fixture_exposures_from_stub_class_definition<'db>(
    db: &'db dyn Db,
    definition: Definition<'db>,
    source_file: ProgramFile<'db>,
) -> Vec<FixtureExposure<'db>> {
    let Some(path) = lexical_name_path_for_definition(db, definition) else {
        return Vec::new();
    };
    let mut path = path;
    let Some(member_name) = path.pop() else {
        return Vec::new();
    };

    let mut exposures = FxIndexSet::default();
    for source_scope in source_scopes_for_lexical_path(db, source_file, path.into_boxed_slice()) {
        exposures.extend(
            fixture_exposures_for_name_in_scope(db, *source_scope, member_name.clone())
                .iter()
                .cloned(),
        );
    }

    exposures.into_iter().collect()
}

/// Returns scopes in `source_file` with the given lexical path.
#[salsa::tracked(returns(deref), heap_size=ruff_memory_usage::heap_size)]
fn source_scopes_for_lexical_path<'db>(
    db: &'db dyn Db,
    source_file: ProgramFile<'db>,
    path: Box<[Name]>,
) -> Box<[ScopeId<'db>]> {
    let index = semantic_index(db, source_file);
    let parsed = parsed_module(db, source_file.python_file(db)).load(db);
    let mut scopes = vec![global_scope(db, source_file)];
    let mut next_scopes = FxIndexSet::default();

    for name in path {
        for scope in scopes {
            next_scopes.extend(
                index
                    .child_scopes(scope.file_scope_id(db))
                    .filter(|(_, child_scope)| {
                        matches!(child_scope.kind(), ScopeKind::Class | ScopeKind::Function)
                    })
                    .map(|(child_scope_id, _)| child_scope_id.to_scope_id(db, source_file))
                    .filter(|child_scope| child_scope.name(db, &parsed) == name.as_str()),
            );
        }
        if next_scopes.is_empty() {
            return Box::default();
        }
        scopes = next_scopes.drain(..).collect();
    }

    scopes.into_boxed_slice()
}

/// Returns fixture exposures supplied by a name's end-of-scope bindings.
#[allow(
    clippy::needless_pass_by_value,
    reason = "Salsa requires owned query keys, and lint expectations cannot observe diagnostics from the generated function"
)]
#[salsa::tracked(
    returns(deref),
    cycle_initial=|_, _, _, _| Box::default(),
    heap_size=ruff_memory_usage::heap_size
)]
fn fixture_exposures_for_name_in_scope<'db>(
    db: &'db dyn Db,
    scope: ScopeId<'db>,
    name: Name,
) -> Box<[FixtureExposure<'db>]> {
    let Some(symbol) = place_table(db, scope).symbol_id(name.as_str()) else {
        return Box::default();
    };

    let resolution = DefinitionResolution::from_bindings(
        db,
        use_def_map(db, scope).end_of_scope_symbol_bindings(symbol),
    );
    fixture_exposures_from_symbol_definitions(db, &name, resolution.definitions())
        .into_boxed_slice()
}

/// Follows an import to fixture exposures supplied by its target.
fn fixture_exposures_from_import<'db>(
    db: &'db dyn Db,
    importing_definition: Definition<'db>,
    import: &ast::StmtImportFrom,
    imported_name: &Name,
    local_name: &Name,
) -> Vec<FixtureExposure<'db>> {
    let program_file = importing_definition.program_file(db);
    let importing_file =
        ImportingFile::File(program_file.file(db), program_file.resolver_environment(db));
    let Some(imported_module) = resolve_module_for_import_from(db, importing_file, import) else {
        return Vec::new();
    };

    let Some(imported_file) = imported_module.file(db) else {
        return Vec::new();
    };
    let source_exposures = fixture_exposures_for_name_in_scope(
        db,
        global_scope(
            db,
            ProgramFile::new(db, imported_file, program_file.program(db)),
        ),
        imported_name.clone(),
    );

    source_exposures
        .iter()
        .filter_map(|source| {
            let declaration = fixture_declaration(db, source.fixture).as_ref()?;
            FixtureExposure::new(
                local_name,
                importing_definition,
                declaration,
                Some(source.local_binding),
            )
        })
        .collect()
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
        return FixtureName::Default;
    }
    let Some(name) = name_type.as_string_literal().map(|string| string.value(db)) else {
        return FixtureName::Unknown;
    };
    if name.is_empty() {
        return FixtureName::Default;
    }

    FixtureName::Explicit {
        name: Name::new(name),
        range: fixture_name_literal_range(&name_keyword.value, name),
    }
}

/// Returns the content range when `expression` spells `name` as one string literal.
fn fixture_name_literal_range(expression: &ast::Expr, name: &str) -> Option<TextRange> {
    expression
        .as_string_literal_expr()?
        .as_single_part_string()
        .filter(|literal| literal.as_str() == name)
        .map(ast::StringLiteral::content_range)
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
    if !expression_type(&call.func)
        .is_some_and(|ty| ty.is_instance_of(db, KnownClass::PytestParametrizeMarkDecorator))
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

/// Returns the statically known string sequence bound to a module symbol.
///
/// For example, given pytest-style plugin registration:
///
/// ```py
/// essential_plugins = ("mark", "main")
/// default_plugins = (*essential_plugins, "fixtures")
/// ```
///
/// Looking up `default_plugins` returns `['mark', 'main', 'fixtures']`.
fn static_string_sequence_for_module_symbol(
    db: &dyn Db,
    file: ProgramFile<'_>,
    name: &str,
) -> Option<Vec<String>> {
    let definition = end_of_scope_definition(db, file, name)?;
    let module = parsed_module(db, file.python_file(db)).load(db);
    let expression = match definition.kind(db) {
        DefinitionKind::Assignment(assignment) => Some(assignment.value(&module)),
        DefinitionKind::AnnotatedAssignment(assignment) => assignment.value(&module),
        _ => None,
    }?;
    static_string_sequence_from_expression(db, definition, expression)
}

/// Evaluates a static string sequence while preserving the order of tuple concatenation.
///
/// Some supported pytest releases use tuple concatenation to define their plugin registration
/// order:
///
/// ```python
/// essential_plugins = ("mark", "main")
/// # Inferred: tuple[Literal["mark"], Literal["main"]]
///
/// default_plugins = essential_plugins + ("fixtures",)
/// # Inferred through tuple.__add__: tuple[Literal["mark", "main", "fixtures"], ...]
/// ```
///
/// The `tuple.__add__` return annotation produces a variable-length tuple whose element type is the
/// union of these literals. Although ty currently displays the union members in first-occurrence
/// order, the type neither (1) requires every member to occur nor (2) records their order.
/// Evaluating the operands separately preserves the concrete sequence.
fn static_string_sequence_from_expression<'db>(
    db: &'db dyn Db,
    definition: Definition<'db>,
    expression: &ast::Expr,
) -> Option<Vec<String>> {
    if let ast::Expr::BinOp(binary) = expression
        && binary.op == ast::Operator::Add
    {
        let mut strings = static_string_sequence_from_expression(db, definition, &binary.left)?;
        strings.extend(static_string_sequence_from_expression(
            db,
            definition,
            &binary.right,
        )?);
        return Some(strings);
    }

    let environment = ProgramEnvironment::from_definition(definition);
    extract_fixed_length_iterable_element_types(db, &environment, expression, |element| {
        definition_expression_type(db, definition, element)
    })?
    .iter()
    .map(|element| {
        element
            .as_string_literal()
            .map(|string| string.value(db).to_owned())
    })
    .collect()
}

/// Returns the sole definition bound to a module symbol at the end of its scope.
fn end_of_scope_definition<'db>(
    db: &'db dyn Db,
    file: ProgramFile<'db>,
    name: &str,
) -> Option<Definition<'db>> {
    let scope = global_scope(db, file);
    let symbol = place_table(db, scope).symbol_id(name)?;
    let mut definitions = use_def_map(db, scope)
        .end_of_scope_symbol_bindings(symbol)
        .filter_map(|binding| binding.binding.definition());
    let definition = definitions.next()?;
    definitions.next().is_none().then_some(definition)
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
    use ruff_db::system::{DbWithWritableSystem, SystemPathBuf};
    use ruff_python_ast as ast;
    use ruff_text_size::Ranged;
    use ty_python_core::definition::Definition;
    use ty_python_core::semantic_index;

    use super::{
        FixtureExposure, FixtureNameSource, end_of_scope_definition,
        fixture_bindings_for_parameter, fixture_exposures_for_definition,
        pytest_global_plugin_files,
    };
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

@staticmethod
@pytest.fixture
def module_staticmethod(): ...

def test_module_staticmethod(module_staticmethod): ...
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
        assert_snapshot!(
            test.function("test_module_staticmethod")
                .fixture_resolution("module_staticmethod"),
            @"No fixture resolved for parameter `module_staticmethod`"
        );
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
    def first_fixture(self): ...

class Second:
    @pytest.fixture(name="resource")
    def second_fixture(self): ...

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
        10 |     def second_fixture(self): ...
           |         --------------
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

parametrize = pytest.mark.parametrize

@parametrize("value", [1])
def test_bare_aliased_direct(value): ...

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
        let test_bare_aliased_direct = test.function("test_bare_aliased_direct");
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

        assert_snapshot!(test_bare_aliased_direct.fixture_resolution("value"), @"No fixture resolved for parameter `value`");

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

    #[test]
    fn resolves_imported_fixture_exposures() {
        let test = PytestTestCase::with_files(
            "/src/test_example.py",
            &[
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
                // Import the same explicitly named fixture twice to verify that its exposures are
                // deduplicated.
                (
                    "/src/test_example.py",
                    r#"
from fixtures import implementation, implementation as second_exposure
from reexports import middle as chained
from star_fixtures import *

def test_use(
    chained,
    public_name,
    resource,
    star_fixture,
): ...
"#,
                ),
            ],
        );

        let test_use = test.function("test_use");

        assert_snapshot!(test_use.fixture_resolution("chained"), @"
        info[pytest-fixture]: Resolve fixture for parameter
         --> src/test_example.py:7:5
          |
        7 |     chained,
          |     ^^^^^^^ fixture requested here
        info: Found 1 fixture
         --> src/fixtures.py:5:5
          |
        5 | def resource(): ...
          |     --------
        ");

        assert_snapshot!(test_use.fixture_resolution("public_name"), @"
        info[pytest-fixture]: Resolve fixture for parameter
         --> src/test_example.py:8:5
          |
        8 |     public_name,
          |     ^^^^^^^^^^^ fixture requested here
        info: Found 1 fixture
         --> src/fixtures.py:8:5
          |
        8 | def implementation(): ...
          |     --------------
        ");

        assert_snapshot!(test_use.fixture_resolution("star_fixture"), @"
        info[pytest-fixture]: Resolve fixture for parameter
          --> src/test_example.py:10:5
           |
        10 |     star_fixture,
           |     ^^^^^^^^^^^^ fixture requested here
        info: Found 1 fixture
         --> src/star_fixtures.py:5:5
          |
        5 | def star_fixture(): ...
          |     ------------
        ");

        assert_snapshot!(test_use.fixture_resolution("resource"), @"No fixture resolved for parameter `resource`");
    }

    #[test]
    fn resolves_fixture_alongside_cyclic_reexport() {
        let test = PytestTestCase::with_files(
            "/src/test_example.py",
            &[
                (
                    "/src/a.py",
                    r#"
import pytest

flag: bool
if flag:
    from b import resource
else:
    @pytest.fixture
    def resource(): ...
"#,
                ),
                (
                    "/src/b.py",
                    r#"
from a import resource
"#,
                ),
                (
                    "/src/test_example.py",
                    r#"
from a import resource

def test_use(resource): ...
"#,
                ),
            ],
        );

        let test_use = test.function("test_use");

        assert_snapshot!(test_use.fixture_resolution("resource"), @"
        info[pytest-fixture]: Resolve fixture for parameter
         --> src/test_example.py:4:14
          |
        4 | def test_use(resource): ...
          |              ^^^^^^^^ fixture requested here
        info: Found 1 fixture
         --> src/a.py:9:9
          |
        9 |     def resource(): ...
          |         --------
        ");
    }

    #[test]
    fn resolves_imported_fixture_declarations_from_source_when_available() {
        let test = PytestTestCase::with_files(
            "/src/test_example.py",
            &[
                // Use the same symbol and fixture name at module and class scope so
                // stub mapping must preserve the class path.
                (
                    "/src/fixtures.py",
                    r#"
import pytest

@pytest.fixture(name="public_name")
def implementation(): ...

class Base:
    @pytest.fixture(name="public_name")
    def implementation(self): ...
"#,
                ),
                // Expose `implementation` only through a synthetic lazy binding in the stub.
                // The binding must survive long enough for fixture discovery to inspect the source.
                (
                    "/src/fixtures.pyi",
                    r#"
def initialize() -> None:
    global implementation
    implementation = ...

class Base:
    def implementation(self): ...
"#,
                ),
                (
                    "/src/test_example.py",
                    r#"
from fixtures import Base, implementation

def test_use(public_name): ...

class TestExample(Base):
    def test_inherited(self, public_name): ...
"#,
                ),
            ],
        );

        let test_use = test.function("test_use");

        assert_snapshot!(test_use.fixture_resolution("public_name"), @"
        info[pytest-fixture]: Resolve fixture for parameter
         --> src/test_example.py:4:14
          |
        4 | def test_use(public_name): ...
          |              ^^^^^^^^^^^ fixture requested here
        info: Found 1 fixture
         --> src/fixtures.py:5:5
          |
        5 | def implementation(): ...
          |     --------------
        ");

        let test_inherited = test.function("TestExample.test_inherited");

        assert_snapshot!(test_inherited.fixture_resolution("public_name"), @"
        info[pytest-fixture]: Resolve fixture for parameter
         --> src/test_example.py:7:30
          |
        7 |     def test_inherited(self, public_name): ...
          |                              ^^^^^^^^^^^ fixture requested here
        info: Found 1 fixture
         --> src/fixtures.py:9:9
          |
        9 |     def implementation(self): ...
          |         --------------
        ");

        let fixture = test.function_definition("/src/fixtures.py", "implementation");
        let stub = test.global_definition("/src/fixtures.pyi", "implementation");
        let stub_exposures = fixture_exposures_for_definition(&test.db, stub);
        assert_single_exposure(&stub_exposures, "public_name", stub, fixture, None);
    }

    #[test]
    fn resolves_fixture_declarations_from_stub_only_classes() {
        let test = PytestTestCase::with_files(
            "/src/test_example.py",
            &[
                (
                    "/src/plugin.pyi",
                    r#"
import pytest

class Plugin:
    @pytest.fixture
    def resource(self): ...
"#,
                ),
                (
                    "/src/test_example.py",
                    r#"
from plugin import Plugin

class TestExample(Plugin):
    def test_use(self, resource): ...
"#,
                ),
            ],
        );

        let test_use = test.function("TestExample.test_use");

        assert_snapshot!(test_use.fixture_resolution("resource"), @"
        info[pytest-fixture]: Resolve fixture for parameter
         --> src/test_example.py:5:24
          |
        5 |     def test_use(self, resource): ...
          |                        ^^^^^^^^ fixture requested here
        info: Found 1 fixture
         --> src/plugin.pyi:6:9
          |
        6 |     def resource(self): ...
          |         --------
        ");
    }

    #[test]
    fn resolves_fixture_reexports_through_stub_only_modules() {
        let test = PytestTestCase::with_files(
            "/src/test_example.py",
            &[
                (
                    "/src/origin.pyi",
                    r#"
import pytest

@pytest.fixture
def module_fixture(): ...

@pytest.fixture
def class_fixture(): ...

@pytest.fixture
def star_fixture(): ...
"#,
                ),
                (
                    "/src/plugin.pyi",
                    r#"
from origin import module_fixture as module_fixture
from origin import *

class Plugin:
    from origin import class_fixture as class_fixture
"#,
                ),
                (
                    "/src/test_example.py",
                    r#"
from plugin import Plugin, module_fixture, star_fixture

def test_module(module_fixture, star_fixture): ...

class TestExample(Plugin):
    def test_use(self, class_fixture): ...
"#,
                ),
            ],
        );

        let test_module = test.function("test_module");
        let test_use = test.function("TestExample.test_use");

        assert_snapshot!(test_module.fixture_resolution("module_fixture"), @"
        info[pytest-fixture]: Resolve fixture for parameter
         --> src/test_example.py:4:17
          |
        4 | def test_module(module_fixture, star_fixture): ...
          |                 ^^^^^^^^^^^^^^ fixture requested here
        info: Found 1 fixture
         --> src/origin.pyi:5:5
          |
        5 | def module_fixture(): ...
          |     --------------
        ");
        assert_snapshot!(test_module.fixture_resolution("star_fixture"), @"
        info[pytest-fixture]: Resolve fixture for parameter
         --> src/test_example.py:4:33
          |
        4 | def test_module(module_fixture, star_fixture): ...
          |                                 ^^^^^^^^^^^^ fixture requested here
        info: Found 1 fixture
          --> src/origin.pyi:11:5
           |
        11 | def star_fixture(): ...
           |     ------------
        ");
        assert_snapshot!(test_use.fixture_resolution("class_fixture"), @"
        info[pytest-fixture]: Resolve fixture for parameter
         --> src/test_example.py:7:24
          |
        7 |     def test_use(self, class_fixture): ...
          |                        ^^^^^^^^^^^^^ fixture requested here
        info: Found 1 fixture
         --> src/origin.pyi:8:5
          |
        8 | def class_fixture(): ...
          |     -------------
        ");
    }

    #[test]
    fn ignores_overwritten_imported_fixtures() {
        let test = PytestTestCase::with_files(
            "/src/test_example.py",
            &[
                (
                    "/src/origin.py",
                    r#"
import pytest

@pytest.fixture
def resource(): ...
"#,
                ),
                (
                    "/src/provider.py",
                    r#"
from origin import resource

resource = object()
"#,
                ),
                (
                    "/src/test_example.py",
                    r#"
from origin import resource as local_resource
from provider import resource

local_resource = object()

def test_use(local_resource, resource): ...
"#,
                ),
            ],
        );

        let test_use = test.function("test_use");

        assert_snapshot!(test_use.fixture_resolution("local_resource"), @"No fixture resolved for parameter `local_resource`");
        assert_snapshot!(test_use.fixture_resolution("resource"), @"No fixture resolved for parameter `resource`");
    }

    #[test]
    fn preserves_conditional_imported_fixture_definitions() {
        let test = PytestTestCase::with_files(
            "/src/test_example.py",
            &[
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
            ],
        );

        let test_use = test.function("test_use");

        assert_snapshot!(test_use.fixture_resolution("resource"), @"
        info[pytest-fixture]: Resolve fixture for parameter
         --> src/test_example.py:9:14
          |
        9 | def test_use(resource): ...
          |              ^^^^^^^^ fixture requested here
        info: Found 2 fixtures
         --> src/first.py:5:5
          |
        5 | def first(): ...
          |     -----
          |
         ::: src/second.py:5:5
          |
        5 | def second(): ...
          |     ------
        ");
    }

    #[test]
    fn ignores_local_fixture_declarations_unavailable_at_runtime() {
        let test = PytestTestCase::new(
            "/src/test_example.py",
            r#"
from typing import TYPE_CHECKING
import pytest

if TYPE_CHECKING:
    @pytest.fixture
    def typing_only(): ...

class Base:
    @pytest.fixture
    def resource(self): ...

class TestDerived(Base):
    if TYPE_CHECKING:
        @pytest.fixture
        def resource(self): ...

    def test_inherited(self, resource): ...

def test_use(typing_only): ...
"#,
        );

        let test_inherited = test.function("TestDerived.test_inherited");
        let test_use = test.function("test_use");

        assert_snapshot!(test_inherited.fixture_resolution("resource"), @"
        info[pytest-fixture]: Resolve fixture for parameter
          --> src/test_example.py:18:30
           |
        18 |     def test_inherited(self, resource): ...
           |                              ^^^^^^^^ fixture requested here
        info: Found 1 fixture
          --> src/test_example.py:11:9
           |
        11 |     def resource(self): ...
           |         --------
        ");

        assert_snapshot!(test_use.fixture_resolution("typing_only"), @"No fixture resolved for parameter `typing_only`");
    }

    #[test]
    fn resolves_dependencies_for_fixture_declarations_unavailable_at_runtime() {
        let test = PytestTestCase::new(
            "/src/test_example.py",
            r#"
from typing import type_check_only
import pytest

@pytest.fixture
def dependency(): ...

@pytest.fixture
@type_check_only
def hidden(dependency): ...
"#,
        );

        let hidden = test.function("hidden");

        assert_snapshot!(hidden.fixture_resolution("dependency"), @"
        info[pytest-fixture]: Resolve fixture for parameter
          --> src/test_example.py:10:12
           |
        10 | def hidden(dependency): ...
           |            ^^^^^^^^^^ fixture requested here
        info: Found 1 fixture
         --> src/test_example.py:6:5
          |
        6 | def dependency(): ...
          |     ----------
        ");
    }

    #[test]
    fn ignores_imported_fixture_exposures_unavailable_at_runtime() {
        let test = PytestTestCase::with_files(
            "/src/test_example.py",
            &[
                (
                    "/src/fixtures.py",
                    r#"
import pytest

@pytest.fixture
def resource(): ...
"#,
                ),
                (
                    "/src/provider.py",
                    r#"
from typing import TYPE_CHECKING
import pytest

if TYPE_CHECKING:
    from fixtures import resource as typing_only_reexport

    @pytest.fixture
    def typing_only_declaration(): ...
"#,
                ),
                (
                    "/src/test_example.py",
                    r#"
from typing import TYPE_CHECKING

from provider import *

if False:
    from fixtures import resource as unreachable

if TYPE_CHECKING:
    from fixtures import resource as typing_only

def test_use(unreachable, typing_only, typing_only_declaration, typing_only_reexport): ...
"#,
                ),
            ],
        );

        let test_use = test.function("test_use");

        assert_snapshot!(test_use.fixture_resolution("unreachable"), @"No fixture resolved for parameter `unreachable`");
        assert_snapshot!(test_use.fixture_resolution("typing_only"), @"No fixture resolved for parameter `typing_only`");
        assert_snapshot!(test_use.fixture_resolution("typing_only_declaration"), @"No fixture resolved for parameter `typing_only_declaration`");
        assert_snapshot!(test_use.fixture_resolution("typing_only_reexport"), @"No fixture resolved for parameter `typing_only_reexport`");
    }

    #[test]
    fn resolves_conftest_fixtures_from_request_directory_to_first_party_root() {
        let test = PytestTestCase::with_files(
            "/src/tests/test_example.py",
            &[
                (
                    "/conftest.py",
                    r#"
import pytest

@pytest.fixture
def outside_root(): ...
"#,
                ),
                (
                    "/src/conftest.py",
                    r#"
import pytest

@pytest.fixture
def root_fixture(): ...

@pytest.fixture
def shadowed(): ...
"#,
                ),
                (
                    "/src/tests/conftest.py",
                    r#"
import pytest

@pytest.fixture
def shadowed(): ...
"#,
                ),
                (
                    "/src/sibling/conftest.py",
                    r#"
import pytest

@pytest.fixture
def sibling_fixture(): ...
"#,
                ),
                (
                    "/src/tests/test_example.py",
                    r#"
def test_use(
    root_fixture,
    shadowed,
    outside_root,
    sibling_fixture,
): ...
"#,
                ),
            ],
        );

        let test_use = test.function("test_use");

        assert_snapshot!(test_use.fixture_resolution("root_fixture"), @"
        info[pytest-fixture]: Resolve fixture for parameter
         --> src/tests/test_example.py:3:5
          |
        3 |     root_fixture,
          |     ^^^^^^^^^^^^ fixture requested here
        info: Found 1 fixture
         --> src/conftest.py:5:5
          |
        5 | def root_fixture(): ...
          |     ------------
        ");

        assert_snapshot!(test_use.fixture_resolution("shadowed"), @"
        info[pytest-fixture]: Resolve fixture for parameter
         --> src/tests/test_example.py:4:5
          |
        4 |     shadowed,
          |     ^^^^^^^^ fixture requested here
        info: Found 1 fixture
         --> src/tests/conftest.py:5:5
          |
        5 | def shadowed(): ...
          |     --------
        ");

        assert_snapshot!(test_use.fixture_resolution("outside_root"), @"No fixture resolved for parameter `outside_root`");

        assert_snapshot!(test_use.fixture_resolution("sibling_fixture"), @"No fixture resolved for parameter `sibling_fixture`");
    }

    #[test]
    fn resolves_conftest_providers_from_outermost_matching_first_party_root() {
        let test = PytestTestCase::with_files_and_src_roots(
            "/src/tests/test_example.py",
            &[
                (
                    "/conftest.py",
                    r#"
import pytest

@pytest.fixture
def outer_fixture(): ...
"#,
                ),
                (
                    "/src/tests/test_example.py",
                    r#"
def test_use(outer_fixture): ...
"#,
                ),
            ],
            // The relative environment roots `["src", "."]` resolve to these absolute paths.
            vec![SystemPathBuf::from("/src"), SystemPathBuf::from("/")],
        );

        assert_snapshot!(test.function("test_use").fixture_resolution("outer_fixture"), @"
        info[pytest-fixture]: Resolve fixture for parameter
         --> src/tests/test_example.py:2:14
          |
        2 | def test_use(outer_fixture): ...
          |              ^^^^^^^^^^^^^ fixture requested here
        info: Found 1 fixture
         --> conftest.py:5:5
          |
        5 | def outer_fixture(): ...
          |     -------------
        ");
    }

    #[test]
    fn resolves_conftest_fixture_dependency_from_parent_conftest() {
        let test = PytestTestCase::with_files(
            "/src/project/conftest.py",
            &[
                (
                    "/src/conftest.py",
                    r#"
import pytest

@pytest.fixture
def resource(): ...
"#,
                ),
                (
                    "/src/project/conftest.py",
                    r#"
import pytest

@pytest.fixture
def consumer(resource): ...
"#,
                ),
            ],
        );

        let fixture = test.function("consumer");

        assert_snapshot!(fixture.fixture_resolution("resource"), @"
        info[pytest-fixture]: Resolve fixture for parameter
         --> src/project/conftest.py:5:14
          |
        5 | def consumer(resource): ...
          |              ^^^^^^^^ fixture requested here
        info: Found 1 fixture
         --> src/conftest.py:5:5
          |
        5 | def resource(): ...
          |     --------
        ");
    }

    #[test]
    fn creating_conftest_updates_fixture_resolution() {
        let mut test = PytestTestCase::new(
            "/src/project/test_example.py",
            r#"
def test_use(resource): ...
"#,
        );

        assert_snapshot!(test.function("test_use").fixture_resolution("resource"), @"No fixture resolved for parameter `resource`");

        test.write_file(
            "/src/project/conftest.py",
            r#"
import pytest

@pytest.fixture
def resource(): ...
"#,
        );
        assert_snapshot!(test.function("test_use").fixture_resolution("resource"), @"
        info[pytest-fixture]: Resolve fixture for parameter
         --> src/project/test_example.py:2:14
          |
        2 | def test_use(resource): ...
          |              ^^^^^^^^ fixture requested here
        info: Found 1 fixture
         --> src/project/conftest.py:5:5
          |
        5 | def resource(): ...
          |     --------
        ");
    }

    #[test]
    fn resolves_installed_core_plugins_in_registration_order() {
        let test = PytestTestCase::new(
            "/src/test_example.py",
            r#"
def test_use(core_value, tmp_path, tmpdir, unused_fixture, request): ...
"#,
        );

        let test_use = test.function("test_use");

        assert_snapshot!(test_use.fixture_resolution("core_value"), @"
        info[pytest-fixture]: Resolve fixture for parameter
         --> src/test_example.py:2:14
          |
        2 | def test_use(core_value, tmp_path, tmpdir, unused_fixture, request): ...
          |              ^^^^^^^^^^ fixture requested here
        info: Found 1 fixture
         --> .venv/lib/python3.13/site-packages/_pytest/override.py:5:5
          |
        5 | def core_value(): ...
          |     ----------
        ");

        assert_snapshot!(test_use.fixture_resolution("tmp_path"), @"
        info[pytest-fixture]: Resolve fixture for parameter
         --> src/test_example.py:2:26
          |
        2 | def test_use(core_value, tmp_path, tmpdir, unused_fixture, request): ...
          |                          ^^^^^^^^ fixture requested here
        info: Found 1 fixture
         --> .venv/lib/python3.13/site-packages/_pytest/tmpdir.py:5:5
          |
        5 | def tmp_path(): ...
          |     --------
        ");

        assert_snapshot!(test_use.fixture_resolution("tmpdir"), @"
        info[pytest-fixture]: Resolve fixture for parameter
         --> src/test_example.py:2:36
          |
        2 | def test_use(core_value, tmp_path, tmpdir, unused_fixture, request): ...
          |                                    ^^^^^^ fixture requested here
        info: Found 1 fixture
         --> .venv/lib/python3.13/site-packages/_pytest/legacypath.py:7:9
          |
        7 |     def tmpdir(): ...
          |         ------
        ");

        assert_snapshot!(test_use.fixture_resolution("unused_fixture"), @"No fixture resolved for parameter `unused_fixture`");
        assert_snapshot!(test_use.fixture_resolution("request"), @"No fixture resolved for parameter `request`");
        assert_eq!(
            test.global_plugin_files(),
            [
                "/.venv/lib/python3.13/site-packages/_pytest/baseplugin.py",
                "/.venv/lib/python3.13/site-packages/_pytest/legacypath.py",
                "/.venv/lib/python3.13/site-packages/_pytest/tmpdir.py",
                "/.venv/lib/python3.13/site-packages/_pytest/override.py",
            ]
        );
    }

    #[test]
    fn updating_core_plugin_registry_updates_fixture_resolution() {
        let mut test = PytestTestCase::with_config(
            "/src/test_example.py",
            &[(
                "/src/test_example.py",
                r#"
def test_use(core_value, tmp_path): ...
"#,
            )],
            r#"
essential_plugins = ("baseplugin",)
additional_plugins = ()
default_plugins = essential_plugins + additional_plugins
"#,
        );

        assert_snapshot!(test.function("test_use").fixture_resolution("core_value"), @"
        info[pytest-fixture]: Resolve fixture for parameter
         --> src/test_example.py:2:14
          |
        2 | def test_use(core_value, tmp_path): ...
          |              ^^^^^^^^^^ fixture requested here
        info: Found 1 fixture
         --> .venv/lib/python3.13/site-packages/_pytest/baseplugin.py:5:5
          |
        5 | def core_value(): ...
          |     ----------
        ");

        test.write_file(
            "/.venv/lib/python3.13/site-packages/_pytest/config/__init__.py",
            r#"
default_plugins = ("tmpdir",)
"#,
        );

        assert_snapshot!(test.function("test_use").fixture_resolution("core_value"), @"No fixture resolved for parameter `core_value`");
        assert_snapshot!(test.function("test_use").fixture_resolution("tmp_path"), @"
        info[pytest-fixture]: Resolve fixture for parameter
         --> src/test_example.py:2:26
          |
        2 | def test_use(core_value, tmp_path): ...
          |                          ^^^^^^^^ fixture requested here
        info: Found 1 fixture
         --> .venv/lib/python3.13/site-packages/_pytest/tmpdir.py:5:5
          |
        5 | def tmp_path(): ...
          |     --------
        ");
    }

    #[test]
    fn project_fixtures_shadow_installed_core_plugins() {
        let test = PytestTestCase::new(
            "/src/test_example.py",
            r#"
import pytest

@pytest.fixture
def core_value(): ...

def test_use(core_value): ...
"#,
        );

        let test_use = test.function("test_use");

        assert_snapshot!(test_use.fixture_resolution("core_value"), @"
        info[pytest-fixture]: Resolve fixture for parameter
         --> src/test_example.py:7:14
          |
        7 | def test_use(core_value): ...
          |              ^^^^^^^^^^ fixture requested here
        info: Found 1 fixture
         --> src/test_example.py:5:5
          |
        5 | def core_value(): ...
          |     ----------
        ");
    }

    #[test]
    fn declines_dynamic_core_plugin_registries() {
        let test = PytestTestCase::with_config(
            "/src/test_example.py",
            &[(
                "/src/test_example.py",
                r#"
def test_use(core_value): ...
"#,
            )],
            r#"
def plugins():
    return ("baseplugin",)

default_plugins = plugins()
"#,
        );

        let test_use = test.function("test_use");

        assert_snapshot!(test_use.fixture_resolution("core_value"), @"No fixture resolved for parameter `core_value`");
    }

    #[test]
    fn skips_invalid_core_plugin_names() {
        let test = PytestTestCase::with_config(
            "/src/test_example.py",
            &[(
                "/src/test_example.py",
                r#"
"#,
            )],
            r#"
default_plugins = ("not-valid", "baseplugin")
"#,
        );

        assert_eq!(
            test.global_plugin_files(),
            ["/.venv/lib/python3.13/site-packages/_pytest/baseplugin.py"]
        );
    }

    #[test]
    fn preserves_fixture_exposure_provenance_across_imports() {
        let test = PytestTestCase::with_files(
            "/src/test_example.py",
            &[
                (
                    "/src/fixtures.py",
                    r#"
import pytest

@pytest.fixture
def resource(): ...
"#,
                ),
                (
                    "/src/reexports.py",
                    r#"
from fixtures import resource as helper
"#,
                ),
                (
                    "/src/test_example.py",
                    r#"
from reexports import helper

def test_use(helper): ...
"#,
                ),
            ],
        );

        let fixture = test.function_definition("/src/fixtures.py", "resource");
        let alias = test.global_definition("/src/reexports.py", "helper");
        let imported_alias = test.global_definition("/src/test_example.py", "helper");

        let fixture_exposures = fixture_exposures_for_definition(&test.db, fixture);
        let fixture_exposure =
            assert_single_exposure(&fixture_exposures, "resource", fixture, fixture, None);
        assert_eq!(
            fixture_exposure.name_source(&test.db),
            FixtureNameSource::Binding(fixture)
        );

        let alias_exposures = fixture_exposures_for_definition(&test.db, alias);
        assert_single_exposure(&alias_exposures, "helper", alias, fixture, Some(fixture));

        let imported_exposures = fixture_exposures_for_definition(&test.db, imported_alias);
        assert_single_exposure(
            &imported_exposures,
            "helper",
            imported_alias,
            fixture,
            Some(alias),
        );

        let test_use = test.function("test_use");
        let request = test_use.parameter_definition("helper");
        let bindings = fixture_bindings_for_parameter(&test.db, request);
        let [binding] = bindings else {
            panic!("fixture request should have one binding");
        };
        assert_eq!(binding.fixture(), fixture);
        assert_eq!(binding.exposures(), imported_exposures);
    }

    #[test]
    fn preserves_explicit_fixture_name_declarations() {
        let test = PytestTestCase::with_files(
            "/src/test_example.py",
            &[
                (
                    "/src/fixtures.py",
                    r#"
import pytest

@pytest.fixture(name="resource")
def implementation(): ...
"#,
                ),
                (
                    "/src/test_example.py",
                    r#"
from fixtures import implementation as helper

def test_use(resource): ...
"#,
                ),
            ],
        );

        let fixture = test.function_definition("/src/fixtures.py", "implementation");
        let alias = test.global_definition("/src/test_example.py", "helper");
        let alias_exposures = fixture_exposures_for_definition(&test.db, alias);
        let alias_exposure =
            assert_single_exposure(&alias_exposures, "resource", alias, fixture, Some(fixture));

        let FixtureNameSource::Explicit {
            fixture: declaring_fixture,
            declaration: Some(declaration),
        } = alias_exposure.name_source(&test.db)
        else {
            panic!("literal fixture name should retain its declaration range");
        };
        assert_eq!(declaring_fixture, fixture);
        let source = ruff_db::source::source_text(&test.db, declaration.file());
        assert_eq!(&source[declaration.range()], "resource");

        let test_use = test.function("test_use");
        let request = test_use.parameter_definition("resource");
        let bindings = fixture_bindings_for_parameter(&test.db, request);
        let [binding] = bindings else {
            panic!("explicit fixture request should have one binding");
        };
        assert_eq!(binding.exposures(), alias_exposures);
    }

    #[test]
    fn excludes_unavailable_definitions_from_fixture_exposures() {
        let test = PytestTestCase::new(
            "/src/test_example.py",
            r#"
import pytest

@pytest.fixture
def resource(): ...

resource = None
"#,
        );
        let fixture = test.function_definition("/src/test_example.py", "resource");

        assert!(fixture_exposures_for_definition(&test.db, fixture).is_empty());
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

        fn with_files(path: &'static str, files: &[(&'static str, &'static str)]) -> Self {
            Self {
                db: pytest_db_with_files(files),
                path,
            }
        }

        fn with_files_and_src_roots(
            path: &'static str,
            files: &[(&'static str, &'static str)],
            src_roots: Vec<SystemPathBuf>,
        ) -> Self {
            Self {
                db: pytest_db_with_files_and_src_roots(files, src_roots),
                path,
            }
        }

        fn with_config(
            path: &'static str,
            files: &[(&'static str, &'static str)],
            config: &'static str,
        ) -> Self {
            Self {
                db: pytest_db_with_config(files, config),
                path,
            }
        }

        fn write_file(&mut self, path: &'static str, source: &'static str) {
            self.db
                .write_file(path, source)
                .expect("valid pytest test file update");
        }

        fn function<'test>(&'test self, name: &str) -> PytestTestFunction<'test> {
            PytestTestFunction {
                test: self,
                name: name.to_owned(),
            }
        }

        fn global_plugin_files(&self) -> Vec<String> {
            let file = system_path_to_file(&self.db, self.path).expect("test file exists");
            let file = self.db.program_file(file);
            pytest_global_plugin_files(&self.db, file.program(&self.db))
                .iter()
                .map(|file| {
                    file.file(&self.db)
                        .path(&self.db)
                        .to_string()
                        .replace('\\', "/")
                })
                .collect()
        }

        fn function_definition<'db>(&'db self, path: &str, name: &str) -> Definition<'db> {
            let file = system_path_to_file(&self.db, path).expect("test file exists");
            let file = self.db.program_file(file);
            let module = parsed_module(&self.db, file.python_file(&self.db)).load(&self.db);
            let function = find_function(module.suite(), name).expect("function exists");
            semantic_index(&self.db, file).expect_single_definition(function)
        }

        fn global_definition<'db>(&'db self, path: &str, name: &str) -> Definition<'db> {
            let file = system_path_to_file(&self.db, path).expect("test file exists");
            let file = self.db.program_file(file);
            end_of_scope_definition(&self.db, file, name).expect("global definition exists")
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

    fn assert_single_exposure<'a, 'db>(
        exposures: &'a [FixtureExposure<'db>],
        name: &str,
        local_binding: Definition<'db>,
        fixture: Definition<'db>,
        source_binding: Option<Definition<'db>>,
    ) -> &'a FixtureExposure<'db> {
        let [exposure] = exposures else {
            panic!("expected exactly one fixture exposure, got {exposures:#?}");
        };
        assert_eq!(exposure.name(), name);
        assert_eq!(exposure.local_binding(), local_binding);
        assert_eq!(exposure.fixture(), fixture);
        assert_eq!(exposure.source_binding(), source_binding);
        exposure
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
        pytest_db_with_files_and_src_roots(files, vec![SystemPathBuf::from("/src")])
    }

    fn pytest_db_with_files_and_src_roots(
        files: &[(&'static str, &'static str)],
        src_roots: Vec<SystemPathBuf>,
    ) -> TestDb {
        pytest_db_with_config_and_src_roots(
            files,
            src_roots,
            r#"
essential_plugins = ("baseplugin",)
default_plugins = (
    *essential_plugins,
    "legacypath",
    "tmpdir",
    "_pytest.tmpdir",
    "override",
    "_pytest.override",
)
"#,
        )
    }

    fn pytest_db_with_config(
        files: &[(&'static str, &'static str)],
        config: &'static str,
    ) -> TestDb {
        pytest_db_with_config_and_src_roots(files, vec![SystemPathBuf::from("/src")], config)
    }

    fn pytest_db_with_config_and_src_roots(
        files: &[(&'static str, &'static str)],
        src_roots: Vec<SystemPathBuf>,
        config: &'static str,
    ) -> TestDb {
        let mut builder = TestDbBuilder::new()
            .with_src_roots(src_roots)
            .with_third_party_packages()
            .with_file(
                "/.venv/lib/python3.13/site-packages/_pytest/__init__.py",
                r#"
"#,
            )
            .with_file(
                "/.venv/lib/python3.13/site-packages/_pytest/__init__.pyi",
                r#"
"#,
            )
            .with_file(
                "/.venv/lib/python3.13/site-packages/_pytest/config/__init__.py",
                config,
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

class _ParametrizeMarkDecorator(MarkDecorator): ...

class MarkGenerator:
    parametrize: _ParametrizeMarkDecorator
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
            .with_file(
                "/.venv/lib/python3.13/site-packages/_pytest/baseplugin.py",
                r#"
from _pytest.fixtures import fixture

@fixture
def core_value(): ...
"#,
            )
            .with_file(
                "/.venv/lib/python3.13/site-packages/_pytest/legacypath.py",
                r#"
from _pytest.fixtures import fixture

class LegacyTmpdirPlugin:
    @staticmethod
    @fixture
    def tmpdir(): ...
"#,
            )
            .with_file(
                "/.venv/lib/python3.13/site-packages/_pytest/tmpdir.py",
                r#"
from _pytest.fixtures import fixture

@fixture
def tmp_path(): ...
"#,
            )
            .with_file(
                "/.venv/lib/python3.13/site-packages/_pytest/override.py",
                r#"
from _pytest.fixtures import fixture

@fixture
def core_value(): ...
"#,
            )
            .with_file(
                "/.venv/lib/python3.13/site-packages/_pytest/unused.py",
                r#"
from _pytest.fixtures import fixture

@fixture
def unused_fixture(): ...
"#,
            );
        for (path, source) in files {
            builder = builder.with_file(*path, source);
        }
        builder.build().expect("valid pytest test database")
    }
}
