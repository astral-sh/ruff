<!-- WARNING: This file is auto-generated (cargo dev generate-all). Edit the lint-declarations in 'crates/ty_python_semantic/src/types/diagnostic.rs' if you want to change anything here. -->

# Rules

## `abstract-method-in-final-class`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.13">0.0.13</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22abstract-method-in-final-class%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L2444" target="_blank">View source</a>
</small>


**What it does**

Checks for `@final` classes that have unimplemented abstract methods.

**Why is this bad?**

A class decorated with `@final` cannot be subclassed. If such a class has abstract
methods that are not implemented, the class can never be properly instantiated, as
the abstract methods can never be implemented (since subclassing is prohibited).

At runtime, instantiation of classes with unimplemented abstract methods is only
prevented for classes that have `ABCMeta` (or a subclass of it) as their metaclass.
However, type checkers also enforce this for classes that do not use `ABCMeta`, since
the intent for the class to be abstract is clear from the use of `@abstractmethod`.

**Example**


```python
from abc import ABC, abstractmethod
from typing import final

class Base(ABC):
    @abstractmethod
    def method(self) -> int: ...

@final
class Derived(Base):  # Error: `Derived` does not implement `method`
    pass
```

## `ambiguous-protocol-member`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'warn'."><code>warn</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.20">0.0.1-alpha.20</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22ambiguous-protocol-member%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L661" target="_blank">View source</a>
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

## `assert-type-unspellable-subtype`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.14">0.0.14</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22assert-type-unspellable-subtype%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L2580" target="_blank">View source</a>
</small>


**What it does**

Checks for `assert_type()` calls where the actual type
is an unspellable subtype of the asserted type.

**Why is this bad?**

