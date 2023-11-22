async def func():
    import trio
    from trio import sleep

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


def func():
    trio.run(trio.sleep(0))  # TRIO115


from trio import Event, sleep

def func():
    sleep(0)  # TRIO115
