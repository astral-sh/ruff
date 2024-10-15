# Boolean literals

## Simple

```py
x = True
y = False
reveal_type(x)  # revealed: Literal[True]
reveal_type(y)  # revealed: Literal[False]
```

## Not operator

### None

```py
a = not None
b = not not None
reveal_type(a)  # revealed: Literal[True]
reveal_type(b)  # revealed: Literal[False]
```

### Function

```py
from typing import reveal_type

def f():
    return 1

a = not f
b = not reveal_type

reveal_type(a)  # revealed: Literal[False]
# TODO Unknown should not be part of the type of typing.reveal_type
# reveal_type(b)  revealed: Literal[False]
```

### Module

```py
import b; import warnings

x = not b
z = not warnings

reveal_type(x)  # revealed: Literal[False]
reveal_type(z)  # revealed: Literal[False]
```

```py path=b.py
y = 1
```

### Union

```py
if flag:
    p = 1
    q = 3.3
    r = "hello"
    s = "world"
    t = 0
else:
    p = "hello"
    q = 4
    r = ""
    s = 0
    t = ""

a = not p
b = not q
c = not r
d = not s
e = not t

reveal_type(a)  # revealed: Literal[False]
reveal_type(b)  # revealed: bool
reveal_type(c)  # revealed: bool
reveal_type(d)  # revealed: bool
reveal_type(e)  # revealed: Literal[True]
```

### Integer literal

```py
a = not 1
b = not 1234567890987654321
e = not 0
x = not -1
y = not -1234567890987654321
z = not --987

reveal_type(a)  # revealed: Literal[False]
reveal_type(b)  # revealed: Literal[False]
reveal_type(e)  # revealed: Literal[True]
reveal_type(x)  # revealed: Literal[False]
reveal_type(y)  # revealed: Literal[False]
reveal_type(z)  # revealed: Literal[False]
```

### Boolean literal

```py
w = True
x = False
y = not w
z = not x

reveal_type(w)  # revealed: Literal[True]
reveal_type(x)  # revealed: Literal[False]
reveal_type(y)  # revealed: Literal[False]
reveal_type(z)  # revealed: Literal[True]
```

### String literal

```py
a = not "hello"
b = not ""
c = not "0"
d = not "hello" + "world"

reveal_type(a)  # revealed: Literal[False]
reveal_type(b)  # revealed: Literal[True]
reveal_type(c)  # revealed: Literal[False]
reveal_type(d)  # revealed: Literal[False]
```

### Bytes literal

```py
a = not b"hello"
b = not b""
c = not b"0"
d = not b"hello" + b"world"

reveal_type(a)  # revealed: Literal[False]
reveal_type(b)  # revealed: Literal[True]
reveal_type(c)  # revealed: Literal[False]
reveal_type(d)  # revealed: Literal[False]
```

### Tuple

```py
a = not (1,)
b = not (1, 2)
c = not (1, 2, 3)
d = not ()
e = not ("hello",)
f = not (1, "hello")

reveal_type(a)  # revealed: Literal[False]
reveal_type(b)  # revealed: Literal[False]
reveal_type(c)  # revealed: Literal[False]
reveal_type(d)  # revealed: Literal[True]
reveal_type(e)  # revealed: Literal[False]
reveal_type(f)  # revealed: Literal[False]
```
