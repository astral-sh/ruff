# Rules

## `byte-string-type-annotation`

**Default level**: error

<details>
<summary>detects byte strings in type annotation positions</summary>

### What it does
Checks for byte-strings in type annotation positions.

### Why is this bad?
Static analysis tools like ty can't analyse type annotations that use byte-string notation.

### Examples
```python
def test(): -> b"int":
    ...
```

Use instead:
```python
def test(): -> "int":
    ...
```

### Links
* [Related issues](https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20byte-string-type-annotation)
* [View source](https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fstring_annotation.rs#L36)
</details>

## `call-non-callable`

**Default level**: error

<details>
<summary>detects calls to non-callable objects</summary>

### What it does
Checks for calls to non-callable objects.

### Why is this bad?
Calling a non-callable object will raise a `TypeError` at runtime.

### Examples
```python
4()  # TypeError: 'int' object is not callable
```

### Links
* [Related issues](https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20call-non-callable)
* [View source](https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L85)
</details>

## `conflicting-argument-forms`

**Default level**: error

<details>
<summary>detects when an argument is used as both a value and a type form in a call</summary>

### What it does
Checks whether an argument is used as both a value and a type form in a call

### Links
* [Related issues](https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20conflicting-argument-forms)
* [View source](https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L116)
</details>

## `conflicting-declarations`

**Default level**: error

<details>
<summary>detects conflicting declarations</summary>

### What it does
Checks whether a variable has been declared as two conflicting types.

### Why is this bad
A variable with two conflicting declarations likely indicates a mistake.
Moreover, it could lead to incorrect or ill-defined type inference for
other code that relies on these variables.

### Links
* [Related issues](https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20conflicting-declarations)
* [View source](https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L126)
</details>

## `conflicting-metaclass`

**Default level**: error

<details>
<summary>detects conflicting metaclasses</summary>

TODO #14889

### Links
* [Related issues](https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20conflicting-metaclass)
* [View source](https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L141)
</details>

## `cyclic-class-definition`

**Default level**: error

<details>
<summary>detects cyclic class definitions</summary>

### What it does
Checks for class definitions with a cyclic inheritance chain.

### Why is it bad?
TODO #14889

### Links
* [Related issues](https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20cyclic-class-definition)
* [View source](https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L150)
</details>

## `division-by-zero`

**Default level**: error

<details>
<summary>detects division by zero</summary>

### What it does
It detects division by zero.

### Why is this bad?
Dividing by zero raises a `ZeroDivisionError` at runtime.

### Examples
```python
5 / 0
```

### Links
* [Related issues](https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20division-by-zero)
* [View source](https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L163)
</details>

## `duplicate-base`

**Default level**: error

<details>
<summary>detects class definitions with duplicate bases</summary>

### What it does
Checks for class definitions with duplicate bases.

### Why is this bad?
Class definitions with duplicate bases raise a `TypeError` at runtime.

### Links
* [Related issues](https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20duplicate-base)
* [View source](https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L181)
</details>

## `escape-character-in-forward-annotation`

**Default level**: error

<details>
<summary>detects forward type annotations with escape characters</summary>

TODO #14889

### Links
* [Related issues](https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20escape-character-in-forward-annotation)
* [View source](https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fstring_annotation.rs#L120)
</details>

## `fstring-type-annotation`

**Default level**: error

<details>
<summary>detects F-strings in type annotation positions</summary>

### What it does
Checks for f-strings in type annotation positions.

### Why is this bad?
Static analysis tools like ty can't analyse type annotations that use f-string notation.

### Examples
```python
def test(): -> f"int":
    ...
```

Use instead:
```python
def test(): -> "int":
    ...
```

### Links
* [Related issues](https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20fstring-type-annotation)
* [View source](https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fstring_annotation.rs#L11)
</details>

## `implicit-concatenated-string-type-annotation`

**Default level**: error

<details>
<summary>detects implicit concatenated strings in type annotations</summary>

### What it does
Checks for implicit concatenated strings in type annotation positions.

### Why is this bad?
Static analysis tools like ty can't analyse type annotations that use implicit concatenated strings.

### Examples
```python
def test(): -> "Literal[" "5" "]":
    ...
```

Use instead:
```python
def test(): -> "Literal[5]":
    ...
```

### Links
* [Related issues](https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20implicit-concatenated-string-type-annotation)
* [View source](https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fstring_annotation.rs#L86)
</details>

## `incompatible-slots`

**Default level**: error

<details>
<summary>detects class definitions whose MRO has conflicting <code>__slots__</code></summary>

### What it does
Checks for classes whose bases define incompatible `__slots__`.

### Why is this bad?
Inheriting from bases with incompatible `__slots__`s
will lead to a `TypeError` at runtime.

Classes with no or empty `__slots__` are always compatible:

```python
class A: ...
class B:
    __slots__ = ()
class C:
    __slots__ = ("a", "b")

## fine
class D(A, B, C): ...
```

Multiple inheritance from more than one different class
defining non-empty `__slots__` is not allowed:

```python
class A:
    __slots__ = ("a", "b")

class B:
    __slots__ = ("a", "b")  # Even if the values are the same

## TypeError: multiple bases have instance lay-out conflict
class C(A, B): ...
```

### Known problems
Dynamic (not tuple or string literal) `__slots__` are not checked.
Additionally, classes inheriting from built-in classes with implicit layouts
like `str` or `int` are also not checked.

```pycon
>>> hasattr(int, "__slots__")
False
>>> hasattr(str, "__slots__")
False
>>> class A(int, str): ...
Traceback (most recent call last):
  File "<python-input-0>", line 1, in <module>
    class A(int, str): ...
TypeError: multiple bases have instance lay-out conflict
```

### Links
* [Related issues](https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20incompatible-slots)
* [View source](https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L194)
</details>

## `inconsistent-mro`

**Default level**: error

<details>
<summary>detects class definitions with an inconsistent MRO</summary>

### What it does
Checks for classes with an inconsistent method resolution order (MRO).

### Why is this bad?
Classes with an inconsistent MRO will raise a `TypeError` at runtime.

### Links
* [Related issues](https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20inconsistent-mro)
* [View source](https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L280)
</details>

## `index-out-of-bounds`

**Default level**: error

<details>
<summary>detects index out of bounds errors</summary>

### What it does
Checks for attempts to use an out of bounds index to get an item from
a container.

### Why is this bad?
Using an out of bounds index will raise an `IndexError` at runtime.

### Links
* [Related issues](https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20index-out-of-bounds)
* [View source](https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L293)
</details>

## `invalid-argument-type`

**Default level**: error

<details>
<summary>detects call arguments whose type is not assignable to the corresponding typed parameter</summary>

### What it does
Detects call arguments whose type is not assignable to the corresponding typed parameter.

### Why is this bad?
Passing an argument of a type the function (or callable object) does not accept violates
the expectations of the function author and may cause unexpected runtime errors within the
body of the function.

### Examples
```python
def func(x: int): ...
func("foo")  # error: [invalid-argument-type]
```

### Links
* [Related issues](https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20invalid-argument-type)
* [View source](https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L307)
</details>

## `invalid-assignment`

**Default level**: error

<details>
<summary>detects invalid assignments</summary>

TODO #14889

### Links
* [Related issues](https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20invalid-assignment)
* [View source](https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L347)
</details>

## `invalid-attribute-access`

**Default level**: error

<details>
<summary>Invalid attribute access</summary>

### What it does
Makes sure that instance attribute accesses are valid.

### Examples
```python
class C:
  var: ClassVar[int] = 1

C.var = 3  # okay
C().var = 3  # error: Cannot assign to class variable
```

### Links
* [Related issues](https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20invalid-attribute-access)
* [View source](https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L1127)
</details>

## `invalid-base`

**Default level**: error

<details>
<summary>detects class definitions with an invalid base</summary>

TODO #14889

### Links
* [Related issues](https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20invalid-base)
* [View source](https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L356)
</details>

## `invalid-context-manager`

**Default level**: error

<details>
<summary>detects expressions used in with statements that don't implement the context manager protocol</summary>

TODO #14889

### Links
* [Related issues](https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20invalid-context-manager)
* [View source](https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L392)
</details>

## `invalid-declaration`

**Default level**: error

<details>
<summary>detects invalid declarations</summary>

TODO #14889

### Links
* [Related issues](https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20invalid-declaration)
* [View source](https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L401)
</details>

## `invalid-exception-caught`

**Default level**: error

<details>
<summary>detects exception handlers that catch classes that do not inherit from <code>BaseException</code></summary>

### What it does
Checks for exception handlers that catch non-exception classes.

### Why is this bad?
Catching classes that do not inherit from `BaseException` will raise a TypeError at runtime.

### Example
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

### References
- [Python documentation: except clause](https://docs.python.org/3/reference/compound_stmts.html#except-clause)
- [Python documentation: Built-in Exceptions](https://docs.python.org/3/library/exceptions.html#built-in-exceptions)

### Ruff rule
 This rule corresponds to Ruff's [`except-with-non-exception-classes` (`B030`)](https://docs.astral.sh/ruff/rules/except-with-non-exception-classes)

### Links
* [Related issues](https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20invalid-exception-caught)
* [View source](https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L410)
</details>

## `invalid-generic-class`

**Default level**: error

<details>
<summary>detects invalid generic classes</summary>

### What it does
Checks for the creation of invalid generic classes

### Why is this bad?
There are several requirements that you must follow when defining a generic class.

### Examples
```python
from typing import Generic, TypeVar

T = TypeVar("T")  # okay

## error: class uses both PEP-695 syntax and legacy syntax
class C[U](Generic[T]): ...
```

### References
- [Typing spec: Generics](https://typing.python.org/en/latest/spec/generics.html#introduction)

### Links
* [Related issues](https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20invalid-generic-class)
* [View source](https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L446)
</details>

## `invalid-legacy-type-variable`

**Default level**: error

<details>
<summary>detects invalid legacy type variables</summary>

### What it does
Checks for the creation of invalid legacy `TypeVar`s

### Why is this bad?
There are several requirements that you must follow when creating a legacy `TypeVar`.

### Examples
```python
from typing import TypeVar

T = TypeVar("T")  # okay
Q = TypeVar("S")  # error: TypeVar name must match the variable it's assigned to
T = TypeVar("T")  # error: TypeVars should not be redefined

## error: TypeVar must be immediately assigned to a variable
def f(t: TypeVar("U")): ...
```

### References
- [Typing spec: Generics](https://typing.python.org/en/latest/spec/generics.html#introduction)

### Links
* [Related issues](https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20invalid-legacy-type-variable)
* [View source](https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L472)
</details>

## `invalid-metaclass`

**Default level**: error

<details>
<summary>detects invalid <code>metaclass=</code> arguments</summary>

### What it does
Checks for arguments to `metaclass=` that are invalid.

### Why is this bad?
Python allows arbitrary expressions to be used as the argument to `metaclass=`.
These expressions, however, need to be callable and accept the same arguments
as `type.__new__`.

### Example

```python
def f(): ...

## TypeError: f() takes 0 positional arguments but 3 were given
class B(metaclass=f): ...
```

### References
- [Python documentation: Metaclasses](https://docs.python.org/3/reference/datamodel.html#metaclasses)

### Links
* [Related issues](https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20invalid-metaclass)
* [View source](https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L500)
</details>

## `invalid-overload`

**Default level**: error

<details>
<summary>detects invalid <code>@overload</code> usages</summary>

### What it does
Checks for various invalid `@overload` usages.

### Why is this bad?
The `@overload` decorator is used to define functions and methods that accepts different
combinations of arguments and return different types based on the arguments passed. This is
mainly beneficial for type checkers. But, if the `@overload` usage is invalid, the type
checker may not be able to provide correct type information.

### Example

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

### References
- [Python documentation: `@overload`](https://docs.python.org/3/library/typing.html#typing.overload)

### Links
* [Related issues](https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20invalid-overload)
* [View source](https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L527)
</details>

## `invalid-parameter-default`

**Default level**: error

<details>
<summary>detects default values that can't be assigned to the parameter's annotated type</summary>

### What it does
Checks for default values that can't be assigned to the parameter's annotated type.

### Why is this bad?
TODO #14889

### Links
* [Related issues](https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20invalid-parameter-default)
* [View source](https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L570)
</details>

## `invalid-protocol`

**Default level**: error

<details>
<summary>detects invalid protocol class definitions</summary>

### What it does
Checks for invalidly defined protocol classes.

### Why is this bad?
An invalidly defined protocol class may lead to the type checker inferring
unexpected things. It may also lead to `TypeError`s at runtime.

### Examples
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

### Links
* [Related issues](https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20invalid-protocol)
* [View source](https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L252)
</details>

## `invalid-raise`

**Default level**: error

<details>
<summary>detects <code>raise</code> statements that raise invalid exceptions or use invalid causes</summary>

Checks for `raise` statements that raise non-exceptions or use invalid
causes for their raised exceptions.

### Why is this bad?
Only subclasses or instances of `BaseException` can be raised.
For an exception's cause, the same rules apply, except that `None` is also
permitted. Violating these rules results in a `TypeError` at runtime.

### Examples
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

### References
- [Python documentation: The `raise` statement](https://docs.python.org/3/reference/simple_stmts.html#raise)
- [Python documentation: Built-in Exceptions](https://docs.python.org/3/library/exceptions.html#built-in-exceptions)

### Links
* [Related issues](https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20invalid-raise)
* [View source](https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L583)
</details>

## `invalid-return-type`

**Default level**: error

<details>
<summary>detects returned values that can't be assigned to the function's annotated return type</summary>

### What it does
Detects returned values that can't be assigned to the function's annotated return type.

### Why is this bad?
Returning an object of a type incompatible with the annotated return type may cause confusion to the user calling the function.

### Examples
```python
def func() -> int:
    return "a"  # error: [invalid-return-type]
```

### Links
* [Related issues](https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20invalid-return-type)
* [View source](https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L328)
</details>

## `invalid-super-argument`

**Default level**: error

<details>
<summary>detects invalid arguments for <code>super()</code></summary>

### What it does
Detects `super()` calls where:
- the first argument is not a valid class literal, or
- the second argument is not an instance or subclass of the first argument.

### Why is this bad?
`super(type, obj)` expects:
- the first argument to be a class,
- and the second argument to satisfy one of the following:
  - `isinstance(obj, type)` is `True`
  - `issubclass(obj, type)` is `True`

Violating this relationship will raise a `TypeError` at runtime.

### Examples
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

### References
- [Python documentation: super()](https://docs.python.org/3/library/functions.html#super)

### Links
* [Related issues](https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20invalid-super-argument)
* [View source](https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L626)
</details>

## `invalid-syntax-in-forward-annotation`

**Default level**: error

<details>
<summary>detects invalid syntax in forward annotations</summary>

TODO #14889

### Links
* [Related issues](https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20invalid-syntax-in-forward-annotation)
* [View source](https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fstring_annotation.rs#L111)
</details>

## `invalid-type-checking-constant`

**Default level**: error

<details>
<summary>detects invalid <code>TYPE_CHECKING</code> constant assignments</summary>

### What it does
Checks for a value other than `False` assigned to the `TYPE_CHECKING` variable, or an
annotation not assignable from `bool`.

### Why is this bad?
The name `TYPE_CHECKING` is reserved for a flag that can be used to provide conditional
code seen only by the type checker, and not at runtime. Normally this flag is imported from
`typing` or `typing_extensions`, but it can also be defined locally. If defined locally, it
must be assigned the value `False` at runtime; the type checker will consider its value to
be `True`. If annotated, it must be annotated as a type that can accept `bool` values.

### Links
* [Related issues](https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20invalid-type-checking-constant)
* [View source](https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L665)
</details>

## `invalid-type-form`

**Default level**: error

<details>
<summary>detects invalid type forms</summary>

### What it does
Checks for invalid type expressions.

### Why is this bad?
TODO #14889

### Links
* [Related issues](https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20invalid-type-form)
* [View source](https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L683)
</details>

## `invalid-type-variable-constraints`

**Default level**: error

<details>
<summary>detects invalid type variable constraints</summary>

TODO #14889

### Links
* [Related issues](https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20invalid-type-variable-constraints)
* [View source](https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L696)
</details>

## `invalid-union-call`

**Default level**: error

<details>
<summary>detects function calls that are inconsistent with one or more union type variants</summary>

### What it does
Checks if a function call on a value with a union type is correct for
all possible variants of that union.

### Why is this bad?
Failing to call a function correctly will raise a `TypeError` at runtime.

### Examples
```python
def f1() -> int: return 0
def f2(name: str) -> int: return 0

def _(flag: bool):
    if flag:
        f = f1
    else:
        f = f2
    x = f(3)
```

### Links
* [Related issues](https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20invalid-union-call)
* [View source](https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L365)
</details>

## `missing-argument`

**Default level**: error

<details>
<summary>detects missing required arguments in a call</summary>

### What it does
Checks for missing required arguments in a call.

### Why is this bad?
Failing to provide a required argument will raise a `TypeError` at runtime.

### Examples
```python
def func(x: int): ...
func()  # TypeError: func() missing 1 required positional argument: 'x'
```

### Links
* [Related issues](https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20missing-argument)
* [View source](https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L705)
</details>

## `no-matching-overload`

**Default level**: error

<details>
<summary>detects calls that do not match any overload</summary>

### What it does
Checks for calls to an overloaded function that do not match any of the overloads.

### Why is this bad?
Failing to provide the correct arguments to one of the overloads will raise a `TypeError`
at runtime.

### Examples
```python
@overload
def func(x: int): ...
@overload
def func(x: bool): ...
func("string")  # error: [no-matching-overload]
```

### Links
* [Related issues](https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20no-matching-overload)
* [View source](https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L724)
</details>

## `non-subscriptable`

**Default level**: error

<details>
<summary>detects subscripting objects that do not support subscripting</summary>

### What it does
Checks for subscripting objects that do not support subscripting.

### Why is this bad?
Subscripting an object that does not support it will raise a `TypeError` at runtime.

### Examples
```python
4[1]  # TypeError: 'int' object is not subscriptable
```

### Links
* [Related issues](https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20non-subscriptable)
* [View source](https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L747)
</details>

## `not-iterable`

**Default level**: error

<details>
<summary>detects iteration over an object that is not iterable</summary>

### What it does
Checks for objects that are not iterable but are used in a context that requires them to be.

### Why is this bad?
Iterating over an object that is not iterable will raise a `TypeError` at runtime.

### Examples

```python
for i in 34:  # TypeError: 'int' object is not iterable
    pass
```

### Links
* [Related issues](https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20not-iterable)
* [View source](https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L765)
</details>

## `parameter-already-assigned`

**Default level**: error

<details>
<summary>detects multiple arguments for the same parameter</summary>

### What it does
Checks for calls which provide more than one argument for a single parameter.

### Why is this bad?
Providing multiple values for a single parameter will raise a `TypeError` at runtime.

### Examples

```python
def f(x: int) -> int: ...

f(1, x=2)  # Error raised here
```

### Links
* [Related issues](https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20parameter-already-assigned)
* [View source](https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L816)
</details>

## `raw-string-type-annotation`

**Default level**: error

<details>
<summary>detects raw strings in type annotation positions</summary>

### What it does
Checks for raw-strings in type annotation positions.

### Why is this bad?
Static analysis tools like ty can't analyse type annotations that use raw-string notation.

### Examples
```python
def test(): -> r"int":
    ...
```

Use instead:
```python
def test(): -> "int":
    ...
```

### Links
* [Related issues](https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20raw-string-type-annotation)
* [View source](https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fstring_annotation.rs#L61)
</details>

## `static-assert-error`

**Default level**: error

<details>
<summary>Failed static assertion</summary>

### What it does
Makes sure that the argument of `static_assert` is statically known to be true.

### Examples
```python
from ty_extensions import static_assert

static_assert(1 + 1 == 3)  # error: evaluates to `False`

static_assert(int(2.0 * 3.0) == 6)  # error: does not have a statically known truthiness
```

### Links
* [Related issues](https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20static-assert-error)
* [View source](https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L1108)
</details>

## `subclass-of-final-class`

**Default level**: error

<details>
<summary>detects subclasses of final classes</summary>

### What it does
Checks for classes that subclass final classes.

### Why is this bad?
Decorating a class with `@final` declares to the type checker that it should not be subclassed.

### Example

```python
from typing import final

@final
class A: ...
class B(A): ...  # Error raised here
```

### Links
* [Related issues](https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20subclass-of-final-class)
* [View source](https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L886)
</details>

## `too-many-positional-arguments`

**Default level**: error

<details>
<summary>detects calls passing too many positional arguments</summary>

### What it does
Checks for calls that pass more positional arguments than the callable can accept.

### Why is this bad?
Passing too many positional arguments will raise `TypeError` at runtime.

### Example

```python
def f(): ...

f("foo")  # Error raised here
```

### Links
* [Related issues](https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20too-many-positional-arguments)
* [View source](https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L931)
</details>

## `type-assertion-failure`

**Default level**: error

<details>
<summary>detects failed type assertions</summary>

### What it does
Checks for `assert_type()` and `assert_never()` calls where the actual type
is not the same as the asserted type.

### Why is this bad?
`assert_type()` allows confirming the inferred type of a certain value.

### Example

```python
def _(x: int):
    assert_type(x, int)  # fine
    assert_type(x, str)  # error: Actual type does not match asserted type
```

### Links
* [Related issues](https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20type-assertion-failure)
* [View source](https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L909)
</details>

## `unavailable-implicit-super-arguments`

**Default level**: error

<details>
<summary>detects invalid <code>super()</code> calls where implicit arguments are unavailable.</summary>

### What it does
Detects invalid `super()` calls where implicit arguments like the enclosing class or first method argument are unavailable.

### Why is this bad?
When `super()` is used without arguments, Python tries to find two things:
the nearest enclosing class and the first argument of the immediately enclosing function (typically self or cls).
If either of these is missing, the call will fail at runtime with a `RuntimeError`.

### Examples
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

### References
- [Python documentation: super()](https://docs.python.org/3/library/functions.html#super)

### Links
* [Related issues](https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20unavailable-implicit-super-arguments)
* [View source](https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L952)
</details>

## `unknown-argument`

**Default level**: error

<details>
<summary>detects unknown keyword arguments in calls</summary>

### What it does
Checks for keyword arguments in calls that don't match any parameter of the callable.

### Why is this bad?
Providing an unknown keyword argument will raise `TypeError` at runtime.

### Example

```python
def f(x: int) -> int: ...

f(x=1, y=2)  # Error raised here
```

### Links
* [Related issues](https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20unknown-argument)
* [View source](https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L1007)
</details>

## `unresolved-attribute`

**Default level**: error

<details>
<summary>detects references to unresolved attributes</summary>

### What it does
Checks for unresolved attributes.

### Why is this bad?
Accessing an unbound attribute will raise an `AttributeError` at runtime. An unresolved attribute is not guaranteed to exist from the type alone, so this could also indicate that the object is not of the type that the user expects.

### Links
* [Related issues](https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20unresolved-attribute)
* [View source](https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L1028)
</details>

## `unresolved-import`

**Default level**: error

<details>
<summary>detects unresolved imports</summary>

### What it does
Checks for import statements for which the module cannot be resolved.

### Why is this bad?
Importing a module that cannot be resolved will raise an `ImportError`
at runtime.

### Links
* [Related issues](https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20unresolved-import)
* [View source](https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L1041)
</details>

## `unresolved-reference`

**Default level**: error

<details>
<summary>detects references to names that are not defined</summary>

### What it does
Checks for references to names that are not defined.

### Why is this bad?
Using an undefined variable will raise a `NameError` at runtime.

### Example

```python
print(x)  # NameError: name 'x' is not defined
```

### Links
* [Related issues](https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20unresolved-reference)
* [View source](https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L1055)
</details>

## `unsupported-bool-conversion`

**Default level**: error

<details>
<summary>detects boolean conversion where the object incorrectly implements <code>__bool__</code></summary>

### What it does
Checks for bool conversions where the object doesn't correctly implement `__bool__`.

### Why is this bad?
If an exception is raised when you attempt to evaluate the truthiness of an object,
using the object in a boolean context will fail at runtime.

### Examples

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

### Links
* [Related issues](https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20unsupported-bool-conversion)
* [View source](https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L785)
</details>

## `unsupported-operator`

**Default level**: error

<details>
<summary>detects binary, unary, or comparison expressions where the operands don't support the operator</summary>

### What it does
Checks for binary expressions, comparisons, and unary expressions where
the operands don't support the operator.

### Why is this bad?
Attempting to use an unsupported operator will raise a `TypeError` at
runtime.

### Links
* [Related issues](https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20unsupported-operator)
* [View source](https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L1074)
</details>

## `zero-stepsize-in-slice`

**Default level**: error

<details>
<summary>detects a slice step size of zero</summary>

### What it does
Checks for step size 0 in slices.

### Why is this bad?
A slice with a step size of zero will raise a `ValueError` at runtime.

### Examples
```python
l = list(range(10))
l[1:10:0]  # ValueError: slice step cannot be zero
```

### Links
* [Related issues](https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20zero-stepsize-in-slice)
* [View source](https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L1089)
</details>

## `call-possibly-unbound-method`

**Default level**: warn

<details>
<summary>detects calls to possibly unbound methods</summary>

### What it does
Checks for calls to possibly unbound methods.

### Why is this bad?
Calling an unbound method will raise an `AttributeError` at runtime.

### Links
* [Related issues](https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20call-possibly-unbound-method)
* [View source](https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L103)
</details>

## `invalid-ignore-comment`

**Default level**: warn

<details>
<summary>detects ignore comments that use invalid syntax</summary>

### What it does
Checks for `type: ignore` and `ty: ignore` comments that are syntactically incorrect.

### Why is this bad?
A syntactically incorrect ignore comment is probably a mistake and is useless.

### Examples
```py
a = 20 / 0  # type: ignoree
```

Use instead:

```py
a = 20 / 0  # type: ignore
```

### Links
* [Related issues](https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20invalid-ignore-comment)
* [View source](https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Fsuppression.rs#L65)
</details>

## `possibly-unbound-attribute`

**Default level**: warn

<details>
<summary>detects references to possibly unbound attributes</summary>

### What it does
Checks for possibly unbound attributes.

### Why is this bad?
Attempting to access an unbound attribute will raise an `AttributeError` at runtime.

### Links
* [Related issues](https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20possibly-unbound-attribute)
* [View source](https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L837)
</details>

## `possibly-unbound-import`

**Default level**: warn

<details>
<summary>detects possibly unbound imports</summary>

### What it does
Checks for imports of symbols that may be unbound.

### Why is this bad?
Importing an unbound module or name will raise a `ModuleNotFoundError`
or `ImportError` at runtime.

### Links
* [Related issues](https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20possibly-unbound-import)
* [View source](https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L850)
</details>

## `redundant-cast`

**Default level**: warn

<details>
<summary>detects redundant <code>cast</code> calls</summary>

### What it does
Detects redundant `cast` calls where the value already has the target type.

### Why is this bad?
These casts have no effect and can be removed.

### Example
```python
def f() -> int:
    return 10

cast(int, f())  # Redundant
```

### Links
* [Related issues](https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20redundant-cast)
* [View source](https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L1146)
</details>

## `undefined-reveal`

**Default level**: warn

<details>
<summary>detects usages of <code>reveal_type</code> without importing it</summary>

### What it does
Checks for calls to `reveal_type` without importing it.

### Why is this bad?
Using `reveal_type` without importing it will raise a `NameError` at runtime.

### Examples
TODO #14889

### Links
* [Related issues](https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20undefined-reveal)
* [View source](https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L991)
</details>

## `unknown-rule`

**Default level**: warn

<details>
<summary>detects <code>ty: ignore</code> comments that reference unknown rules</summary>

### What it does
Checks for `ty: ignore[code]` where `code` isn't a known lint rule.

### Why is this bad?
A `ty: ignore[code]` directive with a `code` that doesn't match
any known rule will not suppress any type errors, and is probably a mistake.

### Examples
```py
a = 20 / 0  # ty: ignore[division-by-zer]
```

Use instead:

```py
a = 20 / 0  # ty: ignore[division-by-zero]
```

### Links
* [Related issues](https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20unknown-rule)
* [View source](https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Fsuppression.rs#L40)
</details>

## `possibly-unresolved-reference`

**Default level**: ignore

<details>
<summary>detects references to possibly undefined names</summary>

### What it does
Checks for references to names that are possibly not defined.

### Why is this bad?
Using an undefined variable will raise a `NameError` at runtime.

### Example

```python
for i in range(0):
    x = i

print(x)  # NameError: name 'x' is not defined
```

### Links
* [Related issues](https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20possibly-unresolved-reference)
* [View source](https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Ftypes%2Fdiagnostic.rs#L864)
</details>

## `unused-ignore-comment`

**Default level**: ignore

<details>
<summary>detects unused <code>type: ignore</code> comments</summary>

### What it does
Checks for `type: ignore` or `ty: ignore` directives that are no longer applicable.

### Why is this bad?
A `type: ignore` directive that no longer matches any diagnostic violations is likely
included by mistake, and should be removed to avoid confusion.

### Examples
```py
a = 20 / 2  # ty: ignore[division-by-zero]
```

Use instead:

```py
a = 20 / 2
```

### Links
* [Related issues](https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20unused-ignore-comment)
* [View source](https://github.com/astral-sh/ruff/blob/main/crates%2Fty_python_semantic%2Fsrc%2Fsuppression.rs#L15)
</details>

