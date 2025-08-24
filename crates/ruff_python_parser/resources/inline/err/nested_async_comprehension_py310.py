# parse_options: {"target-version": "3.10"}
async def f(): return [[x async for x in foo(n)] for n in range(3)]    # list
async def g(): return [{x: 1 async for x in foo(n)} for n in range(3)] # dict
async def h(): return [{x async for x in foo(n)} for n in range(3)]    # set
async def i(): return [([y async for y in range(1)], [z for z in range(2)]) for x in range(5)]
async def j(): return [([y for y in range(1)], [z async for z in range(2)]) for x in range(5)]
