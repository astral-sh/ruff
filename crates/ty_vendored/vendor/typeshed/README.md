# typeshed

[![Tests](https://github.com/python/typeshed/actions/workflows/tests.yml/badge.svg)](https://github.com/python/typeshed/actions/workflows/tests.yml)
[![Pull Requests Welcome](https://img.shields.io/badge/pull%20requests-welcome-brightgreen.svg)](https://github.com/python/typeshed/blob/main/CONTRIBUTING.md)

## About

Typeshed contains type annotations for the Python builtins and the Python
standard library, as well as type stubs for third-party packages. Type
annotations can be used for static analysis, type checking, type inference, and
autocompletion.

Typeshed supports Python versions 3.10 to 3.15.

## Using with Type Checkers

All major type checkers and language servers bundle the standard library stubs.
Type stubs for third-party packages can be installed from
[PyPI](https://pypi.org/). A typeshed stub package for a library `x` is usually
called `types-x`. For example, if you are using `html5lib`, you can install the
type stubs using

```shell
pip install types-html5lib
```

Type checkers will pick up the types from these stub packages when installed.
For more details, see the documentation for your type checker.

See the [stubs/ directory](stubs) for a list of all currently available
third-party stub packages.

**Note:** Not all packages on PyPI whose name starts with `types-` originate
from typeshed. Be careful when installing packages from unknown authors!

### Package Versioning for Third-Party Stubs

The version numbers of typeshed stub packages consist of the target version
number of the runtime package being stubbed, plus a release date. For example,
if the `types-foo` package has version `1.2.0.20240309`, it contains stubs
targeting `foo==1.2.0` and was released on March 9th, 2024.

At typeshed, we try to keep breaking changes to a minimum. However, any version
bump can introduce changes that might make your code fail to type check.

There are several strategies available for specifying the version of a stub
package you're using, each with its own tradeoffs:

1. Use the same bounds that you use for the package being stubbed. For example,
   if you use `foo>=2.30.0,<2.32`, you can use `types-foo>=2.30.0,<2.32`. This
   ensures that the stubs are compatible with the package you are using, but it
   carries a small risk of breaking type checking due to changes in the stubs.

   Another risk of this strategy is that stubs often lag behind the package
   that is being stubbed. You might need a minimum version of the package for a
   critical bug fix, but if updated stubs haven't been released yet, type
   checking results may not be accurate.

2. Pin the stubs to a known good version and update the pin from time to time
   (either manually, or using a tool such as
   [Dependabot](https://github.com/dependabot) or
   [Renovate](https://docs.renovatebot.com/)).

   For example, if you use `types-requests==2.31.0.1`, you can have confidence
   that upgrading dependencies will not break type checking. However, you will
   miss out on improvements in the stubs that could potentially improve type
   checking until you update the pin. You will also need to manually sync the
   version of the runtime and stub packages whenever you update them.

3. Don't pin the stubs. This is the easiest option, and you will automatically
   benefit from improved stubs whenever a new version is released. However, it
   carries the risk that the stubs become incompatible with the package being
   stubbed.

   For example, if a new major version of the package is released, there's a
   chance the stubs might be updated and reflect a version of the runtime
   package that is incompatible with the version you have installed.

You can also switch between the different strategies as needed. For example,
you could default to strategy (1), but fall back to strategy (2) when a stubs
update breaks type checking and you can't fix it quickly.

### Third-Party Stub Removal

Third-party stubs are removed from typeshed when the runtime package either
adds type information or becomes unsupported. The last released stub package
for a removed stub notes this in its description.

See [CONTRIBUTING.md](CONTRIBUTING.md#third-party-library-removal-policy) for
further information.

### The `_typeshed` Package

Typeshed's standard library stubs include a package `_typeshed` with utility
types useful for all type checker users. It's only available during type
checking, so imports from it must be guarded with `if TYPE_CHECKING:` at
runtime. For more information about how to use this package, see the
[documentation](https://github.com/python/typeshed/tree/main/stdlib/_typeshed/README.md).

## Contributing and Reporting Bugs

Information for contributors can be found in
[CONTRIBUTING.md](CONTRIBUTING.md). **Please read it before submitting pull
requests.**

If you think a stub is incorrect or incomplete, we want to hear from you!
Please report problems with our stubs using
[GitHub issues](https://github.com/python/typeshed/issues). The tracker is also
the right place to start discussions about other topics concerning the project.

**Please report issues with stubs to typeshed, not to the project being
stubbed.**

## Discussion

If you have general questions about typing with Python, or you need a review of
your type annotations or stubs outside of typeshed, head over to
[the "Typing" category](https://discuss.python.org/c/typing/32) of
[discuss.python.org](https://discuss.python.org/) or
[our discussion forum](https://github.com/python/typing/discussions) here on
GitHub.

## Further Documentation

See [docs/README.md](./docs/README.md) for an overview of all documentation
available for typeshed.

Further documentation on stub files, typeshed, and Python's typing system in
general, can also be found at [typing.python.org](https://typing.python.org/).
