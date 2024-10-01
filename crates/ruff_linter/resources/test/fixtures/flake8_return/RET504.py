###
# Errors
###
def x():
    a = 1
    return a  # RET504


# Can be refactored false positives
# https://github.com/afonasev/flake8-return/issues/47#issuecomment-1122571066
def get_bar_if_exists(obj):
    result = ""
    if hasattr(obj, "bar"):
        result = str(obj.bar)
    return result


# https://github.com/afonasev/flake8-return/issues/47#issue-641117366
def x():
    formatted = _USER_AGENT_FORMATTER.format(format_string, **values)
    # clean up after any blank components
    formatted = formatted.replace("()", "").replace("  ", " ").strip()
    return formatted


# https://github.com/afonasev/flake8-return/issues/47#issue-641117366
def user_agent_username(username=None):
    if not username:
        return ""

    username = username.replace(" ", "_")  # Avoid spaces or %20.
    try:
        username.encode("ascii")  # just test, but not actually use it
    except UnicodeEncodeError:
        username = quote(username.encode("utf-8"))
    else:
        # % is legal in the default $wgLegalTitleChars
        # This is so that ops know the real pywikibot will not
        # allow a useragent in the username to allow through a hand-coded
        # percent-encoded value.
        if "%" in username:
            username = quote(username)
    return username


###
# Non-errors
###
def x(y):
    a = 1
    print(a)
    return a


def x():
    a = 1
    if y:
        return a
    a = a + 2
    print(a)
    return a


def x():
    a = {}
    a["b"] = 2
    return a


def x():
    a = []
    a.append(2)
    return a


def x():
    a = lambda x: x
    a()
    return a


# Ignore unpacking
def x():
    b, a = [1, 2]
    return a


def x():
    val = ""
    for i in range(5):
        val = val + str(i)
    return val


def x():
    val = ""
    i = 5
    while i:
        val = val + str(i)
        i = i - x
    return val


def x():
    a = 1
    print(f"a={a}")
    return a


# Considered OK, since functions can have side effects.
def x():
    a = 1
    b = 2
    print(b)
    return a


# Considered OK, since functions can have side effects.
def x():
    a = 1
    print()
    return a


# Considered OK, since attribute assignments can have side effects.
class X:
    def x(self):
        a = self.property
        self.property = None
        return a


# Test cases for using value for assignment then returning it
# See:https://github.com/afonasev/flake8-return/issues/47
def resolve_from_url(self, url: str) -> dict:
    local_match = self.local_scope_re.match(url)
    if local_match:
        schema = get_schema(name=local_match.group(1))
        self.store[url] = schema
        return schema
    raise NotImplementedError(...)


my_dict = {}


def my_func():
    foo = calculate_foo()
    my_dict["foo_result"] = foo
    return foo


# https://github.com/afonasev/flake8-return/issues/116#issue-1367575481
def no_exception_loop():
    success = False
    for _ in range(10):
        try:
            success = True
        except Exception:
            print("exception")
    return success


# https://github.com/afonasev/flake8-return/issues/116#issue-1367575481
def no_exception():
    success = False
    try:
        success = True
    except Exception:
        print("exception")
    return success


# https://github.com/afonasev/flake8-return/issues/116#issue-1367575481
def exception():
    success = True
    try:
        print("raising")
        raise Exception
    except Exception:
        success = False
    return success


# https://github.com/afonasev/flake8-return/issues/66
def close(self):
    any_failed = False
    for task in self.tasks:
        try:
            task()
        except BaseException:
            any_failed = True
            report(traceback.format_exc())
    return any_failed


def global_assignment():
    global X
    X = 1
    return X


def nonlocal_assignment():
    X = 1

    def inner():
        nonlocal X
        X = 1
        return X


def decorator() -> Flask:
    app = Flask(__name__)

    @app.route("/hello")
    def hello() -> str:
        """Hello endpoint."""
        return "Hello, World!"

    return app


def default():
    y = 1

    def f(x=y) -> X:
        return x

    return y


# Multiple assignment
def get_queryset(option_1, option_2):
    queryset: Any = None
    queryset = queryset.filter(a=1)
    if option_1:
        queryset = queryset.annotate(b=Value(2))
    if option_2:
        queryset = queryset.filter(c=3)
    return queryset


def get_queryset():
    queryset = Model.filter(a=1)
    queryset = queryset.filter(c=3)
    return queryset


def get_queryset():
    queryset = Model.filter(a=1)
    return queryset  # RET504


# Function arguments
def str_to_bool(val):
    if isinstance(val, bool):
        return val
    val = val.strip().lower()
    if val in ("1", "true", "yes"):
        return True

    return False


def str_to_bool(val):
    if isinstance(val, bool):
        return val
    val = 1
    return val  # RET504


def str_to_bool(val):
    if isinstance(val, bool):
        return some_obj
    return val


# Mixed assignments
def function_assignment(x):
    def f():
        ...

    return f


def class_assignment(x):
    class Foo:
        ...

    return Foo


def mixed_function_assignment(x):
    if x:

        def f():
            ...

    else:
        f = 42

    return f


def mixed_class_assignment(x):
    if x:

        class Foo:
            ...

    else:
        Foo = 42

    return Foo


# `with` statements
def foo():
    with open("foo.txt", "r") as f:
        x = f.read()
    return x  # RET504


def foo():
    with open("foo.txt", "r") as f:
        x = f.read()
        print(x)
    return x


def foo():
    with open("foo.txt", "r") as f:
        x = f.read()
    print(x)
    return x


# Fix cases
def foo():
    a = 1
    b=a
    return b  # RET504


def foo():
    a = 1
    b =a
    return b  # RET504


def foo():
    a = 1
    b= a
    return b  # RET504


def foo():
    a = 1  # Comment
    return a


# Regression test for: https://github.com/astral-sh/ruff/issues/7098
def mavko_debari(P_kbar):
    D=0.4853881 + 3.6006116*P - 0.0117368*(P-1.3822)**2
    return D


# contextlib suppress in with statement
import contextlib


def foo():
    x = 2
    with contextlib.suppress(Exception):
        x = x + 1
    return x


def foo(data):
    with open("in.txt") as file_out, contextlib.suppress(IOError):
        file_out.write(data)
        data = 10
    return data


def foo(data):
    with open("in.txt") as file_out:
        file_out.write(data)
        with contextlib.suppress(IOError):
            data = 10
    return data


def foo():
    y = 1
    x = 2
    with contextlib.suppress(Exception):
        x = 1
    y = y + 2
    return y  # RET504


def foo():
    y = 1
    if y > 0:
        with contextlib.suppress(Exception):
            y = 2
        return y


# See: https://github.com/astral-sh/ruff/issues/10732
def func(a: dict[str, int]) -> list[dict[str, int]]:
    services: list[dict[str, int]]
    if "services" in a:
        services = a["services"]
        return services


# See: https://github.com/astral-sh/ruff/issues/10732
def func(a: dict[str, int]) -> list[dict[str, int]]:
    if "services" in a:
        services = a["services"]
        return services
