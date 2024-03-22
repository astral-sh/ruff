# Changelog

## 0.3.4

### Preview features

- \[`flake8-simplify`\] Detect implicit `else` cases in `needless-bool` (`SIM103`) ([#10414](https://github.com/astral-sh/ruff/pull/10414))
- \[`pylint`\] Implement `nan-comparison` (`PLW0117`) ([#10401](https://github.com/astral-sh/ruff/pull/10401))
- \[`pylint`\] Implement `nonlocal-and-global` (`E115`) ([#10407](https://github.com/astral-sh/ruff/pull/10407))
- \[`pylint`\] Implement `singledispatchmethod-function` (`PLE5120`) ([#10428](https://github.com/astral-sh/ruff/pull/10428))
- \[`refurb`\] Implement `list-reverse-copy` (`FURB187`) ([#10212](https://github.com/astral-sh/ruff/pull/10212))

### Rule changes

- \[`flake8-pytest-style`\] Add automatic fix for `pytest-parametrize-values-wrong-type` (`PT007`) ([#10461](https://github.com/astral-sh/ruff/pull/10461))
- \[`pycodestyle`\] Allow SPDX license headers to exceed the line length (`E501`) ([#10481](https://github.com/astral-sh/ruff/pull/10481))

### Formatter

- Fix unstable formatting for trailing subscript end-of-line comment ([#10492](https://github.com/astral-sh/ruff/pull/10492))

### Bug fixes

- Avoid code comment detection in PEP 723 script tags ([#10464](https://github.com/astral-sh/ruff/pull/10464))
- Avoid incorrect tuple transformation in single-element case (`C409`) ([#10491](https://github.com/astral-sh/ruff/pull/10491))
- Bug fix: Prevent fully defined links [`name`](link) from being reformatted ([#10442](https://github.com/astral-sh/ruff/pull/10442))
- Consider raw source code for `W605` ([#10480](https://github.com/astral-sh/ruff/pull/10480))
- Docs: Link inline settings when not part of options section ([#10499](https://github.com/astral-sh/ruff/pull/10499))
- Don't treat annotations as redefinitions in `.pyi` files ([#10512](https://github.com/astral-sh/ruff/pull/10512))
- Fix `E231` bug: Inconsistent catch compared to pycodestyle, such as when dict nested in list ([#10469](https://github.com/astral-sh/ruff/pull/10469))
- Fix pylint upstream categories not showing in docs ([#10441](https://github.com/astral-sh/ruff/pull/10441))
- Add missing `Options` references to blank line docs ([#10498](https://github.com/astral-sh/ruff/pull/10498))
- 'Revert "F821: Fix false negatives in .py files when `from __future__ import annotations` is active (#10362)"' ([#10513](https://github.com/astral-sh/ruff/pull/10513))
- Apply NFKC normalization to unicode identifiers in the lexer ([#10412](https://github.com/astral-sh/ruff/pull/10412))
- Avoid failures due to non-deterministic binding ordering ([#10478](https://github.com/astral-sh/ruff/pull/10478))
- \[`flake8-bugbear`\] Allow tuples of exceptions (`B030`) ([#10437](https://github.com/astral-sh/ruff/pull/10437))
- \[`flake8-quotes`\] Avoid syntax errors due to invalid quotes (`Q000, Q002`) ([#10199](https://github.com/astral-sh/ruff/pull/10199))

## 0.3.3

### Preview features

- \[`flake8-bandit`\]: Implement `S610` rule ([#10316](https://github.com/astral-sh/ruff/pull/10316))
- \[`pycodestyle`\] Implement `blank-line-at-end-of-file` (`W391`) ([#10243](https://github.com/astral-sh/ruff/pull/10243))
- \[`pycodestyle`\] Implement `redundant-backslash` (`E502`) ([#10292](https://github.com/astral-sh/ruff/pull/10292))
- \[`pylint`\] - implement `redeclared-assigned-name` (`W0128`) ([#9268](https://github.com/astral-sh/ruff/pull/9268))

### Rule changes

- \[`flake8_comprehensions`\] Handled special case for `C400` which also matches `C416` ([#10419](https://github.com/astral-sh/ruff/pull/10419))
- \[`flake8-bandit`\] Implement upstream updates for `S311`, `S324` and `S605` ([#10313](https://github.com/astral-sh/ruff/pull/10313))
- \[`pyflakes`\] Remove `F401` fix for `__init__` imports by default and allow opt-in to unsafe fix ([#10365](https://github.com/astral-sh/ruff/pull/10365))
- \[`pylint`\] Implement `invalid-bool-return-type` (`E304`) ([#10377](https://github.com/astral-sh/ruff/pull/10377))
- \[`pylint`\] Include builtin warnings in useless-exception-statement (`PLW0133`) ([#10394](https://github.com/astral-sh/ruff/pull/10394))

### CLI

- Add message on success to `ruff check` ([#8631](https://github.com/astral-sh/ruff/pull/8631))

### Bug fixes

- \[`PIE970`\] Allow trailing ellipsis in `typing.TYPE_CHECKING` ([#10413](https://github.com/astral-sh/ruff/pull/10413))
- Avoid `TRIO115` if the argument is a variable ([#10376](https://github.com/astral-sh/ruff/pull/10376))
- \[`F811`\] Avoid removing shadowed imports that point to different symbols ([#10387](https://github.com/astral-sh/ruff/pull/10387))
- Fix `F821` and `F822` false positives in `.pyi` files ([#10341](https://github.com/astral-sh/ruff/pull/10341))
- Fix `F821` false negatives in `.py` files when `from __future__ import annotations` is active ([#10362](https://github.com/astral-sh/ruff/pull/10362))
- Fix case where `Indexer` fails to identify continuation preceded by newline #10351 ([#10354](https://github.com/astral-sh/ruff/pull/10354))
- Sort hash maps in `Settings` display ([#10370](https://github.com/astral-sh/ruff/pull/10370))
- Track conditional deletions in the semantic model ([#10415](https://github.com/astral-sh/ruff/pull/10415))
- \[`C413`\] Wrap expressions in parentheses when negating ([#10346](https://github.com/astral-sh/ruff/pull/10346))
- \[`pycodestyle`\] Do not ignore lines before the first logical line in blank lines rules. ([#10382](https://github.com/astral-sh/ruff/pull/10382))
- \[`pycodestyle`\] Do not trigger `E225` and `E275` when the next token is a ')' ([#10315](https://github.com/astral-sh/ruff/pull/10315))
- \[`pylint`\] Avoid false-positive slot non-assignment for `__dict__` (`PLE0237`) ([#10348](https://github.com/astral-sh/ruff/pull/10348))
- Gate f-string struct size test for Rustc \< 1.76 ([#10371](https://github.com/astral-sh/ruff/pull/10371))

### Documentation

- Use `ruff.toml` format in README ([#10393](https://github.com/astral-sh/ruff/pull/10393))
- \[`RUF008`\] Make it clearer that a mutable default in a dataclass is only valid if it is typed as a ClassVar ([#10395](https://github.com/astral-sh/ruff/pull/10395))
- \[`pylint`\] Extend docs and test in `invalid-str-return-type` (`E307`) ([#10400](https://github.com/astral-sh/ruff/pull/10400))
- Remove `.` from `check` and `format` commands ([#10217](https://github.com/astral-sh/ruff/pull/10217))

## 0.3.2

### Preview features

- Improve single-`with` item formatting for Python 3.8 or older ([#10276](https://github.com/astral-sh/ruff/pull/10276))

### Rule changes

- \[`pyupgrade`\] Allow fixes for f-string rule regardless of line length (`UP032`) ([#10263](https://github.com/astral-sh/ruff/pull/10263))
- \[`pycodestyle`\] Include actual conditions in E712 diagnostics ([#10254](https://github.com/astral-sh/ruff/pull/10254))

### Bug fixes

- Fix trailing kwargs end of line comment after slash ([#10297](https://github.com/astral-sh/ruff/pull/10297))
- Fix unstable `with` items formatting ([#10274](https://github.com/astral-sh/ruff/pull/10274))
- Avoid repeating function calls in f-string conversions ([#10265](https://github.com/astral-sh/ruff/pull/10265))
- Fix E203 false positive for slices in format strings ([#10280](https://github.com/astral-sh/ruff/pull/10280))
- Fix incorrect `Parameter` range  for `*args` and `**kwargs`  ([#10283](https://github.com/astral-sh/ruff/pull/10283))
- Treat `typing.Annotated` subscripts as type definitions ([#10285](https://github.com/astral-sh/ruff/pull/10285))

## 0.3.1

### Preview features

- \[`pycodestyle`\] Fix E301 not triggering on decorated methods. ([#10117](https://github.com/astral-sh/ruff/pull/10117))
- \[`pycodestyle`\] Respect `isort` settings in blank line rules (`E3*`) ([#10096](https://github.com/astral-sh/ruff/pull/10096))
- \[`pycodestyle`\] Make blank lines in typing stub files optional (`E3*`) ([#10098](https://github.com/astral-sh/ruff/pull/10098))
- \[`pylint`\] Implement `singledispatch-method` (`E1519`) ([#10140](https://github.com/astral-sh/ruff/pull/10140))
- \[`pylint`\] Implement `useless-exception-statement` (`W0133`) ([#10176](https://github.com/astral-sh/ruff/pull/10176))

### Rule changes

- \[`flake8-debugger`\] Check for use of `debugpy` and `ptvsd` debug modules (#10177) ([#10194](https://github.com/astral-sh/ruff/pull/10194))
- \[`pyupgrade`\] Generate diagnostic for all valid f-string conversions regardless of line length (`UP032`) ([#10238](https://github.com/astral-sh/ruff/pull/10238))
- \[`pep8_naming`\] Add fixes for `N804` and `N805` ([#10215](https://github.com/astral-sh/ruff/pull/10215))

### CLI

- Colorize the output of `ruff format --diff` ([#10110](https://github.com/astral-sh/ruff/pull/10110))
- Make `--config` and `--isolated` global flags ([#10150](https://github.com/astral-sh/ruff/pull/10150))
- Correctly expand tildes and environment variables in paths passed to `--config` ([#10219](https://github.com/astral-sh/ruff/pull/10219))

### Configuration

- Accept a PEP 440 version specifier for `required-version` ([#10216](https://github.com/astral-sh/ruff/pull/10216))
- Implement isort's `default-section` setting ([#10149](https://github.com/astral-sh/ruff/pull/10149))

### Bug fixes

- Remove trailing space from `CapWords` message ([#10220](https://github.com/astral-sh/ruff/pull/10220))
- Respect external codes in file-level exemptions ([#10203](https://github.com/astral-sh/ruff/pull/10203))
- \[`flake8-raise`\] Avoid false-positives for parens-on-raise with `future.exception()` (`RSE102`) ([#10206](https://github.com/astral-sh/ruff/pull/10206))
- \[`pylint`\] Add fix for unary expressions in `PLC2801` ([#9587](https://github.com/astral-sh/ruff/pull/9587))
- \[`ruff`\] Fix RUF028 not allowing `# fmt: skip` on match cases ([#10178](https://github.com/astral-sh/ruff/pull/10178))

## 0.3.0

This release introduces the new Ruff formatter 2024.2 style and adds a new lint rule to
detect invalid formatter suppression comments.

### Preview features

- \[`flake8-bandit`\] Remove suspicious-lxml-import (`S410`) ([#10154](https://github.com/astral-sh/ruff/pull/10154))
- \[`pycodestyle`\] Allow `os.environ` modifications between imports (`E402`) ([#10066](https://github.com/astral-sh/ruff/pull/10066))
- \[`pycodestyle`\] Don't warn about a single whitespace character before a comma in a tuple (`E203`) ([#10094](https://github.com/astral-sh/ruff/pull/10094))

### Rule changes

- \[`eradicate`\] Detect commented out `case` statements (`ERA001`) ([#10055](https://github.com/astral-sh/ruff/pull/10055))
- \[`eradicate`\] Detect single-line code for `try:`, `except:`, etc. (`ERA001`) ([#10057](https://github.com/astral-sh/ruff/pull/10057))
- \[`flake8-boolean-trap`\] Allow boolean positionals in `__post_init__` ([#10027](https://github.com/astral-sh/ruff/pull/10027))
- \[`flake8-copyright`\] Allow © in copyright notices ([#10065](https://github.com/astral-sh/ruff/pull/10065))
- \[`isort`\]: Use one blank line after imports in typing stub files ([#9971](https://github.com/astral-sh/ruff/pull/9971))
- \[`pylint`\] New Rule `dict-iter-missing-items` (`PLE1141`) ([#9845](https://github.com/astral-sh/ruff/pull/9845))
- \[`pylint`\] Ignore `sys.version` and `sys.platform` (`PLR1714`) ([#10054](https://github.com/astral-sh/ruff/pull/10054))
- \[`pyupgrade`\] Detect literals with unary operators (`UP018`) ([#10060](https://github.com/astral-sh/ruff/pull/10060))
- \[`ruff`\] Expand rule for `list(iterable).pop(0)` idiom (`RUF015`) ([#10148](https://github.com/astral-sh/ruff/pull/10148))

### Formatter

This release introduces the Ruff 2024.2 style, stabilizing the following changes:

- Prefer splitting the assignment's value over the target or type annotation ([#8943](https://github.com/astral-sh/ruff/pull/8943))
- Remove blank lines before class docstrings ([#9154](https://github.com/astral-sh/ruff/pull/9154))
- Wrap multiple context managers in `with` parentheses when targeting Python 3.9 or newer ([#9222](https://github.com/astral-sh/ruff/pull/9222))
- Add a blank line after nested classes with a dummy body (`...`) in typing stub files ([#9155](https://github.com/astral-sh/ruff/pull/9155))
- Reduce vertical spacing for classes and functions with a dummy (`...`) body ([#7440](https://github.com/astral-sh/ruff/issues/7440), [#9240](https://github.com/astral-sh/ruff/pull/9240))
- Add a blank line after the module docstring ([#8283](https://github.com/astral-sh/ruff/pull/8283))
- Parenthesize long type hints in assignments ([#9210](https://github.com/astral-sh/ruff/pull/9210))
- Preserve indent for single multiline-string call-expressions ([#9673](https://github.com/astral-sh/ruff/pull/9637))
- Normalize hex escape and unicode escape sequences ([#9280](https://github.com/astral-sh/ruff/pull/9280))
- Format module docstrings ([#9725](https://github.com/astral-sh/ruff/pull/9725))

### CLI

- Explicitly disallow `extend` as part of a `--config` flag ([#10135](https://github.com/astral-sh/ruff/pull/10135))
- Remove `build` from the default exclusion list ([#10093](https://github.com/astral-sh/ruff/pull/10093))
- Deprecate `ruff <path>`, `ruff --explain`, `ruff --clean`, and `ruff --generate-shell-completion` in favor of `ruff check <path>`, `ruff rule`, `ruff clean`, and `ruff generate-shell-completion` ([#10169](https://github.com/astral-sh/ruff/pull/10169))
- Remove the deprecated CLI option `--format` from `ruff rule` and `ruff linter` ([#10170](https://github.com/astral-sh/ruff/pull/10170))

### Bug fixes

- \[`flake8-bugbear`\] Avoid adding default initializers to stubs (`B006`) ([#10152](https://github.com/astral-sh/ruff/pull/10152))
- \[`flake8-type-checking`\] Respect runtime-required decorators for function signatures ([#10091](https://github.com/astral-sh/ruff/pull/10091))
- \[`pycodestyle`\] Mark fixes overlapping with a multiline string as unsafe (`W293`) ([#10049](https://github.com/astral-sh/ruff/pull/10049))
- \[`pydocstyle`\] Trim whitespace when removing blank lines after section (`D413`) ([#10162](https://github.com/astral-sh/ruff/pull/10162))
- \[`pylint`\] Delete entire statement, including semicolons (`PLR0203`) ([#10074](https://github.com/astral-sh/ruff/pull/10074))
- \[`ruff`\] Avoid f-string false positives in `gettext` calls (`RUF027`) ([#10118](https://github.com/astral-sh/ruff/pull/10118))
- Fix `ruff` crashing on PowerPC systems because of too small page size ([#10080](https://github.com/astral-sh/ruff/pull/10080))

### Performance

- Add cold attribute to less likely printer queue branches in the formatter ([#10121](https://github.com/astral-sh/ruff/pull/10121))
- Skip unnecessary string normalization in the formatter ([#10116](https://github.com/astral-sh/ruff/pull/10116))

### Documentation

- Remove "Beta" Label from formatter documentation ([#10144](https://github.com/astral-sh/ruff/pull/10144))
- `line-length` option: fix link to `pycodestyle.max-line-length` ([#10136](https://github.com/astral-sh/ruff/pull/10136))

## 0.2.2

Highlights include:

- Initial support formatting f-strings (in `--preview`).
- Support for overriding arbitrary configuration options via the CLI through an expanded `--config`
    argument (e.g., `--config "lint.isort.combine-as-imports=false"`).
- Significant performance improvements in Ruff's lexer, parser, and lint rules.

### Preview features

- Implement minimal f-string formatting ([#9642](https://github.com/astral-sh/ruff/pull/9642))
- \[`pycodestyle`\] Add blank line(s) rules (`E301`, `E302`, `E303`, `E304`, `E305`, `E306`) ([#9266](https://github.com/astral-sh/ruff/pull/9266))
- \[`refurb`\] Implement `readlines_in_for` (`FURB129`) ([#9880](https://github.com/astral-sh/ruff/pull/9880))

### Rule changes

- \[`ruff`\] Ensure closing parentheses for multiline sequences are always on their own line (`RUF022`, `RUF023`) ([#9793](https://github.com/astral-sh/ruff/pull/9793))
- \[`numpy`\] Add missing deprecation violations (`NPY002`) ([#9862](https://github.com/astral-sh/ruff/pull/9862))
- \[`flake8-bandit`\] Detect `mark_safe` usages in decorators ([#9887](https://github.com/astral-sh/ruff/pull/9887))
- \[`ruff`\] Expand `asyncio-dangling-task` (`RUF006`) to include `new_event_loop` ([#9976](https://github.com/astral-sh/ruff/pull/9976))
- \[`flake8-pyi`\] Ignore 'unused' private type dicts in class scopes ([#9952](https://github.com/astral-sh/ruff/pull/9952))

### Formatter

- Docstring formatting: Preserve tab indentation when using `indent-style=tabs` ([#9915](https://github.com/astral-sh/ruff/pull/9915))
- Disable top-level docstring formatting for notebooks ([#9957](https://github.com/astral-sh/ruff/pull/9957))
- Stabilize quote-style's `preserve` mode ([#9922](https://github.com/astral-sh/ruff/pull/9922))

### CLI

- Allow arbitrary configuration options to be overridden via the CLI ([#9599](https://github.com/astral-sh/ruff/pull/9599))

### Bug fixes

- Make `show-settings` filters directory-agnostic ([#9866](https://github.com/astral-sh/ruff/pull/9866))
- Respect duplicates when rewriting type aliases ([#9905](https://github.com/astral-sh/ruff/pull/9905))
- Respect tuple assignments in typing analyzer ([#9969](https://github.com/astral-sh/ruff/pull/9969))
- Use atomic write when persisting cache ([#9981](https://github.com/astral-sh/ruff/pull/9981))
- Use non-parenthesized range for `DebugText` ([#9953](https://github.com/astral-sh/ruff/pull/9953))
- \[`flake8-simplify`\] Avoid false positive with `async` for loops (`SIM113`) ([#9996](https://github.com/astral-sh/ruff/pull/9996))
- \[`flake8-trio`\] Respect `async with` in `timeout-without-await` ([#9859](https://github.com/astral-sh/ruff/pull/9859))
- \[`perflint`\] Catch a wider range of mutations in `PERF101` ([#9955](https://github.com/astral-sh/ruff/pull/9955))
- \[`pycodestyle`\] Fix `E30X` panics on blank lines with trailing white spaces ([#9907](https://github.com/astral-sh/ruff/pull/9907))
- \[`pydocstyle`\] Allow using `parameters` as a subsection header (`D405`) ([#9894](https://github.com/astral-sh/ruff/pull/9894))
- \[`pydocstyle`\] Fix blank-line docstring rules for module-level docstrings ([#9878](https://github.com/astral-sh/ruff/pull/9878))
- \[`pylint`\] Accept 0.0 and 1.0 as common magic values (`PLR2004`) ([#9964](https://github.com/astral-sh/ruff/pull/9964))
- \[`pylint`\] Avoid suggesting set rewrites for non-hashable types ([#9956](https://github.com/astral-sh/ruff/pull/9956))
- \[`ruff`\] Avoid false negatives with string literals inside of method calls (`RUF027`) ([#9865](https://github.com/astral-sh/ruff/pull/9865))
- \[`ruff`\] Fix panic on with f-string detection (`RUF027`) ([#9990](https://github.com/astral-sh/ruff/pull/9990))
- \[`ruff`\] Ignore builtins when detecting missing f-strings ([#9849](https://github.com/astral-sh/ruff/pull/9849))

### Performance

- Use `memchr` for string lexing ([#9888](https://github.com/astral-sh/ruff/pull/9888))
- Use `memchr` for tab-indentation detection ([#9853](https://github.com/astral-sh/ruff/pull/9853))
- Reduce `Result<Tok, LexicalError>` size by using `Box<str>` instead of `String` ([#9885](https://github.com/astral-sh/ruff/pull/9885))
- Reduce size of `Expr` from 80 to 64 bytes ([#9900](https://github.com/astral-sh/ruff/pull/9900))
- Improve trailing comma rule performance ([#9867](https://github.com/astral-sh/ruff/pull/9867))
- Remove unnecessary string cloning from the parser ([#9884](https://github.com/astral-sh/ruff/pull/9884))

## 0.2.1

This release includes support for range formatting (i.e., the ability to format specific lines
within a source file).

### Preview features

- \[`refurb`\] Implement `missing-f-string-syntax` (`RUF027`) ([#9728](https://github.com/astral-sh/ruff/pull/9728))
- Format module-level docstrings ([#9725](https://github.com/astral-sh/ruff/pull/9725))

### Formatter

- Add `--range` option to `ruff format` ([#9733](https://github.com/astral-sh/ruff/pull/9733))
- Don't trim last empty line in docstrings ([#9813](https://github.com/astral-sh/ruff/pull/9813))

### Bug fixes

- Skip empty lines when determining base indentation ([#9795](https://github.com/astral-sh/ruff/pull/9795))
- Drop `__get__` and `__set__` from `unnecessary-dunder-call` ([#9791](https://github.com/astral-sh/ruff/pull/9791))
- Respect generic `Protocol` in ellipsis removal ([#9841](https://github.com/astral-sh/ruff/pull/9841))
- Revert "Use publicly available Apple Silicon runners (#9726)" ([#9834](https://github.com/astral-sh/ruff/pull/9834))

### Performance

- Skip LibCST parsing for standard dedent adjustments ([#9769](https://github.com/astral-sh/ruff/pull/9769))
- Remove CST-based fixer for `C408` ([#9822](https://github.com/astral-sh/ruff/pull/9822))
- Add our own ignored-names abstractions ([#9802](https://github.com/astral-sh/ruff/pull/9802))
- Remove CST-based fixers for `C400`, `C401`, `C410`, and `C418` ([#9819](https://github.com/astral-sh/ruff/pull/9819))
- Use `AhoCorasick` to speed up quote match ([#9773](https://github.com/astral-sh/ruff/pull/9773))
- Remove CST-based fixers for `C405` and `C409` ([#9821](https://github.com/astral-sh/ruff/pull/9821))
- Add fast-path for comment detection ([#9808](https://github.com/astral-sh/ruff/pull/9808))
- Invert order of checks in `zero-sleep-call` ([#9766](https://github.com/astral-sh/ruff/pull/9766))
- Short-circuit typing matches based on imports ([#9800](https://github.com/astral-sh/ruff/pull/9800))
- Run dunder method rule on methods directly ([#9815](https://github.com/astral-sh/ruff/pull/9815))
- Track top-level module imports in the semantic model ([#9775](https://github.com/astral-sh/ruff/pull/9775))
- Slight speed-up for lowercase and uppercase identifier checks ([#9798](https://github.com/astral-sh/ruff/pull/9798))
- Remove LibCST-based fixer for `C403` ([#9818](https://github.com/astral-sh/ruff/pull/9818))

### Documentation

- Update `max-pos-args` example to `max-positional-args` ([#9797](https://github.com/astral-sh/ruff/pull/9797))
- Fixed example code in `weak_cryptographic_key.rs` ([#9774](https://github.com/astral-sh/ruff/pull/9774))
- Fix references to deprecated `ANN` rules in changelog ([#9771](https://github.com/astral-sh/ruff/pull/9771))
- Fix default for `max-positional-args` ([#9838](https://github.com/astral-sh/ruff/pull/9838))

## 0.2.0

### Breaking changes

- The `NURSERY` selector cannot be used anymore
- Legacy selection of nursery rules by exact codes is no longer allowed without preview enabled

See also, the "Remapped rules" section which may result in disabled rules.

### Deprecations

The following rules are now deprecated:

- [`missing-type-self`](https://docs.astral.sh/ruff/rules/missing-type-self/) (`ANN101`)
- [`missing-type-cls`](https://docs.astral.sh/ruff/rules/missing-type-cls/) (`ANN102`)

The following command line options are now deprecated:

- `--show-source`; use `--output-format full` instead
- `--no-show-source`; use `--output-format concise` instead
- `--output-format text`; use `full` or `concise` instead

The following settings have moved and the previous name is deprecated:

- `ruff.allowed-confusables` → [`ruff.lint.allowed-confusables`](https://docs.astral.sh//ruff/settings/#lint_allowed-confusables)
- `ruff.dummy-variable-rgx` → [`ruff.lint.dummy-variable-rgx`](https://docs.astral.sh//ruff/settings/#lint_dummy-variable-rgx)
- `ruff.explicit-preview-rules` → [`ruff.lint.explicit-preview-rules`](https://docs.astral.sh//ruff/settings/#lint_explicit-preview-rules)
- `ruff.extend-fixable` → [`ruff.lint.extend-fixable`](https://docs.astral.sh//ruff/settings/#lint_extend-fixable)
- `ruff.extend-ignore` → [`ruff.lint.extend-ignore`](https://docs.astral.sh//ruff/settings/#lint_extend-ignore)
- `ruff.extend-per-file-ignores` → [`ruff.lint.extend-per-file-ignores`](https://docs.astral.sh//ruff/settings/#lint_extend-per-file-ignores)
- `ruff.extend-safe-fixes` → [`ruff.lint.extend-safe-fixes`](https://docs.astral.sh//ruff/settings/#lint_extend-safe-fixes)
- `ruff.extend-select` → [`ruff.lint.extend-select`](https://docs.astral.sh//ruff/settings/#lint_extend-select)
- `ruff.extend-unfixable` → [`ruff.lint.extend-unfixable`](https://docs.astral.sh//ruff/settings/#lint_extend-unfixable)
- `ruff.extend-unsafe-fixes` → [`ruff.lint.extend-unsafe-fixes`](https://docs.astral.sh//ruff/settings/#lint_extend-unsafe-fixes)
- `ruff.external` → [`ruff.lint.external`](https://docs.astral.sh//ruff/settings/#lint_external)
- `ruff.fixable` → [`ruff.lint.fixable`](https://docs.astral.sh//ruff/settings/#lint_fixable)
- `ruff.flake8-annotations` → [`ruff.lint.flake8-annotations`](https://docs.astral.sh//ruff/settings/#lint_flake8-annotations)
- `ruff.flake8-bandit` → [`ruff.lint.flake8-bandit`](https://docs.astral.sh//ruff/settings/#lint_flake8-bandit)
- `ruff.flake8-bugbear` → [`ruff.lint.flake8-bugbear`](https://docs.astral.sh//ruff/settings/#lint_flake8-bugbear)
- `ruff.flake8-builtins` → [`ruff.lint.flake8-builtins`](https://docs.astral.sh//ruff/settings/#lint_flake8-builtins)
- `ruff.flake8-comprehensions` → [`ruff.lint.flake8-comprehensions`](https://docs.astral.sh//ruff/settings/#lint_flake8-comprehensions)
- `ruff.flake8-copyright` → [`ruff.lint.flake8-copyright`](https://docs.astral.sh//ruff/settings/#lint_flake8-copyright)
- `ruff.flake8-errmsg` → [`ruff.lint.flake8-errmsg`](https://docs.astral.sh//ruff/settings/#lint_flake8-errmsg)
- `ruff.flake8-gettext` → [`ruff.lint.flake8-gettext`](https://docs.astral.sh//ruff/settings/#lint_flake8-gettext)
- `ruff.flake8-implicit-str-concat` → [`ruff.lint.flake8-implicit-str-concat`](https://docs.astral.sh//ruff/settings/#lint_flake8-implicit-str-concat)
- `ruff.flake8-import-conventions` → [`ruff.lint.flake8-import-conventions`](https://docs.astral.sh//ruff/settings/#lint_flake8-import-conventions)
- `ruff.flake8-pytest-style` → [`ruff.lint.flake8-pytest-style`](https://docs.astral.sh//ruff/settings/#lint_flake8-pytest-style)
- `ruff.flake8-quotes` → [`ruff.lint.flake8-quotes`](https://docs.astral.sh//ruff/settings/#lint_flake8-quotes)
- `ruff.flake8-self` → [`ruff.lint.flake8-self`](https://docs.astral.sh//ruff/settings/#lint_flake8-self)
- `ruff.flake8-tidy-imports` → [`ruff.lint.flake8-tidy-imports`](https://docs.astral.sh//ruff/settings/#lint_flake8-tidy-imports)
- `ruff.flake8-type-checking` → [`ruff.lint.flake8-type-checking`](https://docs.astral.sh//ruff/settings/#lint_flake8-type-checking)
- `ruff.flake8-unused-arguments` → [`ruff.lint.flake8-unused-arguments`](https://docs.astral.sh//ruff/settings/#lint_flake8-unused-arguments)
- `ruff.ignore` → [`ruff.lint.ignore`](https://docs.astral.sh//ruff/settings/#lint_ignore)
- `ruff.ignore-init-module-imports` → [`ruff.lint.ignore-init-module-imports`](https://docs.astral.sh//ruff/settings/#lint_ignore-init-module-imports)
- `ruff.isort` → [`ruff.lint.isort`](https://docs.astral.sh//ruff/settings/#lint_isort)
- `ruff.logger-objects` → [`ruff.lint.logger-objects`](https://docs.astral.sh//ruff/settings/#lint_logger-objects)
- `ruff.mccabe` → [`ruff.lint.mccabe`](https://docs.astral.sh//ruff/settings/#lint_mccabe)
- `ruff.pep8-naming` → [`ruff.lint.pep8-naming`](https://docs.astral.sh//ruff/settings/#lint_pep8-naming)
- `ruff.per-file-ignores` → [`ruff.lint.per-file-ignores`](https://docs.astral.sh//ruff/settings/#lint_per-file-ignores)
- `ruff.pycodestyle` → [`ruff.lint.pycodestyle`](https://docs.astral.sh//ruff/settings/#lint_pycodestyle)
- `ruff.pydocstyle` → [`ruff.lint.pydocstyle`](https://docs.astral.sh//ruff/settings/#lint_pydocstyle)
- `ruff.pyflakes` → [`ruff.lint.pyflakes`](https://docs.astral.sh//ruff/settings/#lint_pyflakes)
- `ruff.pylint` → [`ruff.lint.pylint`](https://docs.astral.sh//ruff/settings/#lint_pylint)
- `ruff.pyupgrade` → [`ruff.lint.pyupgrade`](https://docs.astral.sh//ruff/settings/#lint_pyupgrade)
- `ruff.select` → [`ruff.lint.select`](https://docs.astral.sh//ruff/settings/#lint_select)
- `ruff.task-tags` → [`ruff.lint.task-tags`](https://docs.astral.sh//ruff/settings/#lint_task-tags)
- `ruff.typing-modules` → [`ruff.lint.typing-modules`](https://docs.astral.sh//ruff/settings/#lint_typing-modules)
- `ruff.unfixable` → [`ruff.lint.unfixable`](https://docs.astral.sh//ruff/settings/#lint_unfixable)

### Remapped rules

The following rules have been remapped to new codes:

- [`raise-without-from-inside-except`](https://docs.astral.sh/ruff/rules/raise-without-from-inside-except/): `TRY200` to `B904`
- [`suspicious-eval-usage`](https://docs.astral.sh/ruff/rules/suspicious-eval-usage/): `PGH001` to `S307`
- [`logging-warn`](https://docs.astral.sh/ruff/rules/logging-warn/): `PGH002` to `G010`
- [`static-key-dict-comprehension`](https://docs.astral.sh/ruff/rules/static-key-dict-comprehension): `RUF011` to `B035`
- [`runtime-string-union`](https://docs.astral.sh/ruff/rules/runtime-string-union): `TCH006` to `TCH010`

### Stabilizations

The following rules have been stabilized and are no longer in preview:

- [`trio-timeout-without-await`](https://docs.astral.sh/ruff/rules/trio-timeout-without-await) (`TRIO100`)
- [`trio-sync-call`](https://docs.astral.sh/ruff/rules/trio-sync-call) (`TRIO105`)
- [`trio-async-function-with-timeout`](https://docs.astral.sh/ruff/rules/trio-async-function-with-timeout) (`TRIO109`)
- [`trio-unneeded-sleep`](https://docs.astral.sh/ruff/rules/trio-unneeded-sleep) (`TRIO110`)
- [`trio-zero-sleep-call`](https://docs.astral.sh/ruff/rules/trio-zero-sleep-call) (`TRIO115`)
- [`unnecessary-escaped-quote`](https://docs.astral.sh/ruff/rules/unnecessary-escaped-quote) (`Q004`)
- [`enumerate-for-loop`](https://docs.astral.sh/ruff/rules/enumerate-for-loop) (`SIM113`)
- [`zip-dict-keys-and-values`](https://docs.astral.sh/ruff/rules/zip-dict-keys-and-values) (`SIM911`)
- [`timeout-error-alias`](https://docs.astral.sh/ruff/rules/timeout-error-alias) (`UP041`)
- [`flask-debug-true`](https://docs.astral.sh/ruff/rules/flask-debug-true) (`S201`)
- [`tarfile-unsafe-members`](https://docs.astral.sh/ruff/rules/tarfile-unsafe-members) (`S202`)
- [`ssl-insecure-version`](https://docs.astral.sh/ruff/rules/ssl-insecure-version) (`S502`)
- [`ssl-with-bad-defaults`](https://docs.astral.sh/ruff/rules/ssl-with-bad-defaults) (`S503`)
- [`ssl-with-no-version`](https://docs.astral.sh/ruff/rules/ssl-with-no-version) (`S504`)
- [`weak-cryptographic-key`](https://docs.astral.sh/ruff/rules/weak-cryptographic-key) (`S505`)
- [`ssh-no-host-key-verification`](https://docs.astral.sh/ruff/rules/ssh-no-host-key-verification) (`S507`)
- [`django-raw-sql`](https://docs.astral.sh/ruff/rules/django-raw-sql) (`S611`)
- [`mako-templates`](https://docs.astral.sh/ruff/rules/mako-templates) (`S702`)
- [`generator-return-from-iter-method`](https://docs.astral.sh/ruff/rules/generator-return-from-iter-method) (`PYI058`)
- [`runtime-string-union`](https://docs.astral.sh/ruff/rules/runtime-string-union) (`TCH006`)
- [`numpy2-deprecation`](https://docs.astral.sh/ruff/rules/numpy2-deprecation) (`NPY201`)
- [`quadratic-list-summation`](https://docs.astral.sh/ruff/rules/quadratic-list-summation) (`RUF017`)
- [`assignment-in-assert`](https://docs.astral.sh/ruff/rules/assignment-in-assert) (`RUF018`)
- [`unnecessary-key-check`](https://docs.astral.sh/ruff/rules/unnecessary-key-check) (`RUF019`)
- [`never-union`](https://docs.astral.sh/ruff/rules/never-union) (`RUF020`)
- [`direct-logger-instantiation`](https://docs.astral.sh/ruff/rules/direct-logger-instantiation) (`LOG001`)
- [`invalid-get-logger-argument`](https://docs.astral.sh/ruff/rules/invalid-get-logger-argument) (`LOG002`)
- [`exception-without-exc-info`](https://docs.astral.sh/ruff/rules/exception-without-exc-info) (`LOG007`)
- [`undocumented-warn`](https://docs.astral.sh/ruff/rules/undocumented-warn) (`LOG009`)

Fixes for the following rules have been stabilized and are now available without preview:

- [`triple-single-quotes`](https://docs.astral.sh/ruff/rules/triple-single-quotes) (`D300`)
- [`non-pep604-annotation`](https://docs.astral.sh/ruff/rules/non-pep604-annotation) (`UP007`)
- [`dict-get-with-none-default`](https://docs.astral.sh/ruff/rules/dict-get-with-none-default) (`SIM910`)
- [`in-dict-keys`](https://docs.astral.sh/ruff/rules/in-dict-keys) (`SIM118`)
- [`collapsible-else-if`](https://docs.astral.sh/ruff/rules/collapsible-else-if) (`PLR5501`)
- [`if-with-same-arms`](https://docs.astral.sh/ruff/rules/if-with-same-arms) (`SIM114`)
- [`useless-else-on-loop`](https://docs.astral.sh/ruff/rules/useless-else-on-loop) (`PLW0120`)
- [`unnecessary-literal-union`](https://docs.astral.sh/ruff/rules/unnecessary-literal-union) (`PYI030`)
- [`unnecessary-spread`](https://docs.astral.sh/ruff/rules/unnecessary-spread) (`PIE800`)
- [`error-instead-of-exception`](https://docs.astral.sh/ruff/rules/error-instead-of-exception) (`TRY400`)
- [`redefined-while-unused`](https://docs.astral.sh/ruff/rules/redefined-while-unused) (`F811`)
- [`duplicate-value`](https://docs.astral.sh/ruff/rules/duplicate-value) (`B033`)
- [`multiple-imports-on-one-line`](https://docs.astral.sh/ruff/rules/multiple-imports-on-one-line) (`E401`)
- [`non-pep585-annotation`](https://docs.astral.sh/ruff/rules/non-pep585-annotation) (`UP006`)

Fixes for the following rules have been promoted from unsafe to safe:

- [`unaliased-collections-abc-set-import`](https://docs.astral.sh/ruff/rules/unaliased-collections-abc-set-import) (`PYI025`)

The following behaviors have been stabilized:

- [`module-import-not-at-top-of-file`](https://docs.astral.sh/ruff/rules/module-import-not-at-top-of-file/) (`E402`) allows `sys.path` modifications between imports
- [`reimplemented-container-builtin`](https://docs.astral.sh/ruff/rules/reimplemented-container-builtin/) (`PIE807`) includes lambdas that can be replaced with `dict`
- [`unnecessary-placeholder`](https://docs.astral.sh/ruff/rules/unnecessary-placeholder/) (`PIE790`) applies to unnecessary ellipses (`...`)
- [`if-else-block-instead-of-dict-get`](https://docs.astral.sh/ruff/rules/if-else-block-instead-of-dict-get/) (`SIM401`) applies to `if-else` expressions

### Preview features

- \[`refurb`\] Implement `metaclass_abcmeta` (`FURB180`) ([#9658](https://github.com/astral-sh/ruff/pull/9658))
- Implement `blank_line_after_nested_stub_class` preview style ([#9155](https://github.com/astral-sh/ruff/pull/9155))
- The preview rule [`and-or-ternary`](https://docs.astral.sh/ruff/rules/and-or-ternary) (`PLR1706`) was removed

### Bug fixes

- \[`flake8-async`\] Take `pathlib.Path` into account when analyzing async functions ([#9703](https://github.com/astral-sh/ruff/pull/9703))
- \[`flake8-return`\] - fix indentation syntax error (`RET505`) ([#9705](https://github.com/astral-sh/ruff/pull/9705))
- Detect multi-statement lines in else removal ([#9748](https://github.com/astral-sh/ruff/pull/9748))
- `RUF022`, `RUF023`: never add two trailing commas to the end of a sequence ([#9698](https://github.com/astral-sh/ruff/pull/9698))
- `RUF023`: Don't sort `__match_args__`, only `__slots__` ([#9724](https://github.com/astral-sh/ruff/pull/9724))
- \[`flake8-simplify`\] - Fix syntax error in autofix (`SIM114`) ([#9704](https://github.com/astral-sh/ruff/pull/9704))
- \[`pylint`\] Show verbatim constant in `magic-value-comparison` (`PLR2004`) ([#9694](https://github.com/astral-sh/ruff/pull/9694))
- Removing trailing whitespace inside multiline strings is unsafe ([#9744](https://github.com/astral-sh/ruff/pull/9744))
- Support `IfExp` with dual string arms in `invalid-envvar-default` ([#9734](https://github.com/astral-sh/ruff/pull/9734))
- \[`pylint`\] Add `__mro_entries__` to known dunder methods (`PLW3201`) ([#9706](https://github.com/astral-sh/ruff/pull/9706))

### Documentation

- Removed rules are now retained in the documentation ([#9691](https://github.com/astral-sh/ruff/pull/9691))
- Deprecated rules are now indicated in the documentation ([#9689](https://github.com/astral-sh/ruff/pull/9689))

## 0.1.15

### Preview features

- Error when `NURSERY` selector is used with `--preview` ([#9682](https://github.com/astral-sh/ruff/pull/9682))
- Preserve indentation around multiline strings in formatter ([#9637](https://github.com/astral-sh/ruff/pull/9637))
- \[`flake8-return`\] Add fixes for all rules (`RET505`, `RET506`, `RET507`, `RET508`) ([#9595](https://github.com/astral-sh/ruff/pull/9595))
- \[`flake8-simplify`\] Add fix for `if-with-same-arms` (`SIM114`) ([#9591](https://github.com/astral-sh/ruff/pull/9591))
- \[`pycodestyle`\] Add fix for `multiple-imports-on-one-line` (`E401`) ([#9518](https://github.com/astral-sh/ruff/pull/9518))
- \[`pylint`\] Add fix for `collapsible-else-if` (`PLR5501`) ([#9594](https://github.com/astral-sh/ruff/pull/9594))
- \[`pylint`\] Add fix for `useless-else-on-loop` (`PLW0120`) ([#9590](https://github.com/astral-sh/ruff/pull/9590))
- \[`pylint`\] Implement `assigning-non-slot` (`E0237`) ([#9623](https://github.com/astral-sh/ruff/pull/9623))
- \[`pylint`\] Implement `potential-index-error` (`PLE0643`) ([#9545](https://github.com/astral-sh/ruff/pull/9545))
- \[`pylint`\] Implement `too-many-nested-blocks` (`PLR1702`) ([#9172](https://github.com/astral-sh/ruff/pull/9172))
- \[`ruff`\] Add rule to sort `__slots__` and `__match_args__` ([#9564](https://github.com/astral-sh/ruff/pull/9564))
- \[`ruff`\] Detect unnecessary `dict` comprehensions for iterables (`RUF025`) ([#9613](https://github.com/astral-sh/ruff/pull/9613))
- \[`ruff`\] Guard against use of `default_factory` as a keyword argument (`RUF026`) ([#9651](https://github.com/astral-sh/ruff/pull/9651))
- \[`ruff`\] Implement `mutable-fromkeys-value` (`RUF024`) ([#9597](https://github.com/astral-sh/ruff/pull/9597))

### CLI

- Enable auto-wrapping of `--help` output ([#9633](https://github.com/astral-sh/ruff/pull/9633))

### Bug fixes

- Avoid rendering display-only rules as fixable ([#9649](https://github.com/astral-sh/ruff/pull/9649))
- Detect automagic-like assignments in notebooks ([#9653](https://github.com/astral-sh/ruff/pull/9653))
- Generate custom JSON schema for dynamic setting ([#9632](https://github.com/astral-sh/ruff/pull/9632))
- \[`flake8-no-pep420`\] Include global `--config` when determining namespace packages ([#9603](https://github.com/astral-sh/ruff/pull/9603))
- \[`flake8-pie`\] Omit bound tuples passed to `.startswith` or `.endswith` ([#9661](https://github.com/astral-sh/ruff/pull/9661))
- \[`flake8-return`\] Avoid panic when fixing inlined else blocks ([#9657](https://github.com/astral-sh/ruff/pull/9657))
- \[`flake8-return`\] Consider exception suppression in unnecessary assignment ([#9673](https://github.com/astral-sh/ruff/pull/9673))
- \[`flake8-return`\] Take `NoReturn` annotation into account when analyzing implicit returns ([#9636](https://github.com/astral-sh/ruff/pull/9636))
- \[`flake8-simplify`\] Support inverted returns in `needless-bool` (`SIM103`) ([#9619](https://github.com/astral-sh/ruff/pull/9619))
- \[`flake8-type-checking`\] Add Pydantic's `BaseConfig` to default-copy list ([#9650](https://github.com/astral-sh/ruff/pull/9650))
- \[`flake8-type-checking`\] Avoid marking `InitVar` as a typing-only annotation ([#9688](https://github.com/astral-sh/ruff/pull/9688))
- \[`pycodestyle`\] Allow `dtype` comparisons in `type-comparison` ([#9676](https://github.com/astral-sh/ruff/pull/9676))
- \[`pydocstyle`\] Re-implement `last-line-after-section` (`D413`) ([#9654](https://github.com/astral-sh/ruff/pull/9654))

### Documentation

- \[`flake8-pytest-style`\] Add fix safety documentation for `duplicate-parameterize-test-cases` ([#9678](https://github.com/astral-sh/ruff/pull/9678))
- \[`pylint`\] Document `literal-membership` fix safety conditions ([#9677](https://github.com/astral-sh/ruff/pull/9677))
- \[`isort`\] Fix reference to `isort` rule code ([#9598](https://github.com/astral-sh/ruff/pull/9598))

## 0.1.14

### Preview features

- \[`flake8-bugbear`\] Add fix for `duplicate-value` (`B033`) ([#9510](https://github.com/astral-sh/ruff/pull/9510))
- \[`flake8-simplify`\] Implement `enumerate-for-loop` (`SIM113`) ([#7777](https://github.com/astral-sh/ruff/pull/7777))
- \[`pygrep_hooks`\] Add fix for `deprecated-log-warn` (`PGH002`) ([#9519](https://github.com/astral-sh/ruff/pull/9519))
- \[`pylint`\] Implement `import-private-name` (`C2701`) ([#5920](https://github.com/astral-sh/ruff/pull/5920))
- \[`refurb`\] Implement `regex-flag-alias` with fix (`FURB167`) ([#9516](https://github.com/astral-sh/ruff/pull/9516))
- \[`ruff`\] Add rule and fix to sort contents of `__all__` (`RUF022`) ([#9474](https://github.com/astral-sh/ruff/pull/9474))
- \[`tryceratops`\] Add fix for `error-instead-of-exception` (`TRY400`) ([#9520](https://github.com/astral-sh/ruff/pull/9520))

### Rule changes

- \[`flake8-pyi`\] Fix `PYI047` false negatives on PEP-695 type aliases ([#9566](https://github.com/astral-sh/ruff/pull/9566))
- \[`flake8-pyi`\] Fix `PYI049` false negatives on call-based `TypedDict`s ([#9567](https://github.com/astral-sh/ruff/pull/9567))
- \[`pylint`\] Exclude `self` and `cls` when counting method arguments (`PLR0917`) ([#9563](https://github.com/astral-sh/ruff/pull/9563))

### CLI

- `--show-settings` displays active settings in a far more readable format ([#9464](https://github.com/astral-sh/ruff/pull/9464))
- Add `--extension` support to the formatter ([#9483](https://github.com/astral-sh/ruff/pull/9483))

### Configuration

- Ignore preview status for fixable and unfixable selectors ([#9538](https://github.com/astral-sh/ruff/pull/9538))
- \[`pycodestyle`\] Use the configured tab size when expanding indents ([#9506](https://github.com/astral-sh/ruff/pull/9506))

### Bug fixes

- Recursively visit deferred AST nodes ([#9541](https://github.com/astral-sh/ruff/pull/9541))
- Visit deferred lambdas before type definitions ([#9540](https://github.com/astral-sh/ruff/pull/9540))
- \[`flake8-simplify`\] Avoid some more `enumerate-for-loop` false positives (`SIM113`) ([#9515](https://github.com/astral-sh/ruff/pull/9515))
- \[`pandas-vet`\] Limit inplace diagnostics to methods that accept inplace ([#9495](https://github.com/astral-sh/ruff/pull/9495))
- \[`pylint`\] Add the `__prepare__` method to the list of recognized dunder method ([#9529](https://github.com/astral-sh/ruff/pull/9529))
- \[`pylint`\] Ignore unnecessary dunder calls within dunder definitions ([#9496](https://github.com/astral-sh/ruff/pull/9496))
- \[`refurb`\] Avoid bailing when `reimplemented-operator` is called on function (`FURB118`) ([#9556](https://github.com/astral-sh/ruff/pull/9556))
- \[`ruff`\] Avoid treating named expressions as static keys (`RUF011`) ([#9494](https://github.com/astral-sh/ruff/pull/9494))

### Documentation

- Add instructions on using `noqa` with isort rules ([#9555](https://github.com/astral-sh/ruff/pull/9555))
- Documentation update for URL giving 'page not found' ([#9565](https://github.com/astral-sh/ruff/pull/9565))
- Fix admonition in dark mode ([#9502](https://github.com/astral-sh/ruff/pull/9502))
- Update contributing docs to use `cargo bench -p ruff_benchmark` ([#9535](https://github.com/astral-sh/ruff/pull/9535))
- Update emacs integration section to include `emacs-ruff-format` ([#9403](https://github.com/astral-sh/ruff/pull/9403))
- \[`flake8-blind-except`\] Document exceptions to `blind-except` rule ([#9580](https://github.com/astral-sh/ruff/pull/9580))

## 0.1.13

### Bug fixes

- Include base pyproject when initializing cache settings ([#9480](https://github.com/astral-sh/ruff/pull/9480))
- \[`flake8-simplify`\] Account for possibly-empty f-string values in truthiness logic ([#9484](https://github.com/astral-sh/ruff/pull/9484))
- \[`pylint`\] Add the missing period in `unnecessary-dunder-call` ([#9485](https://github.com/astral-sh/ruff/pull/9485))
- \[`pylint`\] Fix `__aenter__` message in `unnecessary-dunder-call` ([#9492](https://github.com/astral-sh/ruff/pull/9492))

## 0.1.12

### Preview features

- Formatter: Hug multiline-strings in preview style ([#9243](https://github.com/astral-sh/ruff/pull/9243))
- \[`flake8-bandit`\] Add `ssl-with-no-version` (`S504`) ([#9384](https://github.com/astral-sh/ruff/pull/9384))
- \[`flake8-bandit`\] Implement `ssl-insecure-version` (`S502`) ([#9390](https://github.com/astral-sh/ruff/pull/9390))
- \[`flake8-bandit`\] Implement `ssl-with-bad-defaults` (`S503`) ([#9391](https://github.com/astral-sh/ruff/pull/9391))
- \[`flake8-bandit`\] Implement suspicious import rules (`S4XX`) ([#8831](https://github.com/astral-sh/ruff/pull/8831))
- \[`flake8-simplify`\] Implement `zip-dict-keys-and-values` (`SIM911`) ([#9460](https://github.com/astral-sh/ruff/pull/9460))
- \[`pyflakes`\] Add a fix for `redefined-while-unused` (`F811`) ([#9419](https://github.com/astral-sh/ruff/pull/9419))
- \[`pylint`\] Implement `unnecessary-dunder-call` (`C2801`) ([#9166](https://github.com/astral-sh/ruff/pull/9166))
- \[`ruff`\] Add `parenthesize-chained-operators` (`RUF021`) to enforce parentheses in `a or b and c` ([#9440](https://github.com/astral-sh/ruff/pull/9440))

### Rule changes

- \[`flake8-boolean-trap`\] Allow Boolean positional arguments in setters ([#9429](https://github.com/astral-sh/ruff/pull/9429))
- \[`flake8-builtins`\] Restrict `builtin-attribute-shadowing` (`A003`) to actual shadowed references ([#9462](https://github.com/astral-sh/ruff/pull/9462))
- \[`flake8-pyi`\] Add fix for `generator-return-from-iter-method` (`PYI058`) ([#9355](https://github.com/astral-sh/ruff/pull/9355))
- \[`pyflakes`\] Don't flag `redefined-while-unused` (`F811`) in `if` branches ([#9418](https://github.com/astral-sh/ruff/pull/9418))
- \[`pyupgrade`\] Add some additional Python 3.12 typing members to `deprecated-import` ([#9445](https://github.com/astral-sh/ruff/pull/9445))
- \[`ruff`\] Add fix for `parenthesize-chained-operators` (`RUF021`) ([#9449](https://github.com/astral-sh/ruff/pull/9449))
- \[`ruff`\] Include subscripts and attributes in static key rule (`RUF011`) ([#9416](https://github.com/astral-sh/ruff/pull/9416))
- \[`ruff`\] Support variable keys in static dictionary key rule (`RUF011`) ([#9411](https://github.com/astral-sh/ruff/pull/9411))

### Formatter

- Generate deterministic IDs when formatting notebooks ([#9359](https://github.com/astral-sh/ruff/pull/9359))
- Allow `# fmt: skip` with interspersed same-line comments ([#9395](https://github.com/astral-sh/ruff/pull/9395))
- Parenthesize breaking named expressions in match guards ([#9396](https://github.com/astral-sh/ruff/pull/9396))

### Bug fixes

- Add cell indexes to all diagnostics ([#9387](https://github.com/astral-sh/ruff/pull/9387))
- Avoid infinite loop in constant vs. `None` comparisons ([#9376](https://github.com/astral-sh/ruff/pull/9376))
- Handle raises with implicit alternate branches ([#9377](https://github.com/astral-sh/ruff/pull/9377))
- Ignore trailing quotes for unclosed l-brace errors ([#9388](https://github.com/astral-sh/ruff/pull/9388))
- Respect multi-segment submodule imports when resolving qualified names ([#9382](https://github.com/astral-sh/ruff/pull/9382))
- Use `DisplayParseError` for stdin parser errors ([#9409](https://github.com/astral-sh/ruff/pull/9409))
- Use `comment_ranges` for isort directive extraction ([#9414](https://github.com/astral-sh/ruff/pull/9414))
- Use transformed source code for diagnostic locations ([#9408](https://github.com/astral-sh/ruff/pull/9408))
- \[`flake8-pyi`\] Exclude `warnings.deprecated` and `typing_extensions.deprecated` arguments ([#9423](https://github.com/astral-sh/ruff/pull/9423))
- \[`flake8-pyi`\] Fix false negative for `unused-private-protocol` (`PYI046`) with unused generic protocols ([#9405](https://github.com/astral-sh/ruff/pull/9405))
- \[`pydocstyle`\] Disambiguate argument descriptors from section headers ([#9427](https://github.com/astral-sh/ruff/pull/9427))
- \[`pylint`\] Homogenize `PLR0914` message to match other `PLR09XX` rules ([#9399](https://github.com/astral-sh/ruff/pull/9399))
- \[`ruff`\] Allow `Hashable = None` in type annotations (`RUF013`) ([#9442](https://github.com/astral-sh/ruff/pull/9442))

### Documentation

- Fix admonition hyperlink colouring ([#9385](https://github.com/astral-sh/ruff/pull/9385))
- Add missing preview link ([#9386](https://github.com/astral-sh/ruff/pull/9386))

## 0.1.11

### Preview features

- \[`pylint`\] Implement `super-without-brackets` (`W0245`) ([#9257](https://github.com/astral-sh/ruff/pull/9257))

### Bug fixes

- Check path string properly in `python -m ruff` invocations ([#9367](https://github.com/astral-sh/ruff/pull/9367))

### Documentation

- Tweak `relative-imports` message ([#9365](https://github.com/astral-sh/ruff/pull/9365))
- Add fix safety note for `yield-in-for-loop` ([#9364](https://github.com/astral-sh/ruff/pull/9364))

## 0.1.10

### Preview features

- Improve `dummy_implementations` preview style formatting ([#9240](https://github.com/astral-sh/ruff/pull/9240))
- Normalise Hex and unicode escape sequences in strings ([#9280](https://github.com/astral-sh/ruff/pull/9280))
- Parenthesize long type annotations in annotated assignments ([#9210](https://github.com/astral-sh/ruff/pull/9210))
- Parenthesize multi-context managers in `with` statements ([#9222](https://github.com/astral-sh/ruff/pull/9222))
- \[`flake8-pyi`\] Implement `generator-return-from-iter-method` (`PYI058`) ([#9313](https://github.com/astral-sh/ruff/pull/9313))
- \[`pylint`\] Implement `empty-comment` (`PLR2044`) ([#9174](https://github.com/astral-sh/ruff/pull/9174))
- \[`refurb`\] Implement `bit-count` (`FURB161`) ([#9265](https://github.com/astral-sh/ruff/pull/9265))
- \[`ruff`\] Add `never-union` rule to detect redundant `typing.NoReturn` and `typing.Never` ([#9217](https://github.com/astral-sh/ruff/pull/9217))

### CLI

- Add paths to TOML parse errors ([#9358](https://github.com/astral-sh/ruff/pull/9358))
- Add row and column numbers to formatter parse errors ([#9321](https://github.com/astral-sh/ruff/pull/9321))
- Improve responsiveness when invoked via Python ([#9315](https://github.com/astral-sh/ruff/pull/9315))
- Short rule messages should not end with a period ([#9345](https://github.com/astral-sh/ruff/pull/9345))

### Configuration

- Respect runtime-required decorators on functions ([#9317](https://github.com/astral-sh/ruff/pull/9317))

### Bug fixes

- Avoid `asyncio-dangling-task` for nonlocal and global bindings ([#9263](https://github.com/astral-sh/ruff/pull/9263))
- Escape trailing placeholders in rule documentation ([#9301](https://github.com/astral-sh/ruff/pull/9301))
- Fix continuation detection following multi-line strings ([#9332](https://github.com/astral-sh/ruff/pull/9332))
- Fix scoping for generators in named expressions in classes ([#9248](https://github.com/astral-sh/ruff/pull/9248))
- Port from obsolete wsl crate to is-wsl ([#9356](https://github.com/astral-sh/ruff/pull/9356))
- Remove special pre-visit for module docstrings ([#9261](https://github.com/astral-sh/ruff/pull/9261))
- Respect `__str__` definitions from super classes ([#9338](https://github.com/astral-sh/ruff/pull/9338))
- Respect `unused-noqa` via `per-file-ignores` ([#9300](https://github.com/astral-sh/ruff/pull/9300))
- Respect attribute chains when resolving builtin call paths ([#9309](https://github.com/astral-sh/ruff/pull/9309))
- Treat all `typing_extensions` members as typing aliases ([#9335](https://github.com/astral-sh/ruff/pull/9335))
- Use `Display` for formatter parse errors ([#9316](https://github.com/astral-sh/ruff/pull/9316))
- Wrap subscripted dicts in parens for f-string conversion ([#9238](https://github.com/astral-sh/ruff/pull/9238))
- \[`flake8-annotations`\] Avoid adding return types to stub methods ([#9277](https://github.com/astral-sh/ruff/pull/9277))
- \[`flake8-annotations`\] Respect mixed `return` and `raise` cases in return-type analysis ([#9310](https://github.com/astral-sh/ruff/pull/9310))
- \[`flake8-bandit`\] Don't report violations when `SafeLoader` is imported from `yaml.loader` (`S506`) ([#9299](https://github.com/astral-sh/ruff/pull/9299))
- \[`pylint`\] Avoid panic when comment is preceded by Unicode ([#9331](https://github.com/astral-sh/ruff/pull/9331))
- \[`pylint`\] Change `PLR0917` error message to match other `PLR09XX` messages ([#9308](https://github.com/astral-sh/ruff/pull/9308))
- \[`refurb`\] Avoid false positives for `math-constant` (`FURB152`) ([#9290](https://github.com/astral-sh/ruff/pull/9290))

### Documentation

- Expand target name for better rule documentation ([#9302](https://github.com/astral-sh/ruff/pull/9302))
- Fix typos found by codespell ([#9346](https://github.com/astral-sh/ruff/pull/9346))
- \[`perflint`\] Document `PERF102` fix un-safety ([#9351](https://github.com/astral-sh/ruff/pull/9351))
- \[`pyupgrade`\] Document `UP007` fix un-safety ([#9306](https://github.com/astral-sh/ruff/pull/9306))

## 0.1.9

### Breaking changes

- Add site-packages to default exclusions ([#9188](https://github.com/astral-sh/ruff/pull/9188))

### Preview features

- Fix: Avoid parenthesizing subscript targets and values ([#9209](https://github.com/astral-sh/ruff/pull/9209))
- \[`pylint`\] Implement `too-many-locals` (`PLR0914`) ([#9163](https://github.com/astral-sh/ruff/pull/9163))
- Implement `reimplemented_operator` (FURB118) ([#9171](https://github.com/astral-sh/ruff/pull/9171))
- Add a rule to detect string members in runtime-evaluated unions ([#9143](https://github.com/astral-sh/ruff/pull/9143))
- Implement `no_blank_line_before_class_docstring` preview style ([#9154](https://github.com/astral-sh/ruff/pull/9154))

### Rule changes

- `CONSTANT_CASE` variables are improperly flagged for yoda violation (`SIM300`) ([#9164](https://github.com/astral-sh/ruff/pull/9164))
- \[`flake8-pyi`\] Cover ParamSpecs and TypeVarTuples (`PYI018`) ([#9198](https://github.com/astral-sh/ruff/pull/9198))
- \[`flake8-bugbear`\] Add fix for `zip-without-explicit-strict` (`B905`) ([#9176](https://github.com/astral-sh/ruff/pull/9176))
- Add fix to automatically remove `print` and `pprint` statements (`T201`, `T203`) ([#9208](https://github.com/astral-sh/ruff/pull/9208))
- Prefer `Never` to `NoReturn` in auto-typing in Python >= 3.11 (`ANN201`) ([#9213](https://github.com/astral-sh/ruff/pull/9213))

### Formatter

- `can_omit_optional_parentheses`: Exit early for unparenthesized expressions ([#9125](https://github.com/astral-sh/ruff/pull/9125))
- Fix `dynamic` mode with doctests so that it doesn't exceed configured line width ([#9129](https://github.com/astral-sh/ruff/pull/9129))
- Fix `can_omit_optional_parentheses` for expressions with a right most fstring ([#9124](https://github.com/astral-sh/ruff/pull/9124))
- Add `target_version` to formatter options ([#9220](https://github.com/astral-sh/ruff/pull/9220))

### CLI

- Update `ruff format --check` to display message for already formatted files ([#9153](https://github.com/astral-sh/ruff/pull/9153))

### Bug fixes

- Reverse order of arguments for `operator.contains` ([#9192](https://github.com/astral-sh/ruff/pull/9192))
- Iterate over lambdas in deferred type annotations ([#9175](https://github.com/astral-sh/ruff/pull/9175))
- Fix panic in `D208` with multibyte indent ([#9147](https://github.com/astral-sh/ruff/pull/9147))
- Add support for `NoReturn` in auto-return-typing ([#9206](https://github.com/astral-sh/ruff/pull/9206))
- Allow removal of `typing` from `exempt-modules` ([#9214](https://github.com/astral-sh/ruff/pull/9214))
- Avoid `mutable-class-default` violations for Pydantic subclasses ([#9187](https://github.com/astral-sh/ruff/pull/9187))
- Fix dropped union expressions for piped non-types in `PYI055` autofix ([#9161](https://github.com/astral-sh/ruff/pull/9161))
- Enable annotation quoting for multi-line expressions ([#9142](https://github.com/astral-sh/ruff/pull/9142))
- Deduplicate edits when quoting annotations ([#9140](https://github.com/astral-sh/ruff/pull/9140))
- Prevent invalid utf8 indexing in cell magic detection ([#9146](https://github.com/astral-sh/ruff/pull/9146))
- Avoid nested quotations in auto-quoting fix ([#9168](https://github.com/astral-sh/ruff/pull/9168))
- Add base-class inheritance detection to flake8-django rules ([#9151](https://github.com/astral-sh/ruff/pull/9151))
- Avoid `asyncio-dangling-task` violations on shadowed bindings ([#9215](https://github.com/astral-sh/ruff/pull/9215))

### Documentation

- Fix blog post URL in changelog ([#9119](https://github.com/astral-sh/ruff/pull/9119))
- Add error suppression hint for multi-line strings ([#9205](https://github.com/astral-sh/ruff/pull/9205))
- Fix typo in SemanticModel.parent_expression docstring ([#9167](https://github.com/astral-sh/ruff/pull/9167))
- Document link between import sorting and formatter ([#9117](https://github.com/astral-sh/ruff/pull/9117))

## 0.1.8

This release includes opt-in support for formatting Python snippets within
docstrings via the `docstring-code-format` setting.
[Check out the blog post](https://astral.sh/blog/ruff-v0.1.8) for more details!

### Preview features

- Add `"preserve"` quote-style to mimic Black's skip-string-normalization ([#8822](https://github.com/astral-sh/ruff/pull/8822))
- Implement `prefer_splitting_right_hand_side_of_assignments` preview style ([#8943](https://github.com/astral-sh/ruff/pull/8943))
- \[`pycodestyle`\] Add fix for `unexpected-spaces-around-keyword-parameter-equals` ([#9072](https://github.com/astral-sh/ruff/pull/9072))
- \[`pycodestyle`\] Add fix for comment-related whitespace rules ([#9075](https://github.com/astral-sh/ruff/pull/9075))
- \[`pycodestyle`\] Allow `sys.path` modifications between imports ([#9047](https://github.com/astral-sh/ruff/pull/9047))
- \[`refurb`\] Implement `hashlib-digest-hex` (`FURB181`) ([#9077](https://github.com/astral-sh/ruff/pull/9077))

### Rule changes

- Allow `flake8-type-checking` rules to automatically quote runtime-evaluated references ([#6001](https://github.com/astral-sh/ruff/pull/6001))
- Allow transparent cell magics in Jupyter Notebooks ([#8911](https://github.com/astral-sh/ruff/pull/8911))
- \[`flake8-annotations`\] Avoid `ANN2xx` fixes for abstract methods with empty bodies ([#9034](https://github.com/astral-sh/ruff/pull/9034))
- \[`flake8-self`\] Ignore underscore references in type annotations ([#9036](https://github.com/astral-sh/ruff/pull/9036))
- \[`pep8-naming`\] Allow class names when `apps.get_model` is a non-string ([#9065](https://github.com/astral-sh/ruff/pull/9065))
- \[`pycodestyle`\] Allow `matplotlib.use` calls to intersperse imports ([#9094](https://github.com/astral-sh/ruff/pull/9094))
- \[`pyflakes`\] Support fixing unused assignments in tuples by renaming variables (`F841`) ([#9107](https://github.com/astral-sh/ruff/pull/9107))
- \[`pylint`\] Add fix for `subprocess-run-without-check` (`PLW1510`) ([#6708](https://github.com/astral-sh/ruff/pull/6708))

### Formatter

- Add `docstring-code-format` knob to enable docstring snippet formatting ([#8854](https://github.com/astral-sh/ruff/pull/8854))
- Use double quotes for all docstrings, including single-quoted docstrings ([#9020](https://github.com/astral-sh/ruff/pull/9020))
- Implement "dynamic" line width mode for docstring code formatting ([#9098](https://github.com/astral-sh/ruff/pull/9098))
- Support reformatting Markdown code blocks ([#9030](https://github.com/astral-sh/ruff/pull/9030))
- add support for formatting reStructuredText code snippets ([#9003](https://github.com/astral-sh/ruff/pull/9003))
- Avoid trailing comma for single-argument with positional separator ([#9076](https://github.com/astral-sh/ruff/pull/9076))
- Fix handling of trailing target comment ([#9051](https://github.com/astral-sh/ruff/pull/9051))

### CLI

- Hide unsafe fix suggestions when explicitly disabled ([#9095](https://github.com/astral-sh/ruff/pull/9095))
- Add SARIF support to `--output-format` ([#9078](https://github.com/astral-sh/ruff/pull/9078))

### Bug fixes

- Apply unnecessary index rule prior to enumerate rewrite ([#9012](https://github.com/astral-sh/ruff/pull/9012))
- \[`flake8-err-msg`\] Allow `EM` fixes even if `msg` variable is defined ([#9059](https://github.com/astral-sh/ruff/pull/9059))
- \[`flake8-pie`\] Prevent keyword arguments duplication ([#8450](https://github.com/astral-sh/ruff/pull/8450))
- \[`flake8-pie`\] Respect trailing comma in `unnecessary-dict-kwargs` (`PIE804`) ([#9015](https://github.com/astral-sh/ruff/pull/9015))
- \[`flake8-raise`\] Avoid removing parentheses on ctypes.WinError ([#9027](https://github.com/astral-sh/ruff/pull/9027))
- \[`isort`\] Avoid invalid combination of `force-sort-within-types` and `lines-between-types` ([#9041](https://github.com/astral-sh/ruff/pull/9041))
- \[`isort`\] Ensure that from-style imports are always ordered first in `__future__` ([#9039](https://github.com/astral-sh/ruff/pull/9039))
- \[`pycodestyle`\] Allow tab indentation before keyword ([#9099](https://github.com/astral-sh/ruff/pull/9099))
- \[`pylint`\] Ignore `@overrides` and `@overloads` for `too-many-positional` ([#9000](https://github.com/astral-sh/ruff/pull/9000))
- \[`pyupgrade`\] Enable `printf-string-formatting` fix with comments on right-hand side ([#9037](https://github.com/astral-sh/ruff/pull/9037))
- \[`refurb`\] Make `math-constant` (`FURB152`) rule more targeted ([#9054](https://github.com/astral-sh/ruff/pull/9054))
- \[`refurb`\] Support floating-point base in `redundant-log-base` (`FURB163`) ([#9100](https://github.com/astral-sh/ruff/pull/9100))
- \[`ruff`\] Detect `unused-asyncio-dangling-task` (`RUF006`) on unused assignments ([#9060](https://github.com/astral-sh/ruff/pull/9060))

## 0.1.7

### Preview features

- Implement multiline dictionary and list hugging for preview style ([#8293](https://github.com/astral-sh/ruff/pull/8293))
- Implement the `fix_power_op_line_length` preview style ([#8947](https://github.com/astral-sh/ruff/pull/8947))
- Use Python version to determine typing rewrite safety ([#8919](https://github.com/astral-sh/ruff/pull/8919))
- \[`flake8-annotations`\] Enable auto-return-type involving `Optional` and `Union` annotations ([#8885](https://github.com/astral-sh/ruff/pull/8885))
- \[`flake8-bandit`\] Implement `django-raw-sql` (`S611`) ([#8651](https://github.com/astral-sh/ruff/pull/8651))
- \[`flake8-bandit`\] Implement `tarfile-unsafe-members` (`S202`) ([#8829](https://github.com/astral-sh/ruff/pull/8829))
- \[`flake8-pyi`\] Implement fix for `unnecessary-literal-union` (`PYI030`) ([#7934](https://github.com/astral-sh/ruff/pull/7934))
- \[`flake8-simplify`\] Extend `dict-get-with-none-default` (`SIM910`) to non-literals ([#8762](https://github.com/astral-sh/ruff/pull/8762))
- \[`pylint`\] - add `unnecessary-list-index-lookup` (`PLR1736`) + autofix ([#7999](https://github.com/astral-sh/ruff/pull/7999))
- \[`pylint`\] - implement R0202 and R0203 with autofixes ([#8335](https://github.com/astral-sh/ruff/pull/8335))
- \[`pylint`\] Implement `repeated-keyword` (`PLe1132`) ([#8706](https://github.com/astral-sh/ruff/pull/8706))
- \[`pylint`\] Implement `too-many-positional` (`PLR0917`) ([#8995](https://github.com/astral-sh/ruff/pull/8995))
- \[`pylint`\] Implement `unnecessary-dict-index-lookup` (`PLR1733`) ([#8036](https://github.com/astral-sh/ruff/pull/8036))
- \[`refurb`\] Implement `redundant-log-base` (`FURB163`) ([#8842](https://github.com/astral-sh/ruff/pull/8842))

### Rule changes

- \[`flake8-boolean-trap`\] Allow booleans in `@override` methods ([#8882](https://github.com/astral-sh/ruff/pull/8882))
- \[`flake8-bugbear`\] Avoid `B015`,`B018` for last expression in a cell ([#8815](https://github.com/astral-sh/ruff/pull/8815))
- \[`flake8-pie`\] Allow ellipses for enum values in stub files ([#8825](https://github.com/astral-sh/ruff/pull/8825))
- \[`flake8-pyi`\] Check PEP 695 type aliases for `snake-case-type-alias` and `t-suffixed-type-alias` ([#8966](https://github.com/astral-sh/ruff/pull/8966))
- \[`flake8-pyi`\] Check for kwarg and vararg `NoReturn` type annotations ([#8948](https://github.com/astral-sh/ruff/pull/8948))
- \[`flake8-simplify`\] Omit select context managers from `SIM117` ([#8801](https://github.com/astral-sh/ruff/pull/8801))
- \[`pep8-naming`\] Allow Django model loads in `non-lowercase-variable-in-function` (`N806`) ([#8917](https://github.com/astral-sh/ruff/pull/8917))
- \[`pycodestyle`\] Avoid `E703` for last expression in a cell ([#8821](https://github.com/astral-sh/ruff/pull/8821))
- \[`pycodestyle`\] Update `E402` to work at cell level for notebooks ([#8872](https://github.com/astral-sh/ruff/pull/8872))
- \[`pydocstyle`\] Avoid `D100` for Jupyter Notebooks ([#8816](https://github.com/astral-sh/ruff/pull/8816))
- \[`pylint`\] Implement fix for `unspecified-encoding` (`PLW1514`) ([#8928](https://github.com/astral-sh/ruff/pull/8928))

### Formatter

- Avoid unstable formatting in ellipsis-only body with trailing comment ([#8984](https://github.com/astral-sh/ruff/pull/8984))
- Inline trailing comments for type alias similar to assignments ([#8941](https://github.com/astral-sh/ruff/pull/8941))
- Insert trailing comma when function breaks with single argument ([#8921](https://github.com/astral-sh/ruff/pull/8921))

### CLI

- Update `ruff check` and `ruff format` to default to the current directory ([#8791](https://github.com/astral-sh/ruff/pull/8791))
- Stop at the first resolved parent configuration ([#8864](https://github.com/astral-sh/ruff/pull/8864))

### Configuration

- \[`pylint`\] Default `max-positional-args` to `max-args` ([#8998](https://github.com/astral-sh/ruff/pull/8998))
- \[`pylint`\] Add `allow-dunder-method-names` setting for `bad-dunder-method-name` (`PLW3201`) ([#8812](https://github.com/astral-sh/ruff/pull/8812))
- \[`isort`\] Add support for `from-first` setting ([#8663](https://github.com/astral-sh/ruff/pull/8663))
- \[`isort`\]  Add support for `length-sort` settings ([#8841](https://github.com/astral-sh/ruff/pull/8841))

### Bug fixes

- Add support for `@functools.singledispatch` ([#8934](https://github.com/astral-sh/ruff/pull/8934))
- Avoid off-by-one error in stripping noqa following multi-byte char ([#8979](https://github.com/astral-sh/ruff/pull/8979))
- Avoid off-by-one error in with-item named expressions ([#8915](https://github.com/astral-sh/ruff/pull/8915))
- Avoid syntax error via invalid ur string prefix ([#8971](https://github.com/astral-sh/ruff/pull/8971))
- Avoid underflow in `get_model` matching ([#8965](https://github.com/astral-sh/ruff/pull/8965))
- Avoid unnecessary index diagnostics when value is modified ([#8970](https://github.com/astral-sh/ruff/pull/8970))
- Convert over-indentation rule to use number of characters ([#8983](https://github.com/astral-sh/ruff/pull/8983))
- Detect implicit returns in auto-return-types ([#8952](https://github.com/astral-sh/ruff/pull/8952))
- Fix start >= end error in over-indentation ([#8982](https://github.com/astral-sh/ruff/pull/8982))
- Ignore `@overload` and `@override` methods for too-many-arguments checks ([#8954](https://github.com/astral-sh/ruff/pull/8954))
- Lexer start of line is false only for `Mode::Expression` ([#8880](https://github.com/astral-sh/ruff/pull/8880))
- Mark `pydantic_settings.BaseSettings` as having default copy semantics ([#8793](https://github.com/astral-sh/ruff/pull/8793))
- Respect dictionary unpacking in `NamedTuple` assignments ([#8810](https://github.com/astral-sh/ruff/pull/8810))
- Respect local subclasses in `flake8-type-checking` ([#8768](https://github.com/astral-sh/ruff/pull/8768))
- Support type alias statements in simple statement positions ([#8916](https://github.com/astral-sh/ruff/pull/8916))
- \[`flake8-annotations`\] Avoid filtering out un-representable types in return annotation ([#8881](https://github.com/astral-sh/ruff/pull/8881))
- \[`flake8-pie`\] Retain extra ellipses in protocols and abstract methods ([#8769](https://github.com/astral-sh/ruff/pull/8769))
- \[`flake8-pyi`\] Respect local enum subclasses in `simple-defaults` (`PYI052`) ([#8767](https://github.com/astral-sh/ruff/pull/8767))
- \[`flake8-trio`\] Use correct range for `TRIO115` fix ([#8933](https://github.com/astral-sh/ruff/pull/8933))
- \[`flake8-trio`\] Use full arguments range for zero-sleep-call ([#8936](https://github.com/astral-sh/ruff/pull/8936))
- \[`isort`\] fix: mark `__main__` as first-party import ([#8805](https://github.com/astral-sh/ruff/pull/8805))
- \[`pep8-naming`\] Avoid `N806` errors for type alias statements ([#8785](https://github.com/astral-sh/ruff/pull/8785))
- \[`perflint`\] Avoid `PERF101` if there's an append in loop body ([#8809](https://github.com/astral-sh/ruff/pull/8809))
- \[`pycodestyle`\] Allow space-before-colon after end-of-slice ([#8838](https://github.com/astral-sh/ruff/pull/8838))
- \[`pydocstyle`\] Avoid non-character breaks in `over-indentation` (`D208`) ([#8866](https://github.com/astral-sh/ruff/pull/8866))
- \[`pydocstyle`\] Ignore underlines when determining docstring logical lines ([#8929](https://github.com/astral-sh/ruff/pull/8929))
- \[`pylint`\] Extend `self-assigning-variable` to multi-target assignments ([#8839](https://github.com/astral-sh/ruff/pull/8839))
- \[`tryceratops`\] Avoid repeated triggers in nested `tryceratops` diagnostics ([#8772](https://github.com/astral-sh/ruff/pull/8772))

### Documentation

- Add advice for fixing RUF008 when mutability is not desired ([#8853](https://github.com/astral-sh/ruff/pull/8853))
- Added the command to run ruff using pkgx to the installation.md ([#8955](https://github.com/astral-sh/ruff/pull/8955))
- Document fix safety for flake8-comprehensions and some pyupgrade rules ([#8918](https://github.com/astral-sh/ruff/pull/8918))
- Fix doc formatting for zero-sleep-call ([#8937](https://github.com/astral-sh/ruff/pull/8937))
- Remove duplicate imports from os-stat documentation ([#8930](https://github.com/astral-sh/ruff/pull/8930))
- Replace generated reference to MkDocs ([#8806](https://github.com/astral-sh/ruff/pull/8806))
- Update Arch Linux package URL in installation.md ([#8802](https://github.com/astral-sh/ruff/pull/8802))
- \[`flake8-pyi`\] Fix error in `t-suffixed-type-alias` (`PYI043`) example ([#8963](https://github.com/astral-sh/ruff/pull/8963))
- \[`flake8-pyi`\] Improve motivation for `custom-type-var-return-type` (`PYI019`) ([#8766](https://github.com/astral-sh/ruff/pull/8766))

## 0.1.6

### Preview features

- \[`flake8-boolean-trap`\] Extend `boolean-type-hint-positional-argument` (`FBT001`) to include booleans in unions ([#7501](https://github.com/astral-sh/ruff/pull/7501))
- \[`flake8-pie`\] Extend `reimplemented-list-builtin` (`PIE807`) to `dict` reimplementations ([#8608](https://github.com/astral-sh/ruff/pull/8608))
- \[`flake8-pie`\] Extend `unnecessary-pass` (`PIE790`) to include ellipses (`...`) ([#8641](https://github.com/astral-sh/ruff/pull/8641))
- \[`flake8-pie`\] Implement fix for `unnecessary-spread` (`PIE800`) ([#8668](https://github.com/astral-sh/ruff/pull/8668))
- \[`flake8-quotes`\] Implement `unnecessary-escaped-quote` (`Q004`) ([#8630](https://github.com/astral-sh/ruff/pull/8630))
- \[`pycodestyle`\] Implement fix for `multiple-spaces-after-keyword` (`E271`) and `multiple-spaces-before-keyword` (`E272`) ([#8622](https://github.com/astral-sh/ruff/pull/8622))
- \[`pycodestyle`\] Implement fix for `multiple-spaces-after-operator` (`E222`) and `multiple-spaces-before-operator` (`E221`) ([#8623](https://github.com/astral-sh/ruff/pull/8623))
- \[`pyflakes`\] Extend `is-literal` (`F632`) to include comparisons against mutable initializers ([#8607](https://github.com/astral-sh/ruff/pull/8607))
- \[`pylint`\] Implement `redefined-argument-from-local` (`PLR1704`) ([#8159](https://github.com/astral-sh/ruff/pull/8159))
- \[`pylint`\] Implement fix for `unnecessary-lambda` (`PLW0108`) ([#8621](https://github.com/astral-sh/ruff/pull/8621))
- \[`refurb`\] Implement `if-expr-min-max` (`FURB136`) ([#8664](https://github.com/astral-sh/ruff/pull/8664))
- \[`refurb`\] Implement `math-constant` (`FURB152`) ([#8727](https://github.com/astral-sh/ruff/pull/8727))

### Rule changes

- \[`flake8-annotations`\] Add autotyping-like return type inference for annotation rules ([#8643](https://github.com/astral-sh/ruff/pull/8643))
- \[`flake8-future-annotations`\] Implement fix for `future-required-type-annotation` (`FA102`) ([#8711](https://github.com/astral-sh/ruff/pull/8711))
- \[`flake8-implicit-namespace-package`\] Avoid missing namespace violations in scripts with shebangs ([#8710](https://github.com/astral-sh/ruff/pull/8710))
- \[`pydocstyle`\] Update `over-indentation` (`D208`) to preserve indentation offsets when fixing overindented lines ([#8699](https://github.com/astral-sh/ruff/pull/8699))
- \[`pyupgrade`\] Refine `timeout-error-alias` (`UP041`) to remove false positives ([#8587](https://github.com/astral-sh/ruff/pull/8587))

### Formatter

- Fix instability in `await` formatting with fluent style ([#8676](https://github.com/astral-sh/ruff/pull/8676))
- Compare formatted and unformatted ASTs during formatter tests ([#8624](https://github.com/astral-sh/ruff/pull/8624))
- Preserve trailing semicolon for Notebooks ([#8590](https://github.com/astral-sh/ruff/pull/8590))

### CLI

- Improve debug printing for resolving origin of config settings ([#8729](https://github.com/astral-sh/ruff/pull/8729))
- Write unchanged, excluded files to stdout when read via stdin ([#8596](https://github.com/astral-sh/ruff/pull/8596))

### Configuration

- \[`isort`\] Support disabling sections with `no-sections = true` ([#8657](https://github.com/astral-sh/ruff/pull/8657))
- \[`pep8-naming`\] Support local and dynamic class- and static-method decorators ([#8592](https://github.com/astral-sh/ruff/pull/8592))
- \[`pydocstyle`\] Allow overriding pydocstyle convention rules ([#8586](https://github.com/astral-sh/ruff/pull/8586))

### Bug fixes

- Avoid syntax error via importing `trio.lowlevel` ([#8730](https://github.com/astral-sh/ruff/pull/8730))
- Omit unrolled augmented assignments in `PIE794` ([#8634](https://github.com/astral-sh/ruff/pull/8634))
- Slice source code instead of generating it for `EM` fixes ([#7746](https://github.com/astral-sh/ruff/pull/7746))
- Allow whitespace around colon in slices for `whitespace-before-punctuation` (`E203`) ([#8654](https://github.com/astral-sh/ruff/pull/8654))
- Use function range for `no-self-use` ([#8637](https://github.com/astral-sh/ruff/pull/8637))
- F-strings doesn't contain bytes literal for `PLW0129` ([#8675](https://github.com/astral-sh/ruff/pull/8675))
- Improve detection of `TYPE_CHECKING` blocks imported from `typing_extensions` or `_typeshed` ([#8429](https://github.com/astral-sh/ruff/pull/8429))
- Treat display as a builtin in IPython ([#8707](https://github.com/astral-sh/ruff/pull/8707))
- Avoid `FURB113` autofix if comments are present ([#8494](https://github.com/astral-sh/ruff/pull/8494))
- Consider the new f-string tokens for `flake8-commas` ([#8582](https://github.com/astral-sh/ruff/pull/8582))
- Remove erroneous bad-dunder-name reference ([#8742](https://github.com/astral-sh/ruff/pull/8742))
- Avoid recommending Self usages in metaclasses ([#8639](https://github.com/astral-sh/ruff/pull/8639))
- Detect runtime-evaluated base classes defined in the current file ([#8572](https://github.com/astral-sh/ruff/pull/8572))
- Avoid inserting trailing commas within f-strings ([#8574](https://github.com/astral-sh/ruff/pull/8574))
- Remove incorrect deprecation label for stdout and stderr ([#8743](https://github.com/astral-sh/ruff/pull/8743))
- Fix unnecessary parentheses in UP007 fix ([#8610](https://github.com/astral-sh/ruff/pull/8610))
- Remove repeated and erroneous scoped settings headers in docs ([#8670](https://github.com/astral-sh/ruff/pull/8670))
- Trim trailing empty strings when converting to f-strings ([#8712](https://github.com/astral-sh/ruff/pull/8712))
- Fix ordering for `force-sort-within-sections` ([#8665](https://github.com/astral-sh/ruff/pull/8665))
- Run unicode prefix rule over tokens ([#8709](https://github.com/astral-sh/ruff/pull/8709))
- Update UP032 to unescape curly braces in literal parts of converted strings ([#8697](https://github.com/astral-sh/ruff/pull/8697))
- List all ipython builtins ([#8719](https://github.com/astral-sh/ruff/pull/8719))

### Documentation

- Document conventions in the FAQ ([#8638](https://github.com/astral-sh/ruff/pull/8638))
- Redirect from rule codes to rule pages in docs ([#8636](https://github.com/astral-sh/ruff/pull/8636))
- Fix permalink to convention setting ([#8575](https://github.com/astral-sh/ruff/pull/8575))

## 0.1.5

### Preview features

- \[`flake8-bandit`\] Implement `mako-templates` (`S702`) ([#8533](https://github.com/astral-sh/ruff/pull/8533))
- \[`flake8-trio`\] Implement `TRIO105` ([#8490](https://github.com/astral-sh/ruff/pull/8490))
- \[`flake8-trio`\] Implement `TRIO109` ([#8534](https://github.com/astral-sh/ruff/pull/8534))
- \[`flake8-trio`\] Implement `TRIO110` ([#8537](https://github.com/astral-sh/ruff/pull/8537))
- \[`flake8-trio`\] Implement `TRIO115` ([#8486](https://github.com/astral-sh/ruff/pull/8486))
- \[`refurb`\] Implement `type-none-comparison` (`FURB169`) ([#8487](https://github.com/astral-sh/ruff/pull/8487))
- Flag all comparisons against builtin types in `E721` ([#8491](https://github.com/astral-sh/ruff/pull/8491))
- Make `SIM118` fix as safe when the expression is a known dictionary  ([#8525](https://github.com/astral-sh/ruff/pull/8525))

### Formatter

- Fix multiline lambda expression statement formatting ([#8466](https://github.com/astral-sh/ruff/pull/8466))

### CLI

- Add hidden `--extension` to override inference of source type from file extension ([#8373](https://github.com/astral-sh/ruff/pull/8373))

### Configuration

- Account for selector specificity when merging `extend_unsafe_fixes` and `override extend_safe_fixes` ([#8444](https://github.com/astral-sh/ruff/pull/8444))
- Add support for disabling cache with `RUFF_NO_CACHE` environment variable ([#8538](https://github.com/astral-sh/ruff/pull/8538))

### Bug fixes

- \[`E721`\] Flag comparisons to `memoryview` ([#8485](https://github.com/astral-sh/ruff/pull/8485))
- Allow collapsed-ellipsis bodies in other statements ([#8499](https://github.com/astral-sh/ruff/pull/8499))
- Avoid `D301` autofix for `u` prefixed strings ([#8495](https://github.com/astral-sh/ruff/pull/8495))
- Only flag `flake8-trio` rules when `trio` import is present ([#8550](https://github.com/astral-sh/ruff/pull/8550))
- Reject more syntactically invalid Python programs ([#8524](https://github.com/astral-sh/ruff/pull/8524))
- Avoid raising `TRIO115` violations for `trio.sleep(...)` calls with non-number values ([#8532](https://github.com/astral-sh/ruff/pull/8532))
- Fix `F841` false negative on assignment to multiple variables ([#8489](https://github.com/astral-sh/ruff/pull/8489))

### Documentation

- Fix link to isort `known-first-party` ([#8562](https://github.com/astral-sh/ruff/pull/8562))
- Add notes on fix safety to a few rules ([#8500](https://github.com/astral-sh/ruff/pull/8500))
- Add missing toml config tabs ([#8512](https://github.com/astral-sh/ruff/pull/8512))
- Add instructions for configuration of Emacs ([#8488](https://github.com/astral-sh/ruff/pull/8488))
- Improve detail link contrast in dark mode ([#8548](https://github.com/astral-sh/ruff/pull/8548))
- Fix typo in example ([#8506](https://github.com/astral-sh/ruff/pull/8506))
- Added tabs for configuration files in the documentation ([#8480](https://github.com/astral-sh/ruff/pull/8480))
- Recommend `project.requires-python` over `target-version` ([#8513](https://github.com/astral-sh/ruff/pull/8513))
- Add singleton escape hatch to `B008` documentation ([#8501](https://github.com/astral-sh/ruff/pull/8501))
- Fix tab configuration docs ([#8502](https://github.com/astral-sh/ruff/pull/8502))

## 0.1.4

### Preview features

- \[`flake8-trio`\] Implement `timeout-without-await` (`TRIO001`) ([#8439](https://github.com/astral-sh/ruff/pull/8439))
- \[`numpy`\] Implement NumPy 2.0 migration rule (`NPY200`) ([#7702](https://github.com/astral-sh/ruff/pull/7702))
- \[`pylint`\] Implement `bad-open-mode` (`W1501`) ([#8294](https://github.com/astral-sh/ruff/pull/8294))
- \[`pylint`\] Implement `import-outside-toplevel` (`C0415`) rule ([#5180](https://github.com/astral-sh/ruff/pull/5180))
- \[`pylint`\] Implement `useless-with-lock` (`W2101`) ([#8321](https://github.com/astral-sh/ruff/pull/8321))
- \[`pyupgrade`\] Implement `timeout-error-alias` (`UP041`) ([#8476](https://github.com/astral-sh/ruff/pull/8476))
- \[`refurb`\] Implement `isinstance-type-none` (`FURB168`) ([#8308](https://github.com/astral-sh/ruff/pull/8308))
- Detect confusable Unicode-to-Unicode units in `RUF001`, `RUF002`, and `RUF003` ([#4430](https://github.com/astral-sh/ruff/pull/4430))
- Add newline after module docstrings in preview style ([#8283](https://github.com/astral-sh/ruff/pull/8283))

### Formatter

- Add a note on line-too-long to the formatter docs ([#8314](https://github.com/astral-sh/ruff/pull/8314))
- Preserve trailing statement semicolons when using `fmt: skip` ([#8273](https://github.com/astral-sh/ruff/pull/8273))
- Preserve trailing semicolons when using `fmt: off` ([#8275](https://github.com/astral-sh/ruff/pull/8275))
- Avoid duplicating linter-formatter compatibility warnings ([#8292](https://github.com/astral-sh/ruff/pull/8292))
- Avoid inserting a newline after function docstrings ([#8375](https://github.com/astral-sh/ruff/pull/8375))
- Insert newline between docstring and following own line comment ([#8216](https://github.com/astral-sh/ruff/pull/8216))
- Split tuples in return positions by comma first ([#8280](https://github.com/astral-sh/ruff/pull/8280))
- Avoid treating byte strings as docstrings ([#8350](https://github.com/astral-sh/ruff/pull/8350))
- Add `--line-length` option to `format` command ([#8363](https://github.com/astral-sh/ruff/pull/8363))
- Avoid parenthesizing unsplittable because of comments ([#8431](https://github.com/astral-sh/ruff/pull/8431))

### CLI

- Add `--output-format` to `ruff rule` and `ruff linter` ([#8203](https://github.com/astral-sh/ruff/pull/8203))

### Bug fixes

- Respect `--force-exclude` in `lint.exclude` and `format.exclude` ([#8393](https://github.com/astral-sh/ruff/pull/8393))
- Respect `--extend-per-file-ignores` on the CLI ([#8329](https://github.com/astral-sh/ruff/pull/8329))
- Extend `bad-dunder-method-name` to permit `__index__` ([#8300](https://github.com/astral-sh/ruff/pull/8300))
- Fix panic with 8 in octal escape ([#8356](https://github.com/astral-sh/ruff/pull/8356))
- Avoid raising `D300` when both triple quote styles are present ([#8462](https://github.com/astral-sh/ruff/pull/8462))
- Consider unterminated f-strings in `FStringRanges` ([#8154](https://github.com/astral-sh/ruff/pull/8154))
- Avoid including literal `shell=True` for truthy, non-`True` diagnostics ([#8359](https://github.com/astral-sh/ruff/pull/8359))
- Avoid triggering single-element test for starred expressions ([#8433](https://github.com/astral-sh/ruff/pull/8433))
- Detect and ignore Jupyter automagics ([#8398](https://github.com/astral-sh/ruff/pull/8398))
- Fix invalid E231 error with f-strings ([#8369](https://github.com/astral-sh/ruff/pull/8369))
- Avoid triggering `NamedTuple` rewrite with starred annotation ([#8434](https://github.com/astral-sh/ruff/pull/8434))
- Avoid un-setting bracket flag in logical lines ([#8380](https://github.com/astral-sh/ruff/pull/8380))
- Place 'r' prefix before 'f' for raw format strings ([#8464](https://github.com/astral-sh/ruff/pull/8464))
- Remove trailing periods from NumPy 2.0 code actions ([#8475](https://github.com/astral-sh/ruff/pull/8475))
- Fix bug where `PLE1307` was raised when formatting `%c` with characters ([#8407](https://github.com/astral-sh/ruff/pull/8407))
- Remove unicode flag from comparable ([#8440](https://github.com/astral-sh/ruff/pull/8440))
- Improve B015 message ([#8295](https://github.com/astral-sh/ruff/pull/8295))
- Use `fixedOverflowWidgets` for playground popover ([#8458](https://github.com/astral-sh/ruff/pull/8458))
- Mark `byte_bounds` as a non-backwards-compatible NumPy 2.0 change ([#8474](https://github.com/astral-sh/ruff/pull/8474))

### Internals

- Add a dedicated cache directory per Ruff version ([#8333](https://github.com/astral-sh/ruff/pull/8333))
- Allow selective caching for `--fix` and `--diff` ([#8316](https://github.com/astral-sh/ruff/pull/8316))
- Improve performance of comment parsing ([#8193](https://github.com/astral-sh/ruff/pull/8193))
- Improve performance of string parsing ([#8227](https://github.com/astral-sh/ruff/pull/8227))
- Use a dedicated sort key for isort import sorting ([#7963](https://github.com/astral-sh/ruff/pull/7963))

## 0.1.3

This release includes a variety of improvements to the Ruff formatter, removing several known and
unintentional deviations from Black.

### Formatter

- Avoid space around pow for `None`, `True` and `False` ([#8189](https://github.com/astral-sh/ruff/pull/8189))
- Avoid sorting all paths in the format command ([#8181](https://github.com/astral-sh/ruff/pull/8181))
- Insert necessary blank line between class and leading comments ([#8224](https://github.com/astral-sh/ruff/pull/8224))
- Avoid introducing new parentheses in annotated assignments ([#8233](https://github.com/astral-sh/ruff/pull/8233))
- Refine the warnings about incompatible linter options ([#8196](https://github.com/astral-sh/ruff/pull/8196))
- Add test and basic implementation for formatter preview mode ([#8044](https://github.com/astral-sh/ruff/pull/8044))
- Refine warning about incompatible `isort` settings ([#8192](https://github.com/astral-sh/ruff/pull/8192))
- Only omit optional parentheses for starting or ending with parentheses ([#8238](https://github.com/astral-sh/ruff/pull/8238))
- Use source type to determine parser mode for formatting ([#8205](https://github.com/astral-sh/ruff/pull/8205))
- Don't warn about magic trailing comma when `isort.force-single-line` is true ([#8244](https://github.com/astral-sh/ruff/pull/8244))
- Use `SourceKind::diff` for formatter ([#8240](https://github.com/astral-sh/ruff/pull/8240))
- Fix `fmt:off` with trailing child comment ([#8234](https://github.com/astral-sh/ruff/pull/8234))
- Formatter parentheses support for `IpyEscapeCommand` ([#8207](https://github.com/astral-sh/ruff/pull/8207))

### Linter

- \[`pylint`\] Add buffer methods to `bad-dunder-method-name` (`PLW3201`) exclusions ([#8190](https://github.com/astral-sh/ruff/pull/8190))
- Match rule prefixes from `external` codes setting in `unused-noqa` ([#8177](https://github.com/astral-sh/ruff/pull/8177))
- Use `line-length` setting for isort in lieu of `pycodestyle.max-line-length` ([#8235](https://github.com/astral-sh/ruff/pull/8235))
- Update fix for `unnecessary-paren-on-raise-exception` to unsafe for unknown types ([#8231](https://github.com/astral-sh/ruff/pull/8231))
- Correct quick fix message for `W605` ([#8255](https://github.com/astral-sh/ruff/pull/8255))

### Documentation

- Fix typo in max-doc-length documentation ([#8201](https://github.com/astral-sh/ruff/pull/8201))
- Improve documentation around linter-formatter conflicts ([#8257](https://github.com/astral-sh/ruff/pull/8257))
- Fix link to error suppression documentation in `unused-noqa` ([#8172](https://github.com/astral-sh/ruff/pull/8172))
- Add `external` option to `unused-noqa` documentation ([#8171](https://github.com/astral-sh/ruff/pull/8171))
- Add title attribute to icons ([#8060](https://github.com/astral-sh/ruff/pull/8060))
- Clarify unsafe case in RSE102 ([#8256](https://github.com/astral-sh/ruff/pull/8256))
- Fix skipping formatting examples ([#8210](https://github.com/astral-sh/ruff/pull/8210))
- docs: fix name of `magic-trailing-comma` option in README ([#8200](https://github.com/astral-sh/ruff/pull/8200))
- Add note about scope of rule changing in versioning policy ([#8169](https://github.com/astral-sh/ruff/pull/8169))
- Document: Fix default lint rules ([#8218](https://github.com/astral-sh/ruff/pull/8218))
- Fix a wrong setting in configuration.md ([#8186](https://github.com/astral-sh/ruff/pull/8186))
- Fix misspelled TOML headers in the tutorial ([#8209](https://github.com/astral-sh/ruff/pull/8209))

## 0.1.2

This release includes the Beta version of the Ruff formatter — an extremely fast, Black-compatible Python formatter.
Try it today with `ruff format`! [Check out the blog post](https://astral.sh/blog/the-ruff-formatter) and [read the docs](https://docs.astral.sh/ruff/formatter/).

### Preview features

- \[`pylint`\] Implement `non-ascii-module-import` (`C2403`) ([#8056](https://github.com/astral-sh/ruff/pull/8056))
- \[`pylint`\] implement `non-ascii-name` (`C2401`) ([#8038](https://github.com/astral-sh/ruff/pull/8038))
- \[`pylint`\] Implement unnecessary-lambda (W0108) ([#7953](https://github.com/astral-sh/ruff/pull/7953))
- \[`refurb`\] Implement `read-whole-file` (`FURB101`) ([#7682](https://github.com/astral-sh/ruff/pull/7682))
- Add fix for `E223`, `E224`, and `E242` ([#8143](https://github.com/astral-sh/ruff/pull/8143))
- Add fix for `E225`, `E226`, `E227`, and `E228` ([#8136](https://github.com/astral-sh/ruff/pull/8136))
- Add fix for `E252` ([#8142](https://github.com/astral-sh/ruff/pull/8142))
- Add fix for `E261` ([#8114](https://github.com/astral-sh/ruff/pull/8114))
- Add fix for `E273` and `E274` ([#8144](https://github.com/astral-sh/ruff/pull/8144))
- Add fix for `E275` ([#8133](https://github.com/astral-sh/ruff/pull/8133))
- Update `SIM401` to catch ternary operations ([#7415](https://github.com/astral-sh/ruff/pull/7415))
- Update `E721` to allow `is` and `is` not for direct type comparisons ([#7905](https://github.com/astral-sh/ruff/pull/7905))

### Rule changes

- Add `backports.strenum` to `deprecated-imports` ([#8113](https://github.com/astral-sh/ruff/pull/8113))
- Update `SIM112` to ignore `https_proxy`, `http_proxy`, and `no_proxy` ([#8140](https://github.com/astral-sh/ruff/pull/8140))
- Update fix for `literal-membership` (`PLR6201`) to be unsafe ([#8097](https://github.com/astral-sh/ruff/pull/8097))
- Update fix for  `mutable-argument-defaults` (`B006`) to be unsafe ([#8108](https://github.com/astral-sh/ruff/pull/8108))

### Formatter

- Change `line-ending` default to `auto` ([#8057](https://github.com/astral-sh/ruff/pull/8057))
- Respect parenthesized generators in `has_own_parentheses` ([#8100](https://github.com/astral-sh/ruff/pull/8100))
- Add caching to formatter ([#8089](https://github.com/astral-sh/ruff/pull/8089))
- Remove `--line-length` option from `format` command ([#8131](https://github.com/astral-sh/ruff/pull/8131))
- Add formatter to `line-length` documentation ([#8150](https://github.com/astral-sh/ruff/pull/8150))
- Warn about incompatible formatter options ([#8088](https://github.com/astral-sh/ruff/pull/8088))
- Fix range of unparenthesized tuple subject in match statement ([#8101](https://github.com/astral-sh/ruff/pull/8101))
- Remove experimental formatter warning ([#8148](https://github.com/astral-sh/ruff/pull/8148))
- Don't move type param opening parenthesis comment ([#8163](https://github.com/astral-sh/ruff/pull/8163))
- Update versions in format benchmark script ([#8110](https://github.com/astral-sh/ruff/pull/8110))
- Avoid loading files for cached format results ([#8134](https://github.com/astral-sh/ruff/pull/8134))

### CLI

- Show the `ruff format` command in help menus ([#8167](https://github.com/astral-sh/ruff/pull/8167))
- Add `ruff version` command with long version display ([#8034](https://github.com/astral-sh/ruff/pull/8034))

### Configuration

- New `pycodestyle.max-line-length` option ([#8039](https://github.com/astral-sh/ruff/pull/8039))

### Bug fixes

- Detect `sys.version_info` slices in `outdated-version-block` ([#8112](https://github.com/astral-sh/ruff/pull/8112))
- Avoid if-else simplification for `TYPE_CHECKING` blocks ([#8072](https://github.com/astral-sh/ruff/pull/8072))
- Avoid false-positive print separator diagnostic with starred argument ([#8079](https://github.com/astral-sh/ruff/pull/8079))

### Documentation

- Fix message for `too-many-arguments` lint ([#8092](https://github.com/astral-sh/ruff/pull/8092))
- Fix `extend-unsafe-fixes` and `extend-safe-fixes` example ([#8139](https://github.com/astral-sh/ruff/pull/8139))
- Add links to `flake8-import-conventions` options ([#8115](https://github.com/astral-sh/ruff/pull/8115))
- Rework the documentation to incorporate the Ruff formatter ([#7732](https://github.com/astral-sh/ruff/pull/7732))
- Fix `Options` JSON schema description ([#8081](https://github.com/astral-sh/ruff/pull/8081))
- Fix typo (`pytext` -> `pytest`) ([#8117](https://github.com/astral-sh/ruff/pull/8117))
- Improve `magic-value-comparison` example in docs ([#8111](https://github.com/astral-sh/ruff/pull/8111))

## 0.1.1

### Rule changes

- Add unsafe fix for `escape-sequence-in-docstring` (`D301`) ([#7970](https://github.com/astral-sh/ruff/pull/7970))

### Configuration

- Respect `#(deprecated)` attribute in configuration options ([#8035](https://github.com/astral-sh/ruff/pull/8035))
- Add `[format|lint].exclude` options ([#8000](https://github.com/astral-sh/ruff/pull/8000))
- Respect `tab-size` setting in formatter ([#8006](https://github.com/astral-sh/ruff/pull/8006))
- Add `lint.preview` ([#8002](https://github.com/astral-sh/ruff/pull/8002))

### Preview features

- \[`pylint`\] Implement `literal-membership` (`PLR6201`) ([#7973](https://github.com/astral-sh/ruff/pull/7973))
- \[`pylint`\] Implement `too-many-boolean-expressions` (`PLR0916`) ([#7975](https://github.com/astral-sh/ruff/pull/7975))
- \[`pylint`\] Implement `misplaced-bare-raise` (`E0704`) ([#7961](https://github.com/astral-sh/ruff/pull/7961))
- \[`pylint`\] Implement `global-at-module-level` (`W0604`) ([#8058](https://github.com/astral-sh/ruff/pull/8058))
- \[`pylint`\] Implement `unspecified-encoding` (`PLW1514`) ([#7939](https://github.com/astral-sh/ruff/pull/7939))
- Add fix for `triple-single-quotes` (`D300`) ([#7967](https://github.com/astral-sh/ruff/pull/7967))

### Formatter

- New code style badge for `ruff format` ([#7878](https://github.com/astral-sh/ruff/pull/7878))
- Fix comments outside expression parentheses ([#7873](https://github.com/astral-sh/ruff/pull/7873))
- Add `--target-version` to `ruff format` ([#8055](https://github.com/astral-sh/ruff/pull/8055))
- Skip over parentheses when detecting `in` keyword ([#8054](https://github.com/astral-sh/ruff/pull/8054))
- Add `--diff` option to `ruff format` ([#7937](https://github.com/astral-sh/ruff/pull/7937))
- Insert newline after nested function or class statements ([#7946](https://github.com/astral-sh/ruff/pull/7946))
- Use `pass` over ellipsis in non-function/class contexts ([#8049](https://github.com/astral-sh/ruff/pull/8049))

### Bug fixes

- Lazily evaluate all PEP 695 type alias values ([#8033](https://github.com/astral-sh/ruff/pull/8033))
- Avoid failed assertion when showing fixes from stdin ([#8029](https://github.com/astral-sh/ruff/pull/8029))
- Avoid flagging HTTP and HTTPS literals in urllib-open ([#8046](https://github.com/astral-sh/ruff/pull/8046))
- Avoid flagging `bad-dunder-method-name` for `_` ([#8015](https://github.com/astral-sh/ruff/pull/8015))
- Remove Python 2-only methods from `URLOpen` audit ([#8047](https://github.com/astral-sh/ruff/pull/8047))
- Use set bracket replacement for `iteration-over-set` to preserve whitespace and comments ([#8001](https://github.com/astral-sh/ruff/pull/8001))

### Documentation

- Update tutorial to match revised Ruff defaults ([#8066](https://github.com/astral-sh/ruff/pull/8066))
- Update rule `B005` docs ([#8028](https://github.com/astral-sh/ruff/pull/8028))
- Update GitHub actions example in docs to use `--output-format` ([#8014](https://github.com/astral-sh/ruff/pull/8014))
- Document `lint.preview` and `format.preview` ([#8032](https://github.com/astral-sh/ruff/pull/8032))
- Clarify that new rules should be added to `RuleGroup::Preview`. ([#7989](https://github.com/astral-sh/ruff/pull/7989))

## 0.1.0

This is the first release which uses the `CHANGELOG` file. See [GitHub Releases](https://github.com/astral-sh/ruff/releases) for prior changelog entries.

Read Ruff's new [versioning policy](https://docs.astral.sh/ruff/versioning/).

### Breaking changes

- Unsafe fixes are no longer displayed or applied without opt-in ([#7769](https://github.com/astral-sh/ruff/pull/7769))
- Drop formatting specific rules from the default set ([#7900](https://github.com/astral-sh/ruff/pull/7900))
- The deprecated `format` setting has been removed ([#7984](https://github.com/astral-sh/ruff/pull/7984))
    - The `format` setting cannot be used to configure the output format, use `output-format` instead
    - The `RUFF_FORMAT` environment variable is ignored, use `RUFF_OUTPUT_FORMAT` instead
    - The `--format` option has been removed from `ruff check`, use `--output-format` instead

### Rule changes

- Extend `reimplemented-starmap` (`FURB140`) to catch calls with a single and starred argument ([#7768](https://github.com/astral-sh/ruff/pull/7768))
- Improve cases covered by `RUF015` ([#7848](https://github.com/astral-sh/ruff/pull/7848))
- Update `SIM15` to allow `open` followed by `close` ([#7916](https://github.com/astral-sh/ruff/pull/7916))
- Respect `msgspec.Struct` default-copy semantics in `RUF012` ([#7786](https://github.com/astral-sh/ruff/pull/7786))
- Add `sqlalchemy` methods to \`flake8-boolean-trap\`\` exclusion list ([#7874](https://github.com/astral-sh/ruff/pull/7874))
- Add fix for `PLR1714` ([#7910](https://github.com/astral-sh/ruff/pull/7910))
- Add fix for `PIE804` ([#7884](https://github.com/astral-sh/ruff/pull/7884))
- Add fix for `PLC0208` ([#7887](https://github.com/astral-sh/ruff/pull/7887))
- Add fix for `PYI055` ([#7886](https://github.com/astral-sh/ruff/pull/7886))
- Update `non-pep695-type-alias` to require `--unsafe-fixes` outside of stub files ([#7836](https://github.com/astral-sh/ruff/pull/7836))
- Improve fix message for `UP018` ([#7913](https://github.com/astral-sh/ruff/pull/7913))
- Update `PLW3201` to support `Enum` [sunder names](https://docs.python.org/3/library/enum.html#supported-sunder-names) ([#7987](https://github.com/astral-sh/ruff/pull/7987))

### Preview features

- Only show warnings for empty preview selectors when enabling rules ([#7842](https://github.com/astral-sh/ruff/pull/7842))
- Add `unnecessary-key-check` to simplify `key in dct and dct[key]` to `dct.get(key)` ([#7895](https://github.com/astral-sh/ruff/pull/7895))
- Add `assignment-in-assert` to prevent walrus expressions in assert statements ([#7856](https://github.com/astral-sh/ruff/pull/7856))
- \[`refurb`\] Add `single-item-membership-test` (`FURB171`) ([#7815](https://github.com/astral-sh/ruff/pull/7815))
- \[`pylint`\] Add `and-or-ternary` (`R1706`) ([#7811](https://github.com/astral-sh/ruff/pull/7811))

*New rules are added in [preview](https://docs.astral.sh/ruff/preview/).*

### Configuration

- Add `unsafe-fixes` setting ([#7769](https://github.com/astral-sh/ruff/pull/7769))
- Add `extend-safe-fixes` and `extend-unsafe-fixes` for promoting and demoting fixes ([#7841](https://github.com/astral-sh/ruff/pull/7841))

### CLI

- Added `--unsafe-fixes` option for opt-in to display and apply unsafe fixes ([#7769](https://github.com/astral-sh/ruff/pull/7769))
- Fix use of deprecated `--format` option in warning ([#7837](https://github.com/astral-sh/ruff/pull/7837))
- Show changed files when running under `--check` ([#7788](https://github.com/astral-sh/ruff/pull/7788))
- Write summary messages to stderr when fixing via stdin instead of omitting them ([#7838](https://github.com/astral-sh/ruff/pull/7838))
- Update fix summary message in `check --diff` to include unsafe fix hints ([#7790](https://github.com/astral-sh/ruff/pull/7790))
- Add notebook `cell` field to JSON output format ([#7664](https://github.com/astral-sh/ruff/pull/7664))
- Rename applicability levels to `Safe`, `Unsafe`, and `Display` ([#7843](https://github.com/astral-sh/ruff/pull/7843))

### Bug fixes

- Fix bug where f-strings were allowed in match pattern literal ([#7857](https://github.com/astral-sh/ruff/pull/7857))
- Fix `SIM110` with a yield in the condition ([#7801](https://github.com/astral-sh/ruff/pull/7801))
- Preserve trailing comments in `C414` fixes ([#7775](https://github.com/astral-sh/ruff/pull/7775))
- Check sequence type before triggering `unnecessary-enumerate` `len` suggestion ([#7781](https://github.com/astral-sh/ruff/pull/7781))
- Use correct start location for class/function clause header ([#7802](https://github.com/astral-sh/ruff/pull/7802))
- Fix incorrect fixes for `SIM101` ([#7798](https://github.com/astral-sh/ruff/pull/7798))
- Format comment before parameter default correctly ([#7870](https://github.com/astral-sh/ruff/pull/7870))
- Fix `E251` false positive inside f-strings ([#7894](https://github.com/astral-sh/ruff/pull/7894))
- Allow bindings to be created and referenced within annotations ([#7885](https://github.com/astral-sh/ruff/pull/7885))
- Show per-cell diffs when analyzing notebooks over `stdin` ([#7789](https://github.com/astral-sh/ruff/pull/7789))
- Avoid curly brace escape in f-string format spec ([#7780](https://github.com/astral-sh/ruff/pull/7780))
- Fix lexing single-quoted f-string with multi-line format spec ([#7787](https://github.com/astral-sh/ruff/pull/7787))
- Consider nursery rules to be in-preview for `ruff rule` ([#7812](https://github.com/astral-sh/ruff/pull/7812))
- Report precise location for invalid conversion flag ([#7809](https://github.com/astral-sh/ruff/pull/7809))
- Visit pattern match guard as a boolean test ([#7911](https://github.com/astral-sh/ruff/pull/7911))
- Respect `--unfixable` in `ISC` rules ([#7917](https://github.com/astral-sh/ruff/pull/7917))
- Fix edge case with `PIE804` ([#7922](https://github.com/astral-sh/ruff/pull/7922))
- Show custom message in `PTH118` for `Path.joinpath` with starred arguments ([#7852](https://github.com/astral-sh/ruff/pull/7852))
- Fix false negative in `outdated-version-block` when using greater than comparisons ([#7920](https://github.com/astral-sh/ruff/pull/7920))
- Avoid converting f-strings within Django `gettext` calls ([#7898](https://github.com/astral-sh/ruff/pull/7898))
- Fix false positive in `PLR6301` ([#7933](https://github.com/astral-sh/ruff/pull/7933))
- Treat type aliases as typing-only expressions e.g. resolves false positive in `TCH004` ([#7968](https://github.com/astral-sh/ruff/pull/7968))
- Resolve `cache-dir` relative to project root ([#7962](https://github.com/astral-sh/ruff/pull/7962))
- Respect subscripted base classes in type-checking rules e.g. resolves false positive in `TCH003` ([#7954](https://github.com/astral-sh/ruff/pull/7954))
- Fix JSON schema limit for `line-length` ([#7883](https://github.com/astral-sh/ruff/pull/7883))
- Fix commented-out `coalesce` keyword ([#7876](https://github.com/astral-sh/ruff/pull/7876))

### Documentation

- Document `reimplemented-starmap` performance effects ([#7846](https://github.com/astral-sh/ruff/pull/7846))
- Default to following the system dark/light mode ([#7888](https://github.com/astral-sh/ruff/pull/7888))
- Add documentation for fixes ([#7901](https://github.com/astral-sh/ruff/pull/7901))
- Fix typo in docs of `PLR6301` ([#7831](https://github.com/astral-sh/ruff/pull/7831))
- Update `UP038` docs to note that it results in slower code ([#7872](https://github.com/astral-sh/ruff/pull/7872))
- crlf -> cr-lf ([#7766](https://github.com/astral-sh/ruff/pull/7766))
- Add an example of an unsafe fix ([#7924](https://github.com/astral-sh/ruff/pull/7924))
- Fix documented examples for `unnecessary-subscript-reversal` ([#7774](https://github.com/astral-sh/ruff/pull/7774))
- Correct error in tuple example in ruff formatter docs ([#7822](https://github.com/astral-sh/ruff/pull/7822))
- Add versioning policy to documentation ([#7923](https://github.com/astral-sh/ruff/pull/7923))
- Fix invalid code in `FURB177` example ([#7832](https://github.com/astral-sh/ruff/pull/7832))

### Formatter

- Less scary `ruff format` message ([#7867](https://github.com/astral-sh/ruff/pull/7867))
- Remove spaces from import statements ([#7859](https://github.com/astral-sh/ruff/pull/7859))
- Formatter quoting for f-strings with triple quotes ([#7826](https://github.com/astral-sh/ruff/pull/7826))
- Update `ruff_python_formatter` generate.py comment ([#7850](https://github.com/astral-sh/ruff/pull/7850))
- Document one-call chaining deviation ([#7767](https://github.com/astral-sh/ruff/pull/7767))
- Allow f-string modifications in line-shrinking cases ([#7818](https://github.com/astral-sh/ruff/pull/7818))
- Add trailing comment deviation to README ([#7827](https://github.com/astral-sh/ruff/pull/7827))
- Add trailing zero between dot and exponential ([#7956](https://github.com/astral-sh/ruff/pull/7956))
- Force parentheses for power operations in unary expressions ([#7955](https://github.com/astral-sh/ruff/pull/7955))

### Playground

- Fix playground `Quick Fix` action ([#7824](https://github.com/astral-sh/ruff/pull/7824))
