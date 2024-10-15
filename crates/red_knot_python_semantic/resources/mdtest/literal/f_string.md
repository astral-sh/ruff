# f-strings

## Expression

```py
x = 0
y = str()
z = False

a = f'hello'
b = f'h {x}'
c = 'one ' f'single ' f'literal'
d = 'first ' f'second({b})' f' third'
e = f'-{y}-'
f = f'-{y}-' f'--' '--'
g = f'{z} == {False} is {True}'

reveal_type(a)  # revealed: Literal["hello"]
reveal_type(b)  # revealed: Literal["h 0"]
reveal_type(c)  # revealed: Literal["one single literal"]
reveal_type(d)  # revealed: Literal["first second(h 0) third"]
reveal_type(e)  # revealed: str
reveal_type(f)  # revealed: str
reveal_type(g)  # revealed: Literal["False == False is True"]
```

## Conversion Flags

```py
string = 'hello'
a = f'{string!r}'

# TODO: should be `Literal["'hello'"]`
reveal_type(a)  # revealed: str
```

## Format Specifiers

```py
a = f'{1:02}'

# TODO: should be `Literal["01"]`
reveal_type(a)  # revealed: str
```
