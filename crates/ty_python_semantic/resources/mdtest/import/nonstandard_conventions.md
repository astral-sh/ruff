# Nonstandard Import Conventions

This document covers ty-specific extensions to the
[standard import conventions](https://typing.python.org/en/latest/spec/distributing.html#import-conventions),
and other intentional deviations from actual python semantics.

This file currently covers the following details:

- **froms are locals**: a `from..import` can only define locals, it does not have global
    side-effects. Specifically any submodule attribute `a` that's implicitly introduced by either
    `from .a import b` or `from . import a as b` (in an `__init__.py(i)`) is a local and not a
    global. However we only introduce this symbol if the `from..import` is in global-scope. This
    means imports at the start of a file work as you'd expect, while imports in a function don't
    introduce submodule attributes.

- **implicit imports fill missing bindings**: a `from..import` in an `__init__.py(i)` introduces a
    direct submodule as a local only on control-flow paths where that name has not been bound.
    Existing bindings and explicit deletions are preserved. In particular, an import in one
    conditional branch does not prevent another branch from introducing the same submodule.

- **explicit bindings take precedence**: an implicitly introduced submodule does not overwrite an
    existing explicit binding with the same name. The submodule might already have been loaded by
    another import, in which case loading it again would not overwrite that binding at runtime. We
    preserve the explicit binding without tracking import order across modules, even when the
    submodule is actually loaded for the first time.

- **dot re-exports**: `from . import a` in an `__init__.pyi` is considered a re-export of `a`
    (equivalent to `from . import a as a`). This is required to properly handle many stubs in the
    wild. Equivalent imports like `from whatever.thispackage import a` also introduce a re-export
    (this has essentially zero ecosystem impact, we just felt it was more consistent). The only way
    to opt out of this is to rename the import to something else (`from . import a as b`).
    `from .a import b` and equivalent does *not* introduce a re-export.

Note: almost all tests in here have a stub and non-stub version, because we're interested in both
defining symbols *at all* and re-exporting them.

## Relative `from` Import of Direct Submodule in `__init__`

We consider the `from . import submodule` idiom in an `__init__.pyi` an explicit re-export.

### In Stub

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
# error: [possibly-missing-submodule] "Submodule `fails` might not have been imported"
reveal_type(mypackage.fails.Y)  # revealed: Unknown
```

### In Non-Stub

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
# error: [possibly-missing-submodule] "Submodule `fails` might not have been imported"
reveal_type(mypackage.fails.Y)  # revealed: Unknown
```

## Absolute `from` Import of Direct Submodule in `__init__`

If an absolute `from...import` happens to import a submodule (i.e. it's equivalent to
`from . import y`) we also treat it as a re-export.

### In Stub

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
# error: [possibly-missing-submodule] "Submodule `fails` might not have been imported"
reveal_type(mypackage.fails.Y)  # revealed: Unknown
```

### In Non-Stub

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
# error: [possibly-missing-submodule] "Submodule `fails` might not have been imported"
reveal_type(mypackage.fails.Y)  # revealed: Unknown
```

## Import of Direct Submodule in `__init__`

An `import` that happens to import a submodule does not expose the submodule as an attribute. (This
is an arbitrary decision and can be changed!)

### In Stub

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

# TODO: this could work and would be nice to have?
# error: [possibly-missing-submodule] "Submodule `imported` might not have been imported"
reveal_type(mypackage.imported.X)  # revealed: Unknown
```

### In Non-Stub

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

# TODO: this could work and would be nice to have
# error: [possibly-missing-submodule] "Submodule `imported` might not have been imported"
reveal_type(mypackage.imported.X)  # revealed: Unknown
```

## Relative `from` Import of Nested Submodule in `__init__`

`from .submodule import nested` in an `__init__.pyi` does re-export `mypackage.submodule`, but not
`mypackage.submodule.nested` or `nested`.

### In Stub

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

reveal_type(mypackage.submodule)  # revealed: <module 'mypackage.submodule'>
# error: [possibly-missing-submodule] "Submodule `nested` might not have been imported"
reveal_type(mypackage.submodule.nested)  # revealed: Unknown
# error: [possibly-missing-submodule] "Submodule `nested` might not have been imported"
reveal_type(mypackage.submodule.nested.X)  # revealed: Unknown
# error: [unresolved-attribute] "has no member `nested`"
reveal_type(mypackage.nested)  # revealed: Unknown
# error: [unresolved-attribute] "has no member `nested`"
reveal_type(mypackage.nested.X)  # revealed: Unknown
```

### In Non-Stub

`from .submodule import nested` in an `__init__.py` exposes `mypackage.submodule` and `nested`.

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

reveal_type(mypackage.submodule)  # revealed: <module 'mypackage.submodule'>
# TODO: this would be nice to support
# error: [possibly-missing-submodule] "Submodule `nested` might not have been imported"
reveal_type(mypackage.submodule.nested)  # revealed: Unknown
# error: [possibly-missing-submodule] "Submodule `nested` might not have been imported"
reveal_type(mypackage.submodule.nested.X)  # revealed: Unknown
reveal_type(mypackage.nested)  # revealed: <module 'mypackage.submodule.nested'>
reveal_type(mypackage.nested.X)  # revealed: int
```

## Absolute `from` Import of Nested Submodule in `__init__`

`from mypackage.submodule import nested` in an `__init__.pyi` does not re-export
`mypackage.submodule`, `mypackage.submodule.nested`, or `nested`.

### In Stub

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

reveal_type(mypackage.submodule)  # revealed: <module 'mypackage.submodule'>
# error: [possibly-missing-submodule] "Submodule `nested` might not have been imported"
reveal_type(mypackage.submodule.nested)  # revealed: Unknown
# error: [possibly-missing-submodule] "Submodule `nested` might not have been imported"
reveal_type(mypackage.submodule.nested.X)  # revealed: Unknown
# error: [unresolved-attribute] "has no member `nested`"
reveal_type(mypackage.nested)  # revealed: Unknown
# error: [unresolved-attribute] "has no member `nested`"
reveal_type(mypackage.nested.X)  # revealed: Unknown
```

### In Non-Stub

`from mypackage.submodule import nested` in an `__init__.py` creates both `submodule` and `nested`.

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

reveal_type(mypackage.submodule)  # revealed: <module 'mypackage.submodule'>
# TODO: this would be nice to support
# error: [possibly-missing-submodule] "Submodule `nested` might not have been imported"
reveal_type(mypackage.submodule.nested)  # revealed: Unknown
# error: [possibly-missing-submodule] "Submodule `nested` might not have been imported"
reveal_type(mypackage.submodule.nested.X)  # revealed: Unknown
reveal_type(mypackage.nested)  # revealed: <module 'mypackage.submodule.nested'>
reveal_type(mypackage.nested.X)  # revealed: int
```

## Import of Nested Submodule in `__init__`

`import mypackage.submodule.nested` in an `__init__.pyi` does not re-export `mypackage.submodule` or
`mypackage.submodule.nested`.

### In Stub

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

# error: [possibly-missing-submodule] "Submodule `submodule` might not have been imported"
reveal_type(mypackage.submodule)  # revealed: Unknown
# error: [possibly-missing-submodule] "Submodule `submodule` might not have been imported"
reveal_type(mypackage.submodule.nested)  # revealed: Unknown
# error: [possibly-missing-submodule] "Submodule `submodule` might not have been imported"
reveal_type(mypackage.submodule.nested.X)  # revealed: Unknown
```

### In Non-Stub

`import mypackage.submodule.nested` in an `__init__.py` does not define `mypackage.submodule` or
`mypackage.submodule.nested` outside the package.

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

# TODO: this would be nice to support
# error: [possibly-missing-submodule] "Submodule `submodule` might not have been imported"
reveal_type(mypackage.submodule)  # revealed: Unknown
# error: [possibly-missing-submodule] "Submodule `submodule` might not have been imported"
reveal_type(mypackage.submodule.nested)  # revealed: Unknown
# error: [possibly-missing-submodule] "Submodule `submodule` might not have been imported"
reveal_type(mypackage.submodule.nested.X)  # revealed: Unknown
```

## Relative `from` Import of Direct Submodule in `__init__`, Mismatched Alias

Renaming the submodule to something else disables the `__init__.pyi` idiom.

### In Stub

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

# error: [possibly-missing-submodule] "Submodule `imported` might not have been imported"
reveal_type(mypackage.imported.X)  # revealed: Unknown
# error: [unresolved-attribute] "has no member `imported_m`"
reveal_type(mypackage.imported_m.X)  # revealed: Unknown
```

### In Non-Stub

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
# error: [possibly-missing-submodule] "Submodule `imported` might not have been imported"
reveal_type(mypackage.imported.X)  # revealed: Unknown
reveal_type(mypackage.imported_m.X)  # revealed: int
```

## Relative `from` Import of Direct Submodule in `__init__`, Matched Alias

The `__init__.pyi` idiom should definitely always work if the submodule is renamed to itself, as
this is the re-export idiom.

### In Stub

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

### In Non-Stub

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

### In Stub

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

# TODO: this would be nice to support
# error: [unresolved-reference] "`imported` used when not defined"
reveal_type(imported.X)  # revealed: Unknown
reveal_type(Z)  # revealed: int
```

### In Non-Stub

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

A `from` import that imports a non-submodule isn't currently a special case here (various
proposed/tested approaches did treat this specially).

### In Stub

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

reveal_type(mypackage.imported.X)  # revealed: int
```

### In Non-Stub

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

reveal_type(mypackage.imported.X)  # revealed: int
```

## `from` Import of Other Package's Submodule

`from mypackage import submodule` from outside the package is not modeled as a side-effect on
`mypackage`, even in the importing file (this could be changed!).

### In Stub

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

reveal_type(imported.X)  # revealed: int

# TODO: this would be nice to support, but it's dangerous with available_submodule_attributes
# for details, see: https://github.com/astral-sh/ty/issues/1488
# error: [possibly-missing-submodule] "Submodule `imported` might not have been imported"
reveal_type(mypackage.imported.X)  # revealed: Unknown
```

### In Non-Stub

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

reveal_type(imported.X)  # revealed: int

# TODO: this would be nice to support, as it works at runtime
# error: [possibly-missing-submodule] "Submodule `imported` might not have been imported"
reveal_type(mypackage.imported.X)  # revealed: Unknown
```

## `from` Import of Sibling Module

`from . import submodule` from a sibling module is not modeled as a side-effect on `mypackage` or a
re-export from `submodule`.

### In Stub

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
# error: [unresolved-attribute] "has no member `fails`"
reveal_type(imported.fails.Y)  # revealed: Unknown
# error: [possibly-missing-submodule] "Submodule `fails` might not have been imported"
reveal_type(mypackage.fails.Y)  # revealed: Unknown
```

### In Non-Stub

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
# error: [possibly-missing-submodule] "Submodule `fails`"
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

We avoid this by ensuring that the imported name (the right-hand `funcmod` in
`from .funcmod import funcmod`) overwrites the submodule attribute (the left-hand `funcmod`), as it
does at runtime.

### In Stub

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

### In Non-Stub

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

## Re-export Nameclash Problems In Functions

`from` imports in an `__init__.py` at file scope should be visible to functions defined in the file:

`mypackage/__init__.py`:

```py
from .funcmod import funcmod

funcmod(1)

def run():
    funcmod(2)
```

`mypackage/funcmod.py`:

```py
def funcmod(x: int) -> int:
    return x
```

## Re-export Nameclash Problems After Conditional Raise

`from` imports in an `__init__.py` at file scope should overwrite the submodule attribute even when
earlier ambiguous control flow causes us to consider more bindings from nested functions.

`mypackage/__init__.py`:

```py
def should_raise() -> bool:
    return True

if should_raise():
    raise RuntimeError

from .funcmod import funcmod

reveal_type(funcmod)  # revealed: def funcmod(x: int) -> int

def run():
    reveal_type(funcmod)  # revealed: def funcmod(x: int) -> int
    funcmod(1)
```

`mypackage/funcmod.py`:

```py
def funcmod(x: int) -> int:
    return x
```

## Re-export Nameclash Problems In Try-Blocks

`from` imports in an `__init__.py` at file scope in a `try` block should be visible to functions
defined in the `try` block (regression test for a bug):

`mypackage/__init__.py`:

```py
try:
    from .funcmod import funcmod

    funcmod(1)

    def run():
        funcmod(2)

finally:
    x = 1
```

`mypackage/funcmod.py`:

```py
def funcmod(x: int) -> int:
    return x
```

## RHS `from` Imports In Functions

If a `from` import occurs in a function, the RHS symbols should only be visible in that function.

`mypackage/__init__.py`:

```py
def run1():
    from .funcmod import funcmod

    funcmod(1)

def run2():
    from .funcmod import funcmod

    funcmod(2)

def run3():
    # error: [unresolved-reference]
    funcmod(3)

# error: [unresolved-reference]
funcmod(4)
```

`mypackage/funcmod.py`:

```py
def funcmod(x: int) -> int:
    return x
```

## LHS `from` Imports In Functions

If a `from` import occurs in a function, we simply ignore its LHS effects to avoid modeling
execution-order-specific behaviour (and to discourage people writing code that has it).

`mypackage/__init__.py`:

```py
def run1():
    from .funcmod import other

    # TODO: this would be nice to support
    # error: [unresolved-reference]
    funcmod.funcmod(1)

def run2():
    from .funcmod import other

    # TODO: this would be nice to support
    # error: [unresolved-reference]
    funcmod.funcmod(2)

def run3():
    # error: [unresolved-reference]
    funcmod.funcmod(3)

# error: [unresolved-reference]
funcmod.funcmod(4)
```

`mypackage/funcmod.py`:

```py
other: int = 1

def funcmod(x: int) -> int:
    return x
```

## RHS `from` Imports Overwrite Locals

The explicitly imported name on the RHS of a `from..import` overwrites an earlier binding with the
same name, even when that name also matches the submodule being imported.

`mypackage/__init__.py`:

```py
funcmod = 0
from .funcmod import funcmod

funcmod(1)
```

`mypackage/funcmod.py`:

```py
def funcmod(x: int) -> int:
    return x
```

## LHS `from` Imports Overwritten By Local Function

The LHS of a `from..import` introduces a local symbol that can be overwritten by defining a function
(or class) with the same name.

### In Stub

`mypackage/__init__.pyi`:

```pyi
from .funcmod import other

def funcmod(x: int) -> int: ...
```

`mypackage/funcmod/__init__.pyi`:

```pyi
def other(int) -> int: ...
```

`main.py`:

```py
from mypackage import funcmod

x = funcmod(1)
```

### In Non-Stub

`mypackage/__init__.py`:

```py
from .funcmod import other

def funcmod(x: int) -> int:
    return x
```

`mypackage/funcmod/__init__.py`:

```py
def other(x: int) -> int:
    return x
```

`main.py`:

```py
from mypackage import funcmod

x = funcmod(1)
```

## LHS `from` Imports Overwritten By Local Assignment

The LHS of a `from..import` introduces a local symbol that can be overwritten by assigning to it.

### In Stub

`mypackage/__init__.pyi`:

```pyi
from .funcmod import other

funcmod = other
```

`mypackage/funcmod/__init__.pyi`:

```pyi
def other(x: int) -> int: ...
```

`main.py`:

```py
from mypackage import funcmod

x = funcmod(1)
```

### In Non-Stub

`mypackage/__init__.py`:

```py
from .funcmod import other

funcmod = other
```

`mypackage/funcmod/__init__.py`:

```py
def other(x: int) -> int:
    return x
```

`main.py`:

```py
from mypackage import funcmod

x = funcmod(1)
```

## LHS `from` Imports Preserve Re-exports On Repeated Imports

After explicitly importing a function with the same name as its submodule, a later import from that
submodule preserves the function binding.

### In Stub

`mypackage/__init__.pyi`:

```pyi
from .funcmod import funcmod as funcmod
from .funcmod import other
```

`mypackage/funcmod/__init__.pyi`:

```pyi
def other(x: int) -> int: ...
def funcmod(x: int) -> int: ...
```

`main.py`:

```py
from mypackage import funcmod

x = funcmod(1)
```

### In Non-Stub

`mypackage/__init__.py`:

```py
from .funcmod import funcmod
from .funcmod import other
```

`mypackage/funcmod/__init__.py`:

```py
def other(x: int) -> int:
    return x

def funcmod(x: int) -> int:
    return x
```

`main.py`:

```py
from mypackage import funcmod

x = funcmod(1)
```

## LHS `from` Imports Preserve Explicit Bindings

### Local Assignment

An implicit submodule binding does not overwrite an earlier assignment. At runtime, this import
overwrites `funcmod` if the submodule has not already been loaded. We intentionally preserve the
explicit binding rather than track import order across modules.

`mypackage/__init__.py`:

```py
funcmod = 0
from .funcmod import other

reveal_type(funcmod)  # revealed: Literal[0]
```

`mypackage/funcmod.py`:

```py
other = 1
```

### Conditional Assignment

An implicit submodule binding also preserves earlier assignments in conditional branches.

`mypackage/__init__.py`:

```py
def choose() -> bool:
    return True

if choose():
    funcmod = 0
else:
    funcmod = 1

from .funcmod import other

reveal_type(funcmod)  # revealed: Literal[0, 1]
```

`mypackage/funcmod.py`:

```py
other = 2
```

### Narrowed Member

Preserving an explicit binding also preserves narrowing of its members.

`mypackage/__init__.py`:

```py
class Value:
    attr: int | str = 0

funcmod = Value()

if isinstance(funcmod.attr, int):
    from .funcmod import other

    reveal_type(funcmod.attr)  # revealed: int
```

`mypackage/funcmod.py`:

```py
other = 0
```

### Imported Function

Importing `helpers` loads `funcmod` before the package binds that name to the exported function. The
later import from the already loaded submodule preserves the function, both at runtime and when type
checking.

`mypackage/__init__.py`:

```py
from mypackage.helpers import funcmod
from mypackage.funcmod import other

reveal_type(funcmod)  # revealed: def funcmod() -> int
funcmod()
```

`mypackage/helpers.py`:

```py
import mypackage.funcmod

def funcmod() -> int:
    return 1
```

`mypackage/funcmod.py`:

```py
other = 0
```

### Relative Imports In Stubs

Relative imports in stubs also preserve an explicitly re-exported function when a later import
implicitly introduces a submodule with the same name.

`mypackage/__init__.pyi`:

```pyi
from .helpers import funcmod as funcmod
from .funcmod import other
```

`mypackage/helpers.pyi`:

```pyi
def funcmod() -> int: ...
```

`mypackage/funcmod.pyi`:

```pyi
other: int
```

`main.py`:

```py
from mypackage import funcmod

reveal_type(funcmod)  # revealed: def funcmod() -> int
funcmod()
```

## LHS `from` Imports After An Unbound Use

An earlier reference to an unbound name does not prevent an import from implicitly binding that name
to the submodule.

`mypackage/__init__.py`:

```py
# error: [unresolved-reference]
funcmod

from .funcmod import other

reveal_type(funcmod)  # revealed: <module 'mypackage.funcmod'>
```

`mypackage/funcmod.py`:

```py
other = 0
```

## LHS `from` Imports Fill Missing Bindings

These examples enable possibly-unresolved-reference diagnostics to check that implicit imports bind
a submodule on every path where its name has not already been bound.

```toml
[rules]
possibly-unresolved-reference = "error"
```

### Single Conditional Assignment

An implicit import preserves the explicit value on the branch where it was assigned and binds the
submodule on the other branch. The name is definitely bound after the import.

`mypackage/__init__.py`:

```py
def choose() -> bool:
    return True

if choose():
    funcmod = 0

from .funcmod import other

reveal_type(funcmod)  # revealed: Literal[0] | <module 'mypackage.funcmod'>
```

`mypackage/funcmod.py`:

```py
other = 1
```

### Conditional Imported Final

Filling an undefined path does not discard the `Final` qualifier of a binding imported on another
path. The implicit import is allowed, but a subsequent explicit reassignment is rejected.

`mypackage/__init__.py`:

```py
def choose() -> bool:
    return True

if choose():
    from .helpers import funcmod

from .funcmod import other

reveal_type(funcmod)  # revealed: Literal[1] | <module 'mypackage.funcmod'>
# error: [invalid-assignment] "Reassignment of `Final` symbol `funcmod` is not allowed"
funcmod = 1
```

`mypackage/helpers.py`:

```py
from typing import Final

funcmod: Final = 1
```

`mypackage/funcmod.py`:

```py
other = 0
```

### Member Assignment In A Statically True Branch

An implicit import preserves a member assignment on the branch where its receiver is explicitly
bound. Since this condition is always true, the assigned member value remains visible after the
import.

`mypackage/__init__.py`:

```py
class Value:
    attr: int | str = 0

if 1 == 1:
    funcmod = Value()
    funcmod.attr = 1

from .funcmod import other

reveal_type(funcmod.attr)  # revealed: Literal[1]
```

`mypackage/funcmod.py`:

```py
other = 0
attr = "module"
```

### Imports In Both Branches

Both branches independently introduce the submodule binding, so it is available after the
conditional.

`mypackage/__init__.py`:

```py
def choose() -> bool:
    return True

if choose():
    from .funcmod import other
else:
    from .funcmod import other

reveal_type(funcmod)  # revealed: <module 'mypackage.funcmod'>
```

`mypackage/funcmod.py`:

```py
other = 0
```

### Unreachable Earlier Import

An unreachable import does not prevent a later import from introducing the submodule binding.

`mypackage/__init__.py`:

```py
if False:
    from .funcmod import other

from .funcmod import other

reveal_type(funcmod)  # revealed: <module 'mypackage.funcmod'>
```

`mypackage/funcmod.py`:

```py
other = 0
```

### Repeated Imports In A Loop

The first iteration implicitly binds the submodule. Subsequent iterations preserve the explicit
assignment from the preceding iteration, so the name is always bound after the import.

`mypackage/__init__.py`:

```py
for _ in range(2):
    from .funcmod import other

    reveal_type(funcmod)  # revealed: Literal[0] | <module 'mypackage.funcmod'>
    funcmod = 0
```

`mypackage/funcmod.py`:

```py
other = 1
```

### Implicit Bindings Across Loop Iterations

An implicit submodule binding can come from an earlier loop iteration. The name is still possibly
unbound because the first iteration can use it before taking the branch that imports it.

`mypackage/__init__.py`:

```py
def choose() -> bool:
    return True

for _ in range(2):
    if choose():
        # error: [possibly-unresolved-reference]
        reveal_type(funcmod)  # revealed: <module 'mypackage.funcmod'>
    else:
        from .funcmod import other
```

`mypackage/funcmod.py`:

```py
other = 0
```

### Narrowing Of Builtin Members

An implicit submodule binding replaces a builtin name when there is no local binding. Narrowing of a
member on the builtin does not apply to the newly bound module.

`mypackage/__init__.py`:

```py
if str.__name__ == "str":
    from .str import other

    reveal_type(str.__name__)  # revealed: str
```

`mypackage/str.py`:

```py
other = 0
```

## LHS `from` Imports Preserve Deletions

Deleting a binding differs from a name that has never been bound: an implicit import does not
restore an explicitly deleted name.

```toml
[rules]
possibly-unresolved-reference = "error"
```

### Conditional Deletion

The first import loads the submodule. Importing it again does not recreate the package attribute if
it was subsequently deleted, so the name remains possibly unbound.

`mypackage/__init__.py`:

```py
def choose() -> bool:
    return True

from .funcmod import other

if choose():
    del funcmod

from .funcmod import other

# error: [possibly-unresolved-reference]
reveal_type(funcmod)  # revealed: <module 'mypackage.funcmod'>
```

`mypackage/funcmod.py`:

```py
other = 0
```

### Deletion Before The First Import

Deleting an explicit value also prevents an implicit import from binding that name. This is an
intentional approximation: at runtime, the first load of the submodule would recreate the deleted
attribute, but a repeated import would not.

`mypackage/__init__.py`:

```py
funcmod = 0
del funcmod

from .funcmod import other

# error: [unresolved-reference]
reveal_type(funcmod)  # revealed: Unknown
```

`mypackage/funcmod.py`:

```py
other = 1
```
