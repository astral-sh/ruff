# parse_options: {"target-version": "3.11"}
async def f(): return [[x async for x in foo(n)] for n in range(3)]    # list
async def g(): return [{x: 1 async for x in foo(n)} for n in range(3)] # dict
async def h(): return [{x async for x in foo(n)} for n in range(3)]    # set
