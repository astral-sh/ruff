# Classes shadowing

## Implicit error

```py
class C: pass
C = 1 # error: "Implicit shadowing of class `C`; annotate to make it explicit if this is intentional"
```

## Explicit

No diagnostic is raised in the case of explicit shadowing:

```py
class C: pass
C: int = 1
```
