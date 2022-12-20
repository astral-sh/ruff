###
# Errors
###
def x():
    a = 1
    return a  # error


def x():
    a = 1
    print(a)
    a = 2
    return a  # error


def x():
    a = 1
    if True:
        return a  # error
    a = 2
    print(a)
    return a


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


# ignore unpacking
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
    b, a = 1, 2
    print(b)
    return a


# Considered OK, since functions can have side effects.
def x():
    a = 1
    print()
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


# Refactored incorrect false positives
# See test cases above marked: "Can be refactored false positives"
# https://github.com/afonasev/flake8-return/issues/47#issuecomment-1122571066
def get_bar_if_exists(obj):
    if hasattr(obj, "bar"):
        return str(obj.bar)
    return ""


# https://github.com/afonasev/flake8-return/issues/47#issue-641117366
def x():
    formatted = _USER_AGENT_FORMATTER.format(format_string, **values)
    # clean up after any blank components
    return formatted.replace("()", "").replace("  ", " ").strip()


# https://github.com/afonasev/flake8-return/issues/47#issue-641117366
def user_agent_username(username=None):

    if not username:
        return ""

    username = username.replace(" ", "_")  # Avoid spaces or %20.
    try:
        username.encode("ascii")  # just test,
        # but not actually use it
    except UnicodeEncodeError:
        username = quote(username.encode("utf-8"))
    else:
        # % is legal in the default $wgLegalTitleChars
        # This is so that ops know the real pywikibot will not
        # allow a useragent in the username to allow through a
        # hand-coded percent-encoded value.
        if "%" in username:
            username = quote(username)
    return username


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
