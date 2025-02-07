# Unpacking

<!-- snapshot-diagnostics -->

## Right hand side not iterable

```py
a, b = 1  # error: [not-iterable]
```

## Too many values to unpack

```py
a, b = (1, 2, 3)  # error: [invalid-assignment]
```

## Too few values to unpack

```py
a, b = (1,)  # error: [invalid-assignment]
```
