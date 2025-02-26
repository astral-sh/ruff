# Errors

_ = "0123456789"
_ = "01234567"
_ = "0123456789abcdefABCDEF"
_ = "abcdefghijklmnopqrstuvwxyz"
_ = "ABCDEFGHIJKLMNOPQRSTUVWXYZ"
_ = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ"
_ = r"""!"#$%&'()*+,-./:;<=>?@[\]^_`{|}~"""
_ = " \t\n\r\v\f"

_ = '0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ!"#$%&\'()*+,-./:;<=>?@[\\]^_`{|}~ \t\n\r\x0b\x0c'
_ = (
    '0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ!"#$%&'
    "'()*+,-./:;<=>?@[\\]^_`{|}~ \t\n\r\x0b\x0c"
)
_ = id("0123"
       "4567"
       "89")

_ = (
    "0123456789"
).capitalize()

_ = (
    "0123456789"
    # with comment
).capitalize()

# example with augmented assignment
_ += "0123456789"

# OK

_ = "1234567890"
_ = "1234"
_ = "12" in "12345670"


# No errors as the string is considered as a docstring
class C:
    "01234567"


class C:
    def method(self):
        "01234567"


def function():
    """01234567"""

