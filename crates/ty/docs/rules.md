<!-- WARNING: This file is auto-generated (cargo dev generate-all). Edit the lint-declarations in 'crates/ty_python_semantic/src/types/diagnostic.rs' if you want to change anything here. -->

# Rules

## `ambiguous-protocol-member`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'warn'."><code>warn</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.20">0.0.1-alpha.20</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20ambiguous-protocol-member" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L538" target="_blank">View source</a>
</small>


**What it does**

Checks for protocol classes with members that will lead to ambiguous interfaces.

**Why is this bad?**

Assigning to an undeclared variable in a protocol class leads to an ambiguous
interface which may lead to the type checker inferring unexpected things. It's
recommended to ensure that all members of a protocol class are explicitly declared.

**Examples**


```py
from typing import Protocol

class BaseProto(Protocol):
    a: int                               # fine (explicitly declared as `int`)
    def method_member(self) -> int: ...  # fine: a method definition using `def` is considered a declaration
    c = "some variable"                  # error: no explicit declaration, leading to ambiguity
    b = method_member                    # error: no explicit declaration, leading to ambiguity

    # error: this creates implicit assignments of `d` and `e` in the protocol class body.
    # Were they really meant to be considered protocol members?
    for d, e in enumerate(range(42)):
        pass

class SubProto(BaseProto, Protocol):
    a = 42  # fine (declared in superclass)
```

## `byte-string-type-annotation`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.1">0.0.1-alpha.1</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20byte-string-type-annotation" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fstring_annotation.rs#L36" target="_blank">View source</a>
</small>


**What it does**

Checks for byte-strings in type annotation positions.

**Why is this bad?**

Static analysis tools like ty can't analyze type annotations that use byte-string notation.

**Examples**

```python
def test(): -> b"int":
    ...
```

Use instead:
```python
def test(): -> "int":
    ...
```

## `call-non-callable`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.1">0.0.1-alpha.1</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20call-non-callable" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L137" target="_blank">View source</a>
</small>


**What it does**

Checks for calls to non-callable objects.

**Why is this bad?**

Calling a non-callable object will raise a `TypeError` at runtime.

**Examples**

```python
4()  # TypeError: 'int' object is not callable
```

## `call-top-callable`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.7">0.0.7</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20call-top-callable" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L155" target="_blank">View source</a>
</small>


**What it does**

Checks for calls to objects typed as `Top[Callable[..., T]]` (the infinite union of all
callable types with return type `T`).

**Why is this bad?**

When an object is narrowed to `Top[Callable[..., object]]` (e.g., via `callable(x)` or
`isinstance(x, Callable)`), we know the object is callable, but we don't know its
precise signature. This type represents the set of all possible callable types
(including, e.g., functions that take no arguments and functions that require arguments),
so no specific set of arguments can be guaranteed to be valid.

**Examples**

```python
def f(x: object):
    if callable(x):
        x()  # error: We know `x` is callable, but not what arguments it accepts
```

## `conflicting-argument-forms`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.1">0.0.1-alpha.1</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20conflicting-argument-forms" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L206" target="_blank">View source</a>
</small>


**What it does**

Checks whether an argument is used as both a value and a type form in a call.

**Why is this bad?**

Such calls have confusing semantics and often indicate a logic error.

**Examples**

```python
from typing import reveal_type
from ty_extensions import is_singleton

if flag:
    f = repr  # Expects a value
else:
    f = is_singleton  # Expects a type form

f(int)  # error
```

## `conflicting-declarations`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.1">0.0.1-alpha.1</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20conflicting-declarations" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L232" target="_blank">View source</a>
</small>


**What it does**

Checks whether a variable has been declared as two conflicting types.

**Why is this bad**

A variable with two conflicting declarations likely indicates a mistake.
Moreover, it could lead to incorrect or ill-defined type inference for
other code that relies on these variables.

**Examples**

```python
if b:
    a: int
else:
    a: str

a = 1
```

## `conflicting-metaclass`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.1">0.0.1-alpha.1</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20conflicting-metaclass" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L257" target="_blank">View source</a>
</small>


**What it does**

Checks for class definitions where the metaclass of the class
being created would not be a subclass of the metaclasses of
all the class's bases.

**Why is it bad?**

Such a class definition raises a `TypeError` at runtime.

**Examples**

```python
class M1(type): ...
class M2(type): ...
class A(metaclass=M1): ...
class B(metaclass=M2): ...

# TypeError: metaclass conflict
class C(A, B): ...
```

## `cyclic-class-definition`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.1">0.0.1-alpha.1</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20cyclic-class-definition" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L283" target="_blank">View source</a>
</small>


**What it does**

Checks for class definitions in stub files that inherit
(directly or indirectly) from themselves.

**Why is it bad?**

Although forward references are natively supported in stub files,
inheritance cycles are still disallowed, as it is impossible to
resolve a consistent [method resolution order] for a class that
inherits from itself.

**Examples**

```python
# foo.pyi
class A(B): ...
class B(A): ...
```

[method resolution order]: https://docs.python.org/3/glossary.html#term-method-resolution-order

## `cyclic-type-alias-definition`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Preview (since <a href="https://github.com/astral-sh/ty/releases/tag/1.0.0">1.0.0</a>) ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20cyclic-type-alias-definition" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L309" target="_blank">View source</a>
</small>


**What it does**

Checks for type alias definitions that (directly or mutually) refer to themselves.

**Why is it bad?**

Although it is permitted to define a recursive type alias, it is not meaningful
to have a type alias whose expansion can only result in itself, and is therefore not allowed.

**Examples**

```python
type Itself = Itself

type A = B
type B = A
```

## `deprecated`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'warn'."><code>warn</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.16">0.0.1-alpha.16</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20deprecated" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L353" target="_blank">View source</a>
</small>


**What it does**

Checks for uses of deprecated items

**Why is this bad?**

Deprecated items should no longer be used.

**Examples**

```python
@warnings.deprecated("use new_func instead")
def old_func(): ...

old_func()  # emits [deprecated] diagnostic
```

