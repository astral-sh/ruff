# `TypeForm`

A `TypeForm[T]` value is a runtime object that represents a type expression whose type is `T`.
Classes, generic aliases, unions, string forward annotations, and type aliases can all appear in a
`TypeForm` context.

```toml
[environment]
python-version = "3.12"
```

## Basic

When an expression appears in a `TypeForm` context, valid type-expression syntax is interpreted as a
`TypeForm` value. The type argument of `TypeForm` is the type denoted by the expression, not the
ordinary runtime type of the expression object.

```py
from typing_extensions import TypeForm

def accepts_str(form: TypeForm[str]) -> None: ...
def accepts_union(form: TypeForm[int | str]) -> None: ...

string_form: TypeForm[str] = str
union_form: TypeForm[int | str] = int | str
list_form: TypeForm[list[int]] = list[int]
quoted: TypeForm[str] = "str"

type Alias = str
aliased: TypeForm[str] = Alias

accepts_str(str)
accepts_str("str")
accepts_union(int | str)

def returns_union() -> TypeForm[int | str]:
    return int | str

reveal_type(string_form)  # revealed: TypeForm[str]
reveal_type(union_form)  # revealed: TypeForm[int | str]
reveal_type(list_form)  # revealed: TypeForm[list[int]]
reveal_type(quoted)  # revealed: TypeForm[str]
reveal_type(aliased)  # revealed: TypeForm[str]
```

## Contextual typing in containers

`TypeForm` interpretation is also applied when the expected type is nested inside a container or
typed dictionary.

```py
from typing import Annotated, Any, Literal, LiteralString, Never, Self, TypedDict
from typing_extensions import TypeForm

class Config(TypedDict):
    form: TypeForm[int]

forms: list[TypeForm[int]] = [int, Literal[1]]
pair: tuple[TypeForm[int], TypeForm[str]] = (int, str)
config: Config = {"form": int}

stored_union_forms = (int | str,)
stored_union_forms_ok: tuple[TypeForm[int | str], ...] = stored_union_forms

stored_literal_forms = (Literal[1],)
stored_literal_forms_ok: tuple[TypeForm[Literal[1]], ...] = stored_literal_forms

stored_annotated_forms = (Annotated[int, "metadata"],)
stored_annotated_forms_ok: tuple[TypeForm[int], ...] = stored_annotated_forms

type StoredAlias = str
stored_alias_forms = (StoredAlias,)
stored_alias_forms_ok: tuple[TypeForm[str], ...] = stored_alias_forms

stored_special_forms = (Never, LiteralString)
stored_special_forms_ok: tuple[TypeForm[Any], ...] = stored_special_forms

class WithSelf:
    contextual_self_forms: tuple[TypeForm[Self], ...] = (Self,)
    stored_self_forms = (Self,)

    # TODO: This should be accepted; retain `Self`'s class binding through containers.
    stored_self_forms_ok: tuple[TypeForm[Self], ...] = stored_self_forms  # error: [invalid-assignment]

module_self_forms = (Self,)
invalid_module_self_forms: tuple[TypeForm[Any], ...] = module_self_forms  # error: [invalid-assignment]

bad_forms: list[TypeForm[int]] = [str]  # error: [invalid-assignment]
bad_config: Config = {"form": str}  # error: [invalid-argument-type]
```

`TypeForm` is covariant in its type argument:

```py
from typing_extensions import TypeForm

def check_covariance(
    int_form: TypeForm[int],
    object_form: TypeForm[object],
    str_form: TypeForm[str],
) -> None:
    object_form = str_form
    str_form = int_form  # error: [invalid-assignment]

def reject_wide_type_form(object_form: TypeForm[object]) -> None:
    str_form: TypeForm[str] = object_form  # error: [invalid-assignment]

invalid: TypeForm[str] = int  # error: [invalid-assignment]
```

`TypeForm` covariance composes with the variance of nested generic type arguments. `list` is
invariant, while callable parameter types are contravariant:

