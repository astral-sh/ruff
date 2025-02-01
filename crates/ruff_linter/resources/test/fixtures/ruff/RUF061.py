### Errors

def exhaustiveness():
    with open() as f:
        ...

    f.__iter__()
    f.__next__()
    f.detach()
    f.fileno()
    f.flush()
    f.isatty()
    f.read()
    f.readline()
    f.readlines()
    f.reconfigure()
    f.seek()
    f.seekable()
    f.tell()
    f.truncate()
    f.writable()
    f.write()
    f.writelines()

def contains():
    with open() as f:
        ...

    _ = '' in f
    _ = '' not in f
    _ = '' in f is {}
    _ = '' not in f == {}


def for_loop():
    with open() as f:
        ...

    for _ in f: ...


def mode_is_unimportant():
    with open("", "r") as f:
        ...

    f.write()


def _():
    with open() as f:
        ...

    _ = f.name
    f.readlines()


### No errors

def non_operations():
    with open() as f:
        ...

    _ = f.name
    _ = f.line_buffering()


def compare_but_not_contains():
    with open() as f:
        ...

    _ = a != f
    _ = '' is not f not in {}


def for_loop_wrapped():
    with open() as f:
        ...

    for _ in foo(f): ...


def aliasing():
    with open() as f:
        ...

    g = f
    g.readlines()


def multiple():
    with open() as f:
        f.read()

    with open() as f:
        f.write()

    with open() as f:
        f.seek()
