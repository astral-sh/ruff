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
# https://github.com/astral-sh/ruff/issues/19887
nok8 = '\n'.join([r'line1','line2'])
nok9 = '\n'.join([r"raw string", '<""">', "<'''>"])  # Not OK (both triple-quote delimiters appear; should bail)

# Regression test for: https://github.com/astral-sh/ruff/issues/7197
def create_file_public_url(url, filename):
    return''.join([url, filename])

# Regression test for: https://github.com/astral-sh/ruff/issues/19837
nok10 = "".join((foo, '"'))
nok11 = ''.join((foo, "'"))
nok12 = ''.join([foo, "'", '"'])
nok13 = "".join([foo, "'", '"'])

# Regression test for: https://github.com/astral-sh/ruff/issues/21082
# Mixing raw and non-raw strings can cause syntax errors or behavior changes
nok14 = "".join((r"", '"'))  # First is raw, second is not - would break syntax
nok15 = "".join((r"", "\\"))  # First is raw, second has backslash - would break syntax
nok16 = "".join((r"", "\0"))  # First is raw, second has null byte - would introduce null bytes
nok17 = "".join((r"", "\r"))  # First is raw, second has carriage return - would change behavior
nok18 = "".join((r"", "\\r"))  # First is raw, second has backslash followed by literal r - OK (no special handling needed)

# Test that all-raw strings still work (should be OK)
ok7 = "".join((r"", r"something"))  # Both are raw - OK
ok8 = "\n".join((r"line1", r'line2'))  # Both are raw - OK

# Test that all-non-raw strings still work (should be OK)
ok9 = "".join(("", '"'))  # Both are non-raw - OK
ok10 = "\n".join(("line1", "line2"))  # Both are non-raw - OK
