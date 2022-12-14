try:
    1 / 0
except ValueError as e:
    pass


try:
    1 / 0
except ValueError as e:
    print(e)


def f():
    x = 1
    y = 2
    z = x + y


def f():
    foo = (1, 2)
    (a, b) = (1, 2)

    bar = (1, 2)
    (c, d) = bar

    (x, y) = baz = bar


def f():
    locals()
    x = 1


def f():
    _ = 1
    __ = 1
    _discarded = 1


a = 1


def f():
    global a

    # Used in `c` via `nonlocal`.
    b = 1

    def c():
        # F841
        b = 1

    def d():
        nonlocal b


def f():
    annotations = []
    assert len([annotations for annotations in annotations])


def f():
    def connect():
        return None, None

    with connect() as (connection, cursor):
        cursor.execute("SELECT * FROM users")


def f():
    def connect():
        return None, None

    with (connect() as (connection, cursor)):
        cursor.execute("SELECT * FROM users")


def f():
    with open("file") as my_file, open("") as ((this, that)):
        print("hello")


def f():
    with (
        open("file") as my_file,
        open("") as ((this, that)),
    ):
        print("hello")
