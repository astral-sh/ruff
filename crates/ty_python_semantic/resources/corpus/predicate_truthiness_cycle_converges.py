# The loop rebinds the predicate to a class whose base depends on that same predicate.
predicate = lambda: predicate
value = []

class C[T]([0 for _ in () if predicate]):
    pass

while True:
    if (C if value else predicate):
        predicate = value = C
    with value as C:
        pass
