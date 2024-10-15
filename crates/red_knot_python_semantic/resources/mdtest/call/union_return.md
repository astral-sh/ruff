# Union return from calls

## Union of return types

```py
if flag:
    def f() -> int:
        return 1
else:
    def f() -> str:
        return 'foo'

x = f()
reveal_type(x)  # revealed: int | str
```

## Calling with an unknown union

```py
from nonexistent import f # error: [unresolved-import] "Cannot resolve import `nonexistent`"

if flag:
    def f() -> int:
        return 1

x = f()
reveal_type(x)  # revealed: Unknown | int
```

## Non-callable elements in a union

If a variable in a union isn't callable, it should raise an error when called.

```py
if flag:
    f = 1
else:
    def f() -> int:
        return 1

x = f()  # error: "Object of type `Literal[1] | Literal[f]` is not callable (due to union element `Literal[1]`)"
reveal_type(x)  # revealed: Unknown | int
```

## Multiple non-callable elements in a union

When more than one element of a union is not callable, the system should flag both.

```py
if flag:
    f = 1
elif flag2:
    f = 'foo'
else:
    def f() -> int:
        return 1

x = f()  # error: "Object of type `Literal[1] | Literal["foo"] | Literal[f]` is not callable (due to union elements Literal[1], Literal["foo"])"
reveal_type(x)  # revealed: Unknown | int
```

## All non-callable union elements

If none of the elements in a union are callable, the type system should raise an error.

```py
if flag:
    f = 1
else:
    f = 'foo'

x = f()  # error: "Object of type `Literal[1] | Literal["foo"]` is not callable"
reveal_type(x)  # revealed: Unknown
```