```py
from typing import Callable
from typing_extensions import TypeForm

def check_invariant_type_argument(
    str_list_form: TypeForm[list[str]],
    object_list_form: TypeForm[list[object]],
) -> None:
    invalid_object_list: TypeForm[list[object]] = str_list_form  # error: [invalid-assignment]
    invalid_str_list: TypeForm[list[str]] = object_list_form  # error: [invalid-assignment]

def check_contravariant_type_argument(
    accepts_object_form: TypeForm[Callable[[object], None]],
    accepts_str_form: TypeForm[Callable[[str], None]],
) -> None:
    accepts_str_expected: TypeForm[Callable[[str], None]] = accepts_object_form
    accepts_object_expected: TypeForm[Callable[[object], None]] = accepts_str_form  # error: [invalid-assignment]
```

## Preserving existing `TypeForm` values

Contextual `TypeForm` inference should not reinterpret an expression whose ordinary value type is
already a `TypeForm`. This matters for names, subscripts, conditional expressions, and other
ordinary value expressions that can produce a `TypeForm` at runtime.

```py
from typing import Protocol, assert_type, cast
from typing_extensions import TypeForm
from ty_extensions import Intersection, Not

def get_form() -> TypeForm[str]:
    return str

class Holder:
    item: TypeForm[str]

def use_existing(
    value: TypeForm[str],
    holder: Holder,
    values: list[TypeForm[str]],
    flag: bool,
) -> None:
    by_name: TypeForm[str] = value
    by_attribute: TypeForm[str] = holder.item
    by_subscript: TypeForm[str] = values[0]
    by_condition: TypeForm[str] = value if flag else get_form()

def reject_incompatible_existing(value: TypeForm[int]) -> None:
    invalid: TypeForm[str] = value  # error: [invalid-assignment]

type FormUnion = TypeForm[int] | TypeForm[str]

def use_existing_union(form: TypeForm[int] | TypeForm[str]) -> None:
    reveal_type(cast(form, object()))  # revealed: int | str

def use_existing_alias(form: FormUnion, value: int | str) -> None:
    assert_type(value, form)

type RecursiveForm = RecursiveForm | TypeForm[int]

def use_recursive_alias(form: RecursiveForm) -> None:
    reveal_type(cast(form, object()))  # revealed: int
    preserved: TypeForm[int] = form

class HasX(Protocol):
    x: int

def use_intersection(form: Intersection[TypeForm[int], HasX], value: int) -> None:
    reveal_type(cast(form, object()))  # revealed: int
    assert_type(value, form)

def use_negative_facet(form: Intersection[TypeForm[int], Not[TypeForm[bool]]]) -> None:
    reveal_type(cast(form, object()))  # revealed: int

def use_bound[T: TypeForm[int]](form: T, value: int) -> None:
    reveal_type(cast(form, object()))  # revealed: int
    assert_type(value, form)

def use_constraints[T: (TypeForm[int], TypeForm[str])](form: T, value: int | str) -> None:
    reveal_type(cast(form, object()))  # revealed: int | str
    assert_type(value, form)
```

## Runtime class objects and gradual values

Runtime class objects are also valid `TypeForm` values when their instance type is compatible with
the `TypeForm` type argument. Bare `type` is treated as gradual and is only accepted by
`TypeForm[Any]`.

```py
from typing import Any, Never
from typing_extensions import TypeForm

def accept_gradual_type_argument(
    any_form: TypeForm[Any],
    str_form: TypeForm[str],
) -> None:
    any_form = str_form
    str_form = any_form

def accept_runtime_classes(
    exact: type[int],
    broad: type[object],
    bare: type,
) -> None:
    exact_form: TypeForm[int | str] = exact
    broad_form: TypeForm[object] = broad
    unparameterized_form: TypeForm = broad
    bare_form: TypeForm[Any] = bare

    invalid_broad: TypeForm[str] = broad  # error: [invalid-assignment]
    invalid_bare: TypeForm[str] = bare  # error: [invalid-assignment]

def accept_gradual_and_bottom(dynamic: Any, bottom: Never) -> None:
    dynamic_form: TypeForm[str] = dynamic
    bottom_form: TypeForm[str] = bottom
```

