# Errors

sorted(l)[0]

sorted(l)[-1]

sorted(l, reverse=False)[-1]

sorted(l, key=lambda x: x)[0]

sorted(l, key=key_fn)[0]

sorted([1, 2, 3])[0]

# Unsafe

sorted(l, key=key_fn, reverse=True)[-1]

sorted(l, reverse=True)[0]

sorted(l, reverse=True)[-1]

# Non-errors

sorted(l, reverse=foo())[0]

sorted(l)[1]

sorted(get_list())[1]

sorted()[0]

sorted(l)[1]

sorted(l)[-2]

b = True

sorted(l, reverse=b)[0]

sorted(l, invalid_kwarg=True)[0]


def sorted():
    pass


sorted(l)[0]
