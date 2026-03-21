async def f(): return [[x async for x in foo(n)] for n in range(3)]

async def test(): return [[x async for x in elements(n)] async for n in range(3)]

async def f(): [x for x in foo()] and [x async for x in foo()]

async def f():
    def g(): ...
    [x async for x in foo()]
		
[x async for x in y]

