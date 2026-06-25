# Instance subscript

## `__getitem__` unbound

```py
class NotSubscriptable: ...

# error: [not-subscriptable] "Cannot subscript object of type `NotSubscriptable` with no `__getitem__` method"
NotSubscriptable()[0]

# snapshot: not-subscriptable
a = NotSubscriptable()[0]
```

```snapshot
error[not-subscriptable]: Cannot subscript object of type `NotSubscriptable` with no `__getitem__` method
 --> src/mdtest_snippet.py:7:5
  |
7 | a = NotSubscriptable()[0]
  |     ^^^^^^^^^^^^^^^^^^^^^
  |
```

## `__getitem__` not callable

```py
class NotSubscriptable:
    __getitem__ = None

# snapshot: not-subscriptable
a = NotSubscriptable()[0]
```

```snapshot
error[not-subscriptable]: Invalid subscript read
 --> src/mdtest_snippet.py:5:5
  |
5 | a = NotSubscriptable()[0]
  |     ------------------^^^
  |     |                  |
  |     |                  Method `__getitem__` has type `None | Unknown`
  |     |                  An object of type `None | Unknown` may not be callable
  |     Has type `NotSubscriptable`
  |
info: `__getitem__` is implicitly called due to this subscript expression
```

## Valid `__getitem__`

```py
class Identity:
    def __getitem__(self, index: int) -> int:
        return index

reveal_type(Identity()[0])  # revealed: int
```

## Slice bounds for user-defined `__getitem__`

```py
class IntegerSlices:
    def __getitem__(self, key: slice[int | None, int | None, int | None]) -> str:
        return ""

class ArbitrarySlices:
    def __getitem__(self, key: slice) -> str:
        return ""

def _(
    integer_slices: IntegerSlices,
    arbitrary_slices: ArbitrarySlices,
    bound: object,
    invalid_bound: float,
) -> None:
    integer_slices[invalid_bound:]  # error: [invalid-argument-type]
    arbitrary_slices[bound:bound:bound]
```

## `__getitem__` union

```py
def _(flag: bool):
    class Identity:
        if flag:
            def __getitem__(self, index: int) -> int:
                return index

        else:
            def __getitem__(self, index: int) -> str:
                return str(index)

    reveal_type(Identity()[0])  # revealed: int | str
```

## `__getitem__` with too many parameters

```py
class Foo:
    def __getitem__(self, x, y): ...

# error: [missing-argument] "No argument provided for required parameter `y` of bound method `Foo.__getitem__`"
Foo()["x"]

Foo()["x"]  # snapshot: missing-argument
```

```snapshot
error[missing-argument]: No argument provided for required parameter `y` of bound method `Foo.__getitem__`
 --> src/mdtest_snippet.py:7:1
  |
7 | Foo()["x"]  # snapshot: missing-argument
  | ^^^^^^^^^^
  |
info: Parameter declared here
 --> src/mdtest_snippet.py:2:30
  |
2 |     def __getitem__(self, x, y): ...
  |                              ^
  |
```

## `__getitem__` with too few parameters

```py
class Foo:
    def __getitem__(self): ...

Foo()["x"]  # snapshot: too-many-positional-arguments
```

```snapshot
error[too-many-positional-arguments]: Too many positional arguments to bound method `Foo.__getitem__`: expected 1, got 2
 --> src/mdtest_snippet.py:4:1
  |
4 | Foo()["x"]  # snapshot: too-many-positional-arguments
  | ^^^^^^^^^^
  |
info: Method signature here
 --> src/mdtest_snippet.py:2:9
  |
2 |     def __getitem__(self): ...
  |         ^^^^^^^^^^^^^^^^^
  |
```

## `__getitem__` with a bad `self` parameter

```toml
[environment]
python-version = "3.14"
```

```py
class Foo[T]:
    def __getitem__(self: Foo[str], x): ...

def test(x: Foo[int]):
    # TODO: should emit an error here, the `__getitem__` is only valid
    # if called on an instance of `Foo[str]`, not of `Foo[int]`
    x["foo"]
```

## Overloaded bad `__getitem__`

```toml
[environment]
python-version = "3.14"
```

```py
from typing import overload

class Foo[T]:
    @overload
    def __getitem__(self, x: int): ...
    @overload
    def __getitem__(self, x: str, y): ...
    def __getitem__(self, x, y=None): ...

def test(x: Foo[int]):
    x["foo"]  # snapshot: invalid-argument-type
```

```snapshot
error[invalid-argument-type]: Argument to bound method `Foo.__getitem__` is incorrect
  --> src/mdtest_snippet.py:11:5
   |
11 |     x["foo"]  # snapshot: invalid-argument-type
   |     ^^^^^^^^ Expected `int`, found `Literal["foo"]`
   |
info: Matching overload defined here
 --> src/mdtest_snippet.py:5:9
  |
5 |     def __getitem__(self, x: int): ...
  |         ^^^^^^^^^^^       ------ Parameter declared here
  |
info: Non-matching overloads for bound method `__getitem__`:
info:   (self, x: str, y) -> Unknown
```

