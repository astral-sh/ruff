import sys

###
# Outdated checks outside of an `if`/`elif` test.
# https://github.com/astral-sh/ruff/issues/16487
###

PY37 = sys.version_info < (3, 8)

old_pypy = hasattr(sys, "pypy_version_info") and sys.version_info < (3, 8)

print("py3" if sys.version_info >= (3, 8) else "py2")

print(sys.version_info >= (3, 8))

assert sys.version_info >= (3, 8)

while sys.version_info < (3, 8):
    print("py2")

legacy = not sys.version_info >= (3, 8)

supported = [feature for feature in features if sys.version_info >= (3, 8)]


def f(legacy=sys.version_info < (3, 8)):
    return sys.version_info < (3, 8)


class C:
    LEGACY = sys.version_info < (3, 8)


###
# Outdated checks that are only part of an `if`/`elif` test.
# https://github.com/astral-sh/ruff/issues/12093
###

if True and sys.version_info < (3, 5):
    print("35")

if sys.version_info < (3, 5) or sys.version_info > (3, 10):
    print("both operands are outdated")

if not sys.version_info < (3, 8):
    print("py3")

if foo:
    print(1)
elif bar and sys.version_info < (3, 8):
    print(2)

# Every link of the chain is a version check, so the whole branch can go.
if (3, 8) <= sys.version_info < (3, 10):
    print("3.8 or 3.9")

# Outside of a branch test there is nothing to remove, so each link is reported on its own.
is_38_or_39 = (3, 8) <= sys.version_info < (3, 10)

# Only the first link is outdated; the branch is still reachable, so it stays.
if (3, 8) <= sys.version_info < (3, 15):
    print("at most 3.14")

# `foo` is not a version check, so the branch is left alone even though the second link
# is always false.
if foo < sys.version_info < (3, 10):
    print("unreachable")

# The second link cannot be resolved, so the branch is left alone.
if (3, 8) <= sys.version_info <= (3, 13, foo):
    print("maybe")


###
# The version comes first, so the operator has to be mirrored.
###

if (3, 0) > sys.version_info:
    print("py2")

is_py3 = (3, 0) < sys.version_info

if 3 == sys.version_info[0]:
    print("py3")

while (3, 8) > sys.version_info:
    print("py2")


###
# Invalid version specifiers are reported wherever they appear.
###

unsupported = sys.version_info < (3, 10000000)

if foo or sys.version_info == 10000000:
    print("py3")

nonsense = 10000000 == sys.version_info[0]

# Identity is symmetric, so both operand orders are reported.
mistake = sys.version_info[0] is 10000000

also_mistake = 10000000 is sys.version_info[0]


###
# `sys.version_info.major` is handled the same way.
###

major_is_py3 = sys.version_info.major >= 3

if foo and sys.version_info.major < 3:
    print("py2")


###
# No errors: these are not outdated for the minimum supported version.
###

not_outdated = sys.version_info < (3, 15)

if foo and sys.version_info >= (3, 15):
    print("3.15+")

if (3, 15) <= sys.version_info:
    print("3.15+")

if (3, 15) <= sys.version_info < (3, 16):
    print("3.15")

# The version is not a literal, so nothing can be concluded.
minimum = sys.version_info < (3, foo)
