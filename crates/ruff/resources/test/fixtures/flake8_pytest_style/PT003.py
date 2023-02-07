import pytest


@pytest.fixture()
def ok_no_scope():
    ...


@pytest.fixture(scope="module")
def ok_other_scope():
    ...


@pytest.fixture(scope="function")
def error():
    ...


@pytest.fixture(scope="function", name="my_fixture")
def error_multiple_args():
    ...


@pytest.fixture(name="my_fixture", scope="function")
def error_multiple_args():
    ...


@pytest.fixture(name="my_fixture", scope="function", **kwargs)
def error_second_arg():
    ...


# pytest.fixture does not take positional arguments, however this 
# tests the general case as we use a helper function that should
# work for all cases.
@pytest.fixture("my_fixture", scope="function")
def error_arg():
    ...


@pytest.fixture(
    scope="function",
    name="my_fixture",
)
def error_multiple_args():
    ...


@pytest.fixture(
    name="my_fixture",
    scope="function",
)
def error_multiple_args():
    ...


@pytest.fixture(
    "hello", 
    name, 
    *args
    , 

    # another comment ,)
    
    scope=\
        "function"  # some comment ),
    ,
    
    name2=name, name3="my_fixture", **kwargs
)
def error_multiple_args():
    ...
