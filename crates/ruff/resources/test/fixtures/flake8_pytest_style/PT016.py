import pytest


# OK
def f():
    pytest.fail("this is a failure")


def f():
    pytest.fail(msg="this is a failure")


def f():
    pytest.fail(reason="this is a failure")


# Errors
def f():
    pytest.fail()
    pytest.fail("")
    pytest.fail(f"")
    pytest.fail(msg="")
    pytest.fail(msg=f"")
    pytest.fail(reason="")
    pytest.fail(reason=f"")
