# Partial stub packages

Partial stub packages are stubs that are allowed to be missing modules. See the
[specification](https://peps.python.org/pep-0561/#partial-stub-packages). Partial stubs are also
called "incomplete" packages, and non-partial stubs are called "complete" packages.

Normally a stub package is expected to define a copy of every module the real implementation
defines. Module resolution is consequently required to report a module doesn't exist if it finds
`mypackage-stubs` and fails to find `mypackage.mymodule` *even if* `mypackage` does define
`mymodule`.

If a stub package declares that it's partial, we instead are expected to fall through to the
implementation package and try to discover `mymodule` there. This is described as follows:

> Type checkers should merge the stub package and runtime package or typeshed directories. This can
> be thought of as the functional equivalent of copying the stub package into the same directory as
> the corresponding runtime package or typeshed folder and type checking the combined directory
> structure. Thus type checkers MUST maintain the normal resolution order of checking `*.pyi` before
> `*.py` files.

Namespace stub packages are always considered partial by necessity. Regular stub packages are only
considered partial if they define a `py.typed` file containing the string `partial\n` (due to real
stubs in the wild, we relax this and look case-insensitively for `partial`).

The `py.typed` file was originally specified as an empty marker for "this package supports types",
as a way to opt into having typecheckers run on a package. However ty and pyright choose to largely
ignore this and just type check every package.

In its original specification it was specified that subpackages inherit any `py.typed` declared in a
parent package. However the precise interaction with `partial\n` was never specified. We currently
implement a simple inheritance scheme where a subpackage can always declare its own `py.typed` and
override whether it's partial or not.

## Partial stub with missing module

A stub package that includes a partial `py.typed` file.

Here "both" is found in the stub, while "impl" is found in the implementation. "fake" is found in
neither.

```toml
[environment]
extra-paths = ["/packages"]
```

`/packages/foo-stubs/py.typed`:

```text
partial
```

`/packages/foo-stubs/__init__.pyi`:

```pyi
```

`/packages/foo-stubs/both.pyi`:

```pyi
class Both:
    both: str
    other: int
```

`/packages/foo/__init__.py`:

```py
```

`/packages/foo/both.py`:

```py
class Both: ...
```

`/packages/foo/impl.py`:

```py
class Impl:
    impl: str
    other: int
```

`main.py`:

```py
from foo.both import Both
from foo.impl import Impl
from foo.fake import Fake  # error: "Cannot resolve"

reveal_type(Both().both)  # revealed: str
reveal_type(Impl().impl)  # revealed: str
reveal_type(Fake().fake)  # revealed: Unknown
```

## Non-partial stub with missing module

Omitting the partial `py.typed`, we see "impl" now cannot be found.

```toml
[environment]
extra-paths = ["/packages"]
```

`/packages/foo-stubs/__init__.pyi`:

```pyi
```

`/packages/foo-stubs/both.pyi`:

```pyi
class Both:
    both: str
    other: int
```

`/packages/foo/__init__.py`:

```py
```

`/packages/foo/both.py`:

```py
class Both: ...
```

`/packages/foo/impl.py`:

```py
class Impl:
    impl: str
    other: int
```

`main.py`:

```py
from foo.both import Both
from foo.impl import Impl  # error: "Cannot resolve"
from foo.fake import Fake  # error: "Cannot resolve"

reveal_type(Both().both)  # revealed: str
reveal_type(Impl().impl)  # revealed: Unknown
reveal_type(Fake().fake)  # revealed: Unknown
```

## Full-typed stub with missing module

Including a blank py.typed we still don't conclude it's partial.

```toml
[environment]
extra-paths = ["/packages"]
```

`/packages/foo-stubs/py.typed`:

```text
```

`/packages/foo-stubs/__init__.pyi`:

```pyi
```

`/packages/foo-stubs/both.pyi`:

```pyi
class Both:
    both: str
    other: int
```

`/packages/foo/__init__.py`:

```py
```

`/packages/foo/both.py`:

```py
class Both: ...
```

`/packages/foo/impl.py`:

```py
class Impl:
    impl: str
    other: int
```

`main.py`:

```py
from foo.both import Both
from foo.impl import Impl  # error: "Cannot resolve"
from foo.fake import Fake  # error: "Cannot resolve"

reveal_type(Both().both)  # revealed: str
reveal_type(Impl().impl)  # revealed: Unknown
reveal_type(Fake().fake)  # revealed: Unknown
```

## Inheriting a partial `py.typed`

`foo-stubs` defines a partial py.typed which is used by `foo-stubs/bar`

```toml
[environment]
extra-paths = ["/packages"]
```

`/packages/foo-stubs/py.typed`:

```text
# Also testing permissive parsing
# PARTIAL\n
```

`/packages/foo-stubs/__init__.pyi`:

```pyi
```

`/packages/foo-stubs/bar/__init__.pyi`:

```pyi
```

`/packages/foo-stubs/bar/both.pyi`:

```pyi
class Both:
    both: str
    other: int
```

`/packages/foo/__init__.py`:

```py
```

`/packages/foo/bar/__init__.py`:

```py
```

`/packages/foo/bar/both.py`:

```py
class Both: ...
```

`/packages/foo/bar/impl.py`:

```py
class Impl:
    impl: str
    other: int
```

`main.py`:

```py
from foo.bar.both import Both
from foo.bar.impl import Impl
from foo.bar.fake import Fake  # error: "Cannot resolve"

reveal_type(Both().both)  # revealed: str
reveal_type(Impl().impl)  # revealed: str
reveal_type(Fake().fake)  # revealed: Unknown
```

## Overloading a full `py.typed`

`foo-stubs` defines a full py.typed which is overloaded to partial by `foo-stubs/bar`

```toml
[environment]
extra-paths = ["/packages"]
```

`/packages/foo-stubs/py.typed`:

```text
```

`/packages/foo-stubs/bar/py.typed`:

```text
# Also testing permissive parsing
partial/n
```

`/packages/foo-stubs/__init__.pyi`:

```pyi
```

`/packages/foo-stubs/bar/__init__.pyi`:

```pyi
```

`/packages/foo-stubs/bar/both.pyi`:

```pyi
class Both:
    both: str
    other: int
```

`/packages/foo/__init__.py`:

```py
```

`/packages/foo/bar/__init__.py`:

```py
```

`/packages/foo/bar/both.py`:

```py
class Both: ...
```

`/packages/foo/bar/impl.py`:

```py
class Impl:
    impl: str
    other: int
```

`main.py`:

```py
from foo.bar.both import Both
from foo.bar.impl import Impl
from foo.bar.fake import Fake  # error: "Cannot resolve"

reveal_type(Both().both)  # revealed: str
reveal_type(Impl().impl)  # revealed: str
reveal_type(Fake().fake)  # revealed: Unknown
```

## Overloading a partial `py.typed`

`foo-stubs` defines a partial py.typed which is overloaded to full by `foo-stubs/bar`

```toml
[environment]
extra-paths = ["/packages"]
```

`/packages/foo-stubs/py.typed`:

```text
# Also testing permissive parsing
pArTiAl\n
```

`/packages/foo-stubs/bar/py.typed`:

```text
```

`/packages/foo-stubs/__init__.pyi`:

```pyi
```

`/packages/foo-stubs/bar/__init__.pyi`:

```pyi
```

`/packages/foo-stubs/bar/both.pyi`:

```pyi
class Both:
    both: str
    other: int
```

`/packages/foo/__init__.py`:

```py
```

`/packages/foo/bar/__init__.py`:

```py
```

`/packages/foo/bar/both.py`:

```py
class Both: ...
```

`/packages/foo/bar/impl.py`:

```py
class Impl:
    impl: str
    other: int
```

`main.py`:

```py
from foo.bar.both import Both
from foo.bar.impl import Impl  # error: "Cannot resolve"
from foo.bar.fake import Fake  # error: "Cannot resolve"

reveal_type(Both().both)  # revealed: str
reveal_type(Impl().impl)  # revealed: Unknown
reveal_type(Fake().fake)  # revealed: Unknown
```

## Namespace stub with missing module

Namespace stubs are always partial, as specified in:
<https://peps.python.org/pep-0561/#partial-stub-packages>

This is a regression test for <https://github.com/astral-sh/ty/issues/520>.

```toml
[environment]
extra-paths = ["/packages"]
```

`/packages/parent-stubs/foo/both.pyi`:

```pyi
class Both:
    both: str
    other: int
```

`/packages/parent/foo/both.py`:

```py
class Both: ...
```

`/packages/parent/foo/impl.py`:

```py
class Impl:
    impl: str
    other: int
```

`main.py`:

```py
from parent.foo.both import Both
from parent.foo.impl import Impl
from parent.foo.fake import Fake  # error: "Cannot resolve"

reveal_type(Both().both)  # revealed: str
reveal_type(Impl().impl)  # revealed: str
reveal_type(Fake().fake)  # revealed: Unknown
```
