import pytest


def test_ok():
    with pytest.raises(AttributeError):
        [].size


async def test_ok_trivial_with():
    with pytest.raises(AttributeError):
        with context_manager_under_test():
            pass

    with pytest.raises(AttributeError):
        async with context_manager_under_test():
            pass


def test_ok_complex_single_call():
    with pytest.raises(AttributeError):
        my_func(
            [].size,
            [].size,
        )


def test_error_multiple_statements():
    with pytest.raises(AttributeError):
        len([])
        [].size


async def test_error_complex_statement():
    with pytest.raises(AttributeError):
        if True:
            [].size

    with pytest.raises(AttributeError):
        for i in []:
            [].size

    with pytest.raises(AttributeError):
        async for i in []:
            [].size

    with pytest.raises(AttributeError):
        while True:
            [].size

    with pytest.raises(AttributeError):
        with context_manager_under_test():
            [].size

    with pytest.raises(AttributeError):
        async with context_manager_under_test():
            [].size


def test_error_try():
    with pytest.raises(AttributeError):
        try:
            [].size
        except:
            raise
