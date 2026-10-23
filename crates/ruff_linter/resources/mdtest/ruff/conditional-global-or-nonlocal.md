# `conditional-global-or-nonlocal`

```toml
lint.preview = true
lint.select = ["conditional-global-or-nonlocal"]
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
error[conditional-global-or-nonlocal]: `counter` is declared `global` in a block that may not run on every path, but the declaration applies to the entire function
 --> src/mdtest_snippet.py:6:16
  |
6 |         global counter  # snapshot: conditional-global-or-nonlocal
  |                ^^^^^^^
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

Only the name used on another path is flagged, and a use can be a read or any binding form (here, a
walrus):

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

Declaring the name in every branch does not help: each declaration still applies to the whole
function, and the first one is reported.

```py
counter = 0


def update(flag):
    if flag:
        global counter  # error: [conditional-global-or-nonlocal]
        counter = 1
    else:
        global counter
        counter = 2
```

## Used from a nested scope

A nested function, lambda, or comprehension that reads the name is affected by the declaration too:
without it, `inner` below would fail with a `NameError` when `flag` is false, because `counter` would
be an unbound local of `update`. With it, `inner` reads the global. So the use is flagged.

```py
counter = 0


def update(flag):
    if flag:
        global counter  # error: [conditional-global-or-nonlocal]
        counter = 1

    def inner():
        return counter

    return inner
```

```py
counter = 0


def update(flag):
    if flag:
        global counter  # error: [conditional-global-or-nonlocal]
        counter = 1
    else:
        print([counter for _ in range(2)])
```

A nested scope defined inside the same block is on the same path, so it is not flagged:

```py
counter = 0


def update(flag):
    if flag:
        global counter  # no diagnostic
        counter = 1

        def inner():
            return counter

        return inner
```

## Not flagged

Declared at the top of the function (its scope is already obvious):

```py
counter = 0


def update(flag):
    global counter  # no diagnostic
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
        global counter  # no diagnostic
        counter = 1
```

Declared and used only within the same loop body:

```py
counter = 0


def update():
    for _ in range(3):
        global counter  # no diagnostic
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
        global counter  # no diagnostic
        counter = 1
```

A `with` body always runs, so a declaration inside it is not conditional, even when the name is used
after the `with`:

```py
import threading

_lock = threading.Lock()
_instance = None


def get_instance():
    with _lock:
        global _instance  # no diagnostic
        _instance = object()
    return _instance
```
