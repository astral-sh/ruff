# f-strings

## Expression

```py
x = 0
y = str()
z = False

reveal_type(f"hello")  # revealed: Literal["hello"]
reveal_type(f"h {x}")  # revealed: Literal["h 0"]
reveal_type("one " f"single " f"literal")  # revealed: Literal["one single literal"]
reveal_type("first " f"second({x})" f" third")  # revealed: Literal["first second(0) third"]
reveal_type(f"-{y}-")  # revealed: str
reveal_type(f"-{y}-" f"--" "--")  # revealed: str
reveal_type(f"{z} == {False} is {True}")  # revealed: Literal["False == False is True"]
```

## Conversion Flags

```py
string = "hello"

# TODO: should be `Literal["'hello'"]`
reveal_type(f"{string!r}")  # revealed: str
```

## Format Specifiers

```py
# TODO: should be `Literal["01"]`
reveal_type(f"{1:02}")  # revealed: str
```
