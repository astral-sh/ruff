# `del` statement

## Basic

```py
a = 1
del a
# error: [unresolved-reference]
reveal_type(a)  # revealed: Unknown

# error: [invalid-syntax] "Invalid delete target"
del 1

# error: [unresolved-reference]
del a

x, y = 1, 2
del x, y
# error: [unresolved-reference]
reveal_type(x)  # revealed: Unknown
# error: [unresolved-reference]
reveal_type(y)  # revealed: Unknown

def cond() -> bool:
    return True

b = 1
if cond():
    del b

# error: [possibly-unresolved-reference]
reveal_type(b)  # revealed: Literal[1]

c = 1
if cond():
    c = 2
else:
    del c

# error: [possibly-unresolved-reference]
reveal_type(c)  # revealed: Literal[2]

d = [1, 2, 3]

def delete():
    del d  # error: [unresolved-reference] "Name `d` used when not defined"

delete()
reveal_type(d)  # revealed: list[int]

def delete_element():
    # When the `del` target isn't a name, it doesn't force local resolution.
    del d[0]
    print(d)

def delete_global():
    global d
    del d
    # We could lint that `d` is unbound in this trivial case, but because it's global we'd need to
    # be careful about false positives if `d` got reinitialized somehow in between the two `del`s.
    del d

delete_global()
# Again, the variable should have been removed, but we don't check it.
reveal_type(d)  # revealed: list[int]

def delete_nonlocal():
    e = 2

    def delete_nonlocal_bad():
        del e  # error: [unresolved-reference] "Name `e` used when not defined"

    def delete_nonlocal_ok():
        nonlocal e
        del e
        # As with `global` above, we don't track that the nonlocal `e` is unbound.
        del e
```

## `del` forces local resolution even if it's unreachable

Without a `global x` or `nonlocal x` declaration in `foo`, `del x` in `foo` causes `print(x)` in an
inner function `bar` to resolve to `foo`'s binding, in this case an unresolved reference / unbound
local error:

```py
x = 1

def foo():
    print(x)  # error: [unresolved-reference] "Name `x` used when not defined"
    if False:
        # Assigning to `x` would have the same effect here.
        del x

    def bar():
        print(x)  # error: [unresolved-reference] "Name `x` used when not defined"
```

## But `del` doesn't force local resolution of `global` or `nonlocal` variables

However, with `global x` in `foo`, `print(x)` in `bar` resolves in the global scope, despite the
`del` in `foo`:

```py
x = 1

def foo():
    global x
    def bar():
        # allowed, refers to `x` in the global scope
        reveal_type(x)  # revealed: Literal[1]
    bar()
    del x  # allowed, deletes `x` in the global scope (though we don't track that)
```

`nonlocal x` has a similar effect, if we add an extra `enclosing` scope to give it something to
refer to:

```py
def enclosing():
    x = 2
    def foo():
        nonlocal x
        def bar():
            # allowed, refers to `x` in `enclosing`
            reveal_type(x)  # revealed: Literal[2]
        bar()
        del x  # allowed, deletes `x` in `enclosing` (though we don't track that)
```

## Delete attributes

If an attribute is referenced after being deleted, it will be an error at runtime. But we don't
treat this as an error (because there may have been a redefinition by a method between the `del`
statement and the reference). However, deleting an attribute invalidates type narrowing by
assignment, and the attribute type will be the originally declared type.

### Invalidate narrowing

```py
class C:
    x: int = 1

c = C()
del c.x
reveal_type(c.x)  # revealed: int

# error: [unresolved-attribute]
del c.non_existent

c.x = 1
reveal_type(c.x)  # revealed: Literal[1]
del c.x
reveal_type(c.x)  # revealed: int
```

### Delete an instance attribute definition

```py
class C:
    x: int = 1

c = C()
reveal_type(c.x)  # revealed: int

del C.x
c = C()
# This attribute is unresolved, but we won't check it for now.
reveal_type(c.x)  # revealed: int
```

### Property deleters

