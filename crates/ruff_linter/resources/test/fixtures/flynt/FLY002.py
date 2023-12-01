import secrets
from random import random, choice

a = "Hello"
ok1 = " ".join([a, " World"])  # OK
ok2 = "".join(["Finally, ", a, " World"])  # OK
ok3 = "x".join(("1", "2", "3"))  # OK
ok4 = "y".join([1, 2, 3])  # Technically OK, though would've been an error originally
ok5 = "a".join([random(), random()])  # OK (simple calls)
ok6 = "a".join([secrets.token_urlsafe(), secrets.token_hex()])  # OK (attr calls)

nok1 = "x".join({"4", "5", "yee"})  # Not OK (set)
nok2 = a.join(["1", "2", "3"])  # Not OK (not a static joiner)
nok3 = "a".join(a)  # Not OK (not a static joinee)
nok4 = "a".join([a, a, *a])  # Not OK (not a static length)
nok5 = "a".join([choice("flarp")])  # Not OK (not a simple call)
nok6 = "a".join(x for x in "feefoofum")  # Not OK (generator)
nok7 = "a".join([f"foo{8}", "bar"])  # Not OK (contains an f-string)


# Regression test for: https://github.com/astral-sh/ruff/issues/7197
def create_file_public_url(url, filename):
    return''.join([url, filename])
