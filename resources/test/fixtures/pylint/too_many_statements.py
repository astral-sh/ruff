def f():  # OK
    return


async def f():
    class A:
        def __init__(self):
            pass

    def n():
        print(1)
        return

    async for item in async_iterator:
        await awaitable(item)

    for i in range(10):
        a = 1
        continue

    while a == 1:
        b = 1
        break
    else:
        print(2)

    try:
        f()
    except Exception:
        async def c():
            pass  # 20
