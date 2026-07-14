import dishka
from dishka import provide
from dishka import Scope


@provide(scope=Scope.REQUEST)
async def safe_dishka_provide_call():
    with open(""):
        yield  # OK


@dishka.provide
async def safe_dishka_provide_reference():
    with open(""):
        yield  # OK


@some_other_decorator
async def unsafe_other_decorator():
    with open(""):
        yield  # ASYNC119
