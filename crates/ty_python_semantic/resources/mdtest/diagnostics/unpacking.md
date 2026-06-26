# Unpacking

## Right hand side not iterable

```py
a, b = 1  # snapshot: not-iterable
```

```snapshot
error[not-iterable]: Object of type `Literal[1]` is not iterable
 --> src/mdtest_snippet.py:1:8
  |
1 | a, b = 1  # snapshot: not-iterable
  |        ^
  |
info: It doesn't have an `__iter__` method or a `__getitem__` method
```

## Exactly too many values to unpack

```py
a, b = (1, 2, 3)  # snapshot: invalid-assignment
```

```snapshot
error[invalid-assignment]: Too many values to unpack
 --> src/mdtest_snippet.py:1:1
  |
1 | a, b = (1, 2, 3)  # snapshot: invalid-assignment
  | ^^^^   --------- Got 3
  | |
  | Expected 2
  |
```

## Exactly too few values to unpack

```py
a, b = (1,)  # snapshot: invalid-assignment
```

```snapshot
error[invalid-assignment]: Not enough values to unpack
 --> src/mdtest_snippet.py:1:1
  |
1 | a, b = (1,)  # snapshot: invalid-assignment
  | ^^^^   ---- Got 1
  | |
  | Expected 2
  |
```

## Too few values to unpack

```py
[a, *b, c, d] = (1, 2)  # snapshot: invalid-assignment
```

```snapshot
error[invalid-assignment]: Not enough values to unpack
 --> src/mdtest_snippet.py:1:1
  |
1 | [a, *b, c, d] = (1, 2)  # snapshot: invalid-assignment
  | ^^^^^^^^^^^^^   ------ Got 2
  | |
  | Expected at least 3
  |
```
