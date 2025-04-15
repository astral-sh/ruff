# Dataclasses

## Basic

Decorating a class with `@dataclass` is a convenient way to add special methods such as `__init__`,
`__repr__`, and `__eq__` to a class. The following example shows the basic usage of the `@dataclass`
decorator. By default, only the three mentioned methods are generated.

```py
from dataclasses import dataclass

@dataclass
class Person:
    name: str
    age: int | None = None

alice1 = Person("Alice", 30)
alice2 = Person(name="Alice", age=30)
alice3 = Person(age=30, name="Alice")
alice4 = Person("Alice", age=30)

reveal_type(alice1)  # revealed: Person
reveal_type(type(alice1))  # revealed: type[Person]

reveal_type(alice1.name)  # revealed: str
reveal_type(alice1.age)  # revealed: int | None

reveal_type(repr(alice1))  # revealed: str

reveal_type(alice1 == alice2)  # revealed: bool
reveal_type(alice1 == "Alice")  # revealed: bool

bob = Person("Bob")
bob2 = Person("Bob", None)
bob3 = Person(name="Bob")
bob4 = Person(name="Bob", age=None)
```

The signature of the `__init__` method is generated based on the classes attributes. The following
calls are not valid:

```py
# TODO: should be an error: too few arguments
Person()

# TODO: should be an error: too many arguments
Person("Eve", 20, "too many arguments")

# TODO: should be an error: wrong argument type
Person("Eve", "string instead of int")

# TODO: should be an error: wrong argument types
Person(20, "Eve")
```

## `@dataclass` calls with arguments

The `@dataclass` decorator can take several arguments to customize the existence of the generated
methods. The following test makes sure that we still treat the class as a dataclass if (the default)
arguments are passed in:

```py
from dataclasses import dataclass

@dataclass(init=True, repr=True, eq=True)
class Person:
    name: str
    age: int | None = None

alice = Person("Alice", 30)
reveal_type(repr(alice))  # revealed: str
reveal_type(alice == alice)  # revealed: bool
```

If `init` is set to `False`, no `__init__` method is generated:

```py
from dataclasses import dataclass

@dataclass(init=False)
class C:
    x: int

C()  # Okay

# error: [too-many-positional-arguments]
C(1)

repr(C())

C() == C()
```

## Inheritance

### Normal class inheriting from a dataclass

```py
from dataclasses import dataclass

@dataclass
class Base:
    x: int

class Derived(Base): ...

d = Derived(1)  # OK
reveal_type(d.x)  # revealed: int
```

### Dataclass inheriting from normal class

```py
from dataclasses import dataclass

class Base:
    x: int = 1

@dataclass
class Derived(Base):
    y: str

d = Derived("a")

# TODO: should be an error:
Derived(1, "a")
```

### Dataclass inheriting from another dataclass

```py
from dataclasses import dataclass

@dataclass
class Base:
    x: int

@dataclass
class Derived(Base):
    y: str

d = Derived(1, "a")  # OK

reveal_type(d.x)  # revealed: int
reveal_type(d.y)  # revealed: str

# TODO: should be an error:
Derived("a")
```

## Generic dataclasses

```py
from dataclasses import dataclass

@dataclass
class DataWithDescription[T]:
    data: T
    description: str

reveal_type(DataWithDescription[int])  # revealed: Literal[DataWithDescription[int]]

d_int = DataWithDescription[int](1, "description")  # OK
reveal_type(d_int.data)  # revealed: int
reveal_type(d_int.description)  # revealed: str

# TODO: should be an error: wrong argument type
DataWithDescription[int](None, "description")
```

## Frozen instances

To do

## Descriptor-typed fields

To do

## `dataclasses.field`

To do

## Other special cases

### `dataclasses.dataclass`

We also understand dataclasses if they are decorated with the fully qualified name:

```py
import dataclasses

@dataclasses.dataclass
class C:
    x: str

# TODO: should show the proper signature
reveal_type(C.__init__)  # revealed: (*args: Any, **kwargs: Any) -> None
```

### Dataclass with `init=False`

To do

### Dataclass with custom `__init__` method

To do

### Dataclass with `ClassVar`s

To do

### Using `dataclass` as a function

To do

## Internals

The `dataclass` decorator returns the class itself. This means that the type of `Person` is `type`,
and attributes like the MRO are unchanged:

```py
from dataclasses import dataclass

@dataclass
class Person:
    name: str
    age: int | None = None

reveal_type(type(Person))  # revealed: Literal[type]
reveal_type(Person.__mro__)  # revealed: tuple[Literal[Person], Literal[object]]
```

The generated methods have the following signatures:

```py
# TODO: proper signature
reveal_type(Person.__init__)  # revealed: (*args: Any, **kwargs: Any) -> None

reveal_type(Person.__repr__)  # revealed: def __repr__(self) -> str

reveal_type(Person.__eq__)  # revealed: def __eq__(self, value: object, /) -> bool
```
