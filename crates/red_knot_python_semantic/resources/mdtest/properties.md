# Properties

`property` is a built-in class in Python that can be used to model class attributes with custom
getters, setters, and deleters.

## Basic getter

`property` is typically used as a decorator on a getter method. It turns the method into a property
object. When accessing the property on an instance, the descriptor protocol is invoked, which calls
the getter method:

```py
class C:
    @property
    def my_property(self) -> int:
        return 1

reveal_type(C().my_property)  # revealed: int
```

When a property is accessed on the class directly, the descriptor protocol is also invoked, but
`property.__get__` simply returns itself in this case (when `instance` is `None`):

```py
reveal_type(C.my_property)  # revealed: property
```

## Getter and setter

A property can also have a setter method, which is used to set the value of the property. The setter
method is defined using the `@<property_name>.setter` decorator. The setter method takes the value
to be set as an argument.

```py
class C:
    @property
    def my_property(self) -> int:
        return 1

    @my_property.setter
    def my_property(self, value: int) -> None:
        pass

c = C()
reveal_type(c.my_property)  # revealed: int
c.my_property = 2

# error: [invalid-assignment]
c.my_property = "a"
```

## `property.getter`

`property.getter` can be used to overwrite the getter method of a property. This does not overwrite
the existing setter:

```py
class C:
    @property
    def my_property(self) -> int:
        return 1

    @my_property.setter
    def my_property(self, value: int) -> None:
        pass

    @my_property.getter
    def my_property(self) -> str:
        return "a"

c = C()
reveal_type(c.my_property)  # revealed: str
c.my_property = 2

# error: [invalid-assignment]
c.my_property = "b"
```

## `property.deleter`

We do not support `property.deleter` yet, but we make sure that it does not invalidate the getter or
setter:

```py
class C:
    @property
    def my_property(self) -> int:
        return 1

    @my_property.setter
    def my_property(self, value: int) -> None:
        pass

    @my_property.deleter
    def my_property(self) -> None:
        pass

c = C()
reveal_type(c.my_property)  # revealed: int
c.my_property = 2
# error: [invalid-assignment]
c.my_property = "a"
```

## Failure cases

### Attempting to write to a read-only property

When attempting to write to a read-only property, we emit an error:

```py
class C:
    @property
    def attr(self) -> int:
        return 1

c = C()

# error: [invalid-assignment]
c.attr = 2
```

### Attempting to read a write-only property

When attempting to read a write-only property, we emit an error:

```py
class C:
    def attr_setter(self, value: int) -> None:
        pass
    attr = property(fset=attr_setter)

c = C()
c.attr = 1

# TODO: An error should be emitted here, and the type should be `Unknown`
# or `Never`. See https://github.com/astral-sh/ruff/issues/16298 for more
# details.
reveal_type(c.attr)  # revealed: Unknown | property
```

### Wrong setter signature

```py
class C:
    @property
    def attr(self) -> int:
        return 1
    # error: [invalid-argument-type] "Object of type `Literal[attr]` cannot be assigned to parameter 2 (`fset`) of bound method `setter`; expected type `(Any, Any, /) -> None`"
    @attr.setter
    def attr(self) -> None:
        pass
```

### Wrong getter signature

```py
class C:
    # error: [invalid-argument-type] "Object of type `Literal[attr]` cannot be assigned to parameter 1 (`fget`) of class `property`; expected type `((Any, /) -> Any) | None`"
    @property
    def attr(self, x: int) -> int:
        return 1
```

## Limitations

### Manually constructed property

Properties can also be constructed manually using the `property` class. We partially support this:

```py
class C:
    def attr_getter(self) -> int:
        return 1
    attr = property(attr_getter)

c = C()
reveal_type(c.attr)  # revealed: Unknown | int
```

But note that we return `Unknown | int` because we did not declare the `attr` attribute. This is
consistent with how we usually treat attributes, but here, if we try to declare `attr` as
`property`, we fail to understand the property, since the `property` declaration shadows the more
precise type that we infer for `property(attr_getter)` (which includes the actual information about
the getter).

```py
class C:
    def attr_getter(self) -> int:
        return 1
    attr: property = property(attr_getter)

c = C()
reveal_type(c.attr)  # revealed: Unknown
```

## Behind the scenes

In this section, we trace through some of the steps that make properties work. We start with a
simple class `C` and a property `attr`:

```py
class C:
    def __init__(self):
        self._attr: int = 0

    @property
    def attr(self) -> int:
        return self._attr

    @attr.setter
    def attr(self, value: str) -> None:
        self._attr = len(value)
```

Next, we create an instance of `C`. As we have seen above, accessing `attr` on the instance will
return an `int`:

```py
c = C()

reveal_type(c.attr)  # revealed: int
```

Behind the scenes, when we write `c.attr`, the first thing that happens is that we statically look
up the symbol `attr` on the meta-type of `c`, i.e. the class `C`. We can emulate this static lookup
using `inspect.getattr_static`, to see that `attr` is actually an instance of the `property` class:

```py
from inspect import getattr_static

attr_property = getattr_static(C, "attr")
reveal_type(attr_property)  # revealed: property
```

The `property` class has a `__get__` method, which makes it a descriptor. It also has a `__set__`
method, which means that it is a *data* descriptor (if there is no setter, `__set__` is still
available but yields an `AttributeError` at runtime).

```py
reveal_type(type(attr_property).__get__)  # revealed: <wrapper-descriptor `__get__` of `property` objects>
reveal_type(type(attr_property).__set__)  # revealed: <wrapper-descriptor `__set__` of `property` objects>
```

When we access `c.attr`, the `__get__` method of the `property` class is called, passing the
property object itself as the first argument, and the class instance `c` as the second argument. The
third argument is the "owner" which can be set to `None` or to `C` in this case:

```py
reveal_type(type(attr_property).__get__(attr_property, c, C))  # revealed: int
reveal_type(type(attr_property).__get__(attr_property, c, None))  # revealed: int
```

Alternatively, the above can also be written as a method call:

```py
reveal_type(attr_property.__get__(c, C))  # revealed: int
```

When we access `attr` on the class itself, the descriptor protocol is also invoked, but the instance
argument is set to `None`. When `instance` is `None`, the call to `property.__get__` returns the
property instance itself. So the following expressions are all equivalent

```py
reveal_type(attr_property)  # revealed: property
reveal_type(C.attr)  # revealed: property
reveal_type(attr_property.__get__(None, C))  # revealed: property
reveal_type(type(attr_property).__get__(attr_property, None, C))  # revealed: property
```

When we set the property using `c.attr = "a"`, the `__set__` method of the property class is called.
This attribute access desugars to

```py
type(attr_property).__set__(attr_property, c, "a")

# error: [call-non-callable] "Call of wrapper descriptor `property.__set__` failed: calling the setter failed"
type(attr_property).__set__(attr_property, c, 1)
```

which is also equivalent to the following expressions:

```py
attr_property.__set__(c, "a")
# error: [call-non-callable]
attr_property.__set__(c, 1)

C.attr.__set__(c, "a")
# error: [call-non-callable]
C.attr.__set__(c, 1)
```

Properties also have `fget` and `fset` attributes that can be used to retrieve the original getter
and setter functions, respectively.

```py
reveal_type(attr_property.fget)  # revealed: Literal[attr]
reveal_type(attr_property.fget(c))  # revealed: int

reveal_type(attr_property.fset)  # revealed: Literal[attr]
reveal_type(attr_property.fset(c, "a"))  # revealed: None

# error: [invalid-argument-type]
attr_property.fset(c, 1)
```
