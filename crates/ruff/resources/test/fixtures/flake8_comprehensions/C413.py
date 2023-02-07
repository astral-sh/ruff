x = [2, 3, 1]
list(x)
list(sorted(x))
reversed(sorted(x))
reversed(sorted(x, key=lambda e: e))
reversed(sorted(x, reverse=True))


def reversed(*args, **kwargs):
    return None


reversed(sorted(x, reverse=True))
