t = tuple()
l = list()
d1 = dict()
d2 = dict(a=1)
d3 = dict(**d2)


def list():
    return [1, 2, 3]


a = list()

f"{dict(x='y')}"
f'{dict(x="y")}'
f"{dict()}"
f"a {dict()} b"

f"{dict(x='y') | dict(y='z')}"
f"{ dict(x='y') | dict(y='z') }"
f"a {dict(x='y') | dict(y='z')} b"
f"a { dict(x='y') | dict(y='z') } b"

dict(
    # comment
)

tuple(  # comment
)

t"{dict(x='y')}"
t'{dict(x="y")}'
t"{dict()}"
t"a {dict()} b"

t"{dict(x='y') | dict(y='z')}"
t"{ dict(x='y') | dict(y='z') }"
t"a {dict(x='y') | dict(y='z')} b"
t"a { dict(x='y') | dict(y='z') } b"

# https://github.com/astral-sh/ruff/issues/16234
# Python normalizes identifiers to NFKC, but does not normalize string literals, so the fix has to
# normalize the keyword name to preserve the dictionary key at runtime. The character "ℼ" normalizes
# to "π", and "ſ" normalizes to "s".
dict(ℼ=3.14)
dict(ſ=1)
dict(𝕒=1, b=2)
dict(a=1, b=2)  # already NFKC-normalized: unchanged
