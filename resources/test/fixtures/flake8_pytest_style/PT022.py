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
