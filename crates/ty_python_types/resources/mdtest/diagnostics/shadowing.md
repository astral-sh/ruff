# Shadowing

<!-- snapshot-diagnostics -->

## Implicit class shadowing

```py
class C: ...

C = 1  # error: [invalid-assignment]
```

## Implicit function shadowing

```py
def f(): ...

f = 1  # error: [invalid-assignment]
```
