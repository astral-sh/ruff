import pytest


@pytest.fixture()
def ok_complex_logic():
    if some_condition:
        resource = acquire_resource()
        yield resource
        resource.release()
        return
    yield None


@pytest.fixture()
def error():
    resource = acquire_resource()
    yield resource


import typing
from typing import Generator


@pytest.fixture()
def ok_complex_logic() -> typing.Generator[Resource, None, None]:
    if some_condition:
        resource = acquire_resource()
        yield resource
        resource.release()
        return
    yield None


@pytest.fixture()
def error() -> typing.Generator[typing.Any, None, None]:
    resource = acquire_resource()
    yield resource


@pytest.fixture()
def error() -> Generator[Resource, None, None]:
    resource = acquire_resource()
    yield resource
