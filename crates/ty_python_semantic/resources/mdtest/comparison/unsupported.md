# Comparison: Unsupported operators

```py
from typing import Literal

def _(int_literal_or_str_literal: Literal[1, "foo"], flag1: bool, flag2: bool):
    class A: ...
    # snapshot
    a = 1 in 7
    reveal_type(a)  # revealed: bool
```

```snapshot
error[unsupported-operator]: Unsupported `in` operation
 --> src/mdtest_snippet.py:6:9
  |
6 |     a = 1 in 7
  |         -^^^^-
  |         |    |
  |         |    Has type `Literal[7]`
  |         Has type `Literal[1]`
  |
```

```py
    # snapshot
    b = 0 not in 10
    reveal_type(b)  # revealed: bool
```

```snapshot
error[unsupported-operator]: Unsupported `not in` operation
 --> src/mdtest_snippet.py:9:9
  |
9 |     b = 0 not in 10
  |         -^^^^^^^^--
  |         |        |
  |         |        Has type `Literal[10]`
  |         Has type `Literal[0]`
  |
```

```py
    # snapshot: unsupported-operator
    c = object() < 5
    reveal_type(c)  # revealed: Unknown
```

```snapshot
error[unsupported-operator]: Unsupported `<` operation
  --> src/mdtest_snippet.py:12:9
   |
12 |     c = object() < 5
   |         --------^^^-
   |         |          |
   |         |          Has type `Literal[5]`
   |         Has type `object`
   |
```

```py
    # snapshot: unsupported-operator
    d = 5 < object()
    reveal_type(d)  # revealed: Unknown
```

```snapshot
error[unsupported-operator]: Unsupported `<` operation
  --> src/mdtest_snippet.py:15:9
   |
15 |     d = 5 < object()
   |         -^^^--------
   |         |   |
   |         |   Has type `object`
   |         Has type `Literal[5]`
   |
```

```py
    # snapshot
    e = 42 in int_literal_or_str_literal
    reveal_type(e)  # revealed: bool
```

```snapshot
error[unsupported-operator]: Unsupported `in` operation
  --> src/mdtest_snippet.py:18:9
   |
18 |     e = 42 in int_literal_or_str_literal
   |         --^^^^--------------------------
   |         |     |
   |         |     Has type `Literal[1, "foo"]`
   |         Has type `Literal[42]`
   |
info: Operation fails because operator `in` is not supported between objects of type `Literal[42]` and `Literal[1]`
```

```py
    # snapshot: unsupported-operator
    f = (1, 2) < (1, "hello")
    reveal_type(f)  # revealed: Unknown
```

```snapshot
error[unsupported-operator]: Unsupported `<` operation
  --> src/mdtest_snippet.py:21:9
   |
21 |     f = (1, 2) < (1, "hello")
   |         ------^^^------------
   |         |        |
   |         |        Has type `tuple[Literal[1], Literal["hello"]]`
   |         Has type `tuple[Literal[1], Literal[2]]`
   |
info: Operation fails because operator `<` is not supported between the tuple elements at index 2 (of type `Literal[2]` and `Literal["hello"]`)
```

```py
    # snapshot: unsupported-operator
    g = (flag1, A()) < (flag2, A())
    reveal_type(g)  # revealed: Unknown
```

```snapshot
error[unsupported-operator]: Unsupported `<` operation
  --> src/mdtest_snippet.py:24:9
   |
24 |     g = (flag1, A()) < (flag2, A())
   |         ------------^^^------------
   |         |
   |         Both operands have type `tuple[bool, A]`
   |
info: Operation fails because operator `<` is not supported between the tuple elements at index 2 (both of type `A`)
```
