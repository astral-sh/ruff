# parse_options: {"target-version": "3.10"}
async def test(): return [[x async for x in elements(n)] async for n in range(3)]
