# Unsound assignments

In addition to `invalid-assignment`, we also offer a disabled-by-default stricter rule
`unsound-assignment`. This rule forbids assigning a value of type `A` to a fully static declared
type `B` unless `A` is a *subtype* of `B`.

```toml
[rules]
unsound-assignment = "error"
```

## Basics

An assignment that is valid according to the usual assignability rules can still be unsound.

```py
from typing import Any

def returns_any() -> Any:
    return "not an integer"

# snapshot: unsound-assignment
value: int = returns_any()
```

```snapshot
error[unsound-assignment]: Unsound assignment
 --> src/mdtest_snippet.py:7:14
  |
7 | value: int = returns_any()
  |        ---   ^^^^^^^^^^^^^ Inferred as `Any`
  |        |
  |        Expected a subtype of `int` because of this annotation
info: `Any` is assignable to `int`, but not a subtype of `int`
help: Consider using an `assert` to narrow the type before assigning it
```

A nested dynamic type causes the same problem, while genuinely incompatible values cause us to emit
only `invalid-assignment`.

```py
# snapshot: unsound-assignment
nested_value: tuple[tuple[int, int]] = ((42, returns_any()),)

invalid_value: int = "not an integer"  # error: [invalid-assignment]
```

```snapshot
error[unsound-assignment]: Unsound assignment
 --> src/mdtest_snippet.py:9:40
  |
9 | nested_value: tuple[tuple[int, int]] = ((42, returns_any()),)
  |               ----------------------   ^^^^^^^^^^^^^^^^^^^^^^ Inferred as `tuple[tuple[Literal[42], Any]]`
  |               |
  |               Expected a subtype of `tuple[tuple[int, int]]` because of this annotation
info: `tuple[tuple[Literal[42], Any]]` is assignable to `tuple[tuple[int, int]]`, but not a subtype of `tuple[tuple[int, int]]`
info: the first tuple element is not compatible: `tuple[Literal[42], Any]` is not a subtype of `tuple[int, int]`
info: └── the second tuple element is not compatible: `Any` is not a subtype of `int`
help: Consider using an `assert` to narrow the type before assigning it
```

Narrowing a dynamic value before assigning it makes the assignment sound.

```py
dynamic_value = returns_any()
assert isinstance(dynamic_value, int)
narrowed_value: int = dynamic_value
```

## Unsound assignments to gradually typed targets

The rule applies only when the target's declared type is fully static. An explicit `Any`, an alias
of `Any`, or an `Any` nested inside the annotation disables the strict check.

```py
from typing import Any
from typing_extensions import Never, TypeAliasType

AnyAlias = TypeAliasType("AnyAlias", Any)

def returns_any() -> Any:
    return "not an integer"

dynamic_target: Any = returns_any()  # no `unsound-assignment` error
aliased_dynamic_target: AnyAlias = returns_any()  # no `unsound-assignment` error
nested_dynamic_target: tuple[int, Any] = returns_any()  # no `unsound-assignment` error

# error: [missing-type-argument]
unknown_target: list = returns_any()  # no `unsound-assignment` error
```

`Never`, on the other hand, is fully static, so assigning `Any` to it is unsound.

```py
never_target: Never = returns_any()  # error: [unsound-assignment]
```

## Unsound assignments to an existing annotation

The same check applies when an assignment's target was annotated separately.

```py
from typing import Any

def returns_any() -> Any:
    return "not an integer"

value: int

# snapshot: unsound-assignment
value = returns_any()
```

```snapshot
error[unsound-assignment]: Unsound assignment
 --> src/mdtest_snippet.py:9:9
  |
6 | value: int
  |        --- Expected a subtype of `int` because of this annotation
7 |
8 | # snapshot: unsound-assignment
9 | value = returns_any()
  |         ^^^^^^^^^^^^^ Inferred as `Any`
info: `Any` is assignable to `int`, but not a subtype of `int`
help: Consider using an `assert` to narrow the type before assigning it
```

Subsequent reassignments of an annotated variable are also checked for soundness.

```py
another_value: int = 42
another_value = returns_any()  # error: [unsound-assignment]
```

## Unsound assignments to annotated parameters

Reassigning an annotated parameter points to its original type annotation.

