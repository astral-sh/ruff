import functools

import pytest


@pytest.fixture()
def my_fixture():  # OK return
    return 0


@pytest.fixture()
def my_fixture():  # OK yield
    resource = acquire_resource()
    yield resource
    resource.release()


@pytest.fixture()
def my_fixture():  # OK other request
    request = get_request()
    request.addfinalizer(finalizer)
    return request


def create_resource(arg, request):  # OK other function
    resource = Resource(arg)
    request.addfinalizer(resource.release)
    return resource


@pytest.fixture()
def resource_factory(request):
    return functools.partial(create_resource, request=request)


@pytest.fixture()
def resource_factory(request):  # OK other function
    def create_resource(arg) -> Resource:
        resource = Resource(arg)
        request.addfinalizer(resource.release)
        return resource

    return create_resource


@pytest.fixture()
def my_fixture(request):  # Error return
    resource = acquire_resource()
    request.addfinalizer(resource.release)
    return resource


@pytest.fixture()
def my_fixture(request):  # Error yield
    resource = acquire_resource()
    request.addfinalizer(resource.release)
    yield resource
    resource  # prevent PT022
