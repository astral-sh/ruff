# `global-or-nonlocal-in-branch` (`RUF076`)

```toml
lint.preview = true
lint.select = ["RUF076"]
```

## Declared in one branch, used in another

A `global`/`nonlocal` declaration applies to the whole function, so placing it in one branch while
the name is used in another is misleading.

```py
counter = 0


def update(flag):
    if flag:
        global counter  # snapshot: global-or-nonlocal-in-branch
        counter = 1
    else:
        counter = 2
```

```snapshot
error[RUF076]: `counter` is declared `global` in a branch but used in another branch of this function
 --> src/mdtest_snippet.py:6:16
  |
6 |         global counter  # snapshot: global-or-nonlocal-in-branch
  |                ^^^^^^^
  |
```

`nonlocal` behaves the same way:

```py
def outer():
    total = 0

    def inner(flag):
        if flag:
            nonlocal total  # error: [global-or-nonlocal-in-branch]
            total = 1
        else:
            total = 2
```

It is also flagged across `try`/`except` and `match`/`case`:

```py
value = 0


def load(flag):
    try:
        global value  # error: [global-or-nonlocal-in-branch]
        value = 1
    except ValueError:
        value = 2


def dispatch(x):
    match x:
        case 1:
            global value  # error: [global-or-nonlocal-in-branch]
            value = 1
        case _:
            value = 2
```

Only the name used in another branch is flagged, and a use can be a read or any
binding form (here, a walrus):

```py
a = b = 0


def f(flag):
    if flag:
        global a, b  # error: [global-or-nonlocal-in-branch]
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

A use in a nested scope is not a branch of this function, so it is not flagged:

```py
counter = 0


def update(flag):
    if flag:
        global counter
        counter = 1

    def inner():
        return counter
```

Loops are not modeled by the branch analysis, so a declaration in a `for` or `while`
body is not flagged (see the rule's known problems):

```py
counter = 0


def update():
    for _ in range(3):
        global counter
        counter = 1
    counter = 2
```
