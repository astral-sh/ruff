# Unary not

## None

```py
reveal_type(not None)  # revealed: Literal[True]
reveal_type(not not None)  # revealed: Literal[False]
```

## Function

```py
from typing import reveal_type

def f():
    return 1

reveal_type(not f)  # revealed: Literal[False]
# TODO Unknown should not be part of the type of typing.reveal_type
# reveal_type(not reveal_type)  revealed: Literal[False]
```

## Module

```py
import b
import warnings

reveal_type(not b)  # revealed: Literal[False]
reveal_type(not warnings)  # revealed: Literal[False]
```

```py path=b.py
y = 1
```

## Union

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

reveal_type(not p)  # revealed: Literal[False]
reveal_type(not q)  # revealed: bool
reveal_type(not r)  # revealed: bool
reveal_type(not s)  # revealed: bool
reveal_type(not t)  # revealed: Literal[True]
```

## Integer literal

```py
reveal_type(not 1)  # revealed: Literal[False]
reveal_type(not 1234567890987654321)  # revealed: Literal[False]
reveal_type(not 0)  # revealed: Literal[True]
reveal_type(not -1)  # revealed: Literal[False]
reveal_type(not -1234567890987654321)  # revealed: Literal[False]
reveal_type(not --987)  # revealed: Literal[False]
```

## Boolean literal

```py
w = True
reveal_type(w)  # revealed: Literal[True]

x = False
reveal_type(x)  # revealed: Literal[False]

reveal_type(not w)  # revealed: Literal[False]

reveal_type(not x)  # revealed: Literal[True]
```

## String literal

```py
reveal_type(not "hello")  # revealed: Literal[False]
reveal_type(not "")  # revealed: Literal[True]
reveal_type(not "0")  # revealed: Literal[False]
reveal_type(not "hello" + "world")  # revealed: Literal[False]
```

## Bytes literal

```py
reveal_type(not b"hello")  # revealed: Literal[False]
reveal_type(not b"")  # revealed: Literal[True]
reveal_type(not b"0")  # revealed: Literal[False]
reveal_type(not b"hello" + b"world")  # revealed: Literal[False]
```

## Tuple

```py
reveal_type(not (1,))  # revealed: Literal[False]
reveal_type(not (1, 2))  # revealed: Literal[False]
reveal_type(not (1, 2, 3))  # revealed: Literal[False]
reveal_type(not ())  # revealed: Literal[True]
reveal_type(not ("hello",))  # revealed: Literal[False]
reveal_type(not (1, "hello"))  # revealed: Literal[False]
```
