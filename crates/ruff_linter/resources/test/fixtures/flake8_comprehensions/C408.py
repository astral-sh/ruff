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
