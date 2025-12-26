# Classes shadowing

## Implicit error

```py
class C: ...

C = 1  # error: "Implicit shadowing of class `C`"
```

## Explicit

No diagnostic is raised in the case of explicit shadowing:

```py
class C: ...

C: int = 1
```
