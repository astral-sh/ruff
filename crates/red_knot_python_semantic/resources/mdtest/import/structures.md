# Structures

## Class import following

```py
from b import C as D; E = D
reveal_type(E) # revealed: Literal[C]
```

```py path=b.py
class C: pass
```

## Module member resolution

```py
import b; D = b.C
reveal_type(D) # revealed: Literal[C]
```

```py path=b.py
class C: pass
```

## Importing builtin module

```py
import builtins; x = builtins.copyright
reveal_type(x) # revealed: Literal[copyright]
```
