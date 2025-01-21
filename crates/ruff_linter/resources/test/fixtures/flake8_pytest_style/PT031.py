import pytest


def test_ok():
    with pytest.warns(UserWarning):
        ...


async def test_ok_trivial_with():
    with pytest.warns(UserWarning):
        with context_manager_under_test():
            pass

    with pytest.warns(UserWarning):
        with context_manager_under_test():
            foo()

    with pytest.warns(UserWarning):
        async with context_manager_under_test():
            pass


def test_ok_complex_single_call():
    with pytest.warns(UserWarning):
        foo(
            "",
            0,
        )


def test_ok_func_and_class():
    with pytest.warns(UserWarning):
        class Foo:
            pass

    with pytest.warns(UserWarning):
        def foo():
            pass


def test_error_multiple_statements():
    with pytest.warns(UserWarning):
        foo()
        bar()


async def test_error_complex_statement():
    with pytest.warns(UserWarning):
        if True:
            foo()

    with pytest.warns(UserWarning):
        for i in []:
            foo()

    with pytest.warns(UserWarning):
        async for i in []:
            foo()

    with pytest.warns(UserWarning):
        while True:
            foo()

    with pytest.warns(UserWarning):
        async with context_manager_under_test():
            if True:
                foo()


def test_error_try():
    with pytest.warns(UserWarning):
        try:
            foo()
        except:
            raise


# https://github.com/astral-sh/ruff/issues/9730
def test_for_loops():

    ## Errors

    with pytest.warns(RuntimeError):
        for a in b:
            print()

    with pytest.warns(RuntimeError):
        for a in b:
            assert foo

    with pytest.warns(RuntimeError):
        async for a in b:
            print()

    with pytest.warns(RuntimeError):
        async for a in b:
            assert foo


    ## No errors in preview

    with pytest.warns(RuntimeError):
        for a in b:
            pass

    with pytest.warns(RuntimeError):
        for a in b:
            ...

    with pytest.warns(RuntimeError):
        async for a in b:
            pass

    with pytest.warns(RuntimeError):
        async for a in b:
            ...
