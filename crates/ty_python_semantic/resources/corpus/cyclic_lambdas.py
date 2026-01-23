# This test would previous panic with: `infer_definition_types(Id(1406)): execute: too many cycle iterations`.

lambda: name_4

@lambda: name_5
class name_1: ...

name_2 = [lambda: name_4, name_1]

if name_2:
    @(*name_2,)
    class name_3: ...
    assert unique_name_19

@lambda: name_3
class name_4[*name_2](0, name_1=name_3): ...

try:
    [name_5, name_4] = *name_4, = name_4
except* 0:
    ...
else:
    async def name_4(): ...

for name_3 in name_4: ...
