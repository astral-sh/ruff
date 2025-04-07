# parse_options: {"target-version": "3.14"}
a: (x := 1)
def outer():
    b: (yield 1)
    c: (yield from 1)
async def outer():
    d: (await 1)
