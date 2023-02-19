# These SHOULD be detected
try:
    ...
except CustomException:
    pass
except Exception:
    pass

try:
    ...
except Exception:
    ...

try:
    ...
except Exception:
    Ellipsis

try:
    ...
except (Exception, ValueError):
    Ellipsis

try:
    ...
except (ValueError, IndexError, Exception):
    Ellipsis


def somefunc():
    try:
        pass
    except MyException:
        pass  # This is specific, so it should be fine
    except Exception:
        pass  # This is broad, that's weird


def someotherfunc():
    try:
        pass
    except MyException:
        ...  # This is specific, so it should be fine
    except Exception:
        ...  # This is broad, that's weird


class SomeClass:
    def some_method(self):
        try:
            pass
        except (MyException, Exception):
            pass  # This is broad, that's weird


# These should NOT be detected
try:
    ...
except Exception:
    print("hello")

try:
    ...
except Exception:
    print("hello")
    pass

try:
    ...
except Exception:
    if "hello":
        pass
