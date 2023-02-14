import pytest


def test_xxx():
    pytest.fail("this is a failure")  # Test OK arg


def test_xxx():
    pytest.fail(msg="this is a failure")  # Test OK kwarg


def test_xxx():  # Error
    pytest.fail()
    pytest.fail("")
    pytest.fail(f"")
    pytest.fail(msg="")
    pytest.fail(msg=f"")
