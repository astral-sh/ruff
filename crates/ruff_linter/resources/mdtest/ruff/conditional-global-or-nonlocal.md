# `conditional-global-or-nonlocal` (`RUF077`)

```toml
lint.preview = true
lint.select = ["RUF077"]
```

## Declared in a skippable block, used on another path

A `global`/`nonlocal` declaration applies to the whole function, so placing it in a block that may be
skipped while the name is used on a path that does not pass through it is misleading.

```py
counter = 0


def update(flag):
    if flag:
        global counter  # snapshot: conditional-global-or-nonlocal
        counter = 1
    else:
        counter = 2
```

```snapshot
error[RUF077]: `counter` is declared `global` in a block that may not run on every path, but the declaration applies to the entire function
 --> src/mdtest_snippet.py:6:16
  |
6 |         global counter  # snapshot: conditional-global-or-nonlocal
  |                ^^^^^^^
  |
```

`nonlocal` behaves the same way:

```py
def outer():
    total = 0

    def inner(flag):
        if flag:
            nonlocal total  # error: [conditional-global-or-nonlocal]
            total = 1
        else:
            total = 2
```

It is also flagged across `try`/`except` and `match`/`case`:

```py
value = 0


def load(flag):
    try:
        global value  # error: [conditional-global-or-nonlocal]
        value = 1
    except ValueError:
        value = 2


def dispatch(x):
    match x:
        case 1:
            global value  # error: [conditional-global-or-nonlocal]
            value = 1
        case _:
            value = 2
```

Loops count too, since the body may execute zero times. Here the declaration is in the loop body but
the name is used after the loop:

```py
total = 0


def run():
    for _ in range(3):
        global total  # error: [conditional-global-or-nonlocal]
        total = 1
    total = 2


def wait(cond):
    while cond:
        global total  # error: [conditional-global-or-nonlocal]
        total = 1
    total = 2
```

The declaration takes effect even when the loop never iterates, so it is still flagged:

```py
total = 0


def run_empty():
    for _ in range(0):
        global total  # error: [conditional-global-or-nonlocal]
    total = 1
```

Only the name used on another path is flagged, and a use can be a read or any
binding form (here, a walrus):

```py
a = b = 0


def f(flag):
    if flag:
        global a, b  # error: [conditional-global-or-nonlocal]
        a = 1
        b = 1
    else:
        print(a := 2)
```

## Not flagged

Declared at the top of the function (its scope is already obvious):

```py
counter = 0


def update(flag):
    global counter
    if flag:
        counter = 1
    else:
        counter = 2
```

Declared and used only within the same branch:

```py
counter = 0


def update(flag):
    if flag:
        global counter
        counter = 1
```

Declared and used only within the same loop body:

```py
counter = 0


def update():
    for _ in range(3):
        global counter
        counter = 1
```

Declared and used only within a loop's `else` clause, which runs after the loop regardless of how
many times the body iterated:

```py
counter = 0


def update():
    for _ in range(3):
        pass
    else:
        global counter
        counter = 1
```

A use in a nested scope is not on a path of this function, so it is not flagged:

```py
counter = 0


def update(flag):
    if flag:
        global counter
        counter = 1

    def inner():
        return counter
```
