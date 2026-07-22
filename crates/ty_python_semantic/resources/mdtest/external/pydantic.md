# Pydantic

```toml
[environment]
python-version = "3.12"
python-platform = "linux"

[project]
dependencies = ["pydantic==2.13.4", "pydantic-settings==2.14.2"]
```

## Basic model

A basic Pydantic model looks and acts similar to a dataclass:

```py
from pydantic import BaseModel

class User(BaseModel):
    id: int
    name: str

reveal_type(User.__init__)  # revealed: (self: User, *, id: LaxInt, name: LaxStr, **extra: Any) -> None

user = User(id=1, name="John Doe")
reveal_type(user.id)  # revealed: int
reveal_type(user.name)  # revealed: str

# error: [missing-argument] "No argument provided for required parameter `name`"
invalid_user = User(id=2)
```

## Usage of `Field`

`Field` is a field-specifier function. In the following example, `tags` has a default value, and
`internal_price_cent` can be set through its alias `price_cent`:

```py
from pydantic import BaseModel, Field

class Product(BaseModel):
    name: str = Field(min_length=1)
    tags: list[str] = Field(default_factory=list)
    internal_price_cent: int = Field(gt=0, alias="price_cent")

# revealed: (self: Product, *, name: LaxStr, tags: Iterable[LaxStr] = ..., price_cent: LaxInt, **extra: Any) -> None
reveal_type(Product.__init__)

product = Product(name="Laptop", price_cent=999_00)
```

The fields have the expected types:

```py
reveal_type(product.name)  # revealed: str
reveal_type(product.tags)  # revealed: list[str]
reveal_type(product.internal_price_cent)  # revealed: int
```

Omitting the `name` or the `price_cent` is not allowed:

```py
# error: [missing-argument] "No argument provided for required parameter `name`"
Product(price_cent=100_00)
# error: [missing-argument] "No argument provided for required parameter `price_cent`"
Product(name="Phone")
```

Using the internal field name is not possible (the argument will be accepted, but `price_cent` is
missing):

```py
# error: [missing-argument]
Product(name="Laptop", internal_price_cent=999_00)
```

Conversely, accessing a field through the alias is also not allowed:

```py
product.price_cent  # error: [unresolved-attribute]
```

## Usage of ellipsis in `Field`

A positional argument of `...` to the `Field` function indicates that the field *has no default and
is required*:

```py
from pydantic import BaseModel, Field

class Person(BaseModel):
    name: str = Field(..., max_length=255)

Person(name="Alice")
Person()  # error: [missing-argument]
```

## Strict and lax mode

Pydantic distinguishes a "strict" mode in which it will error if a value is of the wrong type, and a
"lax" mode, in which it attempts to coerce the value to the correct type. We model these two modes
in ty so that static analysis supports the runtime validation behavior when possible.

### Using the model config to enable strict mode

Strict mode can be activated for a whole model:

```py
from pydantic import BaseModel, ConfigDict

class Person(BaseModel):
    model_config = ConfigDict(strict=True)

    name: str
    age: int

Person(name="Alice", age=20)  # okay
Person(name="Alice", age="20")  # error: [invalid-argument-type]
```

### Lax mode is the default

When no configuration is given, or when `strict=False`, lax mode is used:

```py
from pydantic import BaseModel, ConfigDict

class Person1(BaseModel):
    name: str
    age: int

Person1(name="Alice", age=20)  # okay
Person1(name="Alice", age="20")  # okay, coerced
# error: [invalid-argument-type]
Person1(name="Alice", age=None)  # error, cannot be coerced

class Person2(BaseModel):
    model_config = ConfigDict(strict=False)

    name: str
    age: int

Person2(name="Alice", age=20)  # okay
Person2(name="Alice", age="20")  # okay
# error: [invalid-argument-type]
Person2(name="Alice", age=None)  # error, cannot be coerced
```

Scalar types follow the Python-input conversions in Pydantic's [conversion table]:

```py
import re
from datetime import date, datetime, time, timedelta
from decimal import Decimal
from ipaddress import (
    IPv4Address,
    IPv4Interface,
    IPv4Network,
    IPv6Address,
    IPv6Interface,
    IPv6Network,
)
from pathlib import Path
from re import Pattern
from uuid import UUID

from pydantic import ByteSize

class LaxBool(BaseModel):
    value: bool

LaxBool(value=True)
LaxBool(value=1.0)
LaxBool(value=1)
LaxBool(value=Decimal(1))
LaxBool(value="true")
LaxBool(value=[True])  # error: [invalid-argument-type]

class LaxBytes(BaseModel):
    value: bytes

LaxBytes(value=b"foo")
LaxBytes(value=bytearray(b"foo"))
LaxBytes(value="foo")
LaxBytes(value=1)  # error: [invalid-argument-type]

class LaxDate(BaseModel):
    value: date

LaxDate(value=date(2020, 1, 1))
LaxDate(value="2020-01-01")
LaxDate(value=b"2020-01-01")
LaxDate(value=datetime(2020, 1, 1))
LaxDate(value=1_577_836_800.0)
LaxDate(value=1_577_836_800)
LaxDate(value=Decimal(1_577_836_800))
LaxDate(value=[2020, 1, 1])  # error: [invalid-argument-type]

class LaxDatetime(BaseModel):
    value: datetime

LaxDatetime(value=datetime(2020, 1, 1, 12, 0))
LaxDatetime(value=date(2020, 1, 1))
LaxDatetime(value=b"2020-01-01T12:00:00")
LaxDatetime(value="2020-01-01T12:00:00")
LaxDatetime(value=1_577_880_000.0)
LaxDatetime(value=1_577_880_000)
LaxDatetime(value=Decimal(1_577_880_000))
LaxDatetime(value=[2020, 1, 1, 12, 0])  # error: [invalid-argument-type]

class LaxFloat(BaseModel):
    value: float

LaxFloat(value=1.0)
LaxFloat(value=1)
LaxFloat(value=True)
LaxFloat(value=b"1.0")
LaxFloat(value="1.0")
LaxFloat(value=Decimal("1.0"))
LaxFloat(value=(1, 0))  # error: [invalid-argument-type]

class LaxInt(BaseModel):
    value: int

LaxInt(value=1)
LaxInt(value=True)
LaxInt(value=b"1")
LaxInt(value=1.0)
LaxInt(value="1")
LaxInt(value=Decimal(1))
LaxInt(value=(1,))  # error: [invalid-argument-type]

class LaxStr(BaseModel):
    value: str

LaxStr(value="foo")
LaxStr(value=b"foo")
LaxStr(value=bytearray(b"foo"))
LaxStr(value=1)  # error: [invalid-argument-type]

class LaxTime(BaseModel):
    value: time

LaxTime(value=time(12, 0))
LaxTime(value=b"12:00:00")
LaxTime(value="12:00:00")
LaxTime(value=43_200.0)
LaxTime(value=43_200)
LaxTime(value=Decimal(43_200))
LaxTime(value=[12, 0])  # error: [invalid-argument-type]

class LaxTimedelta(BaseModel):
    value: timedelta

LaxTimedelta(value=timedelta(days=1))
LaxTimedelta(value=b"P1D")
LaxTimedelta(value="P1D")
LaxTimedelta(value=86_400.0)
LaxTimedelta(value=86_400)
LaxTimedelta(value=Decimal(86_400))
LaxTimedelta(value=[86_400])  # error: [invalid-argument-type]

class LaxByteSize(BaseModel):
    value: ByteSize

LaxByteSize(value=1.0)
LaxByteSize(value=1)
LaxByteSize(value="1 KiB")
LaxByteSize(value=Decimal(1))
LaxByteSize(value=[1, 0])  # error: [invalid-argument-type]

class LaxDecimal(BaseModel):
    value: Decimal

LaxDecimal(value=Decimal("1.0"))
LaxDecimal(value=1.0)
LaxDecimal(value=1)
LaxDecimal(value="1.0")
LaxDecimal(value=b"1.0")  # error: [invalid-argument-type]

ipv4_address = IPv4Address("192.0.2.1")
ipv4_interface = IPv4Interface("192.0.2.0/24")
ipv4_network = IPv4Network("192.0.2.0/24")

class LaxIPv4Address(BaseModel):
    value: IPv4Address

LaxIPv4Address(value=ipv4_address)
LaxIPv4Address(value=ipv4_interface)
LaxIPv4Address(value=ipv4_address.packed)
LaxIPv4Address(value=0xC0000201)
LaxIPv4Address(value="192.0.2.1")
LaxIPv4Address(value=[192, 0, 2, 1])  # error: [invalid-argument-type]

class LaxIPv4Interface(BaseModel):
    value: IPv4Interface

LaxIPv4Interface(value=ipv4_interface)
LaxIPv4Interface(value=ipv4_address)
LaxIPv4Interface(value=ipv4_address.packed)
LaxIPv4Interface(value=0xC0000201)
LaxIPv4Interface(value="192.0.2.1/24")
LaxIPv4Interface(value=("192.0.2.1", 24))
LaxIPv4Interface(value=["192.0.2.1", 24])  # error: [invalid-argument-type]

class LaxIPv4Network(BaseModel):
    value: IPv4Network

LaxIPv4Network(value=ipv4_network)
LaxIPv4Network(value=ipv4_interface)
LaxIPv4Network(value=ipv4_address)
LaxIPv4Network(value=ipv4_network.network_address.packed)
LaxIPv4Network(value=0xC0000200)
LaxIPv4Network(value="192.0.2.0/24")
LaxIPv4Network(value=["192.0.2.0", 24])  # error: [invalid-argument-type]

ipv6_address = IPv6Address("2001:db8::1")
ipv6_interface = IPv6Interface("2001:db8::/64")
ipv6_network = IPv6Network("2001:db8::/64")

class LaxIPv6Address(BaseModel):
    value: IPv6Address

LaxIPv6Address(value=ipv6_address)
LaxIPv6Address(value=ipv6_interface)
LaxIPv6Address(value=ipv6_address.packed)
LaxIPv6Address(value=1)
LaxIPv6Address(value="2001:db8::1")
LaxIPv6Address(value=[0x2001, 0xDB8, 1])  # error: [invalid-argument-type]

class LaxIPv6Interface(BaseModel):
    value: IPv6Interface

LaxIPv6Interface(value=ipv6_interface)
LaxIPv6Interface(value=ipv6_address)
LaxIPv6Interface(value=ipv6_address.packed)
LaxIPv6Interface(value=1)
LaxIPv6Interface(value="2001:db8::1/64")
LaxIPv6Interface(value=("2001:db8::1", 64))
LaxIPv6Interface(value=["2001:db8::1", 64])  # error: [invalid-argument-type]

class LaxIPv6Network(BaseModel):
    value: IPv6Network

LaxIPv6Network(value=ipv6_network)
LaxIPv6Network(value=ipv6_interface)
LaxIPv6Network(value=ipv6_address)
LaxIPv6Network(value=ipv6_network.network_address.packed)
LaxIPv6Network(value=1)
LaxIPv6Network(value="2001:db8::/64")
LaxIPv6Network(value=["2001:db8::", 64])  # error: [invalid-argument-type]

class LaxPath(BaseModel):
    value: Path

LaxPath(value=Path("/tmp/foo"))
LaxPath(value="/tmp/foo")
LaxPath(value=b"/tmp/foo")  # error: [invalid-argument-type]

class LaxStrPattern(BaseModel):
    value: Pattern[str]

LaxStrPattern(value=re.compile("foo"))
LaxStrPattern(value="foo")
LaxStrPattern(value=b"foo")  # error: [invalid-argument-type]

class LaxBytesPattern(BaseModel):
    value: Pattern[bytes]

LaxBytesPattern(value=re.compile(b"foo"))
LaxBytesPattern(value=b"foo")
LaxBytesPattern(value="foo")  # error: [invalid-argument-type]

class LaxUUID(BaseModel):
    value: UUID

LaxUUID(value=UUID("12345678-1234-1234-1234-123456789012"))
LaxUUID(value="12345678-1234-1234-1234-123456789012")
LaxUUID(value=None)  # error: [invalid-argument-type]

class LaxNone(BaseModel):
    value: None

LaxNone(value=None)
LaxNone(value=1)  # error: [invalid-argument-type]
```