```py
from typing import Any

def returns_any() -> Any:
    return "not an integer"

def update(value: int) -> None:
    value = returns_any()  # snapshot: unsound-assignment
```

```snapshot
error[unsound-assignment]: Unsound assignment
 --> src/mdtest_snippet.py:7:13
  |
6 | def update(value: int) -> None:
  |                   --- Expected a subtype of `int` because of this annotation
7 |     value = returns_any()  # snapshot: unsound-assignment
  |             ^^^^^^^^^^^^^ Inferred as `Any`
info: `Any` is assignable to `int`, but not a subtype of `int`
help: Consider using an `assert` to narrow the type before assigning it
```

## Unsound assignments to variadic positional parameters

A variadic positional parameter's annotation describes its arguments, while the parameter itself is
a tuple.

```py
from typing import Any

def returns_any() -> Any:
    return "not a tuple"

def update(*values: int) -> None:
    values = returns_any()  # snapshot: unsound-assignment
```

```snapshot
error[unsound-assignment]: Unsound assignment
 --> src/mdtest_snippet.py:7:14
  |
6 | def update(*values: int) -> None:
  |                     --- Variadic parameter annotation declares the type as `tuple[int, ...]`
7 |     values = returns_any()  # snapshot: unsound-assignment
  |              ^^^^^^^^^^^^^ Inferred as `Any`
info: `Any` is assignable to `tuple[int, ...]`, but not a subtype of `tuple[int, ...]`
help: Consider using an `assert` to narrow the type before assigning it
```

## Unsound assignments with same-named types

A variadic parameter's annotation uses the same qualified type names as the rest of the diagnostic.

`first.py`:

```py
class Value: ...
```

`second.py`:

```py
class Value: ...
```

```py
from typing import Any
import first
import second

def returns_any() -> Any:
    return "not a Value"

def update(*values: first.Value | second.Value) -> None:
    values = (first.Value(), returns_any())  # snapshot: unsound-assignment
```

```snapshot
error[unsound-assignment]: Unsound assignment
 --> src/mdtest_snippet.py:9:14
  |
8 | def update(*values: first.Value | second.Value) -> None:
  |                     -------------------------- Variadic parameter annotation declares the type as `tuple[first.Value | second.Value, ...]`
9 |     values = (first.Value(), returns_any())  # snapshot: unsound-assignment
  |              ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ Inferred as `tuple[first.Value, Any]`
info: `tuple[first.Value, Any]` is assignable to `tuple[first.Value | second.Value, ...]`, but not a subtype of `tuple[first.Value | second.Value, ...]`
help: Consider using an `assert` to narrow the type before assigning it
```

## Unsound assignments to variadic keyword parameters

A variadic keyword parameter's annotation describes its values, while the parameter itself is a
dictionary.

```py
from typing import Any

def returns_any() -> Any:
    return "not a dictionary"

def update(**values: int) -> None:
    values = returns_any()  # snapshot: unsound-assignment
```

```snapshot
error[unsound-assignment]: Unsound assignment
 --> src/mdtest_snippet.py:7:14
  |
6 | def update(**values: int) -> None:
  |                      --- Keyword-variadic parameter annotation declares the type as `dict[str, int]`
7 |     values = returns_any()  # snapshot: unsound-assignment
  |              ^^^^^^^^^^^^^ Inferred as `Any`
info: `Any` is assignable to `dict[str, int]`, but not a subtype of `dict[str, int]`
help: Consider using an `assert` to narrow the type before assigning it
```

## Unsound assignments with conflicting declarations

When conflicting annotations contribute to the declared type, the diagnostic does not identify any
one annotation as the declared type.

```py
from typing import Any

def returns_any() -> Any:
    return "not necessarily an integer or a string"

def update(flag: bool) -> None:
    if flag:
        value: int
    else:
        value: str

    # error: [conflicting-declarations]
    value = returns_any()  # snapshot: unsound-assignment
```

```snapshot
error[unsound-assignment]: Unsound assignment
  --> src/mdtest_snippet.py:13:13
   |
13 |     value = returns_any()  # snapshot: unsound-assignment
   |     -----   ^^^^^^^^^^^^^ Inferred as `Any`
   |     |
   |     Expected a subtype of `int | str` because of its declared type
info: `Any` is assignable to `int | str`, but not a subtype of `int | str`
help: Consider using an `assert` to narrow the type before assigning it
```