## `division-by-zero`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'ignore'."><code>ignore</code></a> ·
Preview (since <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.1">0.0.1-alpha.1</a>) ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20division-by-zero" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L331" target="_blank">View source</a>
</small>


**What it does**

It detects division by zero.

**Why is this bad?**

Dividing by zero raises a `ZeroDivisionError` at runtime.

**Rule status**

This rule is currently disabled by default because of the number of
false positives it can produce.

**Examples**

```python
5 / 0
```

## `duplicate-base`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.1">0.0.1-alpha.1</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20duplicate-base" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L374" target="_blank">View source</a>
</small>


**What it does**

Checks for class definitions with duplicate bases.

**Why is this bad?**

Class definitions with duplicate bases raise `TypeError` at runtime.

**Examples**

```python
class A: ...

# TypeError: duplicate base class
class B(A, A): ...
```

## `duplicate-kw-only`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.12">0.0.1-alpha.12</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20duplicate-kw-only" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L395" target="_blank">View source</a>
</small>


**What it does**

Checks for dataclass definitions with more than one field
annotated with `KW_ONLY`.

**Why is this bad?**

`dataclasses.KW_ONLY` is a special marker used to
emulate the `*` syntax in normal signatures.
It can only be used once per dataclass.

Attempting to annotate two different fields with
it will lead to a runtime error.

**Examples**

```python
from dataclasses import dataclass, KW_ONLY

@dataclass
class A:  # Crash at runtime
    b: int
    _1: KW_ONLY
    c: str
    _2: KW_ONLY
    d: bytes
```

## `escape-character-in-forward-annotation`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.1">0.0.1-alpha.1</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20escape-character-in-forward-annotation" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fstring_annotation.rs#L154" target="_blank">View source</a>
</small>


**What it does**

Checks for forward annotations that contain escape characters.

**Why is this bad?**

Static analysis tools like ty can't analyze type annotations that contain escape characters.

**Example**


```python
def foo() -> "intt\b": ...
```

## `fstring-type-annotation`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.1">0.0.1-alpha.1</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20fstring-type-annotation" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fstring_annotation.rs#L11" target="_blank">View source</a>
</small>


**What it does**

Checks for f-strings in type annotation positions.

**Why is this bad?**

Static analysis tools like ty can't analyze type annotations that use f-string notation.

**Examples**

```python
def test(): -> f"int":
    ...
```

Use instead:
```python
def test(): -> "int":
    ...
```

## `ignore-comment-unknown-rule`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'warn'."><code>warn</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.1">0.0.1-alpha.1</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20ignore-comment-unknown-rule" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Fsuppression.rs#L50" target="_blank">View source</a>
</small>


**What it does**

Checks for `ty: ignore[code]` where `code` isn't a known lint rule.

**Why is this bad?**

A `ty: ignore[code]` directive with a `code` that doesn't match
any known rule will not suppress any type errors, and is probably a mistake.

**Examples**

```py
a = 20 / 0  # ty: ignore[division-by-zer]
```

Use instead:

```py
a = 20 / 0  # ty: ignore[division-by-zero]
```

## `implicit-concatenated-string-type-annotation`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.1">0.0.1-alpha.1</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20implicit-concatenated-string-type-annotation" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fstring_annotation.rs#L86" target="_blank">View source</a>
</small>


**What it does**

Checks for implicit concatenated strings in type annotation positions.

**Why is this bad?**

Static analysis tools like ty can't analyze type annotations that use implicit concatenated strings.

**Examples**

```python
def test(): -> "Literal[" "5" "]":
    ...
```

Use instead:
```python
def test(): -> "Literal[5]":
    ...
```

## `inconsistent-mro`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.1">0.0.1-alpha.1</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20inconsistent-mro" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L621" target="_blank">View source</a>
</small>


**What it does**

Checks for classes with an inconsistent [method resolution order] (MRO).

**Why is this bad?**

Classes with an inconsistent MRO will raise a `TypeError` at runtime.

**Examples**

```python
class A: ...
class B(A): ...

# TypeError: Cannot create a consistent method resolution order
class C(A, B): ...
```

[method resolution order]: https://docs.python.org/3/glossary.html#term-method-resolution-order

## `index-out-of-bounds`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.1">0.0.1-alpha.1</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20index-out-of-bounds" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L645" target="_blank">View source</a>
</small>


**What it does**

Checks for attempts to use an out of bounds index to get an item from
a container.

**Why is this bad?**

Using an out of bounds index will raise an `IndexError` at runtime.

**Examples**

```python
t = (0, 1, 2)
t[3]  # IndexError: tuple index out of range
```

## `instance-layout-conflict`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.12">0.0.1-alpha.12</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20instance-layout-conflict" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L427" target="_blank">View source</a>
</small>


**What it does**

Checks for classes definitions which will fail at runtime due to
"instance memory layout conflicts".

This error is usually caused by attempting to combine multiple classes
that define non-empty `__slots__` in a class's [Method Resolution Order]
(MRO), or by attempting to combine multiple builtin classes in a class's
MRO.

**Why is this bad?**

Inheriting from bases with conflicting instance memory layouts
will lead to a `TypeError` at runtime.

An instance memory layout conflict occurs when CPython cannot determine
the memory layout instances of a class should have, because the instance
memory layout of one of its bases conflicts with the instance memory layout
of one or more of its other bases.

For example, if a Python class defines non-empty `__slots__`, this will
impact the memory layout of instances of that class. Multiple inheritance
from more than one different class defining non-empty `__slots__` is not
allowed:

```python
class A:
    __slots__ = ("a", "b")

class B:
    __slots__ = ("a", "b")  # Even if the values are the same

# TypeError: multiple bases have instance lay-out conflict
class C(A, B): ...
```

An instance layout conflict can also be caused by attempting to use
multiple inheritance with two builtin classes, due to the way that these
classes are implemented in a CPython C extension:

```python
class A(int, float): ...  # TypeError: multiple bases have instance lay-out conflict
```

Note that pure-Python classes with no `__slots__`, or pure-Python classes
with empty `__slots__`, are always compatible:

```python
class A: ...
class B:
    __slots__ = ()
class C:
    __slots__ = ("a", "b")

# fine
class D(A, B, C): ...
```

**Known problems**

Classes that have "dynamic" definitions of `__slots__` (definitions do not consist
of string literals, or tuples of string literals) are not currently considered disjoint
bases by ty.

Additionally, this check is not exhaustive: many C extensions (including several in
the standard library) define classes that use extended memory layouts and thus cannot
coexist in a single MRO. Since it is currently not possible to represent this fact in
stub files, having a full knowledge of these classes is also impossible. When it comes
to classes that do not define `__slots__` at the Python level, therefore, ty, currently
only hard-codes a number of cases where it knows that a class will produce instances with
an atypical memory layout.

**Further reading**

- [CPython documentation: `__slots__`](https://docs.python.org/3/reference/datamodel.html#slots)
- [CPython documentation: Method Resolution Order](https://docs.python.org/3/glossary.html#term-method-resolution-order)

[Method Resolution Order]: https://docs.python.org/3/glossary.html#term-method-resolution-order

## `invalid-argument-type`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.1">0.0.1-alpha.1</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20invalid-argument-type" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L699" target="_blank">View source</a>
</small>


**What it does**

Detects call arguments whose type is not assignable to the corresponding typed parameter.

**Why is this bad?**

Passing an argument of a type the function (or callable object) does not accept violates
the expectations of the function author and may cause unexpected runtime errors within the
body of the function.

**Examples**

```python
def func(x: int): ...
func("foo")  # error: [invalid-argument-type]
```

## `invalid-assignment`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.1">0.0.1-alpha.1</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20invalid-assignment" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L739" target="_blank">View source</a>
</small>


**What it does**

Checks for assignments where the type of the value
is not [assignable to] the type of the assignee.

**Why is this bad?**

Such assignments break the rules of the type system and
weaken a type checker's ability to accurately reason about your code.

**Examples**

```python
a: int = ''
```

[assignable to]: https://typing.python.org/en/latest/spec/glossary.html#term-assignable

## `invalid-attribute-access`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.1">0.0.1-alpha.1</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20invalid-attribute-access" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L2042" target="_blank">View source</a>
</small>


**What it does**

Checks for assignments to class variables from instances
and assignments to instance variables from its class.

**Why is this bad?**

Incorrect assignments break the rules of the type system and
weaken a type checker's ability to accurately reason about your code.

**Examples**

```python
class C:
    class_var: ClassVar[int] = 1
    instance_var: int

C.class_var = 3  # okay
C().class_var = 3  # error: Cannot assign to class variable

C().instance_var = 3  # okay
C.instance_var = 3  # error: Cannot assign to instance variable
```

## `invalid-await`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.19">0.0.1-alpha.19</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20invalid-await" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L761" target="_blank">View source</a>
</small>


**What it does**

Checks for `await` being used with types that are not [Awaitable].

**Why is this bad?**

Such expressions will lead to `TypeError` being raised at runtime.

**Examples**

```python
import asyncio

class InvalidAwait:
    def __await__(self) -> int:
        return 5

async def main() -> None:
    await InvalidAwait()  # error: [invalid-await]
    await 42  # error: [invalid-await]

asyncio.run(main())
```

[Awaitable]: https://docs.python.org/3/library/collections.abc.html#collections.abc.Awaitable

## `invalid-base`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.1">0.0.1-alpha.1</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20invalid-base" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L791" target="_blank">View source</a>
</small>


**What it does**

Checks for class definitions that have bases which are not instances of `type`.

**Why is this bad?**

Class definitions with bases like this will lead to `TypeError` being raised at runtime.

**Examples**

```python
class A(42): ...  # error: [invalid-base]
```

## `invalid-context-manager`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.1">0.0.1-alpha.1</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20invalid-context-manager" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L842" target="_blank">View source</a>
</small>


**What it does**

Checks for expressions used in `with` statements
that do not implement the context manager protocol.

**Why is this bad?**

Such a statement will raise `TypeError` at runtime.

**Examples**

```python
# TypeError: 'int' object does not support the context manager protocol
with 1:
    print(2)
```

## `invalid-declaration`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.1">0.0.1-alpha.1</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20invalid-declaration" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L863" target="_blank">View source</a>
</small>


**What it does**

Checks for declarations where the inferred type of an existing symbol
is not [assignable to] its post-hoc declared type.

**Why is this bad?**

Such declarations break the rules of the type system and
weaken a type checker's ability to accurately reason about your code.

**Examples**

```python
a = 1
a: str
```

[assignable to]: https://typing.python.org/en/latest/spec/glossary.html#term-assignable

## `invalid-exception-caught`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.1">0.0.1-alpha.1</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20invalid-exception-caught" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L886" target="_blank">View source</a>
</small>


**What it does**

Checks for exception handlers that catch non-exception classes.

**Why is this bad?**

Catching classes that do not inherit from `BaseException` will raise a `TypeError` at runtime.

**Example**

```python
try:
    1 / 0
except 1:
    ...
```

Use instead:
```python
try:
    1 / 0
except ZeroDivisionError:
    ...
```

**References**

- [Python documentation: except clause](https://docs.python.org/3/reference/compound_stmts.html#except-clause)
- [Python documentation: Built-in Exceptions](https://docs.python.org/3/library/exceptions.html#built-in-exceptions)

**Ruff rule**

 This rule corresponds to Ruff's [`except-with-non-exception-classes` (`B030`)](https://docs.astral.sh/ruff/rules/except-with-non-exception-classes)

## `invalid-explicit-override`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.28">0.0.1-alpha.28</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20invalid-explicit-override" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L1712" target="_blank">View source</a>
</small>


**What it does**

Checks for methods that are decorated with `@override` but do not override any method in a superclass.

**Why is this bad?**

Decorating a method with `@override` declares to the type checker that the intention is that it should
override a method from a superclass.

**Example**


```python
from typing import override

class A:
    @override
    def foo(self): ...  # Error raised here

class B(A):
    @override
    def ffooo(self): ...  # Error raised here

class C:
    @override
    def __repr__(self): ...  # fine: overrides `object.__repr__`

class D(A):
    @override
    def foo(self): ...  # fine: overrides `A.foo`
```

## `invalid-frozen-dataclass-subclass`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.35">0.0.1-alpha.35</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20invalid-frozen-dataclass-subclass" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L2268" target="_blank">View source</a>
</small>


**What it does**

Checks for dataclasses with invalid frozen inheritance:
- A frozen dataclass cannot inherit from a non-frozen dataclass.
- A non-frozen dataclass cannot inherit from a frozen dataclass.

**Why is this bad?**

Python raises a `TypeError` at runtime when either of these inheritance
patterns occurs.

**Example**


```python
from dataclasses import dataclass

@dataclass
class Base:
    x: int

@dataclass(frozen=True)
class Child(Base):  # Error raised here
    y: int

@dataclass(frozen=True)
class FrozenBase:
    x: int

@dataclass
class NonFrozenChild(FrozenBase):  # Error raised here
    y: int
```

## `invalid-generic-class`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.1">0.0.1-alpha.1</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20invalid-generic-class" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L922" target="_blank">View source</a>
</small>


**What it does**

Checks for the creation of invalid generic classes

**Why is this bad?**

There are several requirements that you must follow when defining a generic class.
Many of these result in `TypeError` being raised at runtime if they are violated.

**Examples**

```python
from typing_extensions import Generic, TypeVar

T = TypeVar("T")
U = TypeVar("U", default=int)

# error: class uses both PEP-695 syntax and legacy syntax
class C[U](Generic[T]): ...

# error: type parameter with default comes before type parameter without default
class D(Generic[U, T]): ...
```

**References**

- [Typing spec: Generics](https://typing.python.org/en/latest/spec/generics.html#introduction)

## `invalid-ignore-comment`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'warn'."><code>warn</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.1">0.0.1-alpha.1</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20invalid-ignore-comment" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Fsuppression.rs#L75" target="_blank">View source</a>
</small>


**What it does**

Checks for `type: ignore` and `ty: ignore` comments that are syntactically incorrect.

**Why is this bad?**

A syntactically incorrect ignore comment is probably a mistake and is useless.

**Examples**

```py
a = 20 / 0  # type: ignoree
```

Use instead:

```py
a = 20 / 0  # type: ignore
```

## `invalid-key`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.17">0.0.1-alpha.17</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20invalid-key" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L666" target="_blank">View source</a>
</small>


**What it does**

Checks for subscript accesses with invalid keys and `TypedDict` construction with an
unknown key.

**Why is this bad?**

Subscripting with an invalid key will raise a `KeyError` at runtime.

Creating a `TypedDict` with an unknown key is likely a mistake; if the `TypedDict` is
`closed=true` it also violates the expectations of the type.

**Examples**

```python
from typing import TypedDict

class Person(TypedDict):
    name: str
    age: int

alice = Person(name="Alice", age=30)
alice["height"]  # KeyError: 'height'

bob: Person = { "name": "Bob", "age": 30 }  # typo!

carol = Person(name="Carol", age=25)  # typo!
```

## `invalid-legacy-type-variable`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.1">0.0.1-alpha.1</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20invalid-legacy-type-variable" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L953" target="_blank">View source</a>
</small>


**What it does**

Checks for the creation of invalid legacy `TypeVar`s

**Why is this bad?**

There are several requirements that you must follow when creating a legacy `TypeVar`.

**Examples**

```python
from typing import TypeVar

T = TypeVar("T")  # okay
Q = TypeVar("S")  # error: TypeVar name must match the variable it's assigned to
T = TypeVar("T")  # error: TypeVars should not be redefined

# error: TypeVar must be immediately assigned to a variable
def f(t: TypeVar("U")): ...
```

**References**

- [Typing spec: Generics](https://typing.python.org/en/latest/spec/generics.html#introduction)

## `invalid-metaclass`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.1">0.0.1-alpha.1</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20invalid-metaclass" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L1050" target="_blank">View source</a>
</small>


**What it does**

Checks for arguments to `metaclass=` that are invalid.

**Why is this bad?**

Python allows arbitrary expressions to be used as the argument to `metaclass=`.
These expressions, however, need to be callable and accept the same arguments
as `type.__new__`.

**Example**


```python
def f(): ...

# TypeError: f() takes 0 positional arguments but 3 were given
class B(metaclass=f): ...
```

**References**

- [Python documentation: Metaclasses](https://docs.python.org/3/reference/datamodel.html#metaclasses)

## `invalid-method-override`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.20">0.0.1-alpha.20</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20invalid-method-override" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L2170" target="_blank">View source</a>
</small>


**What it does**

Detects method overrides that violate the [Liskov Substitution Principle] ("LSP").

The LSP states that an instance of a subtype should be substitutable for an instance of its supertype.
Applied to Python, this means:
1. All argument combinations a superclass method accepts
   must also be accepted by an overriding subclass method.
2. The return type of an overriding subclass method must be a subtype
   of the return type of the superclass method.

**Why is this bad?**

Violating the Liskov Substitution Principle will lead to many of ty's assumptions and
inferences being incorrect, which will mean that it will fail to catch many possible
type errors in your code.

**Example**

```python
class Super:
    def method(self, x) -> int:
        return 42

class Sub(Super):
    # Liskov violation: `str` is not a subtype of `int`,
    # but the supertype method promises to return an `int`.
    def method(self, x) -> str:  # error: [invalid-override]
        return "foo"

def accepts_super(s: Super) -> int:
    return s.method(x=42)

accepts_super(Sub())  # The result of this call is a string, but ty will infer
                      # it to be an `int` due to the violation of the Liskov Substitution Principle.

class Sub2(Super):
    # Liskov violation: the superclass method can be called with a `x=`
    # keyword argument, but the subclass method does not accept it.
    def method(self, y) -> int:  # error: [invalid-override]
       return 42

accepts_super(Sub2())  # TypeError at runtime: method() got an unexpected keyword argument 'x'
                       # ty cannot catch this error due to the violation of the Liskov Substitution Principle.
```

**Common issues**


**Why does ty complain about my `__eq__` method?**


`__eq__` and `__ne__` methods in Python are generally expected to accept arbitrary
objects as their second argument, for example:

```python
class A:
    x: int

    def __eq__(self, other: object) -> bool:
        # gracefully handle an object of an unexpected type
        # without raising an exception
        if not isinstance(other, A):
            return False
        return self.x == other.x
```

If `A.__eq__` here were annotated as only accepting `A` instances for its second argument,
it would imply that you wouldn't be able to use `==` between instances of `A` and
instances of unrelated classes without an exception possibly being raised. While some
classes in Python do indeed behave this way, the strongly held convention is that it should
be avoided wherever possible. As part of this check, therefore, ty enforces that `__eq__`
and `__ne__` methods accept `object` as their second argument.

**Why does ty disagree with Ruff about how to write my method?**


Ruff has several rules that will encourage you to rename a parameter, or change its type
signature, if it thinks you're falling into a certain anti-pattern. For example, Ruff's
[ARG002](https://docs.astral.sh/ruff/rules/unused-method-argument/) rule recommends that an
unused parameter should either be removed or renamed to start with `_`. Applying either of
these suggestions can cause ty to start reporting an `invalid-method-override` error if
the function in question is a method on a subclass that overrides a method on a superclass,
and the change would cause the subclass method to no longer accept all argument combinations
that the superclass method accepts.

This can usually be resolved by adding [`@typing.override`][override] to your method
definition. Ruff knows that a method decorated with `@typing.override` is intended to
override a method by the same name on a superclass, and avoids reporting rules like ARG002
for such methods; it knows that the changes recommended by ARG002 would violate the Liskov
Substitution Principle.

Correct use of `@override` is enforced by ty's `invalid-explicit-override` rule.

[Liskov Substitution Principle]: https://en.wikipedia.org/wiki/Liskov_substitution_principle
[override]: https://docs.python.org/3/library/typing.html#typing.override

## `invalid-named-tuple`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.19">0.0.1-alpha.19</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20invalid-named-tuple" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L573" target="_blank">View source</a>
</small>


**What it does**

Checks for invalidly defined `NamedTuple` classes.

**Why is this bad?**

An invalidly defined `NamedTuple` class may lead to the type checker
drawing incorrect conclusions. It may also lead to `TypeError`s or
`AttributeError`s at runtime.

**Examples**

A class definition cannot combine `NamedTuple` with other base classes
in multiple inheritance; doing so raises a `TypeError` at runtime. The sole
exception to this rule is `Generic[]`, which can be used alongside `NamedTuple`
in a class's bases list.

```pycon
>>> from typing import NamedTuple
>>> class Foo(NamedTuple, object): ...
TypeError: can only inherit from a NamedTuple type and Generic
```

Further, `NamedTuple` field names cannot start with an underscore:

```pycon
>>> from typing import NamedTuple
>>> class Foo(NamedTuple):
...     _bar: int
ValueError: Field names cannot start with an underscore: '_bar'
```

`NamedTuple` classes also have certain synthesized attributes (like `_asdict`, `_make`,
`_replace`, etc.) that cannot be overwritten. Attempting to assign to these attributes
without a type annotation will raise an `AttributeError` at runtime.

```pycon
>>> from typing import NamedTuple
>>> class Foo(NamedTuple):
...     x: int
...     _asdict = 42
AttributeError: Cannot overwrite NamedTuple attribute _asdict
```

## `invalid-newtype`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Preview (since <a href="https://github.com/astral-sh/ty/releases/tag/1.0.0">1.0.0</a>) ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20invalid-newtype" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L1026" target="_blank">View source</a>
</small>


**What it does**

Checks for the creation of invalid `NewType`s

**Why is this bad?**

There are several requirements that you must follow when creating a `NewType`.

**Examples**

```python
from typing import NewType

def get_name() -> str: ...

Foo = NewType("Foo", int)        # okay
Bar = NewType(get_name(), int)   # error: The first argument to `NewType` must be a string literal
Baz = NewType("Baz", int | str)  # error: invalid base for `typing.NewType`
```

## `invalid-overload`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.1">0.0.1-alpha.1</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20invalid-overload" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L1077" target="_blank">View source</a>
</small>


**What it does**

Checks for various invalid `@overload` usages.

**Why is this bad?**

The `@overload` decorator is used to define functions and methods that accepts different
combinations of arguments and return different types based on the arguments passed. This is
mainly beneficial for type checkers. But, if the `@overload` usage is invalid, the type
checker may not be able to provide correct type information.

**Example**


Defining only one overload:

```py
from typing import overload

@overload
def foo(x: int) -> int: ...
def foo(x: int | None) -> int | None:
    return x
```

Or, not providing an implementation for the overloaded definition:

```py
from typing import overload

@overload
def foo() -> None: ...
@overload
def foo(x: int) -> int: ...
```

**References**

- [Python documentation: `@overload`](https://docs.python.org/3/library/typing.html#typing.overload)

## `invalid-parameter-default`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.1">0.0.1-alpha.1</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20invalid-parameter-default" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L1176" target="_blank">View source</a>
</small>


**What it does**

Checks for default values that can't be
assigned to the parameter's annotated type.

**Why is this bad?**

This breaks the rules of the type system and
weakens a type checker's ability to accurately reason about your code.

**Examples**

```python
def f(a: int = ''): ...
```

## `invalid-paramspec`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.1">0.0.1-alpha.1</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20invalid-paramspec" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L981" target="_blank">View source</a>
</small>


**What it does**

Checks for the creation of invalid `ParamSpec`s

**Why is this bad?**

There are several requirements that you must follow when creating a `ParamSpec`.

**Examples**

```python
from typing import ParamSpec

P1 = ParamSpec("P1")  # okay
P2 = ParamSpec("S2")  # error: ParamSpec name must match the variable it's assigned to
```

**References**

- [Typing spec: ParamSpec](https://typing.python.org/en/latest/spec/generics.html#paramspec)

## `invalid-protocol`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.1">0.0.1-alpha.1</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20invalid-protocol" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L509" target="_blank">View source</a>
</small>


**What it does**

Checks for protocol classes that will raise `TypeError` at runtime.

**Why is this bad?**

An invalidly defined protocol class may lead to the type checker inferring
unexpected things. It may also lead to `TypeError`s at runtime.

**Examples**

A `Protocol` class cannot inherit from a non-`Protocol` class;
this raises a `TypeError` at runtime:

```pycon
>>> from typing import Protocol
>>> class Foo(int, Protocol): ...
...
Traceback (most recent call last):
  File "<python-input-1>", line 1, in <module>
    class Foo(int, Protocol): ...
TypeError: Protocols can only inherit from other protocols, got <class 'int'>
```

## `invalid-raise`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.1">0.0.1-alpha.1</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20invalid-raise" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L1196" target="_blank">View source</a>
</small>


Checks for `raise` statements that raise non-exceptions or use invalid
causes for their raised exceptions.

**Why is this bad?**

Only subclasses or instances of `BaseException` can be raised.
For an exception's cause, the same rules apply, except that `None` is also
permitted. Violating these rules results in a `TypeError` at runtime.

**Examples**

```python
def f():
    try:
        something()
    except NameError:
        raise "oops!" from f

def g():
    raise NotImplemented from 42
```

Use instead:
```python
def f():
    try:
        something()
    except NameError as e:
        raise RuntimeError("oops!") from e

def g():
    raise NotImplementedError from None
```

**References**

- [Python documentation: The `raise` statement](https://docs.python.org/3/reference/simple_stmts.html#raise)
- [Python documentation: Built-in Exceptions](https://docs.python.org/3/library/exceptions.html#built-in-exceptions)

## `invalid-return-type`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.1">0.0.1-alpha.1</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20invalid-return-type" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L720" target="_blank">View source</a>
</small>


**What it does**

Detects returned values that can't be assigned to the function's annotated return type.

**Why is this bad?**

Returning an object of a type incompatible with the annotated return type may cause confusion to the user calling the function.

**Examples**

```python
def func() -> int:
    return "a"  # error: [invalid-return-type]
```

## `invalid-super-argument`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.1">0.0.1-alpha.1</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20invalid-super-argument" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L1239" target="_blank">View source</a>
</small>


**What it does**

Detects `super()` calls where:
- the first argument is not a valid class literal, or
- the second argument is not an instance or subclass of the first argument.

**Why is this bad?**

`super(type, obj)` expects:
- the first argument to be a class,
- and the second argument to satisfy one of the following:
  - `isinstance(obj, type)` is `True`
  - `issubclass(obj, type)` is `True`

Violating this relationship will raise a `TypeError` at runtime.

**Examples**

```python
class A:
    ...
class B(A):
    ...

super(A, B())  # it's okay! `A` satisfies `isinstance(B(), A)`

super(A(), B()) # error: `A()` is not a class

super(B, A())  # error: `A()` does not satisfy `isinstance(A(), B)`
super(B, A)  # error: `A` does not satisfy `issubclass(A, B)`
```

**References**

- [Python documentation: super()](https://docs.python.org/3/library/functions.html#super)

## `invalid-syntax-in-forward-annotation`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.1">0.0.1-alpha.1</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20invalid-syntax-in-forward-annotation" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fstring_annotation.rs#L111" target="_blank">View source</a>
</small>


**What it does**

Checks for string-literal annotations where the string cannot be
parsed as a Python expression.

**Why is this bad?**

Type annotations are expected to be Python expressions that
describe the expected type of a variable, parameter, attribute or
`return` statement.

Type annotations are permitted to be string-literal expressions, in
order to enable forward references to names not yet defined.
However, it must be possible to parse the contents of that string
literal as a normal Python expression.

**Example**


```python
def foo() -> "intstance of C":
    return 42

class C: ...
```

Use instead:

```python
def foo() -> "C":
    return 42

class C: ...
```

**References**

- [Typing spec: The meaning of annotations](https://typing.python.org/en/latest/spec/annotations.html#the-meaning-of-annotations)
- [Typing spec: String annotations](https://typing.python.org/en/latest/spec/annotations.html#string-annotations)

## `invalid-type-alias-type`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.6">0.0.1-alpha.6</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20invalid-type-alias-type" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L1005" target="_blank">View source</a>
</small>


**What it does**

Checks for the creation of invalid `TypeAliasType`s

**Why is this bad?**

There are several requirements that you must follow when creating a `TypeAliasType`.

**Examples**

```python
from typing import TypeAliasType

IntOrStr = TypeAliasType("IntOrStr", int | str)  # okay
NewAlias = TypeAliasType(get_name(), int)        # error: TypeAliasType name must be a string literal
```

## `invalid-type-arguments`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.29">0.0.1-alpha.29</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20invalid-type-arguments" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L1471" target="_blank">View source</a>
</small>


**What it does**

Checks for invalid type arguments in explicit type specialization.

**Why is this bad?**

Providing the wrong number of type arguments or type arguments that don't
satisfy the type variable's bounds or constraints will lead to incorrect
type inference and may indicate a misunderstanding of the generic type's
interface.

**Examples**


Using legacy type variables:
```python
from typing import Generic, TypeVar

T1 = TypeVar('T1', int, str)
T2 = TypeVar('T2', bound=int)

class Foo1(Generic[T1]): ...
class Foo2(Generic[T2]): ...

Foo1[bytes]  # error: bytes does not satisfy T1's constraints
Foo2[str]  # error: str does not satisfy T2's bound
```

Using PEP 695 type variables:
```python
class Foo[T]: ...
class Bar[T, U]: ...

Foo[int, str]  # error: too many arguments
Bar[int]  # error: too few arguments
```

## `invalid-type-checking-constant`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.1">0.0.1-alpha.1</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20invalid-type-checking-constant" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L1278" target="_blank">View source</a>
</small>


**What it does**

Checks for a value other than `False` assigned to the `TYPE_CHECKING` variable, or an
annotation not assignable from `bool`.

**Why is this bad?**

The name `TYPE_CHECKING` is reserved for a flag that can be used to provide conditional
code seen only by the type checker, and not at runtime. Normally this flag is imported from
`typing` or `typing_extensions`, but it can also be defined locally. If defined locally, it
must be assigned the value `False` at runtime; the type checker will consider its value to
be `True`. If annotated, it must be annotated as a type that can accept `bool` values.

**Examples**

```python
TYPE_CHECKING: str
TYPE_CHECKING = ''
```

## `invalid-type-form`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.1">0.0.1-alpha.1</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20invalid-type-form" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L1302" target="_blank">View source</a>
</small>


**What it does**

Checks for expressions that are used as [type expressions]
but cannot validly be interpreted as such.

**Why is this bad?**

Such expressions cannot be understood by ty.
In some cases, they might raise errors at runtime.

**Examples**

```python
from typing import Annotated

a: type[1]  # `1` is not a type
b: Annotated[int]  # `Annotated` expects at least two arguments
```
[type expressions]: https://typing.python.org/en/latest/spec/annotations.html#type-and-annotation-expressions

## `invalid-type-guard-call`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.11">0.0.1-alpha.11</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20invalid-type-guard-call" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L1354" target="_blank">View source</a>
</small>


**What it does**

Checks for type guard function calls without a valid target.

**Why is this bad?**

The first non-keyword non-variadic argument to a type guard function
is its target and must map to a symbol.

Starred (`is_str(*a)`), literal (`is_str(42)`) and other non-symbol-like
expressions are invalid as narrowing targets.

**Examples**

```python
from typing import TypeIs

def f(v: object) -> TypeIs[int]: ...

f()  # Error
f(*a)  # Error
f(10)  # Error
```

## `invalid-type-guard-definition`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.11">0.0.1-alpha.11</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20invalid-type-guard-definition" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L1326" target="_blank">View source</a>
</small>


**What it does**

Checks for type guard functions without
a first non-self-like non-keyword-only non-variadic parameter.

**Why is this bad?**

Type narrowing functions must accept at least one positional argument
(non-static methods must accept another in addition to `self`/`cls`).

Extra parameters/arguments are allowed but do not affect narrowing.

**Examples**

```python
from typing import TypeIs

def f() -> TypeIs[int]: ...  # Error, no parameter
def f(*, v: object) -> TypeIs[int]: ...  # Error, no positional arguments allowed
def f(*args: object) -> TypeIs[int]: ... # Error, expect variadic arguments
class C:
    def f(self) -> TypeIs[int]: ...  # Error, only positional argument expected is `self`
```

## `invalid-type-variable-constraints`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.1">0.0.1-alpha.1</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20invalid-type-variable-constraints" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L1382" target="_blank">View source</a>
</small>


**What it does**

Checks for constrained [type variables] with only one constraint.

**Why is this bad?**

A constrained type variable must have at least two constraints.

**Examples**

```python
from typing import TypeVar

T = TypeVar('T', str)  # invalid constrained TypeVar
```

Use instead:
```python
T = TypeVar('T', str, int)  # valid constrained TypeVar
# or
T = TypeVar('T', bound=str)  # valid bound TypeVar
```

[type variables]: https://docs.python.org/3/library/typing.html#typing.TypeVar

## `missing-argument`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.1">0.0.1-alpha.1</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20missing-argument" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L1411" target="_blank">View source</a>
</small>


**What it does**

Checks for missing required arguments in a call.

**Why is this bad?**

Failing to provide a required argument will raise a `TypeError` at runtime.

**Examples**

```python
def func(x: int): ...
func()  # TypeError: func() missing 1 required positional argument: 'x'
```

## `missing-typed-dict-key`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.20">0.0.1-alpha.20</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20missing-typed-dict-key" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L2143" target="_blank">View source</a>
</small>


**What it does**

Detects missing required keys in `TypedDict` constructor calls.

**Why is this bad?**

`TypedDict` requires all non-optional keys to be provided during construction.
Missing items can lead to a `KeyError` at runtime.

**Example**

```python
from typing import TypedDict

class Person(TypedDict):
    name: str
    age: int

alice: Person = {"name": "Alice"}  # missing required key 'age'

alice["age"]  # KeyError
```

## `no-matching-overload`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.1">0.0.1-alpha.1</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20no-matching-overload" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L1430" target="_blank">View source</a>
</small>


**What it does**

Checks for calls to an overloaded function that do not match any of the overloads.

**Why is this bad?**

Failing to provide the correct arguments to one of the overloads will raise a `TypeError`
at runtime.

**Examples**

```python
@overload
def func(x: int): ...
@overload
def func(x: bool): ...
func("string")  # error: [no-matching-overload]
```

## `not-iterable`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.1">0.0.1-alpha.1</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20not-iterable" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L1512" target="_blank">View source</a>
</small>


**What it does**

Checks for objects that are not iterable but are used in a context that requires them to be.

**Why is this bad?**

Iterating over an object that is not iterable will raise a `TypeError` at runtime.

**Examples**


```python
for i in 34:  # TypeError: 'int' object is not iterable
    pass
```

## `not-subscriptable`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.1">0.0.1-alpha.1</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20not-subscriptable" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L1453" target="_blank">View source</a>
</small>


**What it does**

Checks for subscripting objects that do not support subscripting.

**Why is this bad?**

Subscripting an object that does not support it will raise a `TypeError` at runtime.

**Examples**

```python
4[1]  # TypeError: 'int' object is not subscriptable
```

## `override-of-final-method`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.29">0.0.1-alpha.29</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20override-of-final-method" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L1685" target="_blank">View source</a>
</small>


**What it does**

Checks for methods on subclasses that override superclass methods decorated with `@final`.

**Why is this bad?**

Decorating a method with `@final` declares to the type checker that it should not be
overridden on any subclass.

**Example**


```python
from typing import final

class A:
    @final
    def foo(self): ...

class B(A):
    def foo(self): ...  # Error raised here
```

## `parameter-already-assigned`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.1">0.0.1-alpha.1</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20parameter-already-assigned" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L1563" target="_blank">View source</a>
</small>


**What it does**

Checks for calls which provide more than one argument for a single parameter.

**Why is this bad?**

Providing multiple values for a single parameter will raise a `TypeError` at runtime.

**Examples**


```python
def f(x: int) -> int: ...

f(1, x=2)  # Error raised here
```

## `positional-only-parameter-as-kwarg`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.22">0.0.1-alpha.22</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20positional-only-parameter-as-kwarg" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L1896" target="_blank">View source</a>
</small>


**What it does**

Checks for keyword arguments in calls that match positional-only parameters of the callable.

**Why is this bad?**

Providing a positional-only parameter as a keyword argument will raise `TypeError` at runtime.

**Example**


```python
def f(x: int, /) -> int: ...

f(x=1)  # Error raised here
```

## `possibly-missing-attribute`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'warn'."><code>warn</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.22">0.0.1-alpha.22</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20possibly-missing-attribute" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L1584" target="_blank">View source</a>
</small>


**What it does**

Checks for possibly missing attributes.

**Why is this bad?**

Attempting to access a missing attribute will raise an `AttributeError` at runtime.

**Examples**

```python
class A:
    if b:
        c = 0

A.c  # AttributeError: type object 'A' has no attribute 'c'
```

## `possibly-missing-implicit-call`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'warn'."><code>warn</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.22">0.0.1-alpha.22</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20possibly-missing-implicit-call" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L180" target="_blank">View source</a>
</small>


**What it does**

Checks for implicit calls to possibly missing methods.

**Why is this bad?**

Expressions such as `x[y]` and `x * y` call methods
under the hood (`__getitem__` and `__mul__` respectively).
Calling a missing method will raise an `AttributeError` at runtime.

**Examples**

```python
import datetime

class A:
    if datetime.date.today().weekday() != 6:
        def __getitem__(self, v): ...

A()[0]  # TypeError: 'A' object is not subscriptable
```

## `possibly-missing-import`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'ignore'."><code>ignore</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.22">0.0.1-alpha.22</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20possibly-missing-import" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L1606" target="_blank">View source</a>
</small>


**What it does**

Checks for imports of symbols that may be missing.

**Why is this bad?**

Importing a missing module or name will raise a `ModuleNotFoundError`
or `ImportError` at runtime.

**Rule status**

This rule is currently disabled by default because of the number of
false positives it can produce.

**Examples**

```python
# module.py
import datetime

if datetime.date.today().weekday() != 6:
    a = 1

# main.py
from module import a  # ImportError: cannot import name 'a' from 'module'
```

## `possibly-unresolved-reference`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'ignore'."><code>ignore</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.1">0.0.1-alpha.1</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20possibly-unresolved-reference" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L1636" target="_blank">View source</a>
</small>


**What it does**

Checks for references to names that are possibly not defined.

**Why is this bad?**

Using an undefined variable will raise a `NameError` at runtime.

**Rule status**

This rule is currently disabled by default because of the number of
false positives it can produce.

**Example**


```python
for i in range(0):
    x = i

print(x)  # NameError: name 'x' is not defined
```

## `raw-string-type-annotation`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.1">0.0.1-alpha.1</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20raw-string-type-annotation" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fstring_annotation.rs#L61" target="_blank">View source</a>
</small>


**What it does**

Checks for raw-strings in type annotation positions.

**Why is this bad?**

Static analysis tools like ty can't analyze type annotations that use raw-string notation.

**Examples**

```python
def test(): -> r"int":
    ...
```

Use instead:
```python
def test(): -> "int":
    ...
```

## `redundant-cast`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'warn'."><code>warn</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.1">0.0.1-alpha.1</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20redundant-cast" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L2070" target="_blank">View source</a>
</small>


**What it does**

Detects redundant `cast` calls where the value already has the target type.

**Why is this bad?**

These casts have no effect and can be removed.

**Example**

```python
def f() -> int:
    return 10

cast(int, f())  # Redundant
```

## `static-assert-error`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.1">0.0.1-alpha.1</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20static-assert-error" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L2018" target="_blank">View source</a>
</small>


**What it does**

Makes sure that the argument of `static_assert` is statically known to be true.

**Why is this bad?**

A `static_assert` call represents an explicit request from the user
for the type checker to emit an error if the argument cannot be verified
to evaluate to `True` in a boolean context.

**Examples**

```python
from ty_extensions import static_assert

static_assert(1 + 1 == 3)  # error: evaluates to `False`

static_assert(int(2.0 * 3.0) == 6)  # error: does not have a statically known truthiness
```

## `subclass-of-final-class`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.1">0.0.1-alpha.1</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20subclass-of-final-class" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L1662" target="_blank">View source</a>
</small>


**What it does**

Checks for classes that subclass final classes.

**Why is this bad?**

Decorating a class with `@final` declares to the type checker that it should not be subclassed.

**Example**


```python
from typing import final

@final
class A: ...
class B(A): ...  # Error raised here
```

## `super-call-in-named-tuple-method`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Preview (since <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.30">0.0.1-alpha.30</a>) ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20super-call-in-named-tuple-method" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L1830" target="_blank">View source</a>
</small>


**What it does**

Checks for calls to `super()` inside methods of `NamedTuple` classes.

**Why is this bad?**

Using `super()` in a method of a `NamedTuple` class will raise an exception at runtime.

**Examples**

```python
from typing import NamedTuple

class F(NamedTuple):
    x: int

    def method(self):
        super()  # error: super() is not supported in methods of NamedTuple classes
```

**References**

- [Python documentation: super()](https://docs.python.org/3/library/functions.html#super)

## `too-many-positional-arguments`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.1">0.0.1-alpha.1</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20too-many-positional-arguments" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L1770" target="_blank">View source</a>
</small>


**What it does**

Checks for calls that pass more positional arguments than the callable can accept.

**Why is this bad?**

Passing too many positional arguments will raise `TypeError` at runtime.

**Example**


```python
def f(): ...

f("foo")  # Error raised here
```

## `type-assertion-failure`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.1">0.0.1-alpha.1</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20type-assertion-failure" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L1748" target="_blank">View source</a>
</small>


**What it does**

Checks for `assert_type()` and `assert_never()` calls where the actual type
is not the same as the asserted type.

**Why is this bad?**

`assert_type()` allows confirming the inferred type of a certain value.

**Example**


```python
def _(x: int):
    assert_type(x, int)  # fine
    assert_type(x, str)  # error: Actual type does not match asserted type
```

## `unavailable-implicit-super-arguments`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.1">0.0.1-alpha.1</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20unavailable-implicit-super-arguments" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L1791" target="_blank">View source</a>
</small>


**What it does**

Detects invalid `super()` calls where implicit arguments like the enclosing class or first method argument are unavailable.

**Why is this bad?**

When `super()` is used without arguments, Python tries to find two things:
the nearest enclosing class and the first argument of the immediately enclosing function (typically self or cls).
If either of these is missing, the call will fail at runtime with a `RuntimeError`.

**Examples**

```python
super()  # error: no enclosing class or function found

def func():
    super()  # error: no enclosing class or first argument exists

class A:
    f = super()  # error: no enclosing function to provide the first argument

    def method(self):
        def nested():
            super()  # error: first argument does not exist in this nested function

        lambda: super()  # error: first argument does not exist in this lambda

        (super() for _ in range(10))  # error: argument is not available in generator expression

        super()  # okay! both enclosing class and first argument are available
```

**References**

- [Python documentation: super()](https://docs.python.org/3/library/functions.html#super)

## `undefined-reveal`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'warn'."><code>warn</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.1">0.0.1-alpha.1</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20undefined-reveal" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L1857" target="_blank">View source</a>
</small>


**What it does**

Checks for calls to `reveal_type` without importing it.

**Why is this bad?**

Using `reveal_type` without importing it will raise a `NameError` at runtime.

**Examples**

```python
reveal_type(1)  # NameError: name 'reveal_type' is not defined
```

## `unknown-argument`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.1">0.0.1-alpha.1</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20unknown-argument" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L1875" target="_blank">View source</a>
</small>


**What it does**

Checks for keyword arguments in calls that don't match any parameter of the callable.

**Why is this bad?**

Providing an unknown keyword argument will raise `TypeError` at runtime.

**Example**


```python
def f(x: int) -> int: ...

f(x=1, y=2)  # Error raised here
```

## `unresolved-attribute`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.1">0.0.1-alpha.1</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20unresolved-attribute" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L1917" target="_blank">View source</a>
</small>


**What it does**

Checks for unresolved attributes.

**Why is this bad?**

Accessing an unbound attribute will raise an `AttributeError` at runtime.
An unresolved attribute is not guaranteed to exist from the type alone,
so this could also indicate that the object is not of the type that the user expects.

**Examples**

```python
class A: ...

A().foo  # AttributeError: 'A' object has no attribute 'foo'
```

## `unresolved-global`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'warn'."><code>warn</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.15">0.0.1-alpha.15</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20unresolved-global" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L2091" target="_blank">View source</a>
</small>


**What it does**

Detects variables declared as `global` in an inner scope that have no explicit
bindings or declarations in the global scope.

**Why is this bad?**

Function bodies with `global` statements can run in any order (or not at all), which makes
it hard for static analysis tools to infer the types of globals without
explicit definitions or declarations.

**Example**

```python
def f():
    global x  # unresolved global
    x = 42

def g():
    print(x)  # unresolved reference
```

Use instead:

```python
x: int

def f():
    global x
    x = 42

def g():
    print(x)
```

Or:

```python
x: int | None = None

def f():
    global x
    x = 42

def g():
    print(x)
```

## `unresolved-import`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.1">0.0.1-alpha.1</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20unresolved-import" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L1939" target="_blank">View source</a>
</small>


**What it does**

Checks for import statements for which the module cannot be resolved.

**Why is this bad?**

Importing a module that cannot be resolved will raise a `ModuleNotFoundError`
at runtime.

**Examples**

```python
import foo  # ModuleNotFoundError: No module named 'foo'
```

## `unresolved-reference`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.1">0.0.1-alpha.1</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20unresolved-reference" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L1958" target="_blank">View source</a>
</small>


**What it does**

Checks for references to names that are not defined.

**Why is this bad?**

Using an undefined variable will raise a `NameError` at runtime.

**Example**


```python
print(x)  # NameError: name 'x' is not defined
```

## `unsupported-base`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'warn'."><code>warn</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.7">0.0.1-alpha.7</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20unsupported-base" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L809" target="_blank">View source</a>
</small>


**What it does**

Checks for class definitions that have bases which are unsupported by ty.

**Why is this bad?**

If a class has a base that is an instance of a complex type such as a union type,
ty will not be able to resolve the [method resolution order] (MRO) for the class.
This will lead to an inferior understanding of your codebase and unpredictable
type-checking behavior.

**Examples**

```python
import datetime

class A: ...
class B: ...

if datetime.date.today().weekday() != 6:
    C = A
else:
    C = B

class D(C): ...  # error: [unsupported-base]
```

[method resolution order]: https://docs.python.org/3/glossary.html#term-method-resolution-order

## `unsupported-bool-conversion`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.1">0.0.1-alpha.1</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20unsupported-bool-conversion" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L1532" target="_blank">View source</a>
</small>


**What it does**

Checks for bool conversions where the object doesn't correctly implement `__bool__`.

**Why is this bad?**

If an exception is raised when you attempt to evaluate the truthiness of an object,
using the object in a boolean context will fail at runtime.

**Examples**


```python
class NotBoolable:
    __bool__ = None

b1 = NotBoolable()
b2 = NotBoolable()

if b1:  # exception raised here
    pass

b1 and b2  # exception raised here
not b1  # exception raised here
b1 < b2 < b1  # exception raised here
```

## `unsupported-operator`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.1">0.0.1-alpha.1</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20unsupported-operator" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L1977" target="_blank">View source</a>
</small>


**What it does**

Checks for binary expressions, comparisons, and unary expressions where
the operands don't support the operator.

**Why is this bad?**

Attempting to use an unsupported operator will raise a `TypeError` at
runtime.

**Examples**

```python
class A: ...

A() + A()  # TypeError: unsupported operand type(s) for +: 'A' and 'A'
```

## `unused-ignore-comment`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'ignore'."><code>ignore</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.1">0.0.1-alpha.1</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20unused-ignore-comment" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Fsuppression.rs#L25" target="_blank">View source</a>
</small>


**What it does**

Checks for `type: ignore` or `ty: ignore` directives that are no longer applicable.

**Why is this bad?**

A `type: ignore` directive that no longer matches any diagnostic violations is likely
included by mistake, and should be removed to avoid confusion.

**Examples**

```py
a = 20 / 2  # ty: ignore[division-by-zero]
```

Use instead:

```py
a = 20 / 2
```

## `useless-overload-body`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'warn'."><code>warn</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.22">0.0.1-alpha.22</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20useless-overload-body" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L1120" target="_blank">View source</a>
</small>


**What it does**

Checks for various `@overload`-decorated functions that have non-stub bodies.

**Why is this bad?**

Functions decorated with `@overload` are ignored at runtime; they are overridden
by the implementation function that follows the series of overloads. While it is
not illegal to provide a body for an `@overload`-decorated function, it may indicate
a misunderstanding of how the `@overload` decorator works.

**Example**


```py
from typing import overload

@overload
def foo(x: int) -> int:
    return x + 1  # will never be executed

@overload
def foo(x: str) -> str:
    return "Oh no, got a string"  # will never be executed

def foo(x: int | str) -> int | str:
    raise Exception("unexpected type encountered")
```

Use instead:

```py
from typing import assert_never, overload

@overload
def foo(x: int) -> int: ...

@overload
def foo(x: str) -> str: ...

def foo(x: int | str) -> int | str:
    if isinstance(x, int):
        return x + 1
    elif isinstance(x, str):
        return "Oh no, got a string"
    else:
        assert_never(x)
```

**References**

- [Python documentation: `@overload`](https://docs.python.org/3/library/typing.html#typing.overload)

## `zero-stepsize-in-slice`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.1">0.0.1-alpha.1</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20zero-stepsize-in-slice" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L1999" target="_blank">View source</a>
</small>


**What it does**

Checks for step size 0 in slices.

**Why is this bad?**

A slice with a step size of zero will raise a `ValueError` at runtime.

**Examples**

```python
l = list(range(10))
l[1:10:0]  # ValueError: slice step cannot be zero
```

