# Nonstandard Import Conventions

This document covers ty-specific extensions to the
[standard import conventions](https://typing.python.org/en/latest/spec/distributing.html#import-conventions).

It's a common idiom for a package's `__init__.py(i)` to include several imports like
`from . import mysubmodule`, with the intent that the `mypackage.mysubmodule` attribute should work
for anyone who only imports `mypackage`.

In the context of a `.py` we handle this well through our general attempts to faithfully implement
import side-effects. However for `.pyi` files we are expected to apply
[a more strict set of rules](https://typing.python.org/en/latest/spec/distributing.html#import-conventions)
to encourage intentional API design. Although `.pyi` files are explicitly designed to work with
typecheckers, which ostensibly should all enforce these strict rules, every typechecker has its own
defacto "extensions" to them and so a few idioms like `from . import mysubmodule` have found their
way into `.pyi` files too.

Thus for the sake of compatibility, we need to define our own "extensions". Any extensions we define
here have several competing concerns:

- Extensions should ideally be kept narrow to continue to encourage explicit API design
- Extensions should be easy to explain, document, and understand
- Extensions should ideally still be a subset of runtime behaviour (if it works in a stub, it works
    at runtime)
- Extensions should ideally not make `.pyi` files more permissive than `.py` files (if it works in a
    stub, it works in an impl)

To that end we define the following extension:

> If an `__init__.pyi` for `mypackage` contains a `from...import` targetting a direct submodule of
> `mypackage`, then that submodule should be available as an attribute of `mypackage`.

## Relative `from` Import of Direct Submodule in `__init__`

The `from . import submodule` idiom in an `__init__.pyi` is fairly explicit and we should definitely
support it.

`mypackage/__init__.pyi`:

```pyi
from . import imported
```

`mypackage/imported.pyi`:

```pyi
X: int = 42
```

`mypackage/fails.pyi`:

```pyi
Y: int = 47
```

`main.py`:

```py
import mypackage

reveal_type(mypackage.imported.X)  # revealed: int
# error: "has no member `fails`"
reveal_type(mypackage.fails.Y)  # revealed: Unknown
```

## Relative `from` Import of Direct Submodule in `__init__` (Non-Stub Check)

`mypackage/__init__.py`:

```py
from . import imported
```

`mypackage/imported.py`:

```py
X: int = 42
```

`mypackage/fails.py`:

```py
Y: int = 47
```

`main.py`:

```py
import mypackage

reveal_type(mypackage.imported.X)  # revealed: int
# error: "has no member `fails`"
reveal_type(mypackage.fails.Y)  # revealed: Unknown
```

## Absolute `from` Import of Direct Submodule in `__init__`

If an absolute `from...import` happens to import a submodule, it works just as well as a relative
one.

`mypackage/__init__.pyi`:

```pyi
from mypackage import imported
```

`mypackage/imported.pyi`:

```pyi
X: int = 42
```

`mypackage/fails.pyi`:

```pyi
Y: int = 47
```

`main.py`:

```py
import mypackage

reveal_type(mypackage.imported.X)  # revealed: int
# error: "has no member `fails`"
reveal_type(mypackage.fails.Y)  # revealed: Unknown
```

## Absolute `from` Import of Direct Submodule in `__init__` (Non-Stub Check)

`mypackage/__init__.py`:

```py
from mypackage import imported
```

`mypackage/imported.py`:

```py
X: int = 42
```

`mypackage/fails.py`:

```py
Y: int = 47
```

`main.py`:

```py
import mypackage

reveal_type(mypackage.imported.X)  # revealed: int
# error: "has no member `fails`"
reveal_type(mypackage.fails.Y)  # revealed: Unknown
```

## Import of Direct Submodule in `__init__`

An `import` that happens to import a submodule does not expose the submodule as an attribute. (This
is an arbitrary decision and can be changed easily!)

`mypackage/__init__.pyi`:

```pyi
import mypackage.imported
```

`mypackage/imported.pyi`:

```pyi
X: int = 42
```

`main.py`:

```py
import mypackage

# TODO: this is probably safe to allow, as it's an unambiguous import of a submodule
# error: "has no member `imported`"
reveal_type(mypackage.imported.X)  # revealed: Unknown
```

## Import of Direct Submodule in `__init__` (Non-Stub Check)

`mypackage/__init__.py`:

```py
import mypackage.imported
```

`mypackage/imported.py`:

```py
X: int = 42
```

`main.py`:

```py
import mypackage

# TODO: this is probably safe to allow, as it's an unambiguous import of a submodule
# error: "has no member `imported`"
reveal_type(mypackage.imported.X)  # revealed: Unknown
```

## Relative `from` Import of Nested Submodule in `__init__`

`from .submodule import nested` in an `__init__.pyi` is currently not supported as a way to expose
`mypackage.submodule` or `mypackage.submodule.nested` but it could be.

`mypackage/__init__.pyi`:

```pyi
from .submodule import nested
```

`mypackage/submodule/__init__.pyi`:

```pyi
```

`mypackage/submodule/nested.pyi`:

```pyi
X: int = 42
```

`main.py`:

```py
import mypackage

# TODO: this would be nice to allow
# error: "has no member `submodule`"
reveal_type(mypackage.submodule)  # revealed: Unknown
# error: "has no member `submodule`"
reveal_type(mypackage.submodule.nested)  # revealed: Unknown
# error: "has no member `submodule`"
reveal_type(mypackage.submodule.nested.X)  # revealed: Unknown
```

## Relative `from` Import of Nested Submodule in `__init__` (Non-Stub Check)

`mypackage/__init__.py`:

```py
from .submodule import nested
```

`mypackage/submodule/__init__.py`:

```py
```

`mypackage/submodule/nested.py`:

```py
X: int = 42
```

`main.py`:

```py
import mypackage

# TODO: this would be nice to support
# error: "has no member `submodule`"
reveal_type(mypackage.submodule)  # revealed: Unknown
# error: "has no member `submodule`"
reveal_type(mypackage.submodule.nested)  # revealed: Unknown
# error: "has no member `submodule`"
reveal_type(mypackage.submodule.nested.X)  # revealed: Unknown
```

## Absolute `from` Import of Nested Submodule in `__init__`

`from mypackage.submodule import nested` in an `__init__.pyi` is currently not supported as a way to
expose `mypackage.submodule` or `mypackage.submodule.nested` but it could be.

`mypackage/__init__.pyi`:

```pyi
from mypackage.submodule import nested
```

`mypackage/submodule/__init__.pyi`:

```pyi
```

`mypackage/submodule/nested.pyi`:

```pyi
X: int = 42
```

`main.py`:

```py
import mypackage

# TODO: this would be nice to support
# error: "has no member `submodule`"
reveal_type(mypackage.submodule)  # revealed: Unknown
# error: "has no member `submodule`"
reveal_type(mypackage.submodule.nested)  # revealed: Unknown
# error: "has no member `submodule`"
reveal_type(mypackage.submodule.nested.X)  # revealed: Unknown
```

## Absolute `from` Import of Nested Submodule in `__init__` (Non-Stub Check)

`mypackage/__init__.py`:

```py
from mypackage.submodule import nested
```

`mypackage/submodule/__init__.py`:

```py
```

`mypackage/submodule/nested.py`:

```py
X: int = 42
```

`main.py`:

```py
import mypackage

# TODO: this would be nice to support
# error: "has no member `submodule`"
reveal_type(mypackage.submodule)  # revealed: Unknown
# error: "has no member `submodule`"
reveal_type(mypackage.submodule.nested)  # revealed: Unknown
# error: "has no member `submodule`"
reveal_type(mypackage.submodule.nested.X)  # revealed: Unknown
```

## Import of Nested Submodule in `__init__`

`import mypackage.submodule.nested` in an `__init__.pyi` is currently not supported as a way to
expose `mypackage.submodule` or `mypackage.submodule.nested` but it could be.

`mypackage/__init__.pyi`:

```pyi
import mypackage.submodule.nested
```

`mypackage/submodule/__init__.pyi`:

```pyi
```

`mypackage/submodule/nested.pyi`:

```pyi
X: int = 42
```

`main.py`:

```py
import mypackage

# TODO: this would be nice to support, and is probably safe to do as it's unambiguous
# error: "has no member `submodule`"
reveal_type(mypackage.submodule)  # revealed: Unknown
# error: "has no member `submodule`"
reveal_type(mypackage.submodule.nested)  # revealed: Unknown
# error: "has no member `submodule`"
reveal_type(mypackage.submodule.nested.X)  # revealed: Unknown
```

## Import of Nested Submodule in `__init__` (Non-Stub Check)

`mypackage/__init__.py`:

```py
import mypackage.submodule.nested
```

`mypackage/submodule/__init__.py`:

```py
```

`mypackage/submodule/nested.py`:

```py
X: int = 42
```

`main.py`:

```py
import mypackage

# TODO: this would be nice to support, and is probably safe to do as it's unambiguous
# error: "has no member `submodule`"
reveal_type(mypackage.submodule)  # revealed: Unknown
# error: "has no member `submodule`"
reveal_type(mypackage.submodule.nested)  # revealed: Unknown
# error: "has no member `submodule`"
reveal_type(mypackage.submodule.nested.X)  # revealed: Unknown
```

## Relative `from` Import of Direct Submodule in `__init__`, Mismatched Alias

Renaming the submodule to something else disables the `__init__.pyi` idiom.

`mypackage/__init__.pyi`:

```pyi
from . import imported as imported_m
```

`mypackage/imported.pyi`:

```pyi
X: int = 42
```

`main.py`:

```py
import mypackage

# error: "has no member `imported`"
reveal_type(mypackage.imported.X)  # revealed: Unknown
# error: "has no member `imported_m`"
reveal_type(mypackage.imported_m.X)  # revealed: Unknown
```

## Relative `from` Import of Direct Submodule in `__init__`, Mismatched Alias (Non-Stub Check)

`mypackage/__init__.py`:

```py
from . import imported as imported_m
```

`mypackage/imported.py`:

```py
X: int = 42
```

`main.py`:

```py
import mypackage

# TODO: this would be nice to support, as it works at runtime
# error: "has no member `imported`"
reveal_type(mypackage.imported.X)  # revealed: Unknown
reveal_type(mypackage.imported_m.X)  # revealed: int
```

## Relative `from` Import of Direct Submodule in `__init__`, Matched Alias

The `__init__.pyi` idiom should definitely always work if the submodule is renamed to itself, as
this is the re-export idiom.

`mypackage/__init__.pyi`:

```pyi
from . import imported as imported
```

`mypackage/imported.pyi`:

```pyi
X: int = 42
```

`main.py`:

```py
import mypackage

reveal_type(mypackage.imported.X)  # revealed: int
```

## Relative `from` Import of Direct Submodule in `__init__`, Matched Alias (Non-Stub Check)

`mypackage/__init__.py`:

```py
from . import imported as imported
```

`mypackage/imported.py`:

```py
X: int = 42
```

`main.py`:

```py
import mypackage

reveal_type(mypackage.imported.X)  # revealed: int
```

## Star Import Unaffected

Even if the `__init__` idiom is in effect, star imports do not pick it up. (This is an arbitrary
decision that mostly fell out of the implementation details and can be changed!)

`mypackage/__init__.pyi`:

```pyi
from . import imported
Z: int = 17
```

`mypackage/imported.pyi`:

```pyi
X: int = 42
```

`main.py`:

```py
from mypackage import *

# TODO: this would be nice to support (available_submodule_attributes isn't visible to `*` imports)
# error: "`imported` used when not defined"
reveal_type(imported.X)  # revealed: Unknown
reveal_type(Z)  # revealed: int
```

## Star Import Unaffected (Non-Stub Check)

`mypackage/__init__.py`:

```py
from . import imported

Z: int = 17
```

`mypackage/imported.py`:

```py
X: int = 42
```

`main.py`:

```py
from mypackage import *

reveal_type(imported.X)  # revealed: int
reveal_type(Z)  # revealed: int
```

## `from` Import of Non-Submodule

A from import that terminates in a non-submodule should not expose the intermediate submodules as
attributes. This is an arbitrary decision but on balance probably safe and correct, as otherwise it
would be hard for a stub author to be intentional about the submodules being exposed as attributes.

`mypackage/__init__.pyi`:

```pyi
from .imported import X
```

`mypackage/imported.pyi`:

```pyi
X: int = 42
```

`main.py`:

```py
import mypackage

# error: "has no member `imported`"
reveal_type(mypackage.imported.X)  # revealed: Unknown
```

## `from` Import of Non-Submodule (Non-Stub Check)

`mypackage/__init__.py`:

```py
from .imported import X
```

`mypackage/imported.py`:

```py
X: int = 42
```

`main.py`:

```py
import mypackage

# TODO: this would be nice to support, as it works at runtime
# error: "has no member `imported`"
reveal_type(mypackage.imported.X)  # revealed: Unknown
```

## `from` Import of Other Package's Submodule

`from mypackage import submodule` from outside the package is not modeled as a side-effect on
`mypackage`, even in the importing file (this could be changed!).

`mypackage/__init__.pyi`:

```pyi
```

`mypackage/imported.pyi`:

```pyi
X: int = 42
```

`main.py`:

```py
import mypackage
from mypackage import imported

# TODO: this would be nice to support, but it's dangerous with available_submodule_attributes
reveal_type(imported.X)  # revealed: int
# error: "has no member `imported`"
reveal_type(mypackage.imported.X)  # revealed: Unknown
```

## `from` Import of Other Package's Submodule (Non-Stub Check)

`mypackage/__init__.py`:

```py
```

`mypackage/imported.py`:

```py
X: int = 42
```

`main.py`:

```py
import mypackage
from mypackage import imported

# TODO: this would be nice to support, as it works at runtime
reveal_type(imported.X)  # revealed: int
# error: "has no member `imported`"
reveal_type(mypackage.imported.X)  # revealed: Unknown
```

## `from` Import of Sibling Module

`from . import submodule` from a sibling module is not modeled as a side-effect on `mypackage` or a
re-export from `submodule`.

`mypackage/__init__.pyi`:

```pyi
```

`mypackage/imported.pyi`:

```pyi
from . import fails
X: int = 42
```

`mypackage/fails.pyi`:

```pyi
Y: int = 47
```

`main.py`:

```py
import mypackage
from mypackage import imported

reveal_type(imported.X)  # revealed: int
# error: "has no member `fails`"
reveal_type(imported.fails.Y)  # revealed: Unknown
# error: "has no member `fails`"
reveal_type(mypackage.fails.Y)  # revealed: Unknown
```

## `from` Import of Sibling Module (Non-Stub Check)

`mypackage/__init__.py`:

```py
```

`mypackage/imported.py`:

```py
from . import fails

X: int = 42
```

`mypackage/fails.py`:

```py
Y: int = 47
```

`main.py`:

```py
import mypackage
from mypackage import imported

reveal_type(imported.X)  # revealed: int
reveal_type(imported.fails.Y)  # revealed: int
# error: "has no member `fails`"
reveal_type(mypackage.fails.Y)  # revealed: Unknown
```

## Fractal Re-export Nameclash Problems

This precise configuration of:

- a subpackage that defines a submodule with its own name
- that in turn defines a function/class with its own name
- and re-exporting that name through every layer using `from` imports and `__all__`

Can easily result in the typechecker getting "confused" and thinking imports of the name from the
top-level package are referring to the subpackage and not the function/class. This issue can be
found with the `lobpcg` function in `scipy.sparse.linalg`.

This kind of failure mode is why the rule is restricted to *direct* submodule imports, as anything
more powerful than that in the current implementation strategy quickly gets the functions and
submodules mixed up.

`mypackage/__init__.pyi`:

```pyi
from .funcmod import funcmod

__all__ = ["funcmod"]
```

`mypackage/funcmod/__init__.pyi`:

```pyi
from .funcmod import funcmod

__all__ = ["funcmod"]
```

`mypackage/funcmod/funcmod.pyi`:

```pyi
__all__ = ["funcmod"]

def funcmod(x: int) -> int: ...
```

`main.py`:

```py
from mypackage import funcmod

x = funcmod(1)
```

## Fractal Re-export Nameclash Problems (Non-Stub Check)

`mypackage/__init__.py`:

```py
from .funcmod import funcmod

__all__ = ["funcmod"]
```

`mypackage/funcmod/__init__.py`:

```py
from .funcmod import funcmod

__all__ = ["funcmod"]
```

`mypackage/funcmod/funcmod.py`:

```py
__all__ = ["funcmod"]

def funcmod(x: int) -> int:
    return x
```

`main.py`:

```py
from mypackage import funcmod

x = funcmod(1)
```
