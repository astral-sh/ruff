async def func():
    import trio
    from trio import sleep

    await trio.sleep(0)  # ASYNC115
    await trio.sleep(1)  # OK
    await trio.sleep(0, 1)  # OK
    await trio.sleep(...)  # OK
    await trio.sleep()  # OK

    trio.sleep(0)  # ASYNC115
    foo = 0
    trio.sleep(foo)  # OK
    trio.sleep(1)  # OK
    time.sleep(0)  # OK

    sleep(0)  # ASYNC115

    bar = "bar"
    trio.sleep(bar)

    x, y = 0, 2000
    trio.sleep(x)  # OK
    trio.sleep(y)  # OK

    (a, b, [c, (d, e)]) = (1, 2, (0, [4, 0]))
    trio.sleep(c)  # OK
    trio.sleep(d)  # OK
    trio.sleep(e)  # OK

    m_x, m_y = 0
    trio.sleep(m_y)  # OK
    trio.sleep(m_x)  # OK

    m_a = m_b = 0
    trio.sleep(m_a)  # OK
    trio.sleep(m_b)  # OK

    m_c = (m_d, m_e) = (0, 0)
    trio.sleep(m_c)  # OK
    trio.sleep(m_d)  # OK
    trio.sleep(m_e)  # OK


def func():
    import trio

    trio.run(trio.sleep(0))  # ASYNC115


from trio import Event, sleep


def func():
    sleep(0)  # ASYNC115


async def func():
    await sleep(seconds=0)  # ASYNC115


def func():
    import trio

    if (walrus := 0) == 0:
        trio.sleep(walrus)  # OK


def func():
    import trio

    async def main() -> None:
        sleep = 0
        for _ in range(2):
            await trio.sleep(sleep)  # OK
            sleep = 10

    trio.run(main)
