x = set(x for x in range(3))
x = set(
    x for x in range(3)
)


def set(*args, **kwargs):
    return None


set(x for x in range(3))