## Unsound assignments with equivalent declarations

Distinct branches declaring the same type still establish a fully static assignment boundary.

```py
from typing import Any

def returns_any() -> Any:
    return "not an integer"

def update(flag: bool) -> None:
    if flag:
        value: int
    else:
        value: int

    value = returns_any()  # error: [unsound-assignment]
```

## Unsound named and unpacked assignments

Assignment expressions are also checked against an existing annotation:

```py
from typing import Any

def returns_any() -> Any:
    return "not an integer"

named_value: int

if named_value := returns_any():  # snapshot: unsound-assignment
    pass
```

```snapshot
error[unsound-assignment]: Unsound assignment
 --> src/mdtest_snippet.py:8:19
  |
6 | named_value: int
  |              --- Expected a subtype of `int` because of this annotation
7 |
8 | if named_value := returns_any():  # snapshot: unsound-assignment
  |                   ^^^^^^^^^^^^^ Inferred as `Any`
info: `Any` is assignable to `int`, but not a subtype of `int`
help: Consider using an `assert` to narrow the type before assigning it
```

An unpacked assignment points to the individual expression that supplies its unsound value.

```py
unpacked_value: int
unpacked_value, other_value = (returns_any(), "hello")  # snapshot: unsound-assignment
```

```snapshot
error[unsound-assignment]: Unsound assignment
  --> src/mdtest_snippet.py:11:32
   |
10 | unpacked_value: int
   |                 --- Expected a subtype of `int` because of this annotation
11 | unpacked_value, other_value = (returns_any(), "hello")  # snapshot: unsound-assignment
   | --------------                 ^^^^^^^^^^^^^ Inferred as `Any`
   | |
   | Assigned to this variable
info: `Any` is assignable to `int`, but not a subtype of `int`
help: Consider using an `assert` to narrow the type before assigning it
```

## Unsound assignments to nested unpacking targets

A nested tuple target points to the corresponding dynamic expression in the nested value.

```py
from typing import Any

def returns_any() -> Any:
    return "not an integer"

value: int
other, (value, last) = (0, (returns_any(), 1))  # snapshot: unsound-assignment
```

```snapshot
error[unsound-assignment]: Unsound assignment
 --> src/mdtest_snippet.py:7:29
  |
6 | value: int
  |        --- Expected a subtype of `int` because of this annotation
7 | other, (value, last) = (0, (returns_any(), 1))  # snapshot: unsound-assignment
  |         -----               ^^^^^^^^^^^^^ Inferred as `Any`
  |         |
  |         Assigned to this variable
info: `Any` is assignable to `int`, but not a subtype of `int`
help: Consider using an `assert` to narrow the type before assigning it
```

## Unsound assignments to starred unpacking targets

A starred unpacking target identifies the dynamic expression collected into the assigned list.

```py
from typing import Any

def returns_any() -> Any:
    return "not an integer"

middle: list[int]
first, *middle, last = (0, returns_any(), 1)  # snapshot: unsound-assignment
```

```snapshot
error[unsound-assignment]: Unsound assignment
 --> src/mdtest_snippet.py:7:28
  |
6 | middle: list[int]
  |         --------- Expected a subtype of `list[int]` because of this annotation
7 | first, *middle, last = (0, returns_any(), 1)  # snapshot: unsound-assignment
  |         ------             ^^^^^^^^^^^^^ Iterable element inferred as `Any` (expected a subtype of `int`)
  |         |
  |         Assigned to this variable
info: `list[Any]` is assignable to `list[int]`, but not a subtype of `list[int]`
help: Consider using an `assert` to narrow the type before assigning it
```

## Multiple unsound values assigned to starred unpacking targets

When a starred target collects multiple values, the unsound-assignment diagnostic covers the entire
collected slice without including the surrounding unpacked values.

```py
from typing import Any

def returns_any() -> Any:
    return "not an integer"

middle: list[int]
first, *middle, last = (0, 1, returns_any(), 2, 3)  # snapshot: unsound-assignment
```

