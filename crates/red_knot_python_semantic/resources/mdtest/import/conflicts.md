# Conflicting attributes and submodules

## Via import

```py
import a.b

reveal_type(a.b)  # revealed: <module 'a.b'>
```

```py path=a/__init__.py
b = 42
```

```py path=a/b.py
```

## Via from/import

```py
from a import b

reveal_type(b)  # revealed: Literal[42]
```

```py path=a/__init__.py
b = 42
```

```py path=a/b.py
```

## Via both

```py
import a.b
from a import b

reveal_type(b)  # revealed: <module 'a.b'>
```

```py path=a/__init__.py
b = 42
```

```py path=a/b.py
```

## Via both (backwards)

```py
from a import b
import a.b

reveal_type(b)  # revealed: <module 'a.b'>
```

```py path=a/__init__.py
b = 42
```

```py path=a/b.py
```
