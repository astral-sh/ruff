import pytest


@pytest.mark.asyncio()
async def test_something():  # Ok not fixture
    pass


@pytest.mark.asyncio
async def test_something():  # Ok not fixture no parens
    pass


@pytest.mark.asyncio()
@pytest.fixture()
async def my_fixture():  # Error before
    return 0


@pytest.mark.asyncio
@pytest.fixture()
async def my_fixture():  # Error before no parens
    return 0


@pytest.fixture()
@pytest.mark.asyncio()
async def my_fixture():  # Error after
    return 0


@pytest.fixture()
@pytest.mark.asyncio
async def my_fixture():  # Error after no parens
    return 0
