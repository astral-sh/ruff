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

    x, y = 0, 2000
    trio.sleep(x)  # TRIO115
    trio.sleep(y)  # OK

    (a, b, [c, (d, e)]) = (1, 2, (0, [4, 0]))
    trio.sleep(c)  # TRIO115
    trio.sleep(d)  # OK
    trio.sleep(e)  # TRIO115

    m_x, m_y = 0
    trio.sleep(m_y)  # TRIO115
    trio.sleep(m_x)  # TRIO115

    m_a = m_b = 0
    trio.sleep(m_a)  # TRIO115
    trio.sleep(m_b)  # TRIO115


def func():
    trio.run(trio.sleep(0))  # TRIO115


from trio import Event, sleep


def func():
    sleep(0)  # TRIO115