```py
from typing import NoReturn

class ReadOnlyProperty:
    @property
    def x(self) -> int:
        return 1

class SupportsDelete:
    @property
    def x(self) -> int:
        return 1

    @x.deleter
    def x(self) -> None:
        pass

class RejectsDescriptorDelete:
    @property
    def x(self) -> int:
        return 1

    @x.deleter
    def x(self) -> NoReturn:
        raise AttributeError("x")

class ExplicitNoneDeleter:
    def get(self) -> int:
        return 1

    keyword = property(get, fdel=None)
    positional = property(get, None, None)

read_only = ReadOnlyProperty()
# error: [invalid-assignment] "Cannot delete read-only property `x` on object of type `ReadOnlyProperty`"
del read_only.x

supports_delete = SupportsDelete()
del supports_delete.x

rejects_descriptor_delete = RejectsDescriptorDelete()
# error: [invalid-assignment] "Cannot delete attribute `x` on type `RejectsDescriptorDelete` whose `__delete__` method returns `Never`/`NoReturn`"
del rejects_descriptor_delete.x

explicit_none_deleter = ExplicitNoneDeleter()
# error: [invalid-assignment] "Cannot delete read-only property `keyword` on object of type `ExplicitNoneDeleter`"
del explicit_none_deleter.keyword
# error: [invalid-assignment] "Cannot delete read-only property `positional` on object of type `ExplicitNoneDeleter`"
del explicit_none_deleter.positional
```

### Instance `__delattr__`

```py
from typing import NamedTuple, NoReturn

class SupportsCustomDelete:
    @property
    def x(self) -> int:
        return 1

    def __delattr__(self, name: str) -> None:
        pass

class RejectsDelete:
    @property
    def x(self) -> int:
        return 1

    def __delattr__(self, name: str) -> NoReturn:
        raise AttributeError(name)

class BadDelAttr:
    x: int = 1

    # error: [invalid-method-override] "Invalid override of method `__delattr__`: Definition is incompatible with `object.__delattr__`"
    def __delattr__(self, name: int) -> None:
        pass

class DeletableNamedTuple(NamedTuple):
    x: int

    def __delattr__(self, name: str) -> None:
        pass

supports_custom_delete = SupportsCustomDelete()
del supports_custom_delete.x

rejects_delete = RejectsDelete()
# error: [invalid-assignment] "Cannot delete attribute `x` on type `RejectsDelete` whose `__delattr__` method returns `Never`/`NoReturn`"
del rejects_delete.x

bad_delattr = BadDelAttr()
# error: [invalid-assignment] "Cannot delete attribute `x` on type `BadDelAttr` with custom `__delattr__` method"
del bad_delattr.x

deletable_namedtuple = DeletableNamedTuple(1)
del deletable_namedtuple.x
```

### Descriptor `__delete__`

```py
class Weird:
    def __delete__(self, instance: object, extra: object) -> None:
        pass

class FallbackInstanceAttribute:
    def __init__(self) -> None:
        self.x = Weird()

fallback_instance_attribute = FallbackInstanceAttribute()
del fallback_instance_attribute.x
```

### Final attributes

```py
from typing import Final

class FinalAttribute:
    def __init__(self) -> None:
        self.x: Final[int] = 1

class FinalAttributeWithDelAttr:
    def __init__(self) -> None:
        self.x: Final[int] = 1

    def __delattr__(self, name: str) -> None:
        pass

class FinalClassAttribute:
    x: Final[int] = 1

final_attribute = FinalAttribute()
# error: [invalid-assignment] "Cannot delete final attribute `x` on type `FinalAttribute`"
del final_attribute.x

final_attribute_with_delattr = FinalAttributeWithDelAttr()
# error: [invalid-assignment] "Cannot delete final attribute `x` on type `FinalAttributeWithDelAttr`"
del final_attribute_with_delattr.x

# error: [invalid-assignment] "Cannot delete final attribute `x` on type `<class 'FinalClassAttribute'>`"
del FinalClassAttribute.x
```

### Metaclass `__delattr__`

```py
from typing import NoReturn

class SupportsClassDeleteMeta(type):
    def __delattr__(self, name: str) -> None:
        pass

class SupportsClassDelete(metaclass=SupportsClassDeleteMeta):
    x: int = 1

class RejectsClassDeleteMeta(type):
    def __delattr__(self, name: str) -> NoReturn:
        raise AttributeError(name)

class RejectsClassDelete(metaclass=RejectsClassDeleteMeta):
    x: int = 1

del SupportsClassDelete.x

# error: [invalid-assignment] "Cannot delete attribute `x` on type `<class 'RejectsClassDelete'>` whose `__delattr__` method returns `Never`/`NoReturn`"
del RejectsClassDelete.x
```

## Delete items

### Basic item deletion

