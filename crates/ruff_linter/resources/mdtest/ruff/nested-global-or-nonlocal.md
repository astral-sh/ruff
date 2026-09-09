# `nested-global-or-nonlocal` (`RUF076`)

```toml
lint.preview = true
lint.select = ["RUF076"]
```

## Declared inside a block

A `global`/`nonlocal` declaration applies to the whole function, so nesting it inside a block makes
its function-wide scope easy to miss.

```py
counter = 0


def update(flag):
    if flag:
        global counter  # snapshot: nested-global-or-nonlocal
        counter = 1
    else:
        counter = 2
```

```snapshot
error[RUF076]: `counter` is declared `global` inside a nested block, but the declaration applies to the entire function
 --> src/mdtest_snippet.py:6:16
  |
6 |         global counter  # snapshot: nested-global-or-nonlocal
  |                ^^^^^^^
  |
```

`nonlocal` behaves the same way:

```py
def outer():
    total = 0

    def inner(flag):
        if flag:
            nonlocal total  # error: [nested-global-or-nonlocal]
            total = 1
        else:
            total = 2
```

It is flagged regardless of which block it is nested in, including `try`/`except`, `match`/`case`,
`with`, and loops:

```py
value = 0


def load(flag):
    try:
        global value  # error: [nested-global-or-nonlocal]
        value = 1
    except ValueError:
        value = 2


def dispatch(x):
    match x:
        case 1:
            global value  # error: [nested-global-or-nonlocal]
            value = 1
        case _:
            value = 2


def guarded(lock):
    with lock:
        global value  # error: [nested-global-or-nonlocal]
        value = 1


def looped():
    for _ in range(3):
        global value  # error: [nested-global-or-nonlocal]
        value = 1
    value = 2


def waited(cond):
    while cond:
        global value  # error: [nested-global-or-nonlocal]
        value = 1
```

A `global` inside a loop takes effect even when the loop never iterates, so it is still flagged:

```py
counter = 0


def update():
    for _ in range(0):
        global counter  # error: [nested-global-or-nonlocal]
    counter = 1
```

It is flagged even when the name is used only within the same block, since the placement is still
misleading:

```py
counter = 0


def update(flag):
    if flag:
        global counter  # error: [nested-global-or-nonlocal]
        counter = 1
```

Each name in a multi-name declaration is flagged:

```py
a = b = 0


def f(flag):
    if flag:
        # error: [nested-global-or-nonlocal]
        global a, b  # error: [nested-global-or-nonlocal]
        a = 1
        b = 1
```

## Not flagged

Declared at the top of the function, where its scope is obvious:

```py
counter = 0


def update(flag):
    global counter
    if flag:
        counter = 1
    else:
        counter = 2
```

`nonlocal` at the top of the function:

```py
def outer():
    total = 0

    def inner(flag):
        nonlocal total
        if flag:
            total = 1
        else:
            total = 2
```
