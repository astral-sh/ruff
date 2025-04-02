# parse_options: {"target-version": "3.10"}
# this case fails if exit_expr doesn't run
async def f():
    [_ for n in range(3)]
    [_ async for n in range(3)]
# and this fails without exit_stmt
async def f():
    def g(): ...
    [_ async for n in range(3)]
