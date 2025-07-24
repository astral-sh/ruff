# `dataclasses.InitVar`

From the Python documentation on [`dataclasses.InitVar`]:

If a field is an `InitVar`, it is considered a pseudo-field called an init-only field. As it is not
a true field, it is not returned by the module-level `fields()` function. Init-only fields are added
as parameters to the generated `__init__()` method, and are passed to the optional `__post_init__()`
method. They are not otherwise used by dataclasses.

## Basic

Consider the following dataclass example where the `db` attribute is annotated with `InitVar`:

```py
from dataclasses import InitVar, dataclass

class Database: ...

@dataclass(order=True)
class Person:
    db: InitVar[Database]

    name: str
    age: int
```

We can see in the signature if `__init__`, that `db` is included as an argument:

```py
reveal_type(Person.__init__)  # revealed: (self: Person, db: Database, name: str, age: int) -> None
```

However, when we create an instance of this dataclass, the `db` attribute is not accessible:

```py
db = Database()
alice = Person(db, "Alice", 30)

alice.db  # error: [unresolved-attribute]
```

The `db` attribute is also not accessible on the class itself:

```py
Person.db  # error: [unresolved-attribute]
```

Other fields can still be accessed normally:

```py
reveal_type(alice.name)  # revealed: str
reveal_type(alice.age)  # revealed: int
```

## `InitVar` wit default value

An `InitVar` can also have a default value

```py
from dataclasses import InitVar, dataclass

@dataclass
class Person:
    name: str
    age: int

    metadata: InitVar[str] = "default"

reveal_type(Person.__init__)  # revealed: (self: Person, name: str, age: int, metadata: str = Literal["default"]) -> None

alice = Person("Alice", 30)
bob = Person("Bob", 25, "custom metadata")
```

## Error cases

### Syntax

`InitVar` can only be used with a single argument:

```py
from dataclasses import InitVar, dataclass

@dataclass
class Wrong:
    x: InitVar[int, str]  # error: [invalid-type-form] "Type qualifier `InitVar` expected exactly 1 argument, got 2"
```

A bare `InitVar` is not allowed according to the [type annotation grammar]:

```py
@dataclass
class AlsoWrong:
    x: InitVar  # error: [invalid-type-form] "`InitVar` may not be used without a type argument"
```

### Outside of dataclasses

`InitVar` annotations are not allowed outside of dataclass attribute annotations:

```py
from dataclasses import InitVar, dataclass

# error: [invalid-type-form] "`InitVar` annotations are only allowed in class-body scopes"
x: InitVar[int] = 1

def f(x: InitVar[int]) -> None:  # error: [invalid-type-form] "`InitVar` is not allowed in function parameter annotations"
    pass

def g() -> InitVar[int]:  # error: [invalid-type-form] "`InitVar` is not allowed in function return type annotations"
    return 1

class C:
    # TODO: this would ideally be an error
    x: InitVar[int]

@dataclass
class D:
    def __init__(self) -> None:
        self.x: InitVar[int] = 1  # error: [invalid-type-form] "`InitVar` annotations are not allowed for non-name targets"
```

[type annotation grammar]: https://typing.python.org/en/latest/spec/annotations.html#type-and-annotation-expressions
[`dataclasses.initvar`]: https://docs.python.org/3/library/dataclasses.html#dataclasses.InitVar
