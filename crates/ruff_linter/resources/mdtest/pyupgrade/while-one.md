# `while-one` (`UP048`)

```toml
lint.preview = true
lint.select = ["UP048"]
```

## Basic replacement

`while 1:` is a Python 2 idiom for an infinite loop, from when `True` was a rebindable global rather
than a keyword.

```py
while 1:  # snapshot: while-one
    print("Hello, world!")
```

```snapshot
error[UP048]: Use `while True:` instead of `while 1:`
 --> src/mdtest_snippet.py:1:7
  |
1 | while 1:  # snapshot: while-one
  |       ^
help: Replace with `True`
  |
  - while 1:  # snapshot: while-one
1 + while True:  # snapshot: while-one
2 |     print("Hello, world!")
  |
```

## Other spellings of one

Any integer literal equal to one is flagged, whatever its base, and each is fixed to `True`.

```py
while 0x1:  # snapshot: while-one
    ...

while 0b1:  # error: [while-one]
    ...

while 0o1:  # error: [while-one]
    ...

while 1_0:  # ten, not one, so this is left alone
    ...
```

```snapshot
error[UP048]: Use `while True:` instead of `while 1:`
 --> src/mdtest_snippet.py:1:7
  |
1 | while 0x1:  # snapshot: while-one
  |       ^^^
help: Replace with `True`
  |
  - while 0x1:  # snapshot: while-one
1 + while True:  # snapshot: while-one
2 |     ...
  |
```

## Parentheses and comments are preserved

Only the literal itself is rewritten, so surrounding trivia survives the fix.

```py
while (
    # keep me
    1  # snapshot: while-one
):
    ...
```

```snapshot
error[UP048]: Use `while True:` instead of `while 1:`
 --> src/mdtest_snippet.py:3:5
  |
3 |     1  # snapshot: while-one
  |     ^
help: Replace with `True`
  |
2 |     # keep me
  -     1  # snapshot: while-one
3 +     True  # snapshot: while-one
4 | ):
  |
```

## Other conditions are left alone

`while 0:` is unreachable rather than infinite, and rewriting it would change behavior. Non-literal
conditions are out of scope even when they are always truthy, because flagging them would collide
with rules that catch accidentally-constant conditions.

```py
while 0:
    ...

while True:
    ...

while 1.0:
    ...

while "always":
    ...

while [1]:
    ...

while 2:
    ...

while -1:
    ...
```

The rule targets the loop condition only, not integer literals elsewhere in a `while` statement.

```py
x = 1
while x == 1:
    x = 1
```
