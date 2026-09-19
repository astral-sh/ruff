# Row polymorphism

```toml
[environment]
python-version = "3.14"
```

```py
from typing import LiteralString, Literal, Protocol, Any, TYPE_CHECKING

if TYPE_CHECKING:
    from ty_extensions import Top
```

We can use intersection types and generic protocols to simulate row polymorphism in ty's type
system. We start by defining a protocol that represents a type that has a `Key` of type `Value`:

```py
class HasItem[Key: LiteralString, Value](Protocol):
    def __getitem__(self, key: Key) -> Value: ...
    def __setitem__(self, key: Key, value: Value) -> None: ...
    def __delitem__(self, key: Key) -> None: ...
```

We can then define a row as an intersection of multiple `HasItem` types:

```py
type Person = 'HasItem[Literal["name"], str] & HasItem[Literal["age"], int]'

def _(person: Person) -> None:
    reveal_type(person["name"])  # revealed: str
    reveal_type(person["age"])  # revealed: int
    person["name"] = "Alice"  # OK
    person["name"] = 42  # error: [invalid-assignment]
```

We can now write functions that work with any row that has a specific key, for example:

```py
def say_hello(row: HasItem[Literal["name"], str]):
    print(f"Hello, {row['name']}!")
```

And we can write functions that are generic over rows. We use the union of all `HasItem` instances
as an upper bound for the row type variable:

```py
type RowTop = Top[HasItem[Any, Any]]

def add_id[Row: RowTop](row: Row) -> Row & HasItem[Literal["id"], int]:
    row["id"] = 42  # ty:ignore[invalid-assignment]
    return row  # ty:ignore[invalid-return-type]

def _(person: Person) -> None:
    person_with_id = add_id(person)

    reveal_type(person_with_id["id"])  # revealed: int

    # Other fields are still accessible
    reveal_type(person_with_id["name"])  # revealed: str
    reveal_type(person_with_id["age"])  # revealed: int
```

Using negation types and a wrapper (`Filter`), we can even define a function that removes a key from
a row:

```py
if TYPE_CHECKING:
    class Filter[Row: RowTop, Allowed]:
        def __getitem__[Key: LiteralString, V](self: Filter[RowTop & HasItem[Key, V], Allowed], key: "Key & Allowed") -> V: ...
        def __setitem__[Key: LiteralString, V](
            self: Filter[RowTop & HasItem[Key, V], Allowed], key: "Key & Allowed", value: V
        ) -> None: ...
        def __delitem__[Key: LiteralString, V](self: Filter[RowTop & HasItem[Key, V], Allowed], key: "Key & Allowed") -> None: ...

def remove_key[Row: RowTop, Key: LiteralString](row: Row, key: Key) -> Filter[Row, ~Key]:
    del row[key]  # ty:ignore[invalid-argument-type]
    return row  # ty:ignore[invalid-return-type]

def _(person: Person) -> None:
    person_without_age = remove_key(person, "age")

    # This is now an error:
    person_without_age["age"]  # error: [invalid-argument-type]

    # But the "name" key is still accessible:
    reveal_type(person_without_age["name"])  # revealed: str
```

So far, we've only talked about modifying existing rows. To actually create one, we could use a
builder pattern to construct a dictionary that satisfies a given row type:

```py
class Row[R: RowTop = RowTop]:
    def __init__(self):
        self._data = {}

    def add[Key: LiteralString, Value](self, key: Key, value: Value) -> Row[R & HasItem[Key, Value]]:
        self._data[key] = value
        return self  # ty:ignore[invalid-return-type]

    def build(self) -> R:
        return self._data  # ty:ignore[invalid-return-type]

alice: Person = Row().add("name", "Alice").add("age", 30).build()
say_hello(alice)
```

Omitting a key, or using a wrong type, will result in an error:

```py
bob: Person = Row().add("name", "Bob").build()  # error: [invalid-assignment]
eve: Person = Row().add("name", "Eve").add("age", "thirty").build()  # error: [invalid-assignment]
```
