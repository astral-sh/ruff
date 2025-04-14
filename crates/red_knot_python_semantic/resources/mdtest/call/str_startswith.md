# `str.startswith`

We special-case `str.startswith` to allow inference of precise Boolean literal types, because those
are used in [`sys.platform` checks].

```py
reveal_type("abc".startswith(""))  # revealed: Literal[True]
reveal_type("abc".startswith("a"))  # revealed: Literal[True]
reveal_type("abc".startswith("ab"))  # revealed: Literal[True]
reveal_type("abc".startswith("abc"))  # revealed: Literal[True]

reveal_type("abc".startswith("abcd"))  # revealed: Literal[False]
reveal_type("abc".startswith("bc"))  # revealed: Literal[False]

reveal_type("AbC".startswith(""))  # revealed: Literal[True]
reveal_type("AbC".startswith("A"))  # revealed: Literal[True]
reveal_type("AbC".startswith("Ab"))  # revealed: Literal[True]
reveal_type("AbC".startswith("AbC"))  # revealed: Literal[True]

reveal_type("AbC".startswith("a"))  # revealed: Literal[False]
reveal_type("AbC".startswith("aB"))  # revealed: Literal[False]

reveal_type("".startswith(""))  # revealed: Literal[True]

reveal_type("".startswith(" "))  # revealed: Literal[False]
```

Make sure that we fall back to `bool` for more complex cases:

```py
reveal_type("abc".startswith("b", 1))  # revealed: bool
reveal_type("abc".startswith("bc", 1, 3))  # revealed: bool

reveal_type("abc".startswith(("a", "x")))  # revealed: bool
```

And similiarly, we should still infer `bool` if the instance or the prefix are not string literals:

```py
from typing_extensions import LiteralString

def _(string_instance: str, literalstring: LiteralString):
    reveal_type(string_instance.startswith("a"))  # revealed: bool
    reveal_type(literalstring.startswith("a"))  # revealed: bool

    reveal_type("a".startswith(string_instance))  # revealed: bool
    reveal_type("a".startswith(literalstring))  # revealed: bool
```

[`sys.platform` checks]: https://docs.python.org/3/library/sys.html#sys.platform
