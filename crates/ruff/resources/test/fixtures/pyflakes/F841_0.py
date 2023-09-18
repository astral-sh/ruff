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

    with connect() as (connection, cursor):
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


def f():
    exponential, base_multiplier = 1, 2
    hash_map = {
        (exponential := (exponential * base_multiplier) % 3): i + 1 for i in range(2)
    }
    return hash_map


def f(x: int):
    msg1 = "Hello, world!"
    msg2 = "Hello, world!"
    msg3 = "Hello, world!"
    match x:
        case 1:
            print(msg1)
        case 2:
            print(msg2)


def f(x: int):
    import enum

    Foo = enum.Enum("Foo", "A B")
    Bar = enum.Enum("Bar", "A B")
    Baz = enum.Enum("Baz", "A B")

    match x:
        case (Foo.A):
            print("A")
        case [Bar.A, *_]:
            print("A")
        case y:
            pass


def f():
    if any((key := (value := x)) for x in ["ok"]):
        print(key)


def f() -> None:
    is_connected = False

    class Foo:
        @property
        def is_connected(self):
            nonlocal is_connected
            return is_connected

        def do_thing(self):
            # This should resolve to the `is_connected` in the function scope.
            nonlocal is_connected
            print(is_connected)

    obj = Foo()
    obj.do_thing()


def f():
    try:
        pass
    except Exception as _:
        pass