```snapshot
error[unsound-assignment]: Unsound assignment
 --> src/mdtest_snippet.py:7:28
  |
6 | middle: list[int]
  |         --------- Expected a subtype of `list[int]` because of this annotation
7 | first, *middle, last = (0, 1, returns_any(), 2, 3)  # snapshot: unsound-assignment
  |         ------             ^^^^^^^^^^^^^^^^^^^ Iterable element inferred as `Literal[1, 2] | Any` (expected a subtype of `int`)
  |         |
  |         Assigned to this variable
info: `list[Literal[1, 2] | Any]` is assignable to `list[int]`, but not a subtype of `list[int]`
help: Consider using an `assert` to narrow the type before assigning it
```

## Unsound assignments to for-loop targets

An unsound loop assignment points to the target's earlier type annotation.

```py
from typing import Any, cast

value: int

for value in cast(list[Any], []):  # snapshot: unsound-assignment
    pass
```

```snapshot
error[unsound-assignment]: Unsound assignment
 --> src/mdtest_snippet.py:5:5
  |
3 | value: int
  |        --- Expected a subtype of `int` because of this annotation
4 |
5 | for value in cast(list[Any], []):  # snapshot: unsound-assignment
  |     ^^^^^ Inferred as `Any`
info: `Any` is assignable to `int`, but not a subtype of `int`
help: Consider using an `assert` to narrow the type before assigning it
```

## Unsound assignments to context-manager targets

Context-manager targets are checked against their earlier type annotations.

```py
from contextlib import nullcontext
from typing import Any

def returns_any() -> Any:
    return "not an integer"

value: int

with nullcontext(returns_any()) as value:  # error: [unsound-assignment]
    pass
```

## Unsound assignments to global and nonlocal variables

Assignments redirected by `global` or `nonlocal` are checked against the owning scope's declared
type.

```py
from typing import Any

def returns_any() -> Any:
    return "not an integer"

global_value: int = 42

def update_global() -> None:
    global global_value
    global_value = returns_any()  # snapshot: unsound-assignment

def outer() -> None:
    nonlocal_value: int = 42

    def update_nonlocal() -> None:
        nonlocal nonlocal_value
        nonlocal_value = returns_any()  # snapshot: unsound-assignment
```

```snapshot
error[unsound-assignment]: Unsound assignment
  --> src/mdtest_snippet.py:10:20
   |
 6 | global_value: int = 42
   |               --- Expected a subtype of `int` because of this annotation
 7 |
 8 | def update_global() -> None:
 9 |     global global_value
10 |     global_value = returns_any()  # snapshot: unsound-assignment
   |                    ^^^^^^^^^^^^^ Inferred as `Any`
info: `Any` is assignable to `int`, but not a subtype of `int`
help: Consider using an `assert` to narrow the type before assigning it


error[unsound-assignment]: Unsound assignment
  --> src/mdtest_snippet.py:17:26
   |
13 |     nonlocal_value: int = 42
   |                     --- Expected a subtype of `int` because of this annotation
14 |
15 |     def update_nonlocal() -> None:
16 |         nonlocal nonlocal_value
17 |         nonlocal_value = returns_any()  # snapshot: unsound-assignment
   |                          ^^^^^^^^^^^^^ Inferred as `Any`
info: `Any` is assignable to `int`, but not a subtype of `int`
help: Consider using an `assert` to narrow the type before assigning it
```

## Unsound augmented assignments

An augmented assignment highlights the dynamic right-hand operand.

```py
from typing import Any

def returns_any() -> Any:
    return "not an integer"

value: int = 42
value += returns_any()  # snapshot: unsound-assignment
```

```snapshot
error[unsound-assignment]: Unsound assignment
 --> src/mdtest_snippet.py:7:10
  |
6 | value: int = 42
  |        --- Expected a subtype of `int` because of this annotation
7 | value += returns_any()  # snapshot: unsound-assignment
  |          ^^^^^^^^^^^^^ Inferred as `Any`
info: `Any` is assignable to `int`, but not a subtype of `int`
help: Consider using an `assert` to narrow the type before assigning it
```

When an in-place operator returns `Any`, the diagnostic highlights the full operation instead of
incorrectly attributing that type to a statically typed operand.

