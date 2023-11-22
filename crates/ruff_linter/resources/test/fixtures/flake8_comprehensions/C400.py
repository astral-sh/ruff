x = list(x for x in range(3))
x = list(
    x for x in range(3)
)


def list(*args, **kwargs):
    return None


list(x for x in range(3))