Aliasing a scalar type does not affect lax input conversion:

```py
from datetime import datetime as AliasedDatetime

from pydantic import BaseModel

class Model(BaseModel):
    value: AliasedDatetime

reveal_type(Model.__init__)  # revealed: (self: Model, *, value: LaxDatetime, **extra: Any) -> None
```

For collections, we widen something like `list[int]` to `Iterable[LaxInt]`. Pydantic can coerce a
set of specific collection types to `list[int]` (`deque`, `frozenset`, ...), but we cannot use a
union like `list[LaxInt] | deque[LaxInt] | frozenset[LaxInt] | ...` due to invariance of some of
these types. Using the covariant `Iterable` is a good approximation, and allows the element type to
be widened to `LaxInt`.

For mappings, we do the same, but only widen the value type, since `Mapping` is invariant in the key
type. This can (in principle) lead to false positives, as documented in a comment below.

```py
from collections import deque
from collections.abc import Mapping

class LaxListInt(BaseModel):
    value: list[int]

LaxListInt(value=[1, 2, 3])
LaxListInt(value=[1, "2", 3.0])
LaxListInt(value=deque([1, 2, 3]))
LaxListInt(value={1: None, 2: None, 3: None}.keys())
LaxListInt(value={"a": 1, "b": 2, "c": "3"}.values())
LaxListInt(value=frozenset({1, 2, "3"}))
LaxListInt(value=[1, 2, "3"])
LaxListInt(value={1, 2, "3"})
LaxListInt(value=(1, 2, "3"))
LaxListInt(value=[])
LaxListInt(value=[1, 2, None])  # error: [invalid-argument-type]
LaxListInt(value=[1, 2, [3]])  # error: [invalid-argument-type]
LaxListInt(value=1)  # error: [invalid-argument-type]

# This is rejected by Pydantic at runtime, but we accept it since `str` is
# a subtype of `Iterable[LaxInt]` (`LaxInt = int | str | ...`).
LaxListInt(value="abc")

def _(list_int: list[int]):
    LaxListInt(value=list_int)

class LaxDictStrInt(BaseModel):
    value: dict[str, int]

LaxDictStrInt(value={"a": 1, "b": 2})
LaxDictStrInt(value={"a": "1", "b": "2"})
LaxDictStrInt(value={1: 1, 2: 2})  # error: [invalid-argument-type]

# This is actually supported at runtime, but Mapping is invariant in the
# key type, so we cannot widen it from `str` to `LaxStr`.
LaxDictStrInt(value={b"a": 1, b"b": 2})  # error: [invalid-argument-type]

def _(dict_str_int: dict[str, int]):
    LaxDictStrInt(value=dict_str_int)

def _(dict_str_str: dict[str, str]):
    LaxDictStrInt(value=dict_str_str)

def _(map_str_int: Mapping[str, int]):
    LaxDictStrInt(value=map_str_int)

def _(map_str_str: Mapping[str, str]):
    LaxDictStrInt(value=map_str_str)
```

This also works for nested collections:

```py
class Nested(BaseModel):
    value: list[dict[str, int]]

Nested(value=[{"a": 1}, {"b": "2", "c": 3.0}])
Nested(value=[{"a": 1}, {"b": None}])  # error: [invalid-argument-type]
```

In lax mode, fields that refer to an ordinary Pydantic model accept either an instance of that model
or a mapping:

```py
class Child(BaseModel):
    value: int

class Stranger(BaseModel):
    value: int

class Parent(BaseModel):
    child: Child
    children: list[Child]

# revealed: (self: Parent, *, child: Child | Mapping[str, Any], children: Iterable[Child | Mapping[str, Any]], **extra: Any) -> None
reveal_type(Parent.__init__)

child_input = {"value": "1"}

Parent(child=Child(value=1), children=[Child(value=2), Child(value=3)])
Parent(child={"value": "1"}, children=[{"value": "2"}, {"value": "3"}])

Parent(child=Stranger(value=1), children=[])  # error: [invalid-argument-type]
Parent(child=1, children=[])  # error: [invalid-argument-type]
Parent(child={"value": 1}, children=[Stranger(value=2)])  # error: [invalid-argument-type]
Parent(child={"value": 1}, children=[2])  # error: [invalid-argument-type]
```

For fields that refer to generic models, we widen to a gradual specialization, since Pydantic
revalidates same-origin generic model instances against the target specialization:

```py
class Box[T](BaseModel):
    value: T

class HasBox(BaseModel):
    box: Box[int]

# revealed: (self: HasBox, *, box: Box[Unknown] | Mapping[str, Any], **extra: Any) -> None
reveal_type(HasBox.__init__)

HasBox(box=Box(value=1))
HasBox(box=Box(value="1"))
HasBox(box=1)  # error: [invalid-argument-type]

# This would ideally be an error, but we currently do not attempt to detect this:
HasBox(box=Box(value=None))
```

Models configured to validate from attributes can accept arbitrary objects, so their field
parameters remain `Any`:

```py
class AttributeChild(BaseModel):
    model_config = ConfigDict(from_attributes=True)

    value: int

class AttributeParent(BaseModel):
    child: AttributeChild

class AttributeSource:
    def __init__(self, value: int) -> None:
        self.value = value

# revealed: (self: AttributeParent, *, child: Any, **extra: Any) -> None
reveal_type(AttributeParent.__init__)

AttributeParent(child=AttributeSource(1))
```

For enums, we currently fall back to a very permissive `Any`, because Pydantic allows certain
conversions that are not further specified in the documentation.

```py
from enum import Enum

class Color(Enum):
    RED = "red"
    GREEN = "green"
    BLUE = "blue"

class LaxColor(BaseModel):
    value: Color

reveal_type(LaxColor.__init__)  # revealed: (self: LaxColor, *, value: Any, **extra: Any) -> None

LaxColor(value=Color.RED)
LaxColor(value="red")
# This should ideally be an error:
LaxColor(value=None)
```

`Literal` types are not widened, even in lax mode:

```py
from typing import Literal

class LaxLiterals(BaseModel):
    value: Literal[1, "a", True]

LaxLiterals(value=1)
LaxLiterals(value="a")
LaxLiterals(value=True)
LaxLiterals(value=2)  # error: [invalid-argument-type]
LaxLiterals(value="b")  # error: [invalid-argument-type]
LaxLiterals(value=False)  # error: [invalid-argument-type]
```

Unions are converted element-wise:

```py
class LaxUnion(BaseModel):
    value: int | list[str] | None

reveal_type(LaxUnion.__init__)  # revealed: (self: LaxUnion, *, value: LaxInt | Iterable[LaxStr] | None, **extra: Any) -> None

LaxUnion(value=1)
LaxUnion(value="1")
LaxUnion(value=["a", "b"])
LaxUnion(value=[b"a", b"b"])
LaxUnion(value=None)
LaxUnion(value=[1, 2])  # error: [invalid-argument-type]

def _(union: int | list[str] | None):
    LaxUnion(value=union)
```

Rewriting also works through type aliases:

```py
type NestedList = list[int | NestedList]

class LaxNestedList(BaseModel):
    value: NestedList

LaxNestedList(value=[1, 2, 3])
LaxNestedList(value=[[1, 2], [3, 4]])
LaxNestedList(value=[1, "2", 3])
LaxNestedList(value=[1, [2, "3"], 4])
LaxNestedList(value=1)  # error: [invalid-argument-type]
# TODO: this should be an error once we support recursive types
LaxNestedList(value=[1, [2, None]])
```

We support validation of `JsonValue` fields in lax mode:

```py
from pydantic import JsonValue

class JsonValueModel(BaseModel):
    value: JsonValue

JsonValueModel(value=[1, 2])
JsonValueModel(value={"key": 1})
JsonValueModel(value="value")
JsonValueModel(value=1)
JsonValueModel(value=1.0)
JsonValueModel(value=True)
JsonValueModel(value=None)
JsonValueModel(value={"outer": [1, {"inner": "value"}]})

class SomethingElse: ...

JsonValueModel(value=object())  # error: [invalid-argument-type]
JsonValueModel(value=...)  # error: [invalid-argument-type]
JsonValueModel(value=SomethingElse())  # error: [invalid-argument-type]

# TODO: this should be an error once we support recursive types
JsonValueModel(value={"outer": [1, {"inner": SomethingElse()}]})
```

### Changing a specific field

Strict mode can also be activated for a specific field only:

```py
from pydantic import BaseModel, ConfigDict, Field

class Person1(BaseModel):
    name: str
    age: int = Field(strict=True)
```

Here, validation is lax for the `name` field (`bytes` is converted to `str`):

```py
Person1(name="Alice", age=20)
Person1(name=b"Alice", age=20)
```

But `age` is validated in `strict` mode, so the conversion from `str` to `int` is not allowed here:

```py
Person1(name="Alice", age=20)
Person1(name="Alice", age="20")  # error: [invalid-argument-type]
```

The opposite is also possible. A whole model can be in "strict" mode, and a single field can opt
out:

```py
class Person2(BaseModel):
    model_config = ConfigDict(strict=True)

    name: str = Field(strict=False)
    age: int

Person2(name="Alice", age=20)
Person2(name=b"Alice", age=20)

Person2(name="Alice", age=20)
Person2(name="Alice", age="20")  # error: [invalid-argument-type]
```

An explicit `None` does not override the model's strict setting:

```py
class Person3(BaseModel):
    model_config = ConfigDict(strict=True)

    age: int = Field(strict=None)

Person3(age=20)
Person3(age="20")  # error: [invalid-argument-type]
```

Pydantic's strict aliases and `Strict()` metadata also enable strict validation for individual
fields:

```py
from typing import Annotated
from pydantic import Field, Strict, StrictInt

class StrictFields(BaseModel):
    strict_int: StrictInt
    strict_str: Annotated[str, Strict()]

StrictFields(strict_int=1, strict_str="foo")
StrictFields(strict_int="1", strict_str="foo")  # error: [invalid-argument-type]
StrictFields(strict_int=1, strict_str=b"foo")  # error: [invalid-argument-type]

class StrictMetadataOrder(BaseModel):
    field_then_lax: Annotated[int, Field(strict=True), Strict(False)]
    lax_then_field: Annotated[int, Strict(False), Field(strict=True)]

StrictMetadataOrder(field_then_lax="1", lax_then_field=1)
StrictMetadataOrder(field_then_lax=1, lax_then_field="1")  # error: [invalid-argument-type]
```

A field with `Strict(False)` can opt out of strict validation, even in a model with `strict=True`:

```py
class LaxFieldInStrictModel(BaseModel):
    model_config = ConfigDict(strict=True)

    strict_int: int
    lax_int: Annotated[int, Strict(False)]

LaxFieldInStrictModel(strict_int=1, lax_int=1)
LaxFieldInStrictModel(strict_int=1, lax_int="1")
LaxFieldInStrictModel(strict_int="1", lax_int=1)  # error: [invalid-argument-type]
```

## `validate_by_name`, `validate_by_alias`

By default, Pydantic only allows a field to be initialized by its alias name, not by its field name:

```py
from pydantic import BaseModel, ConfigDict, Field

class DefaultOnlyAlias(BaseModel):
    name: int = Field(alias="alias")

DefaultOnlyAlias(alias=1)
# error: [missing-argument]
DefaultOnlyAlias(name=1)
```

When `validate_by_name=True`, a field can also be initialized using its internal name:

```py
class AliasAndName(BaseModel):
    model_config = ConfigDict(validate_by_name=True)

    name: int = Field(alias="alias")

AliasAndName(alias=1)
AliasAndName(name=1)
AliasAndName(name=None)  # error: [invalid-argument-type]
```

The older `populate_by_name=True` setting has the same behavior:

```py
class PopulatedByName(BaseModel):
    model_config = ConfigDict(populate_by_name=True)

    name: int = Field(alias="alias")

PopulatedByName(alias=1)
PopulatedByName(name=1)
PopulatedByName(alias=None)  # error: [invalid-argument-type]
PopulatedByName(name=None)  # error: [invalid-argument-type]
```

Passing none of these should be an error:

```py
# This is a known limitation, it should ideally be an error.
AliasAndName()
```

Conversely, when `validate_by_alias=False`, validation by alias can be disallowed:

```py
class OnlyName(BaseModel):
    model_config = ConfigDict(validate_by_name=True, validate_by_alias=False)

    name: int = Field(alias="alias")

OnlyName(alias=1)  # error: [missing-argument]
OnlyName(name=1)
```

If `validate_by_alias=False` is set without specifying `validate_by_name`, Pydantic implicitly
enables validation by name:

```py
class ImplicitlyOnlyName(BaseModel):
    model_config = ConfigDict(validate_by_alias=False)

    name: int = Field(alias="alias")

ImplicitlyOnlyName(alias=1)  # error: [missing-argument]
ImplicitlyOnlyName(name=1)
```

Pydantic models can also specify a `validation_alias` for a field, which takes precedence over
`alias` when `validate_by_alias=True`:

```py
class ValidationAlias(BaseModel):
    name: int = Field(alias="alias", validation_alias="validation_alias")

ValidationAlias(validation_alias=1)
ValidationAlias(validation_alias=None)  # error: [invalid-argument-type]

ValidationAlias()  # error: [missing-argument]
ValidationAlias(name=1)  # error: [missing-argument]
ValidationAlias(alias=1)  # error: [missing-argument]
```

## Extra fields

By default, Pydantic allows arbitrary extra data which is simply ignored. This often indicates a
mistake though, so ty emits a warning by default:

```py
from pydantic import BaseModel, ConfigDict

class Person(BaseModel):
    name: str

Person(name="Alice", something_else=7)  # error: [pydantic-discarded-extra-argument]
```

The same thing happens when explicitly setting `extra="ignore"`:

```py
class PersonIgnoringExtras(BaseModel):
    model_config = ConfigDict(extra="ignore")

    name: str

PersonIgnoringExtras(name="Alice", something_else=7)  # error: [pydantic-discarded-extra-argument]
```

When `extra="allow"` is set, extra arguments are explicitly allowed (and stored in the model at
runtime), so we do not emit a warning in this case:

```py
class PersonAllowingExtras(BaseModel):
    model_config = ConfigDict(extra="allow")

    name: str

PersonAllowingExtras(name="Alice", something_else=7)
```

Conversely, when setting `extra="forbid"`, a hard `unknown-argument` error is emitted, since the
construction would fail at runtime:

```py
class PersonWithoutExtras(BaseModel):
    model_config = ConfigDict(extra="forbid")

    name: str

# revealed: (self: PersonWithoutExtras, *, name: LaxStr) -> None
reveal_type(PersonWithoutExtras.__init__)
PersonWithoutExtras(name="Alice", something_else=7)  # error: [unknown-argument]
```

## Custom initializers and extra fields

A custom initializer that accepts arbitrary keyword arguments does not prevent a subclass from
accepting extra data:

```py
from typing import Any

from pydantic import BaseModel

class FrameworkBase(BaseModel):
    def __init__(self, **data: Any) -> None:
        super().__init__(**data)

class User(FrameworkBase):
    name: str

reveal_type(User.__init__)  # revealed: (self: User, *, name: LaxStr, **extra: Any) -> None
User(name="Alice", city="Berlin")
```

A fixed custom initializer continues to control the accepted arguments:

```py
class RestrictiveBase(BaseModel):
    def __init__(self, name: str) -> None:
        super().__init__(name=name)

RestrictiveBase(name="Alice")
RestrictiveBase(name="Alice", city="Berlin")  # error: [unknown-argument]

class RestrictiveUser(RestrictiveBase):
    name: str

RestrictiveUser(name="Alice")
RestrictiveUser(name="Alice", city="Berlin")  # error: [unknown-argument]
```

## Field named `extra`

The variadic keyword parameter uses a collision-free name when the model already has a field named
`extra`:

```py
from pydantic import BaseModel, ConfigDict

class PersonWithExtraField(BaseModel):
    model_config = ConfigDict(extra="allow")

    extra: int

# revealed: (self: PersonWithExtraField, *, extra: LaxInt, **extra_: Any) -> None
reveal_type(PersonWithExtraField.__init__)
PersonWithExtraField(extra=1, something_else=2)
```

## Private attributes

Underscore-prefixed attributes are considered private. They remain instance attributes but do not
become model fields or constructor parameters:

```py
from pydantic import BaseModel, PrivateAttr

class Person(BaseModel):
    name: str
    _implicit_private: int
    _private_with_default: int = 1
    _explicit_private: int = PrivateAttr(default=0)

# revealed: (self: Person, *, name: LaxStr, **extra: Any) -> None
reveal_type(Person.__init__)

person = Person(name="Alice")
reveal_type(person._implicit_private)  # revealed: int
reveal_type(person._private_with_default)  # revealed: int
reveal_type(person._explicit_private)  # revealed: int
```

## Using `Annotated` to specify field metadata

`Annotated[T, Field(...)]` can be used to specify field metadata:

```py
from pydantic import BaseModel, Field, ConfigDict
from typing import Annotated

class Person(BaseModel):
    model_config = ConfigDict(strict=True)

    name: Annotated[str, Field(strict=False)]
    id: Annotated[int, Field(default=0)]

Person(name="Alice", id=1)
Person(name=b"Alice", id=1)
Person(name="Alice")

Person(name=None, id=1)  # error: [invalid-argument-type]
Person(id=1)  # error: [missing-argument]
```

Multiple `Field(...)` calls in `Annotated[...]` are merged:

```py
class MultipleAnnotatedFields(BaseModel):
    strict_then_default: Annotated[int, Field(strict=True), Field(default=0)]
    default_then_strict: Annotated[int, Field(default=0), Field(strict=True)]

MultipleAnnotatedFields()
MultipleAnnotatedFields(strict_then_default=1, default_then_strict=1)
MultipleAnnotatedFields(strict_then_default="1")  # error: [invalid-argument-type]
MultipleAnnotatedFields(default_then_strict="1")  # error: [invalid-argument-type]
```

Field metadata in the annotation and on the right hand side is also merged:

```py
class AnnotatedAndAssignedFields(BaseModel):
    strict_then_default: Annotated[int, Field(strict=True)] = Field(default=0)
    default_then_strict: Annotated[int, Field(default=0)] = Field(strict=True)

AnnotatedAndAssignedFields()
AnnotatedAndAssignedFields(strict_then_default=1, default_then_strict=1)
AnnotatedAndAssignedFields(strict_then_default="1")  # error: [invalid-argument-type]
AnnotatedAndAssignedFields(default_then_strict="1")  # error: [invalid-argument-type]
```

Field metadata is also collected through aliases:

```py
AliasField = Annotated[int, Field(default=0)]

class ModelWithAliasField(BaseModel):
    value: AliasField

ModelWithAliasField()
```

Field metadata is also collected through a generic alias, where the `Field(...)` default is carried
on a type variable that is only specialized at the use site:

```py
from typing import TypeVar

T = TypeVar("T")
GenericAliasField = Annotated[T, Field(default=0)]

class ModelWithGenericAliasField(BaseModel):
    value: GenericAliasField[int]

# `value` is optional because the alias supplies `Field(default=0)`.
ModelWithGenericAliasField()
ModelWithGenericAliasField(value=1)
```

## Frozen models and fields

There are various ways to make a field immutable. A model can be globally frozen using a class
parameter:

```py
from pydantic import BaseModel, ConfigDict, Field

class PersonFrozenName1(BaseModel, frozen=True):
    name: str

person = PersonFrozenName1(name="Alice")
person.name = "Bob"  # error: [invalid-assignment]
```

It can also be globally frozen using the model config:

```py
class PersonFrozenName2(BaseModel):
    model_config = ConfigDict(frozen=True)

    name: str

person = PersonFrozenName2(name="Alice")
person.name = "Bob"  # error: [invalid-assignment]
```

Finally, individual fields can also be made immutable on a non-frozen model:

```py
class PersonFrozenName3(BaseModel):
    name: str = Field(frozen=True)
    age: int

person = PersonFrozenName3(name="Alice", age=20)
# TODO: this should be an error
person.name = "Bob"
person.age += 1
```

No error is raised when a frozen model is subclassed. The child model is also frozen:

```py
class Base(BaseModel, frozen=True):
    value: int

class Derived(Base):
    pass

derived = Derived(value=1)
derived.value = 2  # error: [invalid-assignment]
```

## Validation of default values

At runtime, default values are *not* validated against the field type annotation, unless
`validate_default=True` is set. In static analysis, we still need to verify the default values
against the type annotation. Not doing so would be unsound. We do this unconditionally, even if
`validate_default=False` (which is also the default):