## Union contexts

If a union contains both `TypeForm` and non-`TypeForm` arms, ordinary expression inference should
win when it satisfies the non-`TypeForm` arm. Otherwise, ty tries the `TypeForm` interpretation.

```py
from typing import Literal
from typing_extensions import TypeForm

ordinary_string: TypeForm[str] | str = "str"
reveal_type(ordinary_string)  # revealed: Literal["str"]

ordinary_none: TypeForm[None] | None = None
reveal_type(ordinary_none)  # revealed: None

ordinary_int: TypeForm[str] | int = 1

quoted_form: TypeForm[str] | None = "str"
reveal_type(quoted_form)  # revealed: TypeForm[str]

union_form: TypeForm[str | None] | None = str | None

literal_form: TypeForm[None] | None = Literal[None]
reveal_type(literal_form)  # revealed: TypeForm[None]

# Ordinary expressions preserve their normal bidirectional inference in a `TypeForm` context.
# Valid type expressions at contextually inferred nested positions undergo `TypeForm` evaluation.
def either(flag: bool) -> TypeForm[str] | None:
    return str if flag else None

def either_quoted(flag: bool) -> TypeForm[str] | None:
    return "str" if flag else None

def either_union(flag: bool) -> TypeForm[int | str] | None:
    return (int | str) if flag else None

def either_generic(flag: bool) -> TypeForm[list[int]] | None:
    return list[int] if flag else None

def either_invalid(flag: bool) -> TypeForm[str] | None:
    return 1 if flag else None  # error: [invalid-type-form]

def boolean_fallback(value: None) -> TypeForm[str] | None:
    return value or str

def boolean_union(value: None) -> TypeForm[int | str] | None:
    return value or (int | str)

def boolean_quoted(value: None) -> TypeForm[str] | None:
    return value or "str"

def boolean_invalid(value: None) -> TypeForm[str] | None:
    return value or 1  # error: [invalid-type-form]

def identity[T](value: T) -> T:
    return value

def returned_class() -> TypeForm[str]:
    return identity(str)

def returned_quoted() -> TypeForm[str]:
    return identity("str")

def returned_union() -> TypeForm[int | str]:
    return identity(int | str)

def returned_incompatible_class() -> TypeForm[str]:
    return identity(int)  # error: [invalid-return-type]

async def async_identity[T](value: T) -> T:
    return value

async def awaited_class() -> TypeForm[str]:
    return await async_identity(str)

async def awaited_quoted() -> TypeForm[str]:
    return await async_identity("str")

async def awaited_union() -> TypeForm[int | str]:
    return await async_identity(int | str)

def named_class() -> TypeForm[str]:
    return (form := str)
```

## Invalid type-form expressions

A bare `TypeForm` is equivalent to `TypeForm[Any]`, but the assigned expression still has to be a
valid type expression.

```py
from typing import ClassVar, Final, Literal, Optional, Self, TypeVarTuple, Unpack
from typing_extensions import TypeForm

Ts = TypeVarTuple("Ts")
var = 1

def accepts_type_form(x: TypeForm) -> None: ...

accepts_type_form(int)
accepts_type_form("int")
accepts_type_form("not a type")  # error: [invalid-syntax-in-forward-annotation]

bad_call: TypeForm = tuple()  # error: [invalid-type-form]
bad_tuple: TypeForm = (1, 2)  # error: [invalid-type-form]
bad_literal: TypeForm = 1  # error: [invalid-type-form]
bad_self: TypeForm = Self  # error: [invalid-type-form]
bad_literal_var: TypeForm = Literal[var]  # error: [invalid-type-form]
bad_literal_f_string: TypeForm = Literal[f""]  # error: [invalid-type-form]
bad_qualifier: TypeForm = ClassVar[int]  # error: [invalid-type-form]
bad_final: TypeForm = Final[int]  # error: [invalid-type-form]
bad_unpack: TypeForm = Unpack[Ts]  # error: [invalid-type-form]
bad_optional: TypeForm = Optional  # error: [invalid-type-form]
bad_quoted_operator: TypeForm = "int + str"  # error: [invalid-type-form]
```

