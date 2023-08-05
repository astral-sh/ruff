try:
    y = 6 + "7"
except TypeError:
    # RSE102
    raise ValueError()

try:
    x = 1 / 0
except ZeroDivisionError:
    raise

# RSE102
raise TypeError()

# RSE102
raise TypeError ()

# RSE102
raise TypeError \
    ()

# RSE102
raise TypeError(

)

# RSE102
raise TypeError(
    # Hello, world!
)

# OK
raise AssertionError

# OK
raise AttributeError("test message")


def return_error():
    return ValueError("Something")


# OK
raise return_error()


class Class:
    @staticmethod
    def error():
        return ValueError("Something")


# OK
raise Class.error()
