import trio
from trio import sleep


async def afoo():
    await trio.sleep(0)  # TRIO115
    await trio.sleep(1)  # Ok
    await trio.sleep(0, 1)  # Ok
    await trio.sleep(...)  # Ok
    await trio.sleep()  # Ok

    trio.sleep(0)  # TRIO115
    foo = 0
    trio.sleep(foo)  # TRIO115
    trio.sleep(1)  # OK
    time.sleep(0)  # OK

    sleep(0)  # TRIO115


trio.sleep(0)  # TRIO115


def foo():
    trio.run(trio.sleep(0))  # TRIO115

