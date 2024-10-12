# Strings

## Literals

### Simple

We can infer a string literal type and track concatenated string literals as well:

```py
w = "Hello"
x = 'world'
y = "Guten " + 'tag'
z = 'bon ' + "jour"

reveal_type(w)  # revealed: Literal["Hello"]
reveal_type(x)  # revealed: Literal["world"]
reveal_type(y)  # revealed: Literal["Guten tag"]
reveal_type(z)  # revealed: Literal["bon jour"]
```

### Nested Quotes

We can handle string literals with nested quotes:

```py
x = 'I say "hello" to you'
y = "You say \"hey\" back"
z = 'No "closure here'
reveal_type(x)  # revealed: Literal["I say \"hello\" to you"]
reveal_type(y)  # revealed: Literal["You say \"hey\" back"]
reveal_type(z)  # revealed: Literal["No \"closure here"]
```

## f-strings

### Expression

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

### Conversion Flags

TODO: `a` should be `Literal["'hello'"]`

```py
string = 'hello'
a = f'{string!r}'
reveal_type(a)  # revealed: str
```

### Format Specifiers

TODO: `a` should be `Literal["01"]`

```py
a = f'{1:02}'
reveal_type(a)  # revealed: str
```

## Subscript

### Simple

We can infer the type of subscripting a string literal:

```py
s = 'abcde'

a = s[0]
b = s[1]
c = s[-1]
d = s[-2]
e = s[8]        # error: [index-out-of-bounds] "Index 8 is out of bounds for string `Literal["abcde"]` with length 5"
f = s[-8]       # error: [index-out-of-bounds] "Index -8 is out of bounds for string `Literal["abcde"]` with length 5"

reveal_type(a)  # revealed: Literal["a"]
reveal_type(b)  # revealed: Literal["b"]
reveal_type(c)  # revealed: Literal["e"]
reveal_type(d)  # revealed: Literal["d"]
reveal_type(e)  # revealed: Unknown
reveal_type(f)  # revealed: Unknown
```

### Function return

We can infer the type when using a function call for string subscripting:

```py
def add(x: int, y: int) -> int:
    return x + y

a = 'abcde'[add(0, 1)]
reveal_type(a)  # revealed: str
```

## Bytes

We can infer the type of bytes literals and their concatenations:

```py
w = b'red' b'knot'
x = b'hello'
y = b'world' + b'!'
z = b'\xff\x00'

reveal_type(w)  # revealed: Literal[b"redknot"]
reveal_type(x)  # revealed: Literal[b"hello"]
reveal_type(y)  # revealed: Literal[b"world!"]
reveal_type(z)  # revealed: Literal[b"\xff\x00"]
```