```py
class Counter:
    def __iadd__(self, other: int) -> Any:
        return "not a Counter"

counter: Counter = Counter()
counter += 1  # snapshot: unsound-assignment
```

```snapshot
error[unsound-assignment]: Unsound assignment
  --> src/mdtest_snippet.py:13:1
   |
12 | counter: Counter = Counter()
   |          ------- Expected a subtype of `Counter` because of this annotation
13 | counter += 1  # snapshot: unsound-assignment
   | ^^^^^^^^^^^^ Augmented assignment produces a value of type `Any`
info: `Any` is assignable to `Counter`, but not a subtype of `Counter`
help: Consider using an `assert` to narrow the type before assigning it
```

## Assignments in dataclass bodies

Assignments directly in a dataclass body are ignored by `unsound-assignment` because of how heavily
dataclass field specifiers are special-cased by ty and other type checkers. ty considers
`dataclasses.Field[str]` assignable to `str` in order to avoid emitting a diagnostic for
`x: str = dataclasses.field(default="foo")` in a dataclass class body, but limits this special case
to assignability: it does not consider `dataclasses.Field[str]` a *subtype* of `str`. You might
think that we could workaround this with a narrow special case for just `dataclasses.Field`, but
this alone would not be sufficient: third-party libraries often wrap `dataclasses.field()` and
annotate their field specifiers as returning `Any`, so the inferred assignment type no longer
identifies the underlying `Field`. For example:

- [`betterproto` explicitly explains why its field specifiers return `Any`](https://github.com/danielgtaylor/python-betterproto/blob/098989e9e93c97e16e10257b1b3575f987180f8c/src/betterproto/__init__.py#L192-L220).
- [Expression's `case()` and `tag()` field specifiers do the same](https://github.com/cognitedata/Expression/blob/d0bcfbe1ce12634ef74531b4404d1bed6c05a090/expression/core/tagged_union.py#L190-L197).

Flagging those assignments would report a huge number of dataclasses as being unsound, making it
untenable for users to enable the rule.

```py
from dataclasses import dataclass, field
from typing import Any

def returns_any() -> Any:
    return "not an integer"

def wrapped_field() -> Any:
    return field()

@dataclass
class Example:
    required: int = field()
    without_init: int = field(init=False)
    with_default: int = field(default=42)
    with_factory: list[int] = field(default_factory=lambda: [42])
    with_none: int | None = field(default=None)

    wrapped: int = wrapped_field()
    dynamic_value: int = returns_any()
    invalid_default: int = field(default="not an integer")  # error: [invalid-assignment]

    def method(self) -> None:
        value: int = returns_any()  # error: [unsound-assignment]

    class Nested:
        value: int = returns_any()  # error: [unsound-assignment]
```

An ordinary class does not receive the dataclass-body exemption.

```py
class Ordinary:
    value: int = returns_any()  # error: [unsound-assignment]
```

## Assignments in dataclass-transform class bodies

The body of a class inheriting from a `dataclass_transform` base is ignored even when its field
specifier is not registered with the transform.

```py
from typing import Any, TypeVar
from typing_extensions import dataclass_transform

def custom_field() -> Any:
    return 42

@dataclass_transform()
class Model: ...

class CustomExample(Model):
    value: int = custom_field()

    def method(self) -> None:
        value: int = custom_field()  # error: [unsound-assignment]

    class Nested:
        value: int = custom_field()  # error: [unsound-assignment]
```

The same exemption applies when a class becomes dataclass-like through its decorator.

```py
T = TypeVar("T")

@dataclass_transform()
def transform(cls: type[T]) -> type[T]:
    return cls

@transform
class DecoratedModel:
    value: int = custom_field()
```

A dataclass-transform metaclass also makes its class body exempt.

```py
@dataclass_transform()
class ModelMetaclass(type): ...

class MetaclassModel(metaclass=ModelMetaclass):
    value: int = custom_field()
```

## Assignments in stub files

In stub files, assigning to an ellipsis (`= ...`) is a syntactic special case that is allowed
regardless of the declared type. We do not emit `unsound-assignment` for this:

```pyi
def f(x: int = ...): ...  # no error

x: int = ...  # no error
y: str
y = ...  # no error
```
