# Shadowing

We currently show special diagnostic hints when a class or function is shadowed by a variable
assignment.

## Implicit class shadowing

```py
class C: ...

C = 1  # snapshot: invalid-assignment
```

```snapshot
error[invalid-assignment]: Object of type `Literal[1]` is not assignable to `<class 'C'>`
 --> src/mdtest_snippet.py:3:1
  |
3 | C = 1  # snapshot: invalid-assignment
  | -   ^ Incompatible value of type `Literal[1]`
  | |
  | Declared type `<class 'C'>`
info: Implicit shadowing of class `C`. Add an annotation to make it explicit if this is intentional
```

## Implicit function shadowing

```py
def f(): ...

f = 1  # snapshot: invalid-assignment
```

```snapshot
error[invalid-assignment]: Object of type `Literal[1]` is not assignable to `def f() -> Unknown`
 --> src/mdtest_snippet.py:3:1
  |
3 | f = 1  # snapshot: invalid-assignment
  | -   ^ Incompatible value of type `Literal[1]`
  | |
  | Declared type `def f() -> Unknown`
info: Implicit shadowing of function `f`. Add an annotation to make it explicit if this is intentional
```

## No implicit shadowing for attribute assignments

```py
from configparser import ConfigParser

config = ConfigParser()
config.optionxform = str  # snapshot: invalid-assignment
```

```snapshot
error[invalid-assignment]: Object of type `<class 'str'>` is not assignable to attribute `optionxform` of type `def optionxform(self, optionstr: str) -> str`
 --> src/mdtest_snippet.py:4:1
  |
4 | config.optionxform = str  # snapshot: invalid-assignment
  | ^^^^^^^^^^^^^^^^^^
```
