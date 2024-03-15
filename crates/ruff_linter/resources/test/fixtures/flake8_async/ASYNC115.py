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
    trio.sleep(foo)  # ASYNC115
    trio.sleep(1)  # OK
    time.sleep(0)  # OK

    sleep(0)  # ASYNC115

    bar = "bar"
    trio.sleep(bar)

    x, y = 0, 2000
    trio.sleep(x)  # ASYNC115
    trio.sleep(y)  # OK

    (a, b, [c, (d, e)]) = (1, 2, (0, [4, 0]))
    trio.sleep(c)  # ASYNC115
    trio.sleep(d)  # OK
    trio.sleep(e)  # ASYNC115

    m_x, m_y = 0
    trio.sleep(m_y)  # OK
    trio.sleep(m_x)  # OK

    m_a = m_b = 0
    trio.sleep(m_a)  # ASYNC115
    trio.sleep(m_b)  # ASYNC115

    m_c = (m_d, m_e) = (0, 0)
    trio.sleep(m_c)  # OK
    trio.sleep(m_d)  # ASYNC115
    trio.sleep(m_e)  # ASYNC115


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
        trio.sleep(walrus)  # ASYNC115
