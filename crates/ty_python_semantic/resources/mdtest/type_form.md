# `TypeForm`

```toml
[environment]
python-version = "3.12"
```

## Contextual type-form expressions and covariance

```py
from typing import Any, assert_type
from typing_extensions import TypeForm

broad: TypeForm[Any] = int | str
specific: TypeForm[str] = str

assert_type(broad, TypeForm[Any])
assert_type(specific, TypeForm[str])

broad = specific
specific = broad

invalid: TypeForm[str] = int  # error: [invalid-assignment]
```

## Existing type-form values and union contexts

Implicit type-form evaluation should not reinterpret ordinary expressions whose value already has a
`TypeForm` type. It also applies when `TypeForm` is one branch of the expected type.

```py
from typing import Any, Literal, Never
from typing_extensions import TypeForm

def get_form() -> TypeForm[str]:
    return str

def choose(value: TypeForm[str], flag: bool) -> TypeForm[str]:
    return value if flag else get_form()

class Holder:
    item: TypeForm[str]

def use_existing(holder: Holder, values: list[TypeForm[str]]) -> None:
    by_attribute: TypeForm[str] = holder.item
    by_subscript: TypeForm[str] = values[0]

def incompatible_existing(value: TypeForm[int], runtime_type: type[int]) -> None:
    invalid_form: TypeForm[str] = value  # error: [invalid-assignment]
    invalid_runtime_type: TypeForm[str] = runtime_type  # error: [invalid-assignment]

def accept_gradual(value: Any) -> None:
    dynamic: TypeForm[str] = value

def abort() -> Never:
    raise RuntimeError

quoted: TypeForm[str] | None = "str"
union_syntax: TypeForm[str | None] | None = str | None
literal_syntax: TypeForm[None] | None = Literal[None]
ordinary_arm: TypeForm[str] | int = 1
bottom: TypeForm[str] = abort()
```

## Bare `TypeForm` validates type-form expressions

```py
from typing import ClassVar
from typing_extensions import TypeForm

def accepts_type_form(x: TypeForm) -> None: ...

accepts_type_form(int)
accepts_type_form("int")
accepts_type_form("not a type")  # error: [invalid-syntax-in-forward-annotation]

bad_tuple: TypeForm = (1, 2)  # error: [invalid-type-form]
bad_qualifier: TypeForm = ClassVar[int]  # error: [invalid-type-form]
```

## Explicit construction and `type[T]` compatibility

```py
from typing import assert_type
from typing_extensions import TypeForm

constructed = TypeForm("list[int]")
assert_type(constructed, TypeForm[list[int]])

TypeForm("type(1)")  # error: [invalid-type-form]
TypeForm()  # error: [invalid-type-form]
TypeForm(int, str)  # error: [invalid-type-form]
TypeForm(value=int)  # error: [invalid-type-form]
TypeForm(*(int,))  # error: [invalid-type-form]

def get_type() -> type[int]:
    return int

compatible: TypeForm[int | str] = get_type()
incompatible: TypeForm[str] = get_type()  # error: [invalid-assignment]
```

## Generic specialization

```py
from typing import assert_type
from typing_extensions import TypeForm

def construct[T](form: TypeForm[T]) -> T:
    raise NotImplementedError

assert_type(construct(int), int)
assert_type(construct(list[int]), list[int])
assert_type(construct(int | str), int | str)

def use_runtime_type(form: type[int]) -> None:
    assert_type(construct(form), int)

type Alias[T] = TypeForm[T]

def construct_alias[T](form: Alias[T]) -> T:
    raise NotImplementedError

assert_type(construct_alias(int), int)
```

## Narrowing to a runtime class

```py
from typing import assert_type
from typing_extensions import TypeForm

def as_type[T](form: TypeForm[T]) -> type[T] | None:
    if isinstance(form, type):
        assert_type(form, type[T])
        return form
    return None

class A: ...

assert_type(as_type(A), type[A] | None)
```