`assert_type()` is intended to ensure that the inferred type of a value
is exactly the same as the asserted type. But in some situations, ty
has nonstandard extensions to the type system that allow it to infer
more precise types than can be expressed in user annotations. ty emits a
different error code to [`type-assertion-failure`](#type-assertion-failure) in these situations so
that users can easily differentiate between the two cases.

**Example**


```python
def _(x: int):
    assert_type(x, int)  # fine
    if x:
        assert_type(x, int)  # error: [assert-type-unspellable-subtype]
                             # the actual type is `int & ~AlwaysFalsy`,
                             # which excludes types like `Literal[0]`
```

## `call-abstract-method`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Preview (since <a href="https://github.com/astral-sh/ty/releases/tag/0.0.16">0.0.16</a>) ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22call-abstract-method%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L2479" target="_blank">View source</a>
</small>


**What it does**

Checks for calls to abstract `@classmethod`s or `@staticmethod`s
with "trivial bodies" when accessed on the class object itself.

"Trivial bodies" are bodies that solely consist of `...`, `pass`,
a docstring, and/or `raise NotImplementedError`.

**Why is this bad?**

An abstract method with a trivial body has no concrete implementation
to execute, so calling such a method directly on the class will probably
not have the desired effect.

It is also unsound to call these methods directly on the class. Unlike
other methods, ty permits abstract methods with trivial bodies to have
non-`None` return types even though they always return `None` at runtime.
This is because it is expected that these methods will always be
overridden rather than being called directly. As a result of this
exception to the normal rule, ty may infer an incorrect type if one of
these methods is called directly, which may then mean that type errors
elsewhere in your code go undetected by ty.

Calling abstract classmethods or staticmethods via `type[X]` is allowed,
since the actual runtime type could be a concrete subclass with an implementation.

**Example**

```python
from abc import ABC, abstractmethod

class Foo(ABC):
    @classmethod
    @abstractmethod
    def method(cls) -> int: ...

Foo.method()  # Error: cannot call abstract classmethod
```

## `call-non-callable`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.1">0.0.1-alpha.1</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22call-non-callable%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L177" target="_blank">View source</a>
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
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22call-top-callable%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L195" target="_blank">View source</a>
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
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22conflicting-argument-forms%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L246" target="_blank">View source</a>
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
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22conflicting-declarations%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L272" target="_blank">View source</a>
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
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22conflicting-metaclass%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L297" target="_blank">View source</a>
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
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22cyclic-class-definition%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L323" target="_blank">View source</a>
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
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.29">0.0.1-alpha.29</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22cyclic-type-alias-definition%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L349" target="_blank">View source</a>
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

## `dataclass-field-order`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.15">0.0.15</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22dataclass-field-order%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L467" target="_blank">View source</a>
</small>


**What it does**

Checks for dataclass definitions where required fields are defined after
fields with default values.

**Why is this bad?**

In dataclasses, all required fields (fields without default values) must be
defined before fields with default values. This is a Python requirement that
will raise a `TypeError` at runtime if violated.

**Example**

```python
from dataclasses import dataclass

@dataclass
class Example:
    x: int = 1    # Field with default value
    y: str        # Error: Required field after field with default
```

## `deprecated`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'warn'."><code>warn</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.16">0.0.1-alpha.16</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22deprecated%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L393" target="_blank">View source</a>
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
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.1">0.0.1-alpha.1</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22division-by-zero%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L371" target="_blank">View source</a>
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
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22duplicate-base%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L414" target="_blank">View source</a>
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
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22duplicate-kw-only%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L435" target="_blank">View source</a>
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

## `empty-body`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.14">0.0.14</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22empty-body%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L1009" target="_blank">View source</a>
</small>


**What it does**

Detects functions with empty bodies that have a non-`None` return type annotation.

The errors reported by this rule have the same motivation as the [`invalid-return-type`](#invalid-return-type)
rule. The diagnostic exists as a separate error code to allow users to disable this
rule while prototyping code. While we strongly recommend enabling this rule if
possible, users migrating from other type checkers may also find it useful to
temporarily disable this rule on some or all of their codebase if they find it
results in a large number of diagnostics.

**Why is this bad?**

A function with an empty body (containing only `...`, `pass`, or a docstring) will
implicitly return `None` at runtime. Returning `None` when the return type is non-`None`
is unsound, and will lead to ty inferring incorrect types elsewhere.

Functions with empty bodies are permitted in certain contexts where they serve as
declarations rather than implementations:

- Functions in stub files (`.pyi`)
- Methods in Protocol classes
- Abstract methods decorated with `@abstractmethod`
- Overload declarations decorated with `@overload`
- Functions in `if TYPE_CHECKING` blocks

**Examples**

```python
def foo() -> int: ...  # error: [empty-body]

def bar() -> str:
    """A function that does nothing."""
    pass  # error: [empty-body]
```

## `escape-character-in-forward-annotation`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.1">0.0.1-alpha.1</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22escape-character-in-forward-annotation%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fstring_annotation.rs#L106" target="_blank">View source</a>
</small>


**What it does**

Checks for forward annotations that contain escape characters.

**Why is this bad?**

Static analysis tools like ty can't analyze type annotations that contain escape characters.

**Example**


```python
def foo() -> "intt\b": ...
```

## `final-on-non-method`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.20">0.0.20</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22final-on-non-method%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L2391" target="_blank">View source</a>
</small>


**What it does**

Checks for `@final` decorators applied to non-method functions.

**Why is this bad?**

The `@final` decorator is only meaningful on methods and classes.
Applying it to a module-level function or a nested function has no
effect and is likely a mistake.

**Example**


```python
from typing import final

# Error: @final is not allowed on non-method functions
@final
def my_function() -> int:
    return 0
```

## `final-without-value`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.15">0.0.15</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22final-without-value%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L2417" target="_blank">View source</a>
</small>


**What it does**

Checks for `Final` symbols that are declared without a value and are never
assigned a value in their scope.

**Why is this bad?**

A `Final` symbol must be initialized with a value at the time of declaration
or in a subsequent assignment. At module or function scope, the assignment must
occur in the same scope. In a class body, the assignment may occur in `__init__`.

**Examples**

```python
from typing import Final

# Error: `Final` symbol without a value
MY_CONSTANT: Final[int]

# OK: `Final` symbol with a value
MY_CONSTANT: Final[int] = 1
```

## `ignore-comment-unknown-rule`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'warn'."><code>warn</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.1">0.0.1-alpha.1</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22ignore-comment-unknown-rule%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Fsuppression.rs#L84" target="_blank">View source</a>
</small>


**What it does**

Checks for `ty: ignore[code]` or `type: ignore[ty:code]` comments where `code` isn't a known lint rule.

**Why is this bad?**

A `ty: ignore[code]` or a `type:ignore[ty:code] directive with a `code` that doesn't match
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
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22implicit-concatenated-string-type-annotation%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fstring_annotation.rs#L38" target="_blank">View source</a>
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
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22inconsistent-mro%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L778" target="_blank">View source</a>
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
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22index-out-of-bounds%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L802" target="_blank">View source</a>
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

## `ineffective-final`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'warn'."><code>warn</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.33">0.0.1-alpha.33</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22ineffective-final%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L2363" target="_blank">View source</a>
</small>


**What it does**

Checks for calls to `final()` that type checkers cannot interpret.

**Why is this bad?**

The `final()` function is designed to be used as a decorator. When called directly
as a function (e.g., `final(type(...))`), type checkers will not understand the
application of `final` and will not prevent subclassing.

**Example**


```python
from typing import final

# Incorrect: type checkers will not prevent subclassing
MyClass = final(type("MyClass", (), {}))

# Correct: use `final` as a decorator
@final
class MyClass: ...
```

## `instance-layout-conflict`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.12">0.0.1-alpha.12</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22instance-layout-conflict%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L550" target="_blank">View source</a>
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
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22invalid-argument-type%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L940" target="_blank">View source</a>
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
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22invalid-assignment%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L1049" target="_blank">View source</a>
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
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22invalid-attribute-access%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L2909" target="_blank">View source</a>
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

## `invalid-attribute-override`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.33">0.0.33</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22invalid-attribute-override%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L3169" target="_blank">View source</a>
</small>


**What it does**

Detects attribute overrides that change whether an inherited attribute
is a class variable or an instance variable.

**Why is this bad?**

Pure class variables and instance variables have different access and
assignment behavior. Overriding one with the other violates the
[Liskov Substitution Principle] ("LSP"), because code that is valid for
the superclass may no longer be valid for the subclass.

**Example**

```python
from typing import ClassVar

class Base:
    instance_attr: int
    class_attr: ClassVar[int]

class Sub(Base):
    instance_attr: ClassVar[int]  # error: [invalid-attribute-override]
    class_attr: int  # error: [invalid-attribute-override]
```

[Liskov Substitution Principle]: https://en.wikipedia.org/wiki/Liskov_substitution_principle

## `invalid-await`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.19">0.0.1-alpha.19</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22invalid-await%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L1071" target="_blank">View source</a>
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
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22invalid-base%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L1101" target="_blank">View source</a>
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
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22invalid-context-manager%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L1185" target="_blank">View source</a>
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

## `invalid-dataclass`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.12">0.0.12</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22invalid-dataclass%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L519" target="_blank">View source</a>
</small>


**What it does**

Checks for invalid applications of the `@dataclass` decorator.

**Why is this bad?**

Applying `@dataclass` to a class that inherits from `NamedTuple`, `TypedDict`,
`Enum`, or `Protocol` is invalid:

- `NamedTuple` and `TypedDict` classes will raise an exception at runtime when
  instantiating the class.
- `Enum` classes with `@dataclass` are [explicitly not supported].
- `Protocol` classes define interfaces and cannot be instantiated.

**Examples**

```python
from dataclasses import dataclass
from typing import NamedTuple

@dataclass  # error: [invalid-dataclass]
class Foo(NamedTuple):
    x: int
```

[explicitly not supported]: https://docs.python.org/3/howto/enum.html#dataclass-support

## `invalid-dataclass-override`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.13">0.0.13</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22invalid-dataclass-override%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L493" target="_blank">View source</a>
</small>


**What it does**

Checks for dataclass definitions that have both `frozen=True` and a custom `__setattr__` or
`__delattr__` method defined.

**Why is this bad?**

Frozen dataclasses synthesize `__setattr__` and `__delattr__` methods which raise a
`FrozenInstanceError` to emulate immutability.

Overriding either of these methods raises a runtime error.

**Examples**

```python
from dataclasses import dataclass

@dataclass(frozen=True)
class A:
    def __setattr__(self, name: str, value: object) -> None: ...
```

## `invalid-declaration`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.1">0.0.1-alpha.1</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22invalid-declaration%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L1206" target="_blank">View source</a>
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

## `invalid-enum-member-annotation`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'warn'."><code>warn</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.20">0.0.20</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22invalid-enum-member-annotation%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L1265" target="_blank">View source</a>
</small>


**What it does**

Checks for enum members that have explicit type annotations.

**Why is this bad?**

The [typing spec] states that type checkers should infer a literal type
for all enum members. An explicit type annotation on an enum member is
misleading because the annotated type will be incorrect — the actual
runtime type is the enum class itself, not the annotated type.

In CPython's `enum` module, annotated assignments with values are still
treated as members at runtime, but the annotation will confuse readers of the code.

**Examples**

```python
from enum import Enum

class Pet(Enum):
    CAT = 1       # OK
    DOG: int = 2  # Error: enum members should not be annotated
```

Use instead:
```python
from enum import Enum

class Pet(Enum):
    CAT = 1
    DOG = 2
```

**References**

- [Typing spec: Enum members](https://typing.python.org/en/latest/spec/enums.html#enum-members)

[typing spec]: https://typing.python.org/en/latest/spec/enums.html#enum-members

## `invalid-exception-caught`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.1">0.0.1-alpha.1</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22invalid-exception-caught%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L1229" target="_blank">View source</a>
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
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22invalid-explicit-override%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L2522" target="_blank">View source</a>
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
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22invalid-frozen-dataclass-subclass%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L3299" target="_blank">View source</a>
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
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22invalid-generic-class%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L1349" target="_blank">View source</a>
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

## `invalid-generic-enum`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.12">0.0.12</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22invalid-generic-enum%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L1307" target="_blank">View source</a>
</small>


**What it does**

Checks for enum classes that are also generic.

**Why is this bad?**

Enum classes cannot be generic. Python does not support generic enums:
attempting to create one will either result in an immediate `TypeError`
at runtime, or will create a class that cannot be specialized in the way
that a normal generic class can.

**Examples**

```python
from enum import Enum
from typing import Generic, TypeVar

T = TypeVar("T")

# error: enum class cannot be generic (class creation fails with `TypeError`)
class E[T](Enum):
    A = 1

# error: enum class cannot be generic (class creation fails with `TypeError`)
class F(Enum, Generic[T]):
    A = 1

# error: enum class cannot be generic -- the class creation does not immediately fail...
class G(Generic[T], Enum):
    A = 1

# ...but this raises `KeyError`:
x: G[int]
```

**References**

- [Python documentation: Enum](https://docs.python.org/3/library/enum.html)

## `invalid-ignore-comment`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'warn'."><code>warn</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.1">0.0.1-alpha.1</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22invalid-ignore-comment%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Fsuppression.rs#L109" target="_blank">View source</a>
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
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22invalid-key%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L823" target="_blank">View source</a>
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

bob: Person = { "namee": "Bob", "age": 30 }  # typo!

carol = Person(name="Carol", aeg=25)  # typo!
```

## `invalid-legacy-positional-parameter`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'warn'."><code>warn</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.15">0.0.15</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22invalid-legacy-positional-parameter%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L3377" target="_blank">View source</a>
</small>


**What it does**


Checks for parameters that appear to be attempting to use the legacy convention
to specify that a parameter is positional-only, but do so incorrectly.

The "legacy convention" for specifying positional-only parameters was
specified in [PEP 484]. It states that parameters with names starting with
`__` should be considered positional-only by type checkers. [PEP 570], introduced
in Python 3.8, added dedicated syntax for specifying positional-only parameters,
rendering the legacy convention obsolete. However, some codebases may still
use the legacy convention for compatibility with older Python versions.

**Why is this bad?**


In most cases, a type checker will not consider a parameter to be positional-only
if it comes after a positional-or-keyword parameter, even if its name starts with
`__`. This may be unexpected to the author of the code.

**Example**


```python
def f(x, __y):  # Error: `__y` is not considered positional-only
    pass
```

Use instead:

```python
def f(__x, __y):  # If you need compatibility with Python <=3.7
    pass
```

or:

```python
def f(x, y, /):  # Python 3.8+ syntax
    pass
```

**References**


- [Typing spec: positional-only parameters (legacy syntax)](https://typing.python.org/en/latest/spec/historical.html#pos-only-double-underscore)
- [Python glossary: parameters](https://docs.python.org/3/glossary.html#term-parameter)

[PEP 484]: https://peps.python.org/pep-0484/#positional-only-arguments
[PEP 570]: https://peps.python.org/pep-0570/

## `invalid-legacy-type-variable`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.1">0.0.1-alpha.1</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22invalid-legacy-type-variable%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L1406" target="_blank">View source</a>
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

## `invalid-match-pattern`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.18">0.0.18</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22invalid-match-pattern%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L1810" target="_blank">View source</a>
</small>


**What it does**

Checks for invalid match patterns.

**Why is this bad?**

Matching on invalid patterns will lead to a runtime error.

**Examples**

```python
NotAClass = 42

match x:
    case NotAClass():    # TypeError at runtime: must be a class
        ...
```

## `invalid-metaclass`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.1">0.0.1-alpha.1</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22invalid-metaclass%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L1534" target="_blank">View source</a>
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
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22invalid-method-override%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L3201" target="_blank">View source</a>
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
these suggestions can cause ty to start reporting an [`invalid-method-override`](#invalid-method-override) error if
the function in question is a method on a subclass that overrides a method on a superclass,
and the change would cause the subclass method to no longer accept all argument combinations
that the superclass method accepts.

This can usually be resolved by adding [`@typing.override`][override] to your method
definition. Ruff knows that a method decorated with `@typing.override` is intended to
override a method by the same name on a superclass, and avoids reporting rules like ARG002
for such methods; it knows that the changes recommended by ARG002 would violate the Liskov
Substitution Principle.

Correct use of `@override` is enforced by ty's [`invalid-explicit-override`](#invalid-explicit-override) rule.

[Liskov Substitution Principle]: https://en.wikipedia.org/wiki/Liskov_substitution_principle
[override]: https://docs.python.org/3/library/typing.html#typing.override

## `invalid-named-tuple`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.19">0.0.1-alpha.19</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22invalid-named-tuple%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L696" target="_blank">View source</a>
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

## `invalid-named-tuple-override`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'warn'."><code>warn</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.31">0.0.31</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22invalid-named-tuple-override%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L744" target="_blank">View source</a>
</small>


**What it does**

Checks for subclass members that override inherited `NamedTuple` fields.

**Why is this bad?**

Reusing an inherited `NamedTuple` field name in a subclass creates a
class where tuple indexing and `repr()` still reflect the original
field, while attribute access follows the subclass member.

**Default level**

This rule is a warning by default because these overrides do not make
the class invalid at runtime.

**Examples**

```python
from typing import NamedTuple

class User(NamedTuple):
    name: str

class Admin(User):
    name = "shadowed"  # error: [invalid-named-tuple-override]

admin = Admin("Alice")
admin.name  # "shadowed"
admin[0]  # "Alice"
```

## `invalid-newtype`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.27">0.0.1-alpha.27</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22invalid-newtype%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L1479" target="_blank">View source</a>
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
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22invalid-overload%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L1561" target="_blank">View source</a>
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
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22invalid-parameter-default%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L1660" target="_blank">View source</a>
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
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22invalid-paramspec%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L1434" target="_blank">View source</a>
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
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22invalid-protocol%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L632" target="_blank">View source</a>
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
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22invalid-raise%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L1680" target="_blank">View source</a>
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
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22invalid-return-type%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L961" target="_blank">View source</a>
</small>


**What it does**

Detects returned values that can't be assigned to the function's annotated return type.

Note that the special case of a function with a non-`None` return type and an empty body
is handled by the separate [`empty-body`](#empty-body) error code.

**Why is this bad?**

Returning an object of a type incompatible with the annotated return type
is unsound, and will lead to ty inferring incorrect types elsewhere.

**Examples**

```python
def func() -> int:
    return "a"  # error: [invalid-return-type]
```

## `invalid-super-argument`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.1">0.0.1-alpha.1</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22invalid-super-argument%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L1723" target="_blank">View source</a>
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
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22invalid-syntax-in-forward-annotation%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fstring_annotation.rs#L63" target="_blank">View source</a>
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

## `invalid-total-ordering`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.10">0.0.10</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22invalid-total-ordering%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L3337" target="_blank">View source</a>
</small>


**What it does**

Checks for classes decorated with `@functools.total_ordering` that don't
define any ordering method (`__lt__`, `__le__`, `__gt__`, or `__ge__`).

**Why is this bad?**

The `@total_ordering` decorator requires the class to define at least one
ordering method. If none is defined, Python raises a `ValueError` at runtime.

**Example**


```python
from functools import total_ordering

@total_ordering
class MyClass:  # Error: no ordering method defined
    def __eq__(self, other: object) -> bool:
        return True
```

Use instead:

```python
from functools import total_ordering

@total_ordering
class MyClass:
    def __eq__(self, other: object) -> bool:
        return True

    def __lt__(self, other: "MyClass") -> bool:
        return True
```

## `invalid-type-alias-type`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.6">0.0.1-alpha.6</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22invalid-type-alias-type%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L1458" target="_blank">View source</a>
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
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22invalid-type-arguments%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L2070" target="_blank">View source</a>
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
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22invalid-type-checking-constant%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L1762" target="_blank">View source</a>
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
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22invalid-type-form%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L1786" target="_blank">View source</a>
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
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22invalid-type-guard-call%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L1860" target="_blank">View source</a>
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
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22invalid-type-guard-definition%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L1832" target="_blank">View source</a>
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

## `invalid-type-variable-bound`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.15">0.0.15</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22invalid-type-variable-bound%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L1929" target="_blank">View source</a>
</small>


**What it does**

Checks for [type variables] whose bounds reference type variables.

**Why is this bad?**

The bound of a type variable must be a concrete type.

**Examples**

```python
T = TypeVar('T', bound=list['T'])  # error: [invalid-type-variable-bound]
U = TypeVar('U')
T = TypeVar('T', bound=U)  # error: [invalid-type-variable-bound]

def f[T: list[T]](): ...  # error: [invalid-type-variable-bound]
def g[U, T: U](): ...  # error: [invalid-type-variable-bound]
```

[type variable]: https://docs.python.org/3/library/typing.html#typing.TypeVar

## `invalid-type-variable-constraints`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.1">0.0.1-alpha.1</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22invalid-type-variable-constraints%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L1888" target="_blank">View source</a>
</small>


**What it does**


Checks for constrained [type variables] with only one constraint,
or that those constraints reference type variables.

**Why is this bad?**


A constrained type variable must have at least two constraints.

**Examples**


```python
from typing import TypeVar

T = TypeVar('T', str)  # invalid constrained TypeVar

I = TypeVar('I', bound=int)
U = TypeVar('U', list[I], int)  # invalid constrained TypeVar
```

Use instead:

```python
T = TypeVar('T', str, int)  # valid constrained TypeVar

# or

T = TypeVar('T', bound=str)  # valid bound TypeVar

U = TypeVar('U', list[int], int)  # valid constrained Type
```

[type variables]: https://docs.python.org/3/library/typing.html#typing.TypeVar

## `invalid-type-variable-default`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.16">0.0.16</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22invalid-type-variable-default%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L1954" target="_blank">View source</a>
</small>


**What it does**

Checks for [type variables] whose default type is not compatible with
the type variable's bound or constraints.

**Why is this bad?**

If a type variable has a bound, the default must be assignable to that
bound (see: [bound rules]). If a type variable has constraints, the default
must be one of the constraints (see: [constraint rules]).

**Examples**

```python
T = TypeVar("T", bound=str, default=int)  # error: [invalid-type-variable-default]
U = TypeVar("U", int, str, default=bytes)  # error: [invalid-type-variable-default]
```

[type variables]: https://docs.python.org/3/library/typing.html#typing.TypeVar
[bound rules]: https://typing.python.org/en/latest/spec/generics.html#bound-rules
[constraint rules]: https://typing.python.org/en/latest/spec/generics.html#constraint-rules

## `invalid-typed-dict-field`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.28">0.0.28</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22invalid-typed-dict-field%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L3115" target="_blank">View source</a>
</small>


**What it does**

Detects invalid `TypedDict` field declarations.

**Why is this bad?**

`TypedDict` subclasses cannot redefine inherited fields incompatibly. Doing so breaks the
subtype guarantees that `TypedDict` inheritance is meant to preserve.

**Example**

```python
from typing import TypedDict

class Base(TypedDict):
    x: int

class Child(Base):
    x: str  # error: [invalid-typed-dict-field]
```

## `invalid-typed-dict-header`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.14">0.0.14</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22invalid-typed-dict-header%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L3140" target="_blank">View source</a>
</small>


**What it does**

Detects errors in `TypedDict` class headers, such as unexpected arguments
or invalid base classes.

**Why is this bad?**

The typing spec states that `TypedDict`s are not permitted to have
custom metaclasses. Using `**` unpacking in a `TypedDict` header
is also prohibited by ty, as it means that ty cannot statically determine
whether keys in the `TypedDict` are intended to be required or optional.

**Example**

```python
from typing import TypedDict

class Foo(TypedDict, metaclass=whatever):  # error: [invalid-typed-dict-header]
    ...

def f(x: dict):
    class Bar(TypedDict, **x):  # error: [invalid-typed-dict-header]
        ...
```

## `invalid-typed-dict-statement`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.9">0.0.9</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22invalid-typed-dict-statement%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L3090" target="_blank">View source</a>
</small>


**What it does**

Detects statements other than annotated declarations in `TypedDict` class bodies.

**Why is this bad?**

`TypedDict` class bodies aren't allowed to contain any other types of statements. For
example, method definitions and field values aren't allowed. None of these will be
available on "instances of the `TypedDict`" at runtime (as `dict` is the runtime class of
all "`TypedDict` instances").

**Example**

```python
from typing import TypedDict

class Foo(TypedDict):
    def bar(self):  # error: [invalid-typed-dict-statement]
        pass
```

## `invalid-yield`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.25">0.0.25</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22invalid-yield%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L984" target="_blank">View source</a>
</small>


**What it does**

Detects `yield` and `yield from` expressions where the "yield" or "send" type
is incompatible with the generator function's annotated return type.

**Why is this bad?**

Yielding a value of a type that doesn't match the generator's declared yield type,
or using `yield from` with a sub-iterator whose yield or send type is incompatible,
is a type error that may cause downstream consumers of the generator to receive
values of an unexpected type.

**Examples**

```python
from typing import Iterator

def gen() -> Iterator[int]:
    yield "not an int"  # error: [invalid-yield]
```

## `isinstance-against-protocol`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.14">0.0.14</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22isinstance-against-protocol%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L856" target="_blank">View source</a>
</small>


**What it does**

Reports invalid runtime checks against `Protocol` classes.
This includes explicit calls `isinstance()`/`issubclass()` against
non-runtime-checkable protocols, `issubclass()` calls against protocols
that have non-method members, and implicit `isinstance()` checks against
non-runtime-checkable protocols via pattern matching.

**Why is this bad?**

These calls (implicit or explicit) raise `TypeError` at runtime.

**Examples**

```python
from typing_extensions import Protocol, runtime_checkable

class HasX(Protocol):
    x: int

@runtime_checkable
class HasY(Protocol):
    y: int

def f(arg: object, arg2: type):
    isinstance(arg, HasX)  # error: [isinstance-against-protocol] (not runtime-checkable)
    issubclass(arg2, HasX)  # error: [isinstance-against-protocol] (not runtime-checkable)

def g(arg: object):
    match arg:
        case HasX():  # error: [isinstance-against-protocol] (not runtime-checkable)
            pass

def h(arg2: type):
    isinstance(arg2, HasY)  # fine (runtime-checkable)

    # `HasY` is runtime-checkable, but has non-method members,
    # so it still can't be used in `issubclass` checks)
    issubclass(arg2, HasY)  # error: [isinstance-against-protocol]
```

**References**

- [Typing documentation: `@runtime_checkable`](https://docs.python.org/3/library/typing.html#typing.runtime_checkable)

## `isinstance-against-typed-dict`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.15">0.0.15</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22isinstance-against-typed-dict%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L904" target="_blank">View source</a>
</small>


**What it does**

Reports runtime checks against `TypedDict` classes.
This includes explicit calls to `isinstance()`/`issubclass()` and implicit
checks performed by `match` class patterns.

**Why is this bad?**

Using a `TypedDict` class in these contexts raises `TypeError` at runtime.

**Examples**

```python
from typing_extensions import TypedDict

class Movie(TypedDict):
    name: str
    director: str

def f(arg: object, arg2: type):
    isinstance(arg, Movie)  # error: [isinstance-against-typed-dict]
    issubclass(arg2, Movie)  # error: [isinstance-against-typed-dict]

def g(arg: object):
    match arg:
        case Movie():  # error: [isinstance-against-typed-dict]
            pass
```

**References**

- [Typing specification: `TypedDict`](https://typing.python.org/en/latest/spec/typeddict.html)

## `mismatched-type-name`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'warn'."><code>warn</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.30">0.0.30</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22mismatched-type-name%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L1503" target="_blank">View source</a>
</small>


**What it does**

Checks for functional typing definitions whose declared name does not match
the variable they are assigned to.

**Why is this bad?**

Constructors like `TypeVar`, `ParamSpec`, `NewType`, `NamedTuple`,
`TypedDict`, and `TypeAliasType` all take a name argument that is
normally expected to match the assigned variable. A mismatch is usually a
typo and makes later diagnostics harder to understand.

**Default level**

This rule is a warning by default because ty can usually recover and
continue understanding the resulting type.

**Examples**

```python
from typing import NewType, TypeVar
from typing_extensions import TypedDict

T = TypeVar("U")  # error: [mismatched-type-name]
UserId = NewType("Id", int)  # error: [mismatched-type-name]
Movie = TypedDict("Film", {"title": str})  # error: [mismatched-type-name]
```

## `missing-argument`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.1">0.0.1-alpha.1</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22missing-argument%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L2010" target="_blank">View source</a>
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
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22missing-typed-dict-key%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L3063" target="_blank">View source</a>
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
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22no-matching-overload%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L2029" target="_blank">View source</a>
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

## `non-callable-init-subclass`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.30">0.0.30</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22non-callable-init-subclass%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L1380" target="_blank">View source</a>
</small>


**What it does**

Checks for class definitions that will fail due to non-callable `__init_subclass__`
methods.

**Why is this bad?**

If a class defines a non-callable `__init_subclass__` method/attribute, any attempt
to subclass that class will raise a `TypeError` at runtime.

**Examples**

```python
class Super:
    __init_subclass__ = None

class Sub(Super): ...  # error: [non-callable-init-subclass]
```

**References**

- [Python data model: Customizing class creation](https://docs.python.org/3/reference/datamodel.html#customizing-class-creation)

## `not-iterable`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.1">0.0.1-alpha.1</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22not-iterable%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L2111" target="_blank">View source</a>
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
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22not-subscriptable%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L2052" target="_blank">View source</a>
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
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22override-of-final-method%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L2309" target="_blank">View source</a>
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

## `override-of-final-variable`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.16">0.0.16</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22override-of-final-variable%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L2336" target="_blank">View source</a>
</small>


**What it does**

Checks for class variables on subclasses that override a superclass variable
that has been declared as `Final`.

**Why is this bad?**

Declaring a variable as `Final` indicates to the type checker that it should not be
overridden on any subclass.

**Example**


```python
from typing import Final

class A:
    X: Final[int] = 1

class B(A):
    X = 2  # Error raised here
```

## `parameter-already-assigned`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.1">0.0.1-alpha.1</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22parameter-already-assigned%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L2162" target="_blank">View source</a>
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
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22positional-only-parameter-as-kwarg%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L2736" target="_blank">View source</a>
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
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'ignore'."><code>ignore</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.22">0.0.1-alpha.22</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22possibly-missing-attribute%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L2183" target="_blank">View source</a>
</small>


**What it does**

Checks for possibly missing attributes.

**Why is this bad?**

Attempting to access a missing attribute will raise an `AttributeError` at runtime.

**Rule status**

This rule is currently disabled by default because of the number of
false positives it can produce.

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
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22possibly-missing-implicit-call%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L220" target="_blank">View source</a>
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
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22possibly-missing-import%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L2230" target="_blank">View source</a>
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

## `possibly-missing-submodule`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'warn'."><code>warn</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.23">0.0.23</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22possibly-missing-submodule%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L2209" target="_blank">View source</a>
</small>


**What it does**

Checks for accesses of submodules that might not've been imported.

**Why is this bad?**

When module `a` has a submodule `b`, `import a` isn't generally enough to let you access
`a.b.` You either need to explicitly `import a.b`, or else you need the `__init__.py` file
of `a` to include `from . import b`. Without one of those, `a.b` is an `AttributeError`.

**Examples**

```python
import html
html.parser  # AttributeError: module 'html' has no attribute 'parser'
```

## `possibly-unresolved-reference`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'ignore'."><code>ignore</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.1">0.0.1-alpha.1</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22possibly-unresolved-reference%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L2260" target="_blank">View source</a>
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
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22raw-string-type-annotation%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fstring_annotation.rs#L13" target="_blank">View source</a>
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
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22redundant-cast%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L2937" target="_blank">View source</a>
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

## `redundant-final-classvar`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'warn'."><code>warn</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.18">0.0.18</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22redundant-final-classvar%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L2958" target="_blank">View source</a>
</small>


**What it does**

Checks for redundant combinations of the `ClassVar` and `Final` type qualifiers.

**Why is this bad?**

An attribute that is marked `Final` in a class body is implicitly a class variable.
Marking it as `ClassVar` is therefore redundant.

Note that this diagnostic is not emitted for dataclass fields, where
`ClassVar[Final[int]]` has a distinct meaning from `Final[int]`.

**Examples**

```python
from typing import ClassVar, Final

class C:
    x: ClassVar[Final[int]] = 1  # redundant
    y: Final[ClassVar[int]] = 1  # redundant
```

## `shadowed-type-variable`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.20">0.0.20</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22shadowed-type-variable%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L2984" target="_blank">View source</a>
</small>


**What it does**

Checks for type variables in nested generic classes or functions that shadow type variables
from an enclosing scope.

**Why is this bad?**

Shadowing type variables makes the code confusing and is disallowed by the typing spec.

**Examples**

```python
class Outer[T]:
    # Error: `T` is already used by `Outer`
    class Inner[T]: ...

    # Error: `T` is already used by `Outer`
    def method[T](self, x: T) -> T: ...
```

**References**

- [Typing spec: Generics](https://typing.python.org/en/latest/spec/generics.html#introduction)

## `static-assert-error`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.1">0.0.1-alpha.1</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22static-assert-error%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L2885" target="_blank">View source</a>
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
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22subclass-of-final-class%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L2286" target="_blank">View source</a>
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
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.30">0.0.1-alpha.30</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22super-call-in-named-tuple-method%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L2670" target="_blank">View source</a>
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
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22too-many-positional-arguments%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L2610" target="_blank">View source</a>
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
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22type-assertion-failure%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L2558" target="_blank">View source</a>
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
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22unavailable-implicit-super-arguments%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L2631" target="_blank">View source</a>
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

## `unbound-type-variable`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.20">0.0.20</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22unbound-type-variable%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L1980" target="_blank">View source</a>
</small>


**What it does**

Checks for type variables that are used in a scope where they are not bound
to any enclosing generic context.

**Why is this bad?**

Using a type variable outside of a scope that binds it has no well-defined meaning.

**Examples**

```python
from typing import TypeVar, Generic

T = TypeVar("T")
S = TypeVar("S")

x: T  # error: unbound type variable in module scope

class C(Generic[T]):
    x: list[S] = []  # error: S is not in this class's generic context
```

**References**

- [Typing spec: Scoping rules for type variables](https://typing.python.org/en/latest/spec/generics.html#scoping-rules-for-type-variables)

## `undefined-reveal`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'warn'."><code>warn</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.1">0.0.1-alpha.1</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22undefined-reveal%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L2697" target="_blank">View source</a>
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
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22unknown-argument%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L2715" target="_blank">View source</a>
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
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22unresolved-attribute%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L2757" target="_blank">View source</a>
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
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22unresolved-global%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L3011" target="_blank">View source</a>
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
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22unresolved-import%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L2779" target="_blank">View source</a>
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
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22unresolved-reference%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L2798" target="_blank">View source</a>
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
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22unsupported-base%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L1119" target="_blank">View source</a>
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
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22unsupported-bool-conversion%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L2131" target="_blank">View source</a>
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

## `unsupported-dynamic-base`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'ignore'."><code>ignore</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.12">0.0.12</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22unsupported-dynamic-base%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L1152" target="_blank">View source</a>
</small>


**What it does**

Checks for dynamic class definitions (using `type()`) that have bases
which are unsupported by ty.

This is equivalent to [`unsupported-base`](#unsupported-base) but applies to classes created
via `type()` rather than `class` statements.

**Why is this bad?**

If a dynamically created class has a base that is an unsupported type
such as `type[T]`, ty will not be able to resolve the
[method resolution order] (MRO) for the class. This may lead to an inferior
understanding of your codebase and unpredictable type-checking behavior.

**Default level**

This rule is disabled by default because it will not cause a runtime error,
and may be noisy on codebases that use `type()` in highly dynamic ways.

**Examples**

```python
def factory(base: type[Base]) -> type:
    # `base` has type `type[Base]`, not `type[Base]` itself
    return type("Dynamic", (base,), {})  # error: [unsupported-dynamic-base]
```

[method resolution order]: https://docs.python.org/3/glossary.html#term-method-resolution-order

## `unsupported-operator`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'error'."><code>error</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.1">0.0.1-alpha.1</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22unsupported-operator%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L2817" target="_blank">View source</a>
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

## `unused-awaitable`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'warn'."><code>warn</code></a> ·
Preview (since <a href="https://github.com/astral-sh/ty/releases/tag/0.0.21">0.0.21</a>) ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22unused-awaitable%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L2839" target="_blank">View source</a>
</small>


**What it does**

Checks for awaitable objects (such as coroutines) used as expression
statements without being awaited.

**Why is this bad?**

Calling an `async def` function returns a coroutine object. If the
coroutine is never awaited, the body of the async function will never
execute, which is almost always a bug. Python emits a
`RuntimeWarning: coroutine was never awaited` at runtime in this case.

**Examples**

```python
async def fetch_data() -> str:
    return "data"

async def main() -> None:
    fetch_data()  # Warning: coroutine is not awaited
    await fetch_data()  # OK
```

## `unused-ignore-comment`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'warn'."><code>warn</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.1">0.0.1-alpha.1</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22unused-ignore-comment%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Fsuppression.rs#L25" target="_blank">View source</a>
</small>


**What it does**

Checks for `ty: ignore` directives that are no longer applicable.

**Why is this bad?**

A `ty: ignore` directive that no longer matches any diagnostic violations is likely
included by mistake, and should be removed to avoid confusion.

**Examples**

```py
a = 20 / 2  # ty: ignore[division-by-zero]
```

Use instead:

```py
a = 20 / 2
```

**Options**

Set [`analysis.respect-type-ignore-comments`](https://docs.astral.sh/ty/reference/configuration/#respect-type-ignore-comments)
to `false` to prevent this rule from reporting unused `type: ignore` comments.

## `unused-type-ignore-comment`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'warn'."><code>warn</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.14">0.0.14</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22unused-type-ignore-comment%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Fsuppression.rs#L54" target="_blank">View source</a>
</small>


**What it does**

Checks for `type: ignore` directives that are no longer applicable.

**Why is this bad?**

A `type: ignore` directive that no longer matches any diagnostic violations is likely
included by mistake, and should be removed to avoid confusion.

**Examples**

```py
a = 20 / 2  # type: ignore
```

Use instead:

```py
a = 20 / 2
```

**Options**


This rule is skipped if [`analysis.respect-type-ignore-comments`](https://docs.astral.sh/ty/reference/configuration/#respect-type-ignore-comments)
to `false`.

## `useless-overload-body`

<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of 'warn'."><code>warn</code></a> ·
Added in <a href="https://github.com/astral-sh/ty/releases/tag/0.0.1-alpha.22">0.0.1-alpha.22</a> ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22useless-overload-body%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L1604" target="_blank">View source</a>
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
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20%22zero-stepsize-in-slice%22" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L2866" target="_blank">View source</a>
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

