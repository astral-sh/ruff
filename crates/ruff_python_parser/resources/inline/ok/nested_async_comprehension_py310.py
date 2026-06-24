# parse_options: {"target-version": "3.10"}
async def f():
    [_ for n in range(3)]
    [_ async for n in range(3)]
async def f():
    def g(): ...
    [_ async for n in range(3)]
