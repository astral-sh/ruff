# Wildcard (`*`) imports

See the [Python language reference for import statements].

## Basic functionality

### A simple `*` import

`exporter.py`:

```py
X: bool = True
```

`importer.py`:

```py
from exporter import *

reveal_type(X)  # revealed: bool
print(Y)  # error: [unresolved-reference]
```

### Overriding an existing definition

`exporter.py`:

```py
X: bool = True
```

`importer.py`:

```py
X = 42
reveal_type(X)  # revealed: Literal[42]

from exporter import *

reveal_type(X)  # revealed: bool
```

### Overridden by a later definition

`exporter.py`:

```py
X: bool = True
```

`importer.py`:

```py
from exporter import *

reveal_type(X)  # revealed: bool
X = False
reveal_type(X)  # revealed: Literal[False]
```

### Reaching across many modules

`a.py`:

```py
X: bool = True
```

`b.py`:

```py
from a import *
```

`c.py`:

```py
from b import *
```

`main.py`:

```py
from c import *

reveal_type(X)  # revealed: bool
```

### A wildcard import constitutes a re-export

This is specified
[here](https://typing.python.org/en/latest/spec/distributing.html#import-conventions).

`a.pyi`:

```pyi
X: bool = True
```

`b.pyi`:

```pyi
Y: bool = False
```

`c.pyi`:

```pyi
from a import *
from b import Y
```

`main.py`:

```py
# `X` is accessible because the `*` import in `c` re-exports it from `c`
from c import X

# but `Y` is not because the `from b import Y` import does *not* constitute a re-export
from c import Y  # error: [unresolved-import]
```

## Esoteric definitions and redefinintions

```toml
[environment]
python-version = "3.12"
```

We understand all public symbols defined in an external module as being imported by a `*` import,
not just those that are defined in `StmtAssign` nodes and `StmtAnnAssign` nodes. This section
provides tests for definitions, and redefinitions, that use more esoteric AST nodes.

### Global-scope symbols defined using walrus expressions

`exporter.py`:

```py
X = (Y := 3) + 4
```

`b.py`:

```py
from exporter import *

reveal_type(X)  # revealed: Unknown | Literal[7]
reveal_type(Y)  # revealed: Unknown | Literal[3]
```

### Global-scope symbols defined in many other ways

`exporter.py`:

```py
import typing
from collections import OrderedDict
from collections import OrderedDict as Foo

A, B = 1, (C := 2)
D: (E := 4) = (F := 5)  # error: [invalid-type-form]

for G in [1]:
    ...

for (H := 4).whatever in [2]:  # error: [unresolved-attribute]
    ...

class I: ...

def J(): ...

type K = int

class ContextManagerThatMightNotRunToCompletion:
    def __enter__(self) -> "ContextManagerThatMightNotRunToCompletion":
        return self

    def __exit__(self, *args) -> typing.Literal[True]:
        return True

with ContextManagerThatMightNotRunToCompletion() as L:
    U = ...

match 42:
    case {"something": M}:
        ...
    case [*N]:
        ...
    case [O]:
        ...
    case P | Q:  # error: [invalid-syntax] "name capture `P` makes remaining patterns unreachable"
        ...
    case object(foo=R):
        ...

match 56:
    case x if something_unresolvable:  # error: [unresolved-reference]
        ...

    case object(S):
        ...

match 12345:
    case x if something_unresolvable:  # error: [unresolved-reference]
        ...

    case T:
        ...

def boolean_condition() -> bool:
    return True

if boolean_condition():
    V = ...

while boolean_condition():
    W = ...
```

`importer.py`:

```py
from exporter import *

# fmt: off

print((
    A,
    B,
    C,
    D,
    E,
    F,
    G,  # error: [possibly-unresolved-reference]
    H,  # error: [possibly-unresolved-reference]
    I,
    J,
    K,
    L,
    M,  # error: [possibly-unresolved-reference]
    N,  # error: [possibly-unresolved-reference]
    O,  # error: [possibly-unresolved-reference]
    P,  # error: [possibly-unresolved-reference]
    Q,  # error: [possibly-unresolved-reference]
    R,  # error: [possibly-unresolved-reference]
    S,  # error: [possibly-unresolved-reference]
    T,  # error: [possibly-unresolved-reference]
    U,  # TODO: could emit [possibly-unresolved-reference here] (https://github.com/astral-sh/ruff/issues/16996)
    V,  # error: [possibly-unresolved-reference]
    W,  # error: [possibly-unresolved-reference]
    typing,
    OrderedDict,
    Foo,
))
```

### Esoteric possible redefinitions following definitely bound prior definitions

There should be no complaint about the symbols being possibly unbound in `b.py` here: although the
second definition might or might not take place, each symbol is definitely bound by a prior
definition.

`exporter.py`:

```py
from typing import Literal

A = 1
B = 2
C = 3
D = 4
E = 5
F = 6
G = 7
H = 8
I = 9
J = 10
K = 11
L = 12

for A in [1]:
    ...

match 42:
    case {"something": B}:
        ...
    case [*C]:
        ...
    case [D]:
        ...
    case E | F:  # error: [invalid-syntax] "name capture `E` makes remaining patterns unreachable"
        ...
    case object(foo=G):
        ...
    case object(H):
        ...
    case I:
        ...

def boolean_condition() -> bool:
    return True

if boolean_condition():
    J = ...

while boolean_condition():
    K = ...

class ContextManagerThatMightNotRunToCompletion:
    def __enter__(self) -> "ContextManagerThatMightNotRunToCompletion":
        return self

    def __exit__(self, *args) -> Literal[True]:
        return True

with ContextManagerThatMightNotRunToCompletion():
    L = ...
```

`importer.py`:

```py
from exporter import *

print(A)
print(B)
print(C)
print(D)
print(E)
print(F)
print(G)
print(H)
print(I)
print(J)
print(K)
print(L)
```

### Esoteric possible definitions prior to definitely bound prior redefinitions

The same principle applies here to the symbols in `b.py`. Although the first definition might or
might not take place, each symbol is definitely bound by a later definition.

`exporter.py`:

```py
from typing import Literal

for A in [1]:
    ...

match 42:
    case {"something": B}:
        ...
    case [*C]:
        ...
    case [D]:
        ...
    case E | F:  # error: [invalid-syntax] "name capture `E` makes remaining patterns unreachable"
        ...
    case object(foo=G):
        ...
    case object(H):
        ...
    case I:
        ...

def boolean_condition() -> bool:
    return True

if boolean_condition():
    J = ...

while boolean_condition():
    K = ...

class ContextManagerThatMightNotRunToCompletion:
    def __enter__(self) -> "ContextManagerThatMightNotRunToCompletion":
        return self

    def __exit__(self, *args) -> Literal[True]:
        return True

with ContextManagerThatMightNotRunToCompletion():
    L = ...

A = 1
B = 2
C = 3
D = 4
E = 5
F = 6
G = 7
H = 8
I = 9
J = 10
K = 11
L = 12
```

`importer.py`:

```py
from exporter import *

print(A)
print(B)
print(C)
print(D)
print(E)
print(F)
print(G)
print(H)
print(I)
print(J)
print(K)
print(L)
```

### Definitions in function-like scopes are not global definitions

Except for some cases involving walrus expressions inside comprehension scopes.

`exporter.py`:

```py
class Iterator:
    def __next__(self) -> int:
        return 42

class Iterable:
    def __iter__(self) -> Iterator:
        return Iterator()

[a for a in Iterable()]
{b for b in Iterable()}
{c: c for c in Iterable()}
(d for d in Iterable())
lambda e: (f := 42)

# Definitions created by walruses in a comprehension scope are unique;
# they "leak out" of the scope and are stored in the surrounding scope
[(g := h * 2) for h in Iterable()]
[i for j in Iterable() if (i := j - 10) > 0]
{(k := l * 2): (m := l * 3) for l in Iterable()}
list(((o := p * 2) for p in Iterable()))

# A walrus expression nested inside several scopes *still* leaks out
# to the global scope:
[[[[(q := r) for r in Iterable()]] for _ in range(42)] for _ in range(42)]

# A walrus inside a lambda inside a comprehension does not leak out
[(lambda s=s: (t := 42))() for s in Iterable()]
```

`importer.py`:

```py
from exporter import *

# error: [unresolved-reference]
reveal_type(a)  # revealed: Unknown
# error: [unresolved-reference]
reveal_type(b)  # revealed: Unknown
# error: [unresolved-reference]
reveal_type(c)  # revealed: Unknown
# error: [unresolved-reference]
reveal_type(d)  # revealed: Unknown
# error: [unresolved-reference]
reveal_type(e)  # revealed: Unknown
# error: [unresolved-reference]
reveal_type(f)  # revealed: Unknown
# error: [unresolved-reference]
reveal_type(h)  # revealed: Unknown
# error: [unresolved-reference]
reveal_type(j)  # revealed: Unknown
# error: [unresolved-reference]
reveal_type(p)  # revealed: Unknown
# error: [unresolved-reference]
reveal_type(r)  # revealed: Unknown
# error: [unresolved-reference]
reveal_type(s)  # revealed: Unknown
# error: [unresolved-reference]
reveal_type(t)  # revealed: Unknown

# TODO: these should all reveal `Unknown | int` and should not emit errors.
# (We don't generally model elsewhere in ty that bindings from walruses
# "leak" from comprehension scopes into outer scopes, but we should.)
# See https://github.com/astral-sh/ruff/issues/16954
# error: [unresolved-reference]
reveal_type(g)  # revealed: Unknown
# error: [unresolved-reference]
reveal_type(i)  # revealed: Unknown
# error: [unresolved-reference]
reveal_type(k)  # revealed: Unknown
# error: [unresolved-reference]
reveal_type(m)  # revealed: Unknown
# error: [unresolved-reference]
reveal_type(o)  # revealed: Unknown
# error: [unresolved-reference]
reveal_type(q)  # revealed: Unknown
```

### An annotation without a value is a definition in a stub but not a `.py` file

`a.pyi`:

```pyi
X: bool
```

`b.py`:

```py
Y: bool
```

`c.py`:

```py
from a import *
from b import *

reveal_type(X)  # revealed: bool
# error: [unresolved-reference]
reveal_type(Y)  # revealed: Unknown
```

## Which symbols are exported

Not all symbols in the global namespace are considered "public". As a result, not all symbols bound
in the global namespace of an `exporter.py` module will be imported by a `from exporter import *`
statement in an `importer.py` module. The tests in this section elaborate on these semantics.

### Global-scope names starting with underscores

Global-scope names starting with underscores are not imported from a `*` import (unless the
exporting module has an `__all__` symbol in its global scope, and the underscore-prefixed symbols
are included in `__all__`):

`exporter.py`:

```py
_private: bool = False
__protected: bool = False
__dunder__: bool = False
___thunder___: bool = False

Y: bool = True
```

`importer.py`:

```py
from exporter import *

# error: [unresolved-reference]
reveal_type(_private)  # revealed: Unknown
# error: [unresolved-reference]
reveal_type(__protected)  # revealed: Unknown
# error: [unresolved-reference]
reveal_type(__dunder__)  # revealed: Unknown
# error: [unresolved-reference]
reveal_type(___thunder___)  # revealed: Unknown

reveal_type(Y)  # revealed: bool
```

### All public symbols are considered re-exported from `.py` files

For `.py` files, we should consider all public symbols in the global namespace exported by that
module when considering which symbols are made available by a `*` import. Here, `b.py` does not use
the explicit `from a import X as X` syntax to explicitly mark it as publicly re-exported, and `X` is
not included in `__all__`; whether it should be considered a "public name" in module `b` is
ambiguous.

We should consider `X` bound in `c.py`. However, we could consider adding an opt-in rule to warn the
user when they use `X` in `c.py` that it was neither included in `b.__all__` nor marked as an
explicit re-export from `b` through the "redundant alias" convention.

`a.py`:

```py
X: bool = True
```

`b.py`:

```py
from a import X
```

`c.py`:

```py
from b import *

# TODO: we could consider an opt-in diagnostic (see prose commentary above)
reveal_type(X)  # revealed: bool
```

### Only explicit re-exports are considered re-exported from `.pyi` files

For `.pyi` files, we should consider all imports "private to the stub" unless they are included in
`__all__` or use the explicit `from foo import X as X` syntax.

`a.pyi`:

```pyi
X: bool = True
Y: bool = True
```

`b.pyi`:

```pyi
from a import X, Y as Y
```

`c.py`:

```py
from b import *

# This error is correct, as `X` is not considered re-exported from module `b`:
#
# error: [unresolved-reference]
reveal_type(X)  # revealed: Unknown

reveal_type(Y)  # revealed: bool
```

### An implicit import in a `.pyi` file later overridden by another assignment

`a.pyi`:

```pyi
X: bool = True
```

`b.pyi`:

```pyi
from a import X

X: bool = False
```

`c.py`:

```py
from b import *

reveal_type(X)  # revealed: bool
```

## Visibility constraints

If an `importer` module contains a `from exporter import *` statement in its global namespace, the
statement will *not* necessarily import *all* symbols that have definitions in `exporter.py`'s
global scope. For any given symbol in `exporter.py`'s global scope, that symbol will *only* be
imported by the `*` import if at least one definition for that symbol is visible from the *end* of
`exporter.py`'s global scope.

For example, say that `exporter.py` contains a symbol `X` in its global scope, and the definition
for `X` in `exporter.py` has visibility constraints <code>vis<sub>1</sub></code>. The
`from exporter import *` statement in `importer.py` creates a definition for `X` in `importer`, and
there are visibility constraints <code>vis<sub>2</sub></code> on the import statement in
`importer.py`. This means that the overall visibility constraints on the `X` definnition created by
the import statement in `importer.py` will be <code>vis<sub>1</sub> AND vis<sub>2</sub></code>.

A visibility constraint in the external module must be understood and evaluated whether or not its
truthiness can be statically determined.

### Statically known branches in the external module

```toml
[environment]
python-version = "3.11"
```

`exporter.py`:

```py
import sys

if sys.version_info >= (3, 11):
    X: bool = True
else:
    Y: bool = False
    Z: int = 42
```

`importer.py`:

```py
import sys

Z: bool = True

from exporter import *

reveal_type(X)  # revealed: bool

# error: [unresolved-reference]
reveal_type(Y)  # revealed: Unknown

# The `*` import is not considered a redefinition
# of the global variable `Z` in this module, as the symbol in
# the `a` module is in a branch that is statically known
# to be dead code given the `python-version` configuration.
# Thus this still reveals `Literal[True]`.
reveal_type(Z)  # revealed: Literal[True]
```

### Multiple `*` imports with always-false visibility constraints

Our understanding of visibility constraints in an external module remains accurate, even if there
are multiple `*` imports from that module.

```toml
[environment]
python-version = "3.11"
```

`exporter.py`:

```py
import sys

if sys.version_info >= (3, 12):
    Z: str = "foo"
```

`importer.py`:

```py
Z = True

from exporter import *
from exporter import *
from exporter import *

reveal_type(Z)  # revealed: Literal[True]
```

### Ambiguous visibility constraints

Some constraints in the external module may resolve to an "ambiguous truthiness". For these, we
should emit `possibly-unresolved-reference` diagnostics when they are used in the module in which
the `*` import occurs.

`exporter.py`:

```py
def coinflip() -> bool:
    return True

if coinflip():
    A = 1
    B = 2
else:
    B = 3
```

`importer.py`:

```py
from exporter import *

# error: [possibly-unresolved-reference]
reveal_type(A)  # revealed: Unknown | Literal[1]

reveal_type(B)  # revealed: Unknown | Literal[2, 3]
```

### Visibility constraints in the importing module

`exporter.py`:

```py
A = 1
```

`importer.py`:

```py
def coinflip() -> bool:
    return True

if coinflip():
    from exporter import *

# error: [possibly-unresolved-reference]
reveal_type(A)  # revealed: Unknown | Literal[1]
```

### Visibility constraints in the exporting module *and* the importing module

```toml
[environment]
python-version = "3.11"
```

`exporter.py`:

```py
import sys

if sys.version_info >= (3, 12):
    A: bool = True

def coinflip() -> bool:
    return True

if coinflip():
    B: bool = True
```

`importer.py`:

```py
import sys

if sys.version_info >= (3, 12):
    from exporter import *

    # it's correct to have no diagnostics here as this branch is unreachable
    reveal_type(A)  # revealed: Unknown
    reveal_type(B)  # revealed: bool
else:
    from exporter import *

    # error: [unresolved-reference]
    reveal_type(A)  # revealed: Unknown
    # error: [possibly-unresolved-reference]
    reveal_type(B)  # revealed: bool

# error: [unresolved-reference]
reveal_type(A)  # revealed: Unknown
# error: [possibly-unresolved-reference]
reveal_type(B)  # revealed: bool
```

## Relative `*` imports

Relative `*` imports are also supported by Python:

`a/__init__.py`:

```py
```

`a/foo.py`:

```py
X: bool = True
```

`a/bar.py`:

```py
from .foo import *

reveal_type(X)  # revealed: bool
```

## Star imports with `__all__`

If a module `x` contains `__all__`, all symbols included in `x.__all__` are imported by
`from x import *` (but no other symbols are).

### Simple tuple `__all__`

`exporter.py`:

```py
__all__ = ("X", "_private", "__protected", "__dunder__", "___thunder___")

X: bool = True
_private: bool = True
__protected: bool = True
__dunder__: bool = True
___thunder___: bool = True

Y: bool = False
```

`importer.py`:

```py
from exporter import *

reveal_type(X)  # revealed: bool

reveal_type(_private)  # revealed: bool
reveal_type(__protected)  # revealed: bool
reveal_type(__dunder__)  # revealed: bool
reveal_type(___thunder___)  # revealed: bool

# error: [unresolved-reference]
reveal_type(Y)  # revealed: Unknown
```

### Simple list `__all__`

`exporter.py`:

```py
__all__ = ["X"]

X: bool = True
Y: bool = False
```

`importer.py`:

```py
from exporter import *

reveal_type(X)  # revealed: bool

# error: [unresolved-reference]
reveal_type(Y)  # revealed: Unknown
```

### `__all__` with additions later on in the global scope

The
[typing spec](https://typing.python.org/en/latest/spec/distributing.html#library-interface-public-and-private-symbols)
lists certain modifications to `__all__` that must be understood by type checkers.

`a.py`:

```py
FOO: bool = True

__all__ = ["FOO"]
```

`b.py`:

```py
import a
from a import *

__all__ = ["A"]
__all__ += ["B"]
__all__.append("C")
__all__.extend(["D"])
__all__.extend(a.__all__)

A: bool = True
B: bool = True
C: bool = True
D: bool = True
E: bool = False
```

`c.py`:

```py
from b import *

reveal_type(A)  # revealed: bool
reveal_type(B)  # revealed: bool
reveal_type(C)  # revealed: bool
reveal_type(D)  # revealed: bool
reveal_type(FOO)  # revealed: bool

# error: [unresolved-reference]
reveal_type(E)  # revealed: Unknown
```

### `__all__` with subtractions later on in the global scope

Whereas there are many ways of adding to `__all__` that type checkers must support, there is only
one way of subtracting from `__all__` that type checkers are required to support:

`exporter.py`:

```py
__all__ = ["A", "B"]
__all__.remove("B")

A: bool = True
B: bool = True
```

`importer.py`:

```py
from exporter import *

reveal_type(A)  # revealed: bool

# error: [unresolved-reference]
reveal_type(B)  # revealed: Unknown
```

### Invalid `__all__`

If `a.__all__` contains a member that does not refer to a symbol with bindings in the global scope,
a wildcard import from module `a` will fail at runtime.

TODO: Should we:

1. Emit a diagnostic at the invalid definition of `__all__` (which will not fail at runtime)?
1. Emit a diagnostic at the star-import from the module with the invalid `__all__` (which *will*
    fail at runtime)?
1. Emit a diagnostic on both?

`exporter.py`:

```py
__all__ = ["a", "b"]

a = 42
```

`importer.py`:

```py
# TODO we should consider emitting a diagnostic here (see prose description above)
from exporter import *  # fails with `AttributeError: module 'foo' has no attribute 'b'` at runtime
```

### Dynamic `__all__`

If `__all__` contains members that are dynamically computed, we should check that all members of
`__all__` are assignable to `str`. For the purposes of evaluating `*` imports, however, we should
treat the module as though it has no `__all__` at all: all global-scope members of the module should
be considered imported by the import statement. We should probably also emit a warning telling the
user that we cannot statically determine the elements of `__all__`.

`exporter.py`:

```py
def f() -> str:
    return "f"

def g() -> int:
    return 42

# TODO we should emit a warning here for the dynamically constructed `__all__` member.
__all__ = [f()]
```

`importer.py`:

```py
from exporter import *

# At runtime, `f` is imported but `g` is not; to avoid false positives, however,
# we treat `a` as though it does not have `__all__` at all,
# which would imply that both symbols would be present.
reveal_type(f)  # revealed: def f() -> str
reveal_type(g)  # revealed: def g() -> int
```

### `__all__` conditionally defined in a statically known branch

```toml
[environment]
python-version = "3.11"
```

`exporter.py`:

```py
import sys

X: bool = True

if sys.version_info >= (3, 11):
    __all__ = ["X", "Y"]
    Y: bool = True
else:
    __all__ = ("Z",)
    Z: bool = True
```

`importer.py`:

```py
from exporter import *

reveal_type(X)  # revealed: bool
reveal_type(Y)  # revealed: bool

# error: [unresolved-reference]
reveal_type(Z)  # revealed: Unknown
```

### `__all__` conditionally defined in a statically known branch (2)

The same example again, but with a different `python-version` set:

```toml
[environment]
python-version = "3.10"
```

`exporter.py`:

```py
import sys

X: bool = True

if sys.version_info >= (3, 11):
    __all__ = ["X", "Y"]
    Y: bool = True
else:
    __all__ = ("Z",)
    Z: bool = True
```

`importer.py`:

```py
from exporter import *

# error: [unresolved-reference]
reveal_type(X)  # revealed: Unknown

# error: [unresolved-reference]
reveal_type(Y)  # revealed: Unknown

reveal_type(Z)  # revealed: bool
```

### `__all__` conditionally mutated in a statically known branch

```toml
[environment]
python-version = "3.11"
```

`exporter.py`:

```py
import sys

__all__ = []
X: bool = True

if sys.version_info >= (3, 11):
    __all__.extend(["X", "Y"])
    Y: bool = True
else:
    __all__.append("Z")
    Z: bool = True
```

`importer.py`:

```py
from exporter import *

reveal_type(X)  # revealed: bool
reveal_type(Y)  # revealed: bool

# error: [unresolved-reference]
reveal_type(Z)  # revealed: Unknown
```

### `__all__` conditionally mutated in a statically known branch (2)

The same example again, but with a different `python-version` set:

```toml
[environment]
python-version = "3.10"
```

`exporter.py`:

```py
import sys

__all__ = []
X: bool = True

if sys.version_info >= (3, 11):
    __all__.extend(["X", "Y"])
    Y: bool = True
else:
    __all__.append("Z")
    Z: bool = True
```

`importer.py`:

```py
from exporter import *

# error: [unresolved-reference]
reveal_type(X)  # revealed: Unknown

# error: [unresolved-reference]
reveal_type(Y)  # revealed: Unknown

reveal_type(Z)  # revealed: bool
```

### Empty `__all__`

An empty `__all__` is valid, but a `*` import from a module with an empty `__all__` results in 0
bindings being added from the import:

`a.py`:

```py
X: bool = True

__all__ = ()
```

`b.py`:

```py
Y: bool = True

__all__ = []
```

`c.py`:

```py
from a import *
from b import *

# error: [unresolved-reference]
reveal_type(X)  # revealed: Unknown

# error: [unresolved-reference]
reveal_type(Y)  # revealed: Unknown
```

### `__all__` in a stub file

If a name is included in `__all__` in a stub file, it is considered re-exported even if it was only
defined using an import without the explicit `from foo import X as X` syntax:

`a.pyi`:

```pyi
X: bool = True
Y: bool = True
```

`b.pyi`:

```pyi
from a import X, Y

__all__ = ["X", "Z"]

Z: bool = True

Nope: bool = True
```

`c.py`:

```py
from b import *

# `X` is re-exported from `b.pyi` due to presence in `__all__`
reveal_type(X)  # revealed: bool

# This diagnostic is accurate: `Y` does not use the "redundant alias" convention in `b.pyi`,
# nor is it included in `b.__all__`, so it is not exported from `b.pyi`. It would still be
# an error if it used the "redundant alias" convention as `__all__` would take precedence.
#
# error: [unresolved-reference]
reveal_type(Y)  # revealed: Unknown

# `Z` is defined in `b.pyi` and included in `__all__`
reveal_type(Z)  # revealed: bool

# error: [unresolved-reference]
reveal_type(Nope)  # revealed: Unknown
```

## `global` statements in non-global scopes

A `global` statement in a nested function scope, combined with a definition in the same function
scope of the name that was declared `global`, can add a symbol to the global namespace.

`a.py`:

```py
def f():
    global g, h

    g: bool = True

f()
```

`b.py`:

```py
from a import *

reveal_type(f)  # revealed: def f() -> Unknown

# TODO: we're undecided about whether we should consider this a false positive or not.
# Mutating the global scope to add a symbol from an inner scope will not *necessarily* result
# in the symbol being bound from the perspective of other modules (the function that creates
# the inner scope, and adds the symbol to the global scope, might never be called!)
# See discussion in https://github.com/astral-sh/ruff/pull/16959
#
# error: [unresolved-reference]
reveal_type(g)  # revealed: Unknown

# this diagnostic is accurate, though!
# error: [unresolved-reference]
reveal_type(h)  # revealed: Unknown
```

## Cyclic star imports

Believe it or not, this code does *not* raise an exception at runtime!

`a.py`:

```py
from b import *

A: bool = True
```

`b.py`:

```py
from a import *

B: bool = True
```

`c.py`:

```py
from a import *

reveal_type(A)  # revealed: bool
reveal_type(B)  # revealed: bool
```

## Integration test: `collections.abc`

The `collections.abc` standard-library module provides a good integration test, as all its symbols
are present due to `*` imports.

```py
import collections.abc

reveal_type(collections.abc.Sequence)  # revealed: <class 'Sequence'>
reveal_type(collections.abc.Callable)  # revealed: typing.Callable
reveal_type(collections.abc.Set)  # revealed: <class 'AbstractSet'>
```

## Invalid `*` imports

### Unresolved module

If the module is unresolved, we emit a diagnostic just like for any other unresolved import:

```py
# TODO: not a great error message
from foo import *  # error: [unresolved-import] "Cannot resolve imported module `foo`"
```

### Nested scope

A `*` import in a nested scope are always a syntax error. Ty does not infer any bindings from them:

`exporter.py`:

```py
X: bool = True
```

`importer.py`:

```py
def f():
    # TODO: we should emit a syntax error here (tracked by https://github.com/astral-sh/ruff/issues/17412)
    from exporter import *

    # error: [unresolved-reference]
    reveal_type(X)  # revealed: Unknown
```

### `*` combined with other aliases in the list

`a.py`:

```py
X: bool = True
_Y: bool = False
_Z: bool = True
```

`b.py`:

<!-- blacken-docs:off -->

```py
from a import *, _Y  # error: [invalid-syntax]

# The import statement above is invalid syntax,
# but it's pretty obvious that the user wanted to do a `*` import,
# so we import all public names from `a` anyway, to minimize cascading errors
reveal_type(X)  # revealed: bool
reveal_type(_Y)  # revealed: bool
```

These tests are more to assert that we don't panic on these various kinds of invalid syntax than
anything else:

`c.py`:

```py
from a import *, _Y  # error: [invalid-syntax]
from a import _Y, *, _Z  # error: [invalid-syntax]
from a import *, _Y as fooo  # error: [invalid-syntax]
from a import *, *, _Y  # error: [invalid-syntax]
```

<!-- blacken-docs:on -->

[python language reference for import statements]: https://docs.python.org/3/reference/simple_stmts.html#the-import-statement
