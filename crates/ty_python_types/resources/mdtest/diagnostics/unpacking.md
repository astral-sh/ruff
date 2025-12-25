# Unpacking

<!-- snapshot-diagnostics -->

## Right hand side not iterable

```py
a, b = 1  # error: [not-iterable]
```

## Exactly too many values to unpack

```py
a, b = (1, 2, 3)  # error: [invalid-assignment]
```

## Exactly too few values to unpack

```py
a, b = (1,)  # error: [invalid-assignment]
```

## Too few values to unpack

```py
[a, *b, c, d] = (1, 2)  # error: [invalid-assignment]
```