## Union of bad `__getitem__` methods

```py
class Foo:
    def __getitem__(self, x: str): ...

class Bar:
    def __getitem__(self, x: str): ...

def test(x: Foo | Bar):
    # error: [invalid-argument-type] "Cannot subscript an object of type `Bar` with a key of type `Literal[42]` (expected `str`)"
    # error: [invalid-argument-type] "Cannot subscript an object of type `Foo` with a key of type `Literal[42]` (expected `str`)"
    x[42]

    # snapshot: invalid-argument-type
    # snapshot: invalid-argument-type
    x[42]
```

```snapshot
error[invalid-argument-type]: Invalid subscript read
  --> src/mdtest_snippet.py:14:5
   |
14 |     x[42]
   |     -^^^^
   |     | |
   |     | Expected `str`, got object of type `Literal[42]`
   |     Has type `Foo | Bar`
   |
info: This subscript expression implicitly calls `Bar.__getitem__`
 --> src/mdtest_snippet.py:5:9
  |
5 |     def __getitem__(self, x: str): ...
  |         ^^^^^^^^^^^ Method defined here
  |


error[invalid-argument-type]: Invalid subscript read
  --> src/mdtest_snippet.py:14:5
   |
14 |     x[42]
   |     -^^^^
   |     | |
   |     | Expected `str`, got object of type `Literal[42]`
   |     Has type `Foo | Bar`
   |
info: This subscript expression implicitly calls `Foo.__getitem__`
 --> src/mdtest_snippet.py:2:9
  |
2 |     def __getitem__(self, x: str): ...
  |         ^^^^^^^^^^^ Method defined here
  |
```

## Enum complement as overloaded `__getitem__` receiver

`overloaded.pyi`:

```pyi
from enum import Enum
from typing import Literal, overload

class Color(Enum):
    RED = 1
    GREEN = 2
    BLUE = 3

    @overload
    def __getitem__(self: Literal[Color.GREEN], index: int) -> int: ...
    @overload
    def __getitem__(self: Literal[Color.BLUE], index: int) -> str: ...
```

```py
from overloaded import Color

def _(color: Color):
    if color is Color.RED:
        return
    reveal_type(color[0])  # revealed: int | str
```

## Enum complement as overloaded subscript mutation receiver

`overloaded.pyi`:

```pyi
from enum import Enum
from typing import Literal, overload

class Color(Enum):
    RED = 1
    GREEN = 2
    BLUE = 3

    @overload
    def __setitem__(self: Literal[Color.GREEN], index: int, value: int) -> None: ...
    @overload
    def __setitem__(self: Literal[Color.BLUE], index: int, value: int) -> None: ...
    @overload
    def __delitem__(self: Literal[Color.GREEN], index: int) -> None: ...
    @overload
    def __delitem__(self: Literal[Color.BLUE], index: int) -> None: ...
```

```py
from typing import Literal

from overloaded import Color

def narrowed(color: Color):
    if color is Color.RED:
        return
    color[0] = 1
    del color[0]

def explicit(color: Literal[Color.GREEN, Color.BLUE]):
    color[0] = 1
    del color[0]
```

## `__getitem__` with invalid index argument

```py
class Identity:
    def __getitem__(self, index: int) -> int:
        return index

a = Identity()
# snapshot: invalid-argument-type
a["a"]
```

```snapshot
error[invalid-argument-type]: Invalid subscript read
 --> src/mdtest_snippet.py:7:1
  |
7 | a["a"]
  | -^^^^^
  | | |
  | | Expected `int`, got object of type `Literal["a"]`
  | Has type `Identity`
  |
info: This subscript expression implicitly calls `Identity.__getitem__`
 --> src/mdtest_snippet.py:2:9
  |
2 |     def __getitem__(self, index: int) -> int:
  |         ^^^^^^^^^^^ Method defined here
  |
```

## `__setitem__` with no `__getitem__`

```py
class NoGetitem:
    def __setitem__(self, index: int, value: int) -> None:
        pass

a = NoGetitem()
a[0] = 0
```

## Subscript store with no `__setitem__`

```py
class NoSetitem: ...

a = NoSetitem()
a[0] = 0  # error: "Cannot assign to a subscript on an object of type `NoSetitem`"
```

## `__setitem__` not callable

```py
class NoSetitem:
    __setitem__ = None

a = NoSetitem()
a[0] = 0  # error: "Method `__setitem__` of type `None | Unknown` may not be callable on object of type `NoSetitem`"
```

## Valid `__setitem__` method

```py
class Identity:
    def __setitem__(self, index: int, value: int) -> None:
        pass

a = Identity()
a[0] = 0
```

## `__setitem__` with invalid index argument

```py
class Identity:
    def __setitem__(self, index: int, value: int) -> None:
        pass

a = Identity()
# error: [invalid-assignment] "Invalid subscript assignment with key of type `Literal["a"]` and value of type `Literal[0]` on object of type `Identity`"
a["a"] = 0
```
