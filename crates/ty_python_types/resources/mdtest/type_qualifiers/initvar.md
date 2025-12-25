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

We can see in the signature of `__init__` that `db` is included as an argument:

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

## `InitVar` with default value

An `InitVar` can also have a default value. In this case, the attribute *is* accessible on the class
and on instances:

```py
from dataclasses import InitVar, dataclass

@dataclass
class Person:
    name: str
    age: int

    metadata: InitVar[str] = "default"

reveal_type(Person.__init__)  # revealed: (self: Person, name: str, age: int, metadata: str = "default") -> None

alice = Person("Alice", 30)
bob = Person("Bob", 25, "custom metadata")

reveal_type(bob.metadata)  # revealed: str

reveal_type(Person.metadata)  # revealed: str
```

## Overwritten `InitVar`

We do not emit an error if an `InitVar` attribute is later overwritten on the instance. In that
case, we also allow the attribute to be accessed:

```py
from dataclasses import InitVar, dataclass

@dataclass
class Person:
    name: str
    metadata: InitVar[str]

    def __post_init__(self, metadata: str) -> None:
        self.metadata = f"Person with name {self.name}"

alice = Person("Alice", "metadata that will be overwritten")

reveal_type(alice.metadata)  # revealed: str
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

A trailing comma in a subscript creates a single-element tuple. We need to handle this gracefully
and emit a proper error rather than crashing (see
[ty#1793](https://github.com/astral-sh/ty/issues/1793)).

```py
from dataclasses import InitVar, dataclass

@dataclass
class AlsoWrong:
    # error: [invalid-type-form] "Tuple literals are not allowed in this context in a type expression: Did you mean `tuple[()]`?"
    x: InitVar[(),]

# revealed: (self: AlsoWrong, x: Unknown) -> None
reveal_type(AlsoWrong.__init__)

# error: [unresolved-attribute]
reveal_type(AlsoWrong(42).x)  # revealed: Unknown
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
