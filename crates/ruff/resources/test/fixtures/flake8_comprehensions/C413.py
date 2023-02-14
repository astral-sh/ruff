x = [2, 3, 1]
list(x)
list(sorted(x))
reversed(sorted(x))
reversed(sorted(x, key=lambda e: e))
reversed(sorted(x, reverse=True))
reversed(sorted(x, key=lambda e: e, reverse=True))
reversed(sorted(x, reverse=True, key=lambda e: e))
reversed(sorted(x, reverse=False))

def reversed(*args, **kwargs):
    return None


reversed(sorted(x, reverse=True))