```py
from pydantic import BaseModel, ConfigDict, Field

class Person1(BaseModel):
    # error: [invalid-assignment]
    name: str = Field(default=None)

class Person2(BaseModel):
    model_config = ConfigDict(validate_default=False)

    # error: [invalid-assignment]
    name: str = Field(default=None)

class Person3(BaseModel):
    model_config = ConfigDict(validate_default=True)

    # error: [invalid-assignment]
    name: str = Field(default=None)

class Person4(BaseModel):
    # error: [invalid-assignment]
    name: str = Field(default=None, validate_default=False)

class Person5(BaseModel):
    # TODO: this should be an error
    name: str = Field(default=None, validate_default=True)
```

## Verification of models

A field without a type annotation leads to a runtime error.

```py
from pydantic import BaseModel, Field

# TODO: this should ideally be an error
class PersonUntypedField(BaseModel):
    name: str
    age = Field(default=0)
```

## BaseSettings

A model derived from `BaseSettings` can use environment variables, so we assume that it is okay not
to provide their values:

```py
from pydantic_settings import BaseSettings

class Settings(BaseSettings):
    host: str
    port: int

# Would succeed at runtime if HOST and PORT environment variables are set
Settings()
Settings(host="localhost")
Settings(port=8000)
Settings(host="localhost", port=8000)
Settings(host=None)  # error: [invalid-argument-type]

# `BaseSettings` defines a specialized constructor and forbids extra values by default.
Settings(host="localhost", port=8000, something_else=7)  # error: [unknown-argument]
```

## Root models

Unlike fields on ordinary Pydantic models, a root model's `root` field can be passed either
positionally or by keyword:

```py
from pydantic import RootModel, BaseModel

class IntList(RootModel[list[int]]): ...

reveal_type(IntList.__init__)  # revealed: (self: IntList, root: Iterable[LaxInt]) -> None

IntList([1, 2, 3])
IntList(root=[1, 2, 3])
IntList(["1", "2", "3"])

IntList(1)  # error: [invalid-argument-type]
```

When a root model field is included in a normal model, it can be set using the `root` type directly:

```py
class Model(BaseModel):
    int_list: IntList

Model(int_list=IntList([1, 2, 3]))
Model(int_list=[1, 2, 3])
Model(int_list=["1", "2", "3"])

Model(int_list=1)  # error: [invalid-argument-type]
```

Generic root models can accept root models with a different specialization:

```py
class GenericRoot[T](RootModel[T]): ...

class HasGenericRoot(BaseModel):
    root: GenericRoot[int]

HasGenericRoot(root=GenericRoot(1))
HasGenericRoot(root=GenericRoot("1"))

# This would ideally be an error, but we currently do not attempt to detect this:
HasGenericRoot(root=GenericRoot(None))
```

## Model configuration

The tests in this section use `extra` as an exemplary setting, but primarily test how model
configuration is detected, inherited, merged, and overridden.

```py
from pydantic import BaseModel, ConfigDict

class ForbidExtras(BaseModel):
    model_config = ConfigDict(extra="forbid")

class InheritsForbidExtras(ForbidExtras):
    name: str

InheritsForbidExtras(name="Alice", something_else=7)  # error: [unknown-argument]

class OverridesForbidExtras(ForbidExtras):
    model_config = ConfigDict(extra="allow")

    name: str

OverridesForbidExtras(name="Alice", something_else=7)

class KeepsInheritedExtraSetting(ForbidExtras):
    model_config = ConfigDict(strict=True)

    name: str

KeepsInheritedExtraSetting(name="Alice", something_else=7)  # error: [unknown-argument]
```

Pydantic merges configs from multiple bases from left to right, so the rightmost base takes
precedence:

```py
class AllowExtras(BaseModel):
    model_config = ConfigDict(extra="allow")

class RightmostForbids(AllowExtras, ForbidExtras):
    name: str

RightmostForbids(name="Alice", something_else=7)  # error: [unknown-argument]

class RightmostAllows(ForbidExtras, AllowExtras):
    name: str

RightmostAllows(name="Alice", something_else=7)
```

Mixins (classes that do not inherit from `BaseModel`) can also change the configuration:

```py
class AllowExtrasMixin:
    model_config = ConfigDict(extra="allow")

class ConfigMixinOverridesForbid(ForbidExtras, AllowExtrasMixin):
    name: str

ConfigMixinOverridesForbid(name="Alice", something_else=7)
```

Config values passed as class keywords take precedence over inherited `model_config`s:

```py
class KeywordForbidsExtras(BaseModel, extra="forbid"):
    name: str

KeywordForbidsExtras(name="Alice", something_else=7)  # error: [unknown-argument]

class KeywordOverridesForbid(ForbidExtras, extra="allow"):
    name: str

KeywordOverridesForbid(name="Alice", something_else=7)
```

The `ConfigDict` class is recognized by identity, including through an import alias:

```py
from pydantic import ConfigDict as ModelConfig

class AliasedConfigDict(BaseModel):
    model_config = ModelConfig(extra="forbid")

    name: str

AliasedConfigDict(name="Alice", something_else=7)  # error: [unknown-argument]
```

Pydantic also accepts a plain dictionary as `model_config`:

```py
class PlainDictConfig(BaseModel):
    model_config = {"extra": "forbid"}

    name: str

PlainDictConfig(name="Alice", something_else=7)  # error: [unknown-argument]

class DictCallConfig(BaseModel):
    model_config = dict(extra="forbid")

    name: str

DictCallConfig(name="Alice", something_else=7)  # error: [unknown-argument]
```

## Mixins

Annotated attributes on mixin-classes that do not inherit from `BaseModel` also become fields on the
model:

