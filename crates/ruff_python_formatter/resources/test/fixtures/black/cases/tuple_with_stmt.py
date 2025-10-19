# don't remove the brackets here, it changes the meaning of the code.
# even though the code will always trigger a runtime error
with (name_5, name_4), name_5:
    pass


with c, (a, b):
    pass


with c, (a, b), d:
    pass


with c, (a, b, e, f, g), d:
    pass


def test_tuple_as_contextmanager():
    from contextlib import nullcontext

    try:
        with (nullcontext(), nullcontext()), nullcontext():
            pass
    except TypeError:
        # test passed
        pass
    else:
        # this should be a type error
        assert False