## Explicit construction

`TypeForm(...)` explicitly constructs a `TypeForm` from exactly one positional type-expression
argument. The argument is checked using the same type-expression rules as contextual `TypeForm`
inference.

```py
from typing_extensions import TypeForm

constructed = TypeForm("list[int]")
reveal_type(constructed)  # revealed: TypeForm[list[int]]

constructed_class = TypeForm(int)
reveal_type(constructed_class)  # revealed: TypeForm[int]

TypeForm("type(1)")  # error: [invalid-type-form]
TypeForm()  # error: [invalid-type-form]
TypeForm(int, str)  # error: [invalid-type-form]
TypeForm(value=int)  # error: [invalid-type-form]
TypeForm(*(int,))  # error: [invalid-type-form]
```

## Generic specialization and aliases

When `TypeForm` appears in a generic parameter or return annotation, the type argument can be
inferred from the type expression passed by the caller or returned by the function.

```py
from typing import LiteralString
from typing_extensions import TypeForm

def construct[T](form: TypeForm[T]) -> T:
    raise NotImplementedError

reveal_type(construct(int))  # revealed: int
reveal_type(construct(list[int]))  # revealed: list[int]
reveal_type(construct(int | str))  # revealed: int | str

stored_union_forms = (int | str,)

def first[T](forms: tuple[TypeForm[T], ...]) -> T:
    raise NotImplementedError

reveal_type(first(stored_union_forms))  # revealed: int | str

stored_special_forms = (LiteralString,)
reveal_type(first(stored_special_forms))  # revealed: LiteralString

def use_runtime_type(form: type[int]) -> None:
    reveal_type(construct(form))  # revealed: int

def return_form() -> TypeForm[int | str]:
    return int | str

type Alias[T] = TypeForm[T]

def construct_alias[T](form: Alias[T]) -> T:
    raise NotImplementedError

reveal_type(construct_alias(int))  # revealed: int
```

## Overload resolution

Overload matching interprets a type expression using each `TypeForm` parameter context.

```py
from typing import overload
from typing_extensions import TypeForm

@overload
def foo(form: TypeForm[int]) -> int: ...
@overload
def foo(form: TypeForm[str]) -> str: ...
def foo(form: TypeForm[int] | TypeForm[str]) -> int | str:
    raise NotImplementedError

reveal_type(foo(int))  # revealed: int
reveal_type(foo(str))  # revealed: str
foo(float)  # error: [no-matching-overload]
```

## Narrowing to runtime classes

A `TypeForm[T]` value may or may not be a runtime class object. An `isinstance(..., type)` check can
narrow the form to `type[T]`, but only when that narrowing is sound for the original `TypeForm`
argument.

```py
from typing import Any
from typing_extensions import TypeForm, TypeIs

def as_type[T](form: TypeForm[T]) -> type[T] | None:
    if isinstance(form, type):
        reveal_type(form)  # revealed: type[T@as_type]
        return form
    return None

type StringAlias = str

def as_aliased_type(form: TypeForm[StringAlias]) -> type[str] | None:
    if isinstance(form, type):
        return form
    return None

def is_bare_runtime_type(value: TypeForm[Any]) -> TypeIs[type]:
    return isinstance(value, type)

def reject_broad_runtime_type_narrowing(
    value: TypeForm[str],
) -> TypeIs[type]:  # error: [invalid-type-guard-definition]
    return isinstance(value, type)

class A: ...

reveal_type(as_type(A))  # revealed: type[A] | None
```

## Availability from `typing`

`TypeForm` is available from the standard-library `typing` module starting in Python 3.15.

```toml
[environment]
python-version = "3.15"
```

```py
from typing import TypeForm

string_form: TypeForm[str] = str
```
