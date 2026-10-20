# Lambda bodies must retain their inputs when collection inference reads a binding
# that the surrounding unpacking assignment is still computing.
node = lambda: node
for [node] in {
    **{0: 0 for _ in [] if node},
    (lambda values: {key: 0 for key in values}): 0,
    **0,
}:
    pass


async def check():
    node = lambda: node
    for [node] in {
        **{0: 0 for _ in [] if node},
        (lambda values: {key: 0 for key in values}): 0,
        **0,
    }:
        pass
