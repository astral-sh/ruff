# Pytest fixture support

Status: semantic provider stack, Go to Definition, and Find References implemented; type
inference planned

Tracking issue: [astral-sh/ty#1986](https://github.com/astral-sh/ty/issues/1986)

## Purpose

Pytest injects fixtures by matching parameter names to fixture providers. Those relationships are
not represented by ordinary Python name binding, so ty needs an explicit semantic model for
navigation, references, type inference, completion, and inlay hints.

This document records the architecture, delivery sequence, and follow-on work. It is a living plan:
completed changes should be checked off and annotated with their Jujutsu change ID and pull request.
Material changes to the architecture or delivery boundaries should be recorded in the decision log.

Test-item discovery and test-runner UI are outside the scope of this plan.

## Design influences

[Pyrefly](https://github.com/facebook/pyrefly/blob/b52e609470554983d33a2cb84074f619d5272d85/website/docs/pytest.mdx) and
[Pylance](https://github.com/microsoft/pylance-release/blob/e7943247c666c1032b6ef6460fe1ef07b9d3b54c/testing/single/tests/pytest_test.py)
model fixture parameters as framework-specific relationships instead of ordinary imports. ty should
do the same while preserving its existing Python definitions.

The design resembles ty's Pydantic support in its use of known third-party identities, dedicated
semantic metadata, cycle-safe queries, and links back to source definitions. It intentionally does
not use Pydantic's synthetic-member or code-generation machinery: fixture lookup depends on the
requesting file and pytest's provider hierarchy rather than transforming a class.

## Reference architecture

### Semantic model

The model separates four concepts:

- A **fixture declaration** is the canonical decorated function and its fixture metadata, including
    an explicit public name.
- A **fixture exposure** is a name made available by one class, module, conftest, or plugin provider.
    Multiple exposures can point to the same declaration.
- A **fixture request** is an eligible test or fixture parameter and the context from which pytest
    resolves it.
- A **fixture binding** links the parameter's ordinary Python definition to the canonical fixture
    declaration selected by pytest lookup.

The decorated source function definition is the fixture's identity. Import aliases, reexports, and
side-by-side stub declarations create exposures; they do not create new fixture identities. A stub
definition is mapped to its ordinary source definition before fixture metadata is inspected.
Without an explicit decorator `name=`, the local binding or import alias supplies the exposure
name. A static `name=` overrides that name. Dynamic names are left unresolved rather than guessed.

The parameter retains its ordinary Python definition. Fixture-aware consumers follow the explicit
binding when appropriate; general definition, reference, and rename behavior remains unchanged.

### Provider lookup

Providers are searched in this order:

1. The requesting test class, then its base classes in method-resolution order.
1. The current module.
1. The nearest applicable `conftest.py`.
1. Successive ancestor conftests.
1. Core fixtures from the installed pytest package.
1. Additional globally registered pytest plugins, in reverse registration order.

The first non-empty provider layer for a name wins. If multiple definitions are equally reachable
within that layer, all remain possible bindings; definitions from shadowed layers are not mixed in.
When an overriding fixture requests its own public name, its own exposure is skipped so lookup can
continue to the next provider.

Core fixtures come from the installed pytest distribution rather than a ty-maintained fixture list.
ty statically interprets `_pytest.config.default_plugins`, including literal string sequences,
same-module aliases, and starred expansion. It preserves registration order and fails closed when
the registry is dynamic or unsupported. The special `request` fixture is a synthetic exposure: it
participates in lookup and completion, but has no fixture declaration to navigate to or find as a
project-wide reference. The type-inference delivery assigns `pytest.FixtureRequest` to eligible
`request` parameters.

The provider abstraction retains an ordered global-plugin layer even though the first delivery only
populates it with pytest's core plugins. Later registration sources can feed that layer without
changing fixture lookup. Plugin disabling, `pytest_plugins`, installed `pytest11` entry points, and
other dynamic registration sources remain follow-on work.

### Query boundaries and incrementality

The initial cross-crate contract is equivalent to:

```rust
fixture_bindings_for_parameter(
    db,
    parameter_definition,
) -> &[FixtureBinding]
```

The query must be safe to call from type inference in a later delivery. Fixture discovery therefore
uses semantic-index and use-def facts plus cycle-safe function-decorator and static class-MRO
queries; it must not depend on full inferred member types.

Provider lookup is demand-driven. There is no project-wide semantic fixture index. Conftests are
found by probing the exact `conftest.py` path in each ancestor directory and stopping at the selected
file root. IDE operations that already need a project scan, such as Find References, resolve each
candidate parameter in its own context.

Find References adds a compact, tracked per-file candidate summary rather than a persistent global
reverse index. Each summary contains fixture declarations, fixture exposures, and eligible fixture
request parameters. The IDE scans project files, exact registered provider modules, and canonical
target files; it does not scan every installed package.

The semantic reference contract is equivalent to:

```rust
fixture_reference_targets(
    db,
    definition,
) -> &[Definition]
```

It maps a fixture declaration or exposure to its canonical source fixture definition, and a request
parameter to the declarations selected in that request's context. Side-by-side stub definitions are
normalized through ty's ordinary stub-to-source mapping before this comparison. A parameter that is
not a resolved fixture request has no fixture target, nor does the synthetic `request` exposure. The
`Definition` values themselves are the identities; the implementation does not introduce a second
fixture-reference identity type.

For an ambiguous request that can select fixtures `A` or `B`, starting Find References on the request
searches for both `A` and `B`. Starting on `A` includes the ambiguous request and its lexical uses but
does not add `B`, or requests that only select `B`, to the search. The target set is fixed from the
location where the operation started; reference discovery does not compute a transitive closure.

Type inference uses a separate semantic contract equivalent to:

```rust
fixture_value_type_for_parameter(
    db,
    parameter_definition,
) -> Option<Type>
```

`None` means the parameter is not a resolved fixture request. `Some(Type::Unknown)` means it is a
fixture request whose injected value is unknown. Other values are the inferred injected types. This
query remains distinct from fixture bindings so navigation never needs to infer fixture bodies.

### Consumer contracts

- **Go to Definition** follows a fixture binding and then applies ordinary stub-to-source mapping.
- **Find References** compares canonical fixture identities and unions fixture declarations,
    exposures, requests, and ordinary lexical references. The static string in `name="..."` is not a
    reference. Rename remains a separate feature.
- **Type inference** derives the injected value from the fixture's existing ty return type, or the
    yielded element type for a generator fixture. It changes the local parameter binding, not the
    function's callable signature.
- **Completion** enumerates visible exposures and inserts a parameter name without adding an import.
- **Inlay hints** display an already inferred fixture type and may offer an opt-in annotation edit.

### Rejected approaches

- An IDE-only resolver would duplicate semantics and could not support type inference or completion.
- Replacing a parameter's Python definition would accidentally change references and rename.
- An eager project-wide index would create broad Salsa dependencies for a path-scoped lookup.
- Modeling fixtures as generated Pydantic-style members would obscure pytest's provider hierarchy.

## Delivery graph

The plan and initial implementation are sibling branches from the same base:

```text
common base
├── pytest-fixture-plan
│   └── [ty] Record the pytest fixture implementation plan
└── pytest-fixture-goto-definition
    ├── Model local pytest fixture bindings
    ├── Resolve imported pytest fixture exposures
    ├── Resolve pytest fixtures through conftest
    ├── Resolve installed core pytest fixture providers
    ├── Support go-to-def for pytest fixtures
    ├── Model pytest fixture reference relationships
    ├── Find references to pytest fixtures
    ├── Model pytest fixture value types
    └── Infer pytest fixture parameter types
```

The first checkpoint completes the semantic provider stack before exposing Go to Definition. The
navigation adapter was implemented against local bindings first, but it is reordered after the
import, conftest, and core-provider changes because it consumes the provider-independent binding
query. Its integration tests cover every provider kind in the same change; there is no separate IDE
verification change.

## Initial implementation sequence

### 1. Model local pytest fixture bindings

- [x] Complete — jj change `zvrlmmkz`

- Add canonical identities for the relevant `pytest`, `_pytest.fixtures`, and
    `_pytest.mark.structures` modules.

- Recognize canonical `pytest.fixture` and deprecated `yield_fixture` decorators in bare, called,
    and imported-alias forms.

- Recognize a static explicit fixture name and decline dynamic names.

- Model declarations, exposures, requests, and bindings in `ty_python_semantic`.

- Recognize fixture requests in fixture functions and default pytest test functions and methods.

- Include positional-or-keyword and keyword-only parameters without defaults.

- Exclude positional-only, variadic, defaulted, nested, ordinary, and non-collected parameters.

- Exclude statically recognized direct parametrization through the canonical pytest `mark` object,
    including imported aliases; retain indirect fixture requests.

- Resolve fixture dependencies and class-MRO-before-module shadowing.

- Add semantic tests that call the binding query directly.

This change is semantic-only and remains safe if no IDE consumer lands.

### 2. Resolve imported pytest fixture exposures

- [x] Complete — jj change `slrnsurx`

- Generalize recursive import-definition resolution for semantic consumers.

- Build exposures from end-of-scope definitions so overwritten imports are ignored.

- Support explicit imports, aliases, chained reexports, and modeled star imports.

- Use the local alias unless the decorator provides an explicit fixture name.

- Preserve canonical declaration identity and conditional definitions in the winning layer.

- Map side-by-side stub definitions to their source declarations before inspecting fixture
    decorators, making the decorated source function the canonical identity.

- Add semantic tests for import and shadowing behavior.

This change is semantic-only and remains safe if no IDE consumer lands.

### 3. Resolve fixtures through the conftest hierarchy

- [x] Complete — jj change `rzmrwnqz`

- Add tracked nearest-to-outermost conftest discovery, bounded by the selected file root.

- Reuse declaration and import exposure logic in each conftest.

- Apply class, module, nearest-conftest, and outer-conftest precedence.

- Exclude sibling conftests and avoid including the current conftest twice.

- Support same-name fixture overrides that request the next outer provider.

- Add semantic tests for nested directories, shadowing, fallback, imported fixtures, root
    boundaries, and file updates.

This change is semantic-only and remains safe if no IDE consumer lands.

### 4. Resolve installed core pytest fixture providers

- [x] Complete — jj change `skpxvsvw`

- Statically interpret the installed `_pytest.config.default_plugins` registry rather than
    maintaining a fixture list in ty.

- Preserve plugin registration order and fixture shadowing within the global-provider layer.

- Reuse declaration and import exposure logic in each registered core plugin.

- Fail closed for unsupported or dynamic registry expressions.

- Model `request` as a synthetic exposure with no source target; assign its
    `pytest.FixtureRequest` value type when fixture inference is enabled.

- Verify that project and conftest fixtures shadow core providers.

This change is semantic-only and remains safe if no IDE consumer lands. It also establishes the
ordered global-provider abstraction that later plugin registration sources can reuse.

### 5. Navigate to pytest fixtures

- [x] Complete — jj change `youolnmt`

- Make only Go to Definition's parameter interpretation consume fixture bindings.

- Navigate to canonical fixture declarations and apply existing stub-to-source mapping.

- Fall back to ordinary parameter navigation when no fixture binds.

- Leave shared definitions, references, rename, hover, declaration navigation, and document
    highlights unchanged.

- Keep cursor tests for tests, fixture dependencies, inherited class fixtures, shadowing, explicit
    names, indirect parametrization (including imported `mark` aliases), imported fixtures,
    side-by-side stub/source providers, conftest fixtures, core fixtures, and negative cases.

This checkpoint provides Go to Definition for every fixture provider modeled by the preceding
semantic changes.

## Follow-on work

### 6. Find fixture references

- [x] Complete in the working stack — semantic jj change `tvrmynpp`; IDE jj change `ozuqtklr`;
    tracked by [astral-sh/ty#4115](https://github.com/astral-sh/ty/issues/4115)

Delivered as two independently reviewable changes:

1. **Model pytest fixture reference relationships.** Add the per-file candidate summary and
    `fixture_reference_targets` semantic API. Test declarations, exposures, requesting parameters,
    shadowing, ambiguous winning-layer providers, and fixed initial target sets directly in
    `ty_python_semantic`.
1. **Find references to pytest fixtures.** Add the `ty_ide` project scan, resolve every candidate
    request in its own context, compare canonical definitions, and reuse ordinary lexical-reference
    collection for source uses. Scan project files, registered core-provider modules, and the exact
    canonical target files without scanning unrelated installed packages. Add end-to-end tests
    across files and provider kinds, including inherited class providers, imported direct
    parametrization, and side-by-side stub/source modules.

The result includes:

- The decorated fixture function and ordinary references to its Python binding.
- Import and reexport exposures of the fixture.
- Test parameters and dependent-fixture parameters that request it.
- Ordinary lexical uses of every matching request parameter.

The literal string in `@pytest.fixture(name="public_name")` is not a reference. Autouse activation
does not synthesize a reference. Static strings passed to `usefixtures` or `getfixturevalue` are
deferred.

With `includeDeclaration: false`, omit fixture declarations, exposure declarations, and request
parameter declarations, but retain ordinary read uses. Document highlights remain unchanged.
Fixture rename is a separate feature and must not be implemented as an extension of Find References.

### 7. Infer injected fixture types

- [ ] Planned; tracked by
    [astral-sh/ty#4116](https://github.com/astral-sh/ty/issues/4116)

Deliver as two independently reviewable changes:

1. **Model pytest fixture value types.** Add `fixture_value_type_for_parameter` and test it directly.
    The first delivery supports ordinary synchronous fixtures and synchronous generator fixtures.
1. **Infer pytest fixture parameter types.** Apply the query to unannotated fixture request
    parameters, extend the external pytest mdtest, and verify `reveal_type`, diagnostics, hover, and
    member completion.

Use ty's existing inferred return type for an ordinary synchronous fixture. For a generator fixture,
use ty's generator classification and extract the yielded element type. Do not add pytest-specific
body-return inference: an unannotated fixture whose ordinary ty return type is `Unknown` continues to
provide `Unknown`. A stub-only fixture also remains `Unknown` unless normal source mapping finds its
implementation.

Equally viable providers in the winning layer contribute a union; shadowed providers do not.
`Unknown` remains a member of such a union rather than being discarded. Cycles resolve safely to
`Unknown` without a pytest-specific cycle diagnostic.

An explicit parameter annotation continues to determine the local parameter type. A later,
independently shippable diagnostic may report annotations that are incompatible with the injected
fixture value, but that diagnostic is not required to ship inference. Async fixture inference is
deferred until pytest-asyncio registration, decorator, and mode behavior are modeled accurately.

### 8. Complete fixture parameter names

- [ ] Planned; tracked by
    [astral-sh/ty#4117](https://github.com/astral-sh/ty/issues/4117)

Expected changes:

1. Expose precedence-aware enumeration of visible fixture exposures.
1. Offer those names in eligible test and fixture signatures without adding imports.

Exclude existing parameters and direct-parametrization names.

### 9. Show fixture parameter inlay hints

- [ ] Planned; tracked by
    [astral-sh/ty#4120](https://github.com/astral-sh/ty/issues/4120)

- Display the inferred injected type for unannotated fixture requests.

- Add a dedicated `inlayHints.pytestParameters` setting, disabled by default.

- Suppress exactly `Unknown`; retain `Any`, `Never`, and unions containing `Unknown`.

- When requested by the client, offer an annotation insertion edit and use the normal importer if
    the type can be rendered and imported safely. Fall back to a display-only hint otherwise.

### 10. Add configuration and plugin fidelity

- [ ] Planned

Deliver these as independent later changes:

- Custom `python_files`, `python_functions`, and `python_classes` patterns.
- Pytest root and `confcutdir` behavior.
- Modules named by `pytest_plugins`.
- Installed entry-point plugins.
- Plugin autoloading, disabling, and other registration sources.
- Wrapped and custom fixture decorators.
- Other decorators that rewrite test signatures.
- Imported fixture dependencies registered in multiple runtime contexts.
- Pytest-asyncio registration, decorators, and mode behavior.

## Verification

Each implementation change must pass focused tests and scoped Clippy for every modified ty crate.
The completed stack must pass the relevant nextest suites, `prek` for every changed file, snapshot
review, and the repository's review-and-iterate workflow.

## Living-plan updates

Implementation branches do not edit this file. After a change lands, use a documentation-only
update to:

- Check off the corresponding item.
- Record the final Jujutsu change ID and pull request.
- Record deviations from the planned boundary.
- Amend later steps only when new evidence changes their dependencies or architecture.

## Decision log

- Keep the reference architecture and full delivery plan in a persistent repository file.
- Keep the plan branch and initial implementation branch as siblings from the same base.
- The initial checkpoint was first implemented as same-file semantic bindings followed immediately
    by Go to Definition.
- Before landing navigation, complete imported exposures and conftest lookup as semantic-only
    changes between `zvrlmmkz` and `youolnmt`.
- Resolve installed core fixture providers in another semantic-only change before `youolnmt`.
- Keep `youolnmt` as a generic binding consumer and fold all IDE integration verification into that
    change instead of retaining a separate test-only descendant.
- Build core providers from the installed pytest registry, fail closed on unsupported registry
    forms, and preserve an ordered global-provider abstraction for later plugin sources.
- Represent pytest's special `request` fixture as a synthetic exposure rather than a
    `FixtureBinding` with an invented source target, and give requests their
    `pytest.FixtureRequest` value type in the inference delivery.
- Keep Find References separate from rename. Include fixture declarations, exposures, requesting
    parameters, and their lexical uses, but exclude decorator-name strings and autouse activation.
- Deliver Find References as a semantic relationship-and-candidate-summary change followed by an
    IDE project-scan change.
- Keep the initial canonical fixture target set fixed throughout each Find References search.
- Search inherited class fixtures in method-resolution order before the requesting module.
- Recognize direct parametrization by the canonical pytest `MarkGenerator`, including an imported
    `mark` alias, rather than by attribute spelling alone.
- Normalize side-by-side stubs to decorated source declarations before fixture identity comparison;
    retain the stub declaration as an exposure while using the source definition as the canonical
    target.
- Derive fixture value types through a semantic query that preserves ty's existing return-type
    behavior, then apply that query to unannotated request parameters in a separate change.
- Limit the first inference delivery to synchronous functions and generators. Defer annotation
    compatibility diagnostics and async-plugin fidelity to independent follow-ups.
