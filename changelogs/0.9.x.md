# Changelog 0.9.x

## 0.9.0

Check out the [blog post](https://astral.sh/blog/ruff-v0.9.0) for a migration guide and overview of the changes!

### Breaking changes

Ruff now formats your code according to the 2025 style guide. As a result, your code might now get formatted differently. See the formatter section for a detailed list of changes.

This release doesn’t remove or remap any existing stable rules.

### Stabilization

The following rules have been stabilized and are no longer in preview:

- [`stdlib-module-shadowing`](https://docs.astral.sh/ruff/rules/stdlib-module-shadowing/) (`A005`).
    This rule has also been renamed: previously, it was called `builtin-module-shadowing`.
- [`builtin-lambda-argument-shadowing`](https://docs.astral.sh/ruff/rules/builtin-lambda-argument-shadowing/) (`A006`)
- [`slice-to-remove-prefix-or-suffix`](https://docs.astral.sh/ruff/rules/slice-to-remove-prefix-or-suffix/) (`FURB188`)
- [`boolean-chained-comparison`](https://docs.astral.sh/ruff/rules/boolean-chained-comparison/) (`PLR1716`)
- [`decimal-from-float-literal`](https://docs.astral.sh/ruff/rules/decimal-from-float-literal/) (`RUF032`)
- [`post-init-default`](https://docs.astral.sh/ruff/rules/post-init-default/) (`RUF033`)
- [`useless-if-else`](https://docs.astral.sh/ruff/rules/useless-if-else/) (`RUF034`)

The following behaviors have been stabilized:

- [`pytest-parametrize-names-wrong-type`](https://docs.astral.sh/ruff/rules/pytest-parametrize-names-wrong-type/) (`PT006`): Detect [`pytest.parametrize`](https://docs.pytest.org/en/7.1.x/how-to/parametrize.html#parametrize) calls outside decorators and calls with keyword arguments.
- [`module-import-not-at-top-of-file`](https://docs.astral.sh/ruff/rules/module-import-not-at-top-of-file/) (`E402`): Ignore [`pytest.importorskip`](https://docs.pytest.org/en/7.1.x/reference/reference.html#pytest-importorskip) calls between import statements.
- [`mutable-dataclass-default`](https://docs.astral.sh/ruff/rules/mutable-dataclass-default/) (`RUF008`) and [`function-call-in-dataclass-default-argument`](https://docs.astral.sh/ruff/rules/function-call-in-dataclass-default-argument/) (`RUF009`): Add support for [`attrs`](https://www.attrs.org/en/stable/).
- [`bad-version-info-comparison`](https://docs.astral.sh/ruff/rules/bad-version-info-comparison/) (`PYI006`): Extend the rule to check non-stub files.

The following fixes or improvements to fixes have been stabilized:

- [`redundant-numeric-union`](https://docs.astral.sh/ruff/rules/redundant-numeric-union/) (`PYI041`)
- [`duplicate-union-members`](https://docs.astral.sh/ruff/rules/duplicate-union-member/) (`PYI016`)

### Formatter

This release introduces the new 2025 stable style ([#13371](https://github.com/astral-sh/ruff/issues/13371)), stabilizing the following changes:

- Format expressions in f-string elements ([#7594](https://github.com/astral-sh/ruff/issues/7594))
- Alternate quotes for strings inside f-strings ([#13860](https://github.com/astral-sh/ruff/pull/13860))
- Preserve the casing of hex codes in f-string debug expressions ([#14766](https://github.com/astral-sh/ruff/issues/14766))
- Choose the quote style for each string literal in an implicitly concatenated f-string rather than for the entire string ([#13539](https://github.com/astral-sh/ruff/pull/13539))
- Automatically join an implicitly concatenated string into a single string literal if it fits on a single line ([#9457](https://github.com/astral-sh/ruff/issues/9457))
- Remove the [`ISC001`](https://docs.astral.sh/ruff/rules/single-line-implicit-string-concatenation/) incompatibility warning ([#15123](https://github.com/astral-sh/ruff/pull/15123))
- Prefer parenthesizing the `assert` message over breaking the assertion expression ([#9457](https://github.com/astral-sh/ruff/issues/9457))
- Automatically parenthesize over-long `if` guards in `match` `case` clauses ([#13513](https://github.com/astral-sh/ruff/pull/13513))
- More consistent formatting for `match` `case` patterns ([#6933](https://github.com/astral-sh/ruff/issues/6933))
- Avoid unnecessary parentheses around return type annotations ([#13381](https://github.com/astral-sh/ruff/pull/13381))
- Keep the opening parentheses on the same line as the `if` keyword for comprehensions where the condition has a leading comment ([#12282](https://github.com/astral-sh/ruff/pull/12282))
- More consistent formatting for `with` statements with a single context manager for Python 3.8 or older ([#10276](https://github.com/astral-sh/ruff/pull/10276))
- Correctly calculate the line-width for code blocks in docstrings when using `max-doc-code-line-length = "dynamic"` ([#13523](https://github.com/astral-sh/ruff/pull/13523))

### Preview features

- \[`flake8-bugbear`\] Implement `class-as-data-structure` (`B903`) ([#9601](https://github.com/astral-sh/ruff/pull/9601))
- \[`flake8-type-checking`\] Apply `quoted-type-alias` more eagerly in `TYPE_CHECKING` blocks and ignore it in stubs (`TC008`) ([#15180](https://github.com/astral-sh/ruff/pull/15180))
- \[`pylint`\] Ignore `eq-without-hash` in stub files (`PLW1641`) ([#15310](https://github.com/astral-sh/ruff/pull/15310))
- \[`pyupgrade`\] Split `UP007` into two individual rules: `UP007` for `Union` and `UP045` for `Optional` (`UP007`, `UP045`) ([#15313](https://github.com/astral-sh/ruff/pull/15313))
- \[`ruff`\] New rule that detects classes that are both an enum and a `dataclass` (`RUF049`) ([#15299](https://github.com/astral-sh/ruff/pull/15299))
- \[`ruff`\] Recode `RUF025` to `RUF037` (`RUF037`) ([#15258](https://github.com/astral-sh/ruff/pull/15258))

### Rule changes

- \[`flake8-builtins`\] Ignore [`stdlib-module-shadowing`](https://docs.astral.sh/ruff/rules/stdlib-module-shadowing/) in stub files(`A005`) ([#15350](https://github.com/astral-sh/ruff/pull/15350))
- \[`flake8-return`\] Add support for functions returning `typing.Never` (`RET503`) ([#15298](https://github.com/astral-sh/ruff/pull/15298))

### Server

- Improve the observability by removing the need for the ["trace" value](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#traceValue) to turn on or off logging. The server logging is solely controlled using the [`logLevel` server setting](https://docs.astral.sh/ruff/editors/settings/#loglevel)
    which defaults to `info`. This addresses the issue where users were notified about an error and told to consult the log, but it didn’t contain any messages. ([#15232](https://github.com/astral-sh/ruff/pull/15232))
- Ignore diagnostics from other sources for code action requests ([#15373](https://github.com/astral-sh/ruff/pull/15373))

### CLI

- Improve the error message for `--config key=value` when the `key` is for a table and it’s a simple `value`

### Bug fixes

- \[`eradicate`\] Ignore metadata blocks directly followed by normal blocks (`ERA001`) ([#15330](https://github.com/astral-sh/ruff/pull/15330))
- \[`flake8-django`\] Recognize other magic methods (`DJ012`) ([#15365](https://github.com/astral-sh/ruff/pull/15365))
- \[`pycodestyle`\] Avoid false positives related to type aliases (`E252`) ([#15356](https://github.com/astral-sh/ruff/pull/15356))
- \[`pydocstyle`\] Avoid treating newline-separated sections as sub-sections (`D405`) ([#15311](https://github.com/astral-sh/ruff/pull/15311))
- \[`pyflakes`\] Remove call when removing final argument from `format` (`F523`) ([#15309](https://github.com/astral-sh/ruff/pull/15309))
- \[`refurb`\] Mark fix as unsafe when the right-hand side is a string (`FURB171`) ([#15273](https://github.com/astral-sh/ruff/pull/15273))
- \[`ruff`\] Treat `)` as a regex metacharacter (`RUF043`, `RUF055`) ([#15318](https://github.com/astral-sh/ruff/pull/15318))
- \[`ruff`\] Parenthesize the `int`-call argument when removing the `int` call would change semantics (`RUF046`) ([#15277](https://github.com/astral-sh/ruff/pull/15277))

## 0.9.1

### Preview features

- \[`pycodestyle`\] Run `too-many-newlines-at-end-of-file` on each cell in notebooks (`W391`) ([#15308](https://github.com/astral-sh/ruff/pull/15308))
- \[`ruff`\] Omit diagnostic for shadowed private function parameters in `used-dummy-variable` (`RUF052`) ([#15376](https://github.com/astral-sh/ruff/pull/15376))

### Rule changes

- \[`flake8-bugbear`\] Improve `assert-raises-exception` message (`B017`) ([#15389](https://github.com/astral-sh/ruff/pull/15389))

### Formatter

- Preserve trailing end-of line comments for the last string literal in implicitly concatenated strings ([#15378](https://github.com/astral-sh/ruff/pull/15378))

### Server

- Fix a bug where the server and client notebooks were out of sync after reordering cells ([#15398](https://github.com/astral-sh/ruff/pull/15398))

### Bug fixes

- \[`flake8-pie`\] Correctly remove wrapping parentheses (`PIE800`) ([#15394](https://github.com/astral-sh/ruff/pull/15394))
- \[`pyupgrade`\] Handle comments and multiline expressions correctly (`UP037`) ([#15337](https://github.com/astral-sh/ruff/pull/15337))

## 0.9.2

### Preview features

- \[`airflow`\] Fix typo "security_managr" to "security_manager" (`AIR303`) ([#15463](https://github.com/astral-sh/ruff/pull/15463))
- \[`airflow`\] extend and fix AIR302 rules ([#15525](https://github.com/astral-sh/ruff/pull/15525))
- \[`fastapi`\] Handle parameters with `Depends` correctly (`FAST003`) ([#15364](https://github.com/astral-sh/ruff/pull/15364))
- \[`flake8-pytest-style`\] Implement pytest.warns diagnostics (`PT029`, `PT030`, `PT031`) ([#15444](https://github.com/astral-sh/ruff/pull/15444))
- \[`flake8-pytest-style`\] Test function parameters with default arguments (`PT028`) ([#15449](https://github.com/astral-sh/ruff/pull/15449))
- \[`flake8-type-checking`\] Avoid false positives for `|` in `TC008` ([#15201](https://github.com/astral-sh/ruff/pull/15201))

### Rule changes

- \[`flake8-todos`\] Allow VSCode GitHub PR extension style links in `missing-todo-link` (`TD003`) ([#15519](https://github.com/astral-sh/ruff/pull/15519))
- \[`pyflakes`\] Show syntax error message for `F722` ([#15523](https://github.com/astral-sh/ruff/pull/15523))

### Formatter

- Fix curly bracket spacing around f-string expressions containing curly braces ([#15471](https://github.com/astral-sh/ruff/pull/15471))
- Fix joining of f-strings with different quotes when using quote style `Preserve` ([#15524](https://github.com/astral-sh/ruff/pull/15524))

### Server

- Avoid indexing the same workspace multiple times ([#15495](https://github.com/astral-sh/ruff/pull/15495))
- Display context for `ruff.configuration` errors ([#15452](https://github.com/astral-sh/ruff/pull/15452))

### Configuration

- Remove `flatten` to improve deserialization error messages ([#15414](https://github.com/astral-sh/ruff/pull/15414))

### Bug fixes

- Parse triple-quoted string annotations as if parenthesized ([#15387](https://github.com/astral-sh/ruff/pull/15387))
- \[`fastapi`\] Update `Annotated` fixes (`FAST002`) ([#15462](https://github.com/astral-sh/ruff/pull/15462))
- \[`flake8-bandit`\] Check for `builtins` instead of `builtin` (`S102`, `PTH123`) ([#15443](https://github.com/astral-sh/ruff/pull/15443))
- \[`flake8-pathlib`\] Fix `--select` for `os-path-dirname` (`PTH120`) ([#15446](https://github.com/astral-sh/ruff/pull/15446))
- \[`ruff`\] Fix false positive on global keyword (`RUF052`) ([#15235](https://github.com/astral-sh/ruff/pull/15235))

## 0.9.3

### Preview features

- \[`airflow`\] Argument `fail_stop` in DAG has been renamed as `fail_fast` (`AIR302`) ([#15633](https://github.com/astral-sh/ruff/pull/15633))
- \[`airflow`\] Extend `AIR303` with more symbols ([#15611](https://github.com/astral-sh/ruff/pull/15611))
- \[`flake8-bandit`\] Report all references to suspicious functions (`S3`) ([#15541](https://github.com/astral-sh/ruff/pull/15541))
- \[`flake8-pytest-style`\] Do not emit diagnostics for empty `for` loops (`PT012`, `PT031`) ([#15542](https://github.com/astral-sh/ruff/pull/15542))
- \[`flake8-simplify`\] Avoid double negations (`SIM103`) ([#15562](https://github.com/astral-sh/ruff/pull/15562))
- \[`pyflakes`\] Fix infinite loop with unused local import in `__init__.py` (`F401`) ([#15517](https://github.com/astral-sh/ruff/pull/15517))
- \[`pylint`\] Do not report methods with only one `EM101`-compatible `raise` (`PLR6301`) ([#15507](https://github.com/astral-sh/ruff/pull/15507))
- \[`pylint`\] Implement `redefined-slots-in-subclass` (`W0244`) ([#9640](https://github.com/astral-sh/ruff/pull/9640))
- \[`pyupgrade`\] Add rules to use PEP 695 generics in classes and functions (`UP046`, `UP047`) ([#15565](https://github.com/astral-sh/ruff/pull/15565), [#15659](https://github.com/astral-sh/ruff/pull/15659))
- \[`refurb`\] Implement `for-loop-writes` (`FURB122`) ([#10630](https://github.com/astral-sh/ruff/pull/10630))
- \[`ruff`\] Implement `needless-else` clause (`RUF047`) ([#15051](https://github.com/astral-sh/ruff/pull/15051))
- \[`ruff`\] Implement `starmap-zip` (`RUF058`) ([#15483](https://github.com/astral-sh/ruff/pull/15483))

### Rule changes

- \[`flake8-bugbear`\] Do not raise error if keyword argument is present and target-python version is less or equals than 3.9 (`B903`) ([#15549](https://github.com/astral-sh/ruff/pull/15549))
- \[`flake8-comprehensions`\] strip parentheses around generators in `unnecessary-generator-set` (`C401`) ([#15553](https://github.com/astral-sh/ruff/pull/15553))
- \[`flake8-pytest-style`\] Rewrite references to `.exception` (`PT027`) ([#15680](https://github.com/astral-sh/ruff/pull/15680))
- \[`flake8-simplify`\] Mark fixes as unsafe (`SIM201`, `SIM202`) ([#15626](https://github.com/astral-sh/ruff/pull/15626))
- \[`flake8-type-checking`\] Fix some safe fixes being labeled unsafe (`TC006`,`TC008`) ([#15638](https://github.com/astral-sh/ruff/pull/15638))
- \[`isort`\] Omit trailing whitespace in `unsorted-imports` (`I001`) ([#15518](https://github.com/astral-sh/ruff/pull/15518))
- \[`pydoclint`\] Allow ignoring one line docstrings for `DOC` rules ([#13302](https://github.com/astral-sh/ruff/pull/13302))
- \[`pyflakes`\] Apply redefinition fixes by source code order (`F811`) ([#15575](https://github.com/astral-sh/ruff/pull/15575))
- \[`pyflakes`\] Avoid removing too many imports in `redefined-while-unused` (`F811`) ([#15585](https://github.com/astral-sh/ruff/pull/15585))
- \[`pyflakes`\] Group redefinition fixes by source statement (`F811`) ([#15574](https://github.com/astral-sh/ruff/pull/15574))
- \[`pylint`\] Include name of base class in message for `redefined-slots-in-subclass` (`W0244`) ([#15559](https://github.com/astral-sh/ruff/pull/15559))
- \[`ruff`\] Update fix for `RUF055` to use `var == value` ([#15605](https://github.com/astral-sh/ruff/pull/15605))

### Formatter

- Fix bracket spacing for single-element tuples in f-string expressions ([#15537](https://github.com/astral-sh/ruff/pull/15537))
- Fix unstable f-string formatting for expressions containing a trailing comma ([#15545](https://github.com/astral-sh/ruff/pull/15545))

### Performance

- Avoid quadratic membership check in import fixes ([#15576](https://github.com/astral-sh/ruff/pull/15576))

### Server

- Allow `unsafe-fixes` settings for code actions ([#15666](https://github.com/astral-sh/ruff/pull/15666))

### Bug fixes

- \[`flake8-bandit`\] Add missing single-line/dotall regex flag (`S608`) ([#15654](https://github.com/astral-sh/ruff/pull/15654))
- \[`flake8-import-conventions`\] Fix infinite loop between `ICN001` and `I002` (`ICN001`) ([#15480](https://github.com/astral-sh/ruff/pull/15480))
- \[`flake8-simplify`\] Do not emit diagnostics for expressions inside string type annotations (`SIM222`, `SIM223`) ([#15405](https://github.com/astral-sh/ruff/pull/15405))
- \[`pyflakes`\] Treat arguments passed to the `default=` parameter of `TypeVar` as type expressions (`F821`) ([#15679](https://github.com/astral-sh/ruff/pull/15679))
- \[`pyupgrade`\] Avoid syntax error when the iterable is a non-parenthesized tuple (`UP028`) ([#15543](https://github.com/astral-sh/ruff/pull/15543))
- \[`ruff`\] Exempt `NewType` calls where the original type is immutable (`RUF009`) ([#15588](https://github.com/astral-sh/ruff/pull/15588))
- Preserve raw string prefix and escapes in all codegen fixes ([#15694](https://github.com/astral-sh/ruff/pull/15694))

### Documentation

- Generate documentation redirects for lowercase rule codes ([#15564](https://github.com/astral-sh/ruff/pull/15564))
- `TRY300`: Add some extra notes on not catching exceptions you didn't expect ([#15036](https://github.com/astral-sh/ruff/pull/15036))

## 0.9.4

### Preview features

- \[`airflow`\] Extend airflow context parameter check for `BaseOperator.execute` (`AIR302`) ([#15713](https://github.com/astral-sh/ruff/pull/15713))
- \[`airflow`\] Update `AIR302` to check for deprecated context keys ([#15144](https://github.com/astral-sh/ruff/pull/15144))
- \[`flake8-bandit`\] Permit suspicious imports within stub files (`S4`) ([#15822](https://github.com/astral-sh/ruff/pull/15822))
- \[`pylint`\] Do not trigger `PLR6201` on empty collections ([#15732](https://github.com/astral-sh/ruff/pull/15732))
- \[`refurb`\] Do not emit diagnostic when loop variables are used outside loop body (`FURB122`) ([#15757](https://github.com/astral-sh/ruff/pull/15757))
- \[`ruff`\] Add support for more `re` patterns (`RUF055`) ([#15764](https://github.com/astral-sh/ruff/pull/15764))
- \[`ruff`\] Check for shadowed `map` before suggesting fix (`RUF058`) ([#15790](https://github.com/astral-sh/ruff/pull/15790))
- \[`ruff`\] Do not emit diagnostic when all arguments to `zip()` are variadic (`RUF058`) ([#15744](https://github.com/astral-sh/ruff/pull/15744))
- \[`ruff`\] Parenthesize fix when argument spans multiple lines for `unnecessary-round` (`RUF057`) ([#15703](https://github.com/astral-sh/ruff/pull/15703))

### Rule changes

- Preserve quote style in generated code ([#15726](https://github.com/astral-sh/ruff/pull/15726), [#15778](https://github.com/astral-sh/ruff/pull/15778), [#15794](https://github.com/astral-sh/ruff/pull/15794))
- \[`flake8-bugbear`\] Exempt `NewType` calls where the original type is immutable (`B008`) ([#15765](https://github.com/astral-sh/ruff/pull/15765))
- \[`pylint`\] Honor banned top-level imports by `TID253` in `PLC0415`. ([#15628](https://github.com/astral-sh/ruff/pull/15628))
- \[`pyupgrade`\] Ignore `is_typeddict` and `TypedDict` for `deprecated-import` (`UP035`) ([#15800](https://github.com/astral-sh/ruff/pull/15800))

### CLI

- Fix formatter warning message for `flake8-quotes` option ([#15788](https://github.com/astral-sh/ruff/pull/15788))
- Implement tab autocomplete for `ruff config` ([#15603](https://github.com/astral-sh/ruff/pull/15603))

### Bug fixes

- \[`flake8-comprehensions`\] Do not emit `unnecessary-map` diagnostic when lambda has different arity (`C417`) ([#15802](https://github.com/astral-sh/ruff/pull/15802))
- \[`flake8-comprehensions`\] Parenthesize `sorted` when needed for `unnecessary-call-around-sorted` (`C413`) ([#15825](https://github.com/astral-sh/ruff/pull/15825))
- \[`pyupgrade`\] Handle end-of-line comments for `quoted-annotation` (`UP037`) ([#15824](https://github.com/astral-sh/ruff/pull/15824))

### Documentation

- Add missing config docstrings ([#15803](https://github.com/astral-sh/ruff/pull/15803))
- Add references to `trio.run_process` and `anyio.run_process` ([#15761](https://github.com/astral-sh/ruff/pull/15761))
- Use `uv init --lib` in tutorial ([#15718](https://github.com/astral-sh/ruff/pull/15718))

## 0.9.5

### Preview features

- Recognize all symbols named `TYPE_CHECKING` for `in_type_checking_block` ([#15719](https://github.com/astral-sh/ruff/pull/15719))
- \[`flake8-comprehensions`\] Handle builtins at top of file correctly for `unnecessary-dict-comprehension-for-iterable` (`C420`) ([#15837](https://github.com/astral-sh/ruff/pull/15837))
- \[`flake8-logging`\] `.exception()` and `exc_info=` outside exception handlers (`LOG004`, `LOG014`) ([#15799](https://github.com/astral-sh/ruff/pull/15799))
- \[`flake8-pyi`\] Fix incorrect behaviour of `custom-typevar-return-type` preview-mode autofix if `typing` was already imported (`PYI019`) ([#15853](https://github.com/astral-sh/ruff/pull/15853))
- \[`flake8-pyi`\] Fix more complex cases (`PYI019`) ([#15821](https://github.com/astral-sh/ruff/pull/15821))
- \[`flake8-pyi`\] Make `PYI019` autofixable for `.py` files in preview mode as well as stubs ([#15889](https://github.com/astral-sh/ruff/pull/15889))
- \[`flake8-pyi`\] Remove type parameter correctly when it is the last (`PYI019`) ([#15854](https://github.com/astral-sh/ruff/pull/15854))
- \[`pylint`\] Fix missing parens in unsafe fix for `unnecessary-dunder-call` (`PLC2801`) ([#15762](https://github.com/astral-sh/ruff/pull/15762))
- \[`pyupgrade`\] Better messages and diagnostic range (`UP015`) ([#15872](https://github.com/astral-sh/ruff/pull/15872))
- \[`pyupgrade`\] Rename private type parameters in PEP 695 generics (`UP049`) ([#15862](https://github.com/astral-sh/ruff/pull/15862))
- \[`refurb`\] Also report non-name expressions (`FURB169`) ([#15905](https://github.com/astral-sh/ruff/pull/15905))
- \[`refurb`\] Mark fix as unsafe if there are comments (`FURB171`) ([#15832](https://github.com/astral-sh/ruff/pull/15832))
- \[`ruff`\] Classes with mixed type variable style (`RUF053`) ([#15841](https://github.com/astral-sh/ruff/pull/15841))
- \[`airflow`\] `BashOperator` has been moved to `airflow.providers.standard.operators.bash.BashOperator` (`AIR302`) ([#15922](https://github.com/astral-sh/ruff/pull/15922))
- \[`flake8-pyi`\] Add autofix for unused-private-type-var (`PYI018`) ([#15999](https://github.com/astral-sh/ruff/pull/15999))
- \[`flake8-pyi`\] Significantly improve accuracy of `PYI019` if preview mode is enabled ([#15888](https://github.com/astral-sh/ruff/pull/15888))

### Rule changes

- Preserve triple quotes and prefixes for strings ([#15818](https://github.com/astral-sh/ruff/pull/15818))
- \[`flake8-comprehensions`\] Skip when `TypeError` present from too many (kw)args for `C410`,`C411`, and `C418` ([#15838](https://github.com/astral-sh/ruff/pull/15838))
- \[`flake8-pyi`\] Rename `PYI019` and improve its diagnostic message ([#15885](https://github.com/astral-sh/ruff/pull/15885))
- \[`pep8-naming`\] Ignore `@override` methods (`N803`) ([#15954](https://github.com/astral-sh/ruff/pull/15954))
- \[`pyupgrade`\] Reuse replacement logic from `UP046` and `UP047` to preserve more comments (`UP040`) ([#15840](https://github.com/astral-sh/ruff/pull/15840))
- \[`ruff`\] Analyze deferred annotations before enforcing `mutable-(data)class-default` and `function-call-in-dataclass-default-argument` (`RUF008`,`RUF009`,`RUF012`) ([#15921](https://github.com/astral-sh/ruff/pull/15921))
- \[`pycodestyle`\] Exempt `sys.path += ...` calls (`E402`) ([#15980](https://github.com/astral-sh/ruff/pull/15980))

### Configuration

- Config error only when `flake8-import-conventions` alias conflicts with `isort.required-imports` bound name ([#15918](https://github.com/astral-sh/ruff/pull/15918))
- Workaround Even Better TOML crash related to `allOf` ([#15992](https://github.com/astral-sh/ruff/pull/15992))

### Bug fixes

- \[`flake8-comprehensions`\] Unnecessary `list` comprehension (rewrite as a `set` comprehension) (`C403`) - Handle extraneous parentheses around list comprehension ([#15877](https://github.com/astral-sh/ruff/pull/15877))
- \[`flake8-comprehensions`\] Handle trailing comma in fixes for `unnecessary-generator-list/set` (`C400`,`C401`) ([#15929](https://github.com/astral-sh/ruff/pull/15929))
- \[`flake8-pyi`\] Fix several correctness issues with `custom-type-var-return-type` (`PYI019`) ([#15851](https://github.com/astral-sh/ruff/pull/15851))
- \[`pep8-naming`\] Consider any number of leading underscore for `N801` ([#15988](https://github.com/astral-sh/ruff/pull/15988))
- \[`pyflakes`\] Visit forward annotations in `TypeAliasType` as types (`F401`) ([#15829](https://github.com/astral-sh/ruff/pull/15829))
- \[`pylint`\] Correct min/max auto-fix and suggestion for (`PL1730`) ([#15930](https://github.com/astral-sh/ruff/pull/15930))
- \[`refurb`\] Handle unparenthesized tuples correctly (`FURB122`, `FURB142`) ([#15953](https://github.com/astral-sh/ruff/pull/15953))
- \[`refurb`\] Avoid `None | None` as well as better detection and fix (`FURB168`) ([#15779](https://github.com/astral-sh/ruff/pull/15779))

### Documentation

- Add deprecation warning for `ruff-lsp` related settings ([#15850](https://github.com/astral-sh/ruff/pull/15850))
- Docs (`linter.md`): clarify that Python files are always searched for in subdirectories ([#15882](https://github.com/astral-sh/ruff/pull/15882))
- Fix a typo in `non_pep695_generic_class.rs` ([#15946](https://github.com/astral-sh/ruff/pull/15946))
- Improve Docs: Pylint subcategories' codes ([#15909](https://github.com/astral-sh/ruff/pull/15909))
- Remove non-existing `lint.extendIgnore` editor setting ([#15844](https://github.com/astral-sh/ruff/pull/15844))
- Update black deviations ([#15928](https://github.com/astral-sh/ruff/pull/15928))
- Mention `UP049` in `UP046` and `UP047`, add `See also` section to `UP040` ([#15956](https://github.com/astral-sh/ruff/pull/15956))
- Add instance variable examples to `RUF012` ([#15982](https://github.com/astral-sh/ruff/pull/15982))
- Explain precedence for `ignore` and `select` config ([#15883](https://github.com/astral-sh/ruff/pull/15883))

## 0.9.6

### Preview features

- \[`airflow`\] Add `external_task.{ExternalTaskMarker, ExternalTaskSensor}` for `AIR302` ([#16014](https://github.com/astral-sh/ruff/pull/16014))
- \[`flake8-builtins`\] Make strict module name comparison optional (`A005`) ([#15951](https://github.com/astral-sh/ruff/pull/15951))
- \[`flake8-pyi`\] Extend fix to Python \<= 3.9 for `redundant-none-literal` (`PYI061`) ([#16044](https://github.com/astral-sh/ruff/pull/16044))
- \[`pylint`\] Also report when the object isn't a literal (`PLE1310`) ([#15985](https://github.com/astral-sh/ruff/pull/15985))
- \[`ruff`\] Implement `indented-form-feed` (`RUF054`) ([#16049](https://github.com/astral-sh/ruff/pull/16049))
- \[`ruff`\] Skip type definitions for `missing-f-string-syntax` (`RUF027`) ([#16054](https://github.com/astral-sh/ruff/pull/16054))

### Rule changes

- \[`flake8-annotations`\] Correct syntax for `typing.Union` in suggested return type fixes for `ANN20x` rules ([#16025](https://github.com/astral-sh/ruff/pull/16025))
- \[`flake8-builtins`\] Match upstream module name comparison (`A005`) ([#16006](https://github.com/astral-sh/ruff/pull/16006))
- \[`flake8-comprehensions`\] Detect overshadowed `list`/`set`/`dict`, ignore variadics and named expressions (`C417`) ([#15955](https://github.com/astral-sh/ruff/pull/15955))
- \[`flake8-pie`\] Remove following comma correctly when the unpacked dictionary is empty (`PIE800`) ([#16008](https://github.com/astral-sh/ruff/pull/16008))
- \[`flake8-simplify`\] Only trigger `SIM401` on known dictionaries ([#15995](https://github.com/astral-sh/ruff/pull/15995))
- \[`pylint`\] Do not report calls when object type and argument type mismatch, remove custom escape handling logic (`PLE1310`) ([#15984](https://github.com/astral-sh/ruff/pull/15984))
- \[`pyupgrade`\] Comments within parenthesized value ranges should not affect applicability (`UP040`) ([#16027](https://github.com/astral-sh/ruff/pull/16027))
- \[`pyupgrade`\] Don't introduce invalid syntax when upgrading old-style type aliases with parenthesized multiline values (`UP040`) ([#16026](https://github.com/astral-sh/ruff/pull/16026))
- \[`pyupgrade`\] Ensure we do not rename two type parameters to the same name (`UP049`) ([#16038](https://github.com/astral-sh/ruff/pull/16038))
- \[`pyupgrade`\] \[`ruff`\] Don't apply renamings if the new name is shadowed in a scope of one of the references to the binding (`UP049`, `RUF052`) ([#16032](https://github.com/astral-sh/ruff/pull/16032))
- \[`ruff`\] Update `RUF009` to behave similar to `B008` and ignore attributes with immutable types ([#16048](https://github.com/astral-sh/ruff/pull/16048))

### Server

- Root exclusions in the server to project root ([#16043](https://github.com/astral-sh/ruff/pull/16043))

### Bug fixes

- \[`flake8-datetime`\] Ignore `.replace()` calls while looking for `.astimezone` ([#16050](https://github.com/astral-sh/ruff/pull/16050))
- \[`flake8-type-checking`\] Avoid `TC004` false positive where the runtime definition is provided by `__getattr__` ([#16052](https://github.com/astral-sh/ruff/pull/16052))

### Documentation

- Improve `ruff-lsp` migration document ([#16072](https://github.com/astral-sh/ruff/pull/16072))
- Undeprecate `ruff.nativeServer` ([#16039](https://github.com/astral-sh/ruff/pull/16039))

## 0.9.7

### Preview features

- Consider `__new__` methods as special function type for enforcing class method or static method rules ([#13305](https://github.com/astral-sh/ruff/pull/13305))
- \[`airflow`\] Improve the internal logic to differentiate deprecated symbols (`AIR303`) ([#16013](https://github.com/astral-sh/ruff/pull/16013))
- \[`refurb`\] Manual timezone monkeypatching (`FURB162`) ([#16113](https://github.com/astral-sh/ruff/pull/16113))
- \[`ruff`\] Implicit class variable in dataclass (`RUF045`) ([#14349](https://github.com/astral-sh/ruff/pull/14349))
- \[`ruff`\] Skip singleton starred expressions for `incorrectly-parenthesized-tuple-in-subscript` (`RUF031`) ([#16083](https://github.com/astral-sh/ruff/pull/16083))
- \[`refurb`\] Check for subclasses includes subscript expressions (`FURB189`) ([#16155](https://github.com/astral-sh/ruff/pull/16155))

### Rule changes

- \[`flake8-debugger`\] Also flag `sys.breakpointhook` and `sys.__breakpointhook__` (`T100`) ([#16191](https://github.com/astral-sh/ruff/pull/16191))
- \[`pycodestyle`\] Exempt `site.addsitedir(...)` calls (`E402`) ([#16251](https://github.com/astral-sh/ruff/pull/16251))

### Formatter

- Fix unstable formatting of trailing end-of-line comments of parenthesized attribute values ([#16187](https://github.com/astral-sh/ruff/pull/16187))

### Server

- Fix handling of requests received after shutdown message ([#16262](https://github.com/astral-sh/ruff/pull/16262))
- Ignore `source.organizeImports.ruff` and `source.fixAll.ruff` code actions for a notebook cell ([#16154](https://github.com/astral-sh/ruff/pull/16154))
- Include document specific debug info for `ruff.printDebugInformation` ([#16215](https://github.com/astral-sh/ruff/pull/16215))
- Update server to return the debug info as string with `ruff.printDebugInformation` ([#16214](https://github.com/astral-sh/ruff/pull/16214))

### CLI

- Warn on invalid `noqa` even when there are no diagnostics ([#16178](https://github.com/astral-sh/ruff/pull/16178))
- Better error messages while loading configuration `extend`s ([#15658](https://github.com/astral-sh/ruff/pull/15658))

### Bug fixes

- \[`flake8-comprehensions`\] Handle trailing comma in `C403` fix ([#16110](https://github.com/astral-sh/ruff/pull/16110))
- \[`flake8-pyi`\] Avoid flagging `custom-typevar-for-self` on metaclass methods (`PYI019`) ([#16141](https://github.com/astral-sh/ruff/pull/16141))
- \[`pydocstyle`\] Handle arguments with the same names as sections (`D417`) ([#16011](https://github.com/astral-sh/ruff/pull/16011))
- \[`pylint`\] Correct ordering of arguments in fix for `if-stmt-min-max` (`PLR1730`) ([#16080](https://github.com/astral-sh/ruff/pull/16080))
- \[`pylint`\] Do not offer fix for raw strings (`PLE251`) ([#16132](https://github.com/astral-sh/ruff/pull/16132))
- \[`pyupgrade`\] Do not upgrade functional `TypedDicts` with private field names to the class-based syntax (`UP013`) ([#16219](https://github.com/astral-sh/ruff/pull/16219))
- \[`pyupgrade`\] Handle micro version numbers correctly (`UP036`) ([#16091](https://github.com/astral-sh/ruff/pull/16091))
- \[`pyupgrade`\] Unwrap unary expressions correctly (`UP018`) ([#15919](https://github.com/astral-sh/ruff/pull/15919))
- \[`refurb`\] Correctly handle lengths of literal strings in `slice-to-remove-prefix-or-suffix` (`FURB188`) ([#16237](https://github.com/astral-sh/ruff/pull/16237))
- \[`ruff`\] Skip `RUF001` diagnostics when visiting string type definitions ([#16122](https://github.com/astral-sh/ruff/pull/16122))

### Documentation

- Add FAQ entry for `source.*` code actions in Notebook ([#16212](https://github.com/astral-sh/ruff/pull/16212))
- Add `SECURITY.md` ([#16224](https://github.com/astral-sh/ruff/pull/16224))

## 0.9.8

### Preview features

- Start detecting version-related syntax errors in the parser ([#16090](https://github.com/astral-sh/ruff/pull/16090))

### Rule changes

- \[`pylint`\] Mark fix unsafe (`PLW1507`) ([#16343](https://github.com/astral-sh/ruff/pull/16343))
- \[`pylint`\] Catch `case np.nan`/`case math.nan` in `match` statements (`PLW0177`) ([#16378](https://github.com/astral-sh/ruff/pull/16378))
- \[`ruff`\] Add more Pydantic models variants to the list of default copy semantics (`RUF012`) ([#16291](https://github.com/astral-sh/ruff/pull/16291))

### Server

- Avoid indexing the project if `configurationPreference` is `editorOnly` ([#16381](https://github.com/astral-sh/ruff/pull/16381))
- Avoid unnecessary info at non-trace server log level ([#16389](https://github.com/astral-sh/ruff/pull/16389))
- Expand `ruff.configuration` to allow inline config ([#16296](https://github.com/astral-sh/ruff/pull/16296))
- Notify users for invalid client settings ([#16361](https://github.com/astral-sh/ruff/pull/16361))

### Configuration

- Add `per-file-target-version` option ([#16257](https://github.com/astral-sh/ruff/pull/16257))

### Bug fixes

- \[`refurb`\] Do not consider docstring(s) (`FURB156`) ([#16391](https://github.com/astral-sh/ruff/pull/16391))
- \[`flake8-self`\] Ignore attribute accesses on instance-like variables (`SLF001`) ([#16149](https://github.com/astral-sh/ruff/pull/16149))
- \[`pylint`\] Fix false positives, add missing methods, and support positional-only parameters (`PLE0302`) ([#16263](https://github.com/astral-sh/ruff/pull/16263))
- \[`flake8-pyi`\] Mark `PYI030` fix unsafe when comments are deleted ([#16322](https://github.com/astral-sh/ruff/pull/16322))

### Documentation

- Fix example for `S611` ([#16316](https://github.com/astral-sh/ruff/pull/16316))
- Normalize inconsistent markdown headings in docstrings ([#16364](https://github.com/astral-sh/ruff/pull/16364))
- Document MSRV policy ([#16384](https://github.com/astral-sh/ruff/pull/16384))

## 0.9.9

### Preview features

- Fix caching of unsupported-syntax errors ([#16425](https://github.com/astral-sh/ruff/pull/16425))

### Bug fixes

- Only show unsupported-syntax errors in editors when preview mode is enabled ([#16429](https://github.com/astral-sh/ruff/pull/16429))

## 0.9.10

### Preview features

- \[`ruff`\] Add new rule `RUF059`: Unused unpacked assignment ([#16449](https://github.com/astral-sh/ruff/pull/16449))
- \[`syntax-errors`\] Detect assignment expressions before Python 3.8 ([#16383](https://github.com/astral-sh/ruff/pull/16383))
- \[`syntax-errors`\] Named expressions in decorators before Python 3.9 ([#16386](https://github.com/astral-sh/ruff/pull/16386))
- \[`syntax-errors`\] Parenthesized keyword argument names after Python 3.8 ([#16482](https://github.com/astral-sh/ruff/pull/16482))
- \[`syntax-errors`\] Positional-only parameters before Python 3.8 ([#16481](https://github.com/astral-sh/ruff/pull/16481))
- \[`syntax-errors`\] Tuple unpacking in `return` and `yield` before Python 3.8 ([#16485](https://github.com/astral-sh/ruff/pull/16485))
- \[`syntax-errors`\] Type parameter defaults before Python 3.13 ([#16447](https://github.com/astral-sh/ruff/pull/16447))
- \[`syntax-errors`\] Type parameter lists before Python 3.12 ([#16479](https://github.com/astral-sh/ruff/pull/16479))
- \[`syntax-errors`\] `except*` before Python 3.11 ([#16446](https://github.com/astral-sh/ruff/pull/16446))
- \[`syntax-errors`\] `type` statements before Python 3.12 ([#16478](https://github.com/astral-sh/ruff/pull/16478))

### Bug fixes

- Escape template filenames in glob patterns in configuration ([#16407](https://github.com/astral-sh/ruff/pull/16407))
- \[`flake8-simplify`\] Exempt unittest context methods for `SIM115` rule ([#16439](https://github.com/astral-sh/ruff/pull/16439))
- Formatter: Fix syntax error location in notebooks ([#16499](https://github.com/astral-sh/ruff/pull/16499))
- \[`pyupgrade`\] Do not offer fix when at least one target is `global`/`nonlocal` (`UP028`) ([#16451](https://github.com/astral-sh/ruff/pull/16451))
- \[`flake8-builtins`\] Ignore variables matching module attribute names (`A001`) ([#16454](https://github.com/astral-sh/ruff/pull/16454))
- \[`pylint`\] Convert `code` keyword argument to a positional argument in fix for (`PLR1722`) ([#16424](https://github.com/astral-sh/ruff/pull/16424))

### CLI

- Move rule code from `description` to `check_name` in GitLab output serializer ([#16437](https://github.com/astral-sh/ruff/pull/16437))

### Documentation

- \[`pydocstyle`\] Clarify that `D417` only checks docstrings with an arguments section ([#16494](https://github.com/astral-sh/ruff/pull/16494))
