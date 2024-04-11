# Errors

sorted(l)[0]

sorted(l)[-1]

sorted(l, reverse=True)[0]

sorted(l, reverse=True)[-1]

sorted(l, reverse=False)[-1]

sorted(l, key=lambda x: x)[0]

sorted(l, key=key_fn, reverse=True)[-1]

sorted(l, key=key_fn)[0]

# Non-errors

sorted(l, reverse=foo())[0]

sorted([1, 2, 3])[0]

sorted(l)[1]

sorted(get_list())[1]
