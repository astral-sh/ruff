# A collection can refer to a lambda whose name is rebound by unpacking that collection.
node = lambda: node

for [node] in {
    **{0: 0 for _ in [] if node},
    (lambda values: {key: 0 for key in values}): 0,
    **0,
}:
    pass


# In an async function, condition diagnostics also inspect whether a callable returns an awaitable.
async def check():
    node = lambda: node

    for [node] in {
        **{0: 0 for _ in [] if node},
        (lambda values: {key: 0 for key in values}): 0,
        **0,
    }:
        pass
