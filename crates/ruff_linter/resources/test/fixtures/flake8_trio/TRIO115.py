import trio
from trio import sleep


async def func():
    await trio.sleep(0)  # TRIO115
    await trio.sleep(1)  # OK
    await trio.sleep(0, 1)  # OK
    await trio.sleep(...)  # OK
    await trio.sleep()  # OK

    trio.sleep(0)  # TRIO115
    foo = 0
    trio.sleep(foo)  # TRIO115
    trio.sleep(1)  # OK
    time.sleep(0)  # OK

    sleep(0)  # TRIO115

    bar = "bar"
    trio.sleep(bar)


trio.sleep(0)  # TRIO115


def func():
    trio.run(trio.sleep(0))  # TRIO115