```py
from pydantic import BaseModel

class Mixin:
    mixin_field: bool

class MyModel(BaseModel, Mixin):
    model_field: bool

# revealed: (self: MyModel, *, mixin_field: LaxBool, model_field: LaxBool, **extra: Any) -> None
reveal_type(MyModel.__init__)
MyModel(model_field=True, mixin_field=False)
```

## Differences from dataclasses

Pydantic uses `@dataclass_transform(...)` on its `ModelMetaclass` to help type checkers understand
that models derived from classes like `BaseModel` (which have `ModelMetaclass` as their metaclass)
are similar to dataclasses. However, there are some crucial differences.

Pydantic models allow a required field after one with a default:

```py
from pydantic import BaseModel

class RequiredAfterDefault(BaseModel):
    defaulted: int = 0
    required: int

# revealed: (self: RequiredAfterDefault, *, defaulted: LaxInt = 0, required: LaxInt, **extra: Any) -> None
reveal_type(RequiredAfterDefault.__init__)
RequiredAfterDefault(required=1)
```

Pydantic models do not expose some of the special attributes of dataclasses:

```py
RequiredAfterDefault.__dataclass_fields__  # error: [unresolved-attribute]
RequiredAfterDefault.__dataclass_params__  # error: [unresolved-attribute]
RequiredAfterDefault.__match_args__  # error: [unresolved-attribute]
```

They do, however, expose various Pydantic-specific fields (inherited from `BaseModel`), for example:

```py
reveal_type(RequiredAfterDefault.__pydantic_fields__)  # revealed: dict[str, FieldInfo]
```

Invalid type qualifiers are diagnosed as Pydantic model fields rather than dataclass fields:

```py
from typing_extensions import NotRequired, ReadOnly, Required

class InvalidFieldQualifiers(BaseModel):
    # error: [invalid-type-form] "`NotRequired` is not allowed in Pydantic model fields"
    not_required: NotRequired[int]
    # error: [invalid-type-form] "`ReadOnly` is not allowed in Pydantic model fields"
    read_only: ReadOnly[int]
    # error: [invalid-type-form] "`Required` is not allowed in Pydantic model fields"
    required: Required[int]
```

## Pydantic dataclasses

Pydantic's dataclasses are similar to the standard library dataclasses:

```py
from pydantic import Field
from pydantic.dataclasses import dataclass

@dataclass
class Person:
    name: str
    id: int = Field(default=0, init=False)
    age: int = Field(default=0)

# `id` is absent in the constructor signature:
reveal_type(Person.__init__)  # revealed: (self: Person, name: str, age: int = 0) -> None

Person(name="Alice")
Person(name="Alice", age=20)
Person(name="Alice", something_else=7)  # error: [unknown-argument]
```

## Inherited `ModelMetaclass`

Pydantic's metaclass-based `@dataclass_transform` metadata should continue to apply when a custom
metaclass inherits from `ModelMetaclass`.

```py
from pydantic import BaseModel
from pydantic._internal._model_construction import ModelMetaclass

class RegistryMeta(ModelMetaclass): ...

class User(BaseModel, metaclass=RegistryMeta):
    name: str
    age: int = 0

reveal_type(User.__init__)  # revealed: (self: User, *, name: LaxStr, age: LaxInt = 0, **extra: Any) -> None

User(name="alice")
User(name="alice", age=1)

# error: [pydantic-discarded-extra-argument]
User(name="alice", extra=1)

# error: [missing-argument]
User()
```

## Validator and serializer decorators with explicit `@classmethod`

Pydantic [recommends](https://docs.pydantic.dev/latest/concepts/validators/#class-validators) using
an explicit `@classmethod` decorator below `@field_validator` / `@model_validator(mode="before")` /
`@field_serializer` to get proper type checking. The first parameter should be inferred as
`type[Self]`. ty does not support recognizing these functions as *implicit* class methods, so the
`@classmethod` decorator is required for correct type inference.

```py
from pydantic import BaseModel, field_validator, model_validator, field_serializer

class User(BaseModel):
    name: str

    @field_validator("name")
    @classmethod
    def validate_name(cls, v: str) -> str:
        reveal_type(cls)  # revealed: type[Self@validate_name]
        return v.strip()

    @model_validator(mode="before")
    @classmethod
    def validate_model_before(cls, values: dict[str, object]) -> dict[str, object]:
        reveal_type(cls)  # revealed: type[Self@validate_model_before]
        return values

    @field_serializer("name")
    @classmethod
    def serialize_name(cls, v: str) -> str:
        reveal_type(cls)  # revealed: type[Self@serialize_name]
        return v.upper()

    # No @classmethod for "after" validators: the first parameter should be inferred as "Self"
    @model_validator(mode="after")
    def validate_model_after(self) -> "User":
        reveal_type(self)  # revealed: Self@validate_model_after
        return self
```

## First-party modules named `pydantic`

A first-party module that happens to use Pydantic's module and class names should not receive
Pydantic-specific behavior.

`/src/pydantic/__init__.py`:

```py
from .main import BaseModel as BaseModel
```

`/src/pydantic/main.py`:

```py
from typing import dataclass_transform

@dataclass_transform(kw_only_default=True)
class ModelMetaclass(type): ...

class BaseModel(metaclass=ModelMetaclass): ...
```

`/src/main.py`:

```py
from pydantic import BaseModel

class Person(BaseModel):
    name: str

reveal_type(Person.__init__)  # revealed: (self: Person, *, name: str) -> None
Person(name="Alice")
Person(name="Alice", something_else=7)  # error: [unknown-argument]
```

[conversion table]: https://pydantic.dev/docs/validation/latest/concepts/conversion_table
