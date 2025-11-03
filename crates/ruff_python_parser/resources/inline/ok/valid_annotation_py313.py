# parse_options: {"target-version": "3.13"}
a: (x := 1)
def outer():
    b: (yield 1)
    c: (yield from 1)
async def outer():
    d: (await 1)
