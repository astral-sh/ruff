# f-strings

## Expression

```py
x = 0
y = str()
z = False

a = f'hello'
reveal_type(a)  # revealed: Literal["hello"]

b = f'h {x}'
reveal_type(b)  # revealed: Literal["h 0"]

c = 'one ' f'single ' f'literal'
reveal_type(c)  # revealed: Literal["one single literal"]

d = 'first ' f'second({b})' f' third'
reveal_type(d)  # revealed: Literal["first second(h 0) third"]

e = f'-{y}-'
reveal_type(e)  # revealed: str

f = f'-{y}-' f'--' '--'
reveal_type(f)  # revealed: str

g = f'{z} == {False} is {True}'
reveal_type(g)  # revealed: Literal["False == False is True"]
```

## Conversion Flags

```py
string = 'hello'

# TODO: should be `Literal["'hello'"]`
reveal_type(f'{string!r}')  # revealed: str
```

## Format Specifiers

```py
# TODO: should be `Literal["01"]`
reveal_type(f'{1:02}')  # revealed: str
```