Deleting an item also invalidates the narrowing by the assignment, but accessing the item itself is
still valid.

```py
def f(l: list[int]):
    del l[0]
    # If the length of `l` was 1, this will be a runtime error,
    # but if it was greater than that, it will not be an error.
    reveal_type(l[0])  # revealed: int

    # error: [invalid-argument-type]
    del l["string"]

    l[0] = 1
    reveal_type(l[0])  # revealed: Literal[1]
    del l[0]
    reveal_type(l[0])  # revealed: int
```

### `__delitem__` without `__getitem__`

A class or protocol that only defines `__delitem__` (without `__getitem__`) should still support
item deletion. The `__delitem__` method is independent of `__getitem__`.

```py
from typing import Protocol, TypeVar

KT = TypeVar("KT")

class CanDelItem(Protocol[KT]):
    def __delitem__(self, k: KT, /) -> None: ...

def f(x: CanDelItem[int], k: int):
    # This should be valid - the object has __delitem__
    del x[k]

class OnlyDelItem:
    def __delitem__(self, key: int) -> None:
        pass

d = OnlyDelItem()
del d[0]  # OK

# error: [not-subscriptable] "Cannot subscript object of type `OnlyDelItem` with no `__getitem__` method"
d[0]
```

### `__getitem__` without `__delitem__`

A class that only defines `__getitem__` (without `__delitem__`) should not support item deletion.

```py
class OnlyGetItem:
    def __getitem__(self, key: int) -> str:
        return "value"

g = OnlyGetItem()
reveal_type(g[0])  # revealed: str

# error: [not-subscriptable] "Cannot delete subscript on object of type `OnlyGetItem` with no `__delitem__` method"
del g[0]
```

### TypedDict deletion

Deleting a required key from a TypedDict is a type error because it would make the object no longer
a valid instance of that TypedDict type. However, deleting `NotRequired` keys (or keys in
`total=False` TypedDicts) is allowed.

```py
from typing_extensions import TypedDict, NotRequired

class Movie(TypedDict):
    name: str
    year: int

class PartialMovie(TypedDict, total=False):
    name: str
    year: int

class MixedMovie(TypedDict):
    name: str
    year: NotRequired[int]

m: Movie = {"name": "Blade Runner", "year": 1982}
p: PartialMovie = {"name": "Test"}
mixed: MixedMovie = {"name": "Test"}
```

Required keys cannot be deleted.

```py
# snapshot: invalid-argument-type
del m["name"]
```

```snapshot
error[invalid-argument-type]: Cannot delete required key "name" from TypedDict `Movie`
  --> src/mdtest_snippet.py:19:7
   |
19 | del m["name"]
   |       ^^^^^^
   |
info: Field defined here
 --> src/mdtest_snippet.py:3:7
  |
3 | class Movie(TypedDict):
  |       ---------------- `Movie` defined here
4 |     name: str
  |     ---------
  |     |
  |     `name` declared as required here
  |     Consider making it `NotRequired`
  |
info: Only keys marked as `NotRequired` (or in a TypedDict with `total=False`) can be deleted
```

In a partial TypedDict (`total=False`), all keys can be deleted.

```py
del p["name"]
```

`NotRequired` keys can always be deleted.

```py
del mixed["year"]
```

But required keys in mixed `TypedDict` still cannot be deleted.

```py
# snapshot: invalid-argument-type
del mixed["name"]
```

```snapshot
error[invalid-argument-type]: Cannot delete required key "name" from TypedDict `MixedMovie`
  --> src/mdtest_snippet.py:23:11
   |
23 | del mixed["name"]
   |           ^^^^^^
   |
info: Field defined here
  --> src/mdtest_snippet.py:11:7
   |
11 | class MixedMovie(TypedDict):
   |       --------------------- `MixedMovie` defined here
12 |     name: str
   |     ---------
   |     |
   |     `name` declared as required here
   |     Consider making it `NotRequired`
   |
info: Only keys marked as `NotRequired` (or in a TypedDict with `total=False`) can be deleted
```

And keys that don't exist cannot be deleted.

```py
# snapshot: invalid-argument-type
del mixed["non_existent"]
```

```snapshot
error[invalid-argument-type]: Cannot delete unknown key "non_existent" from TypedDict `MixedMovie`
  --> src/mdtest_snippet.py:25:11
   |
25 | del mixed["non_existent"]
   |           ^^^^^^^^^^^^^^
   |
```
