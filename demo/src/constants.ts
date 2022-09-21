export const DEFAULT_SOURCE = `#: E721
if type(res) == type(42):
    pass
#: E721
if type(res) != type(""):
    pass
#: E721
import types

if res == types.IntType:
    pass
#: E721
import types

if type(res) is not types.ListType:
    pass
#: E721
assert type(res) == type(False) or type(res) == type(None)
#: E721
assert type(res) == type([])
#: E721
assert type(res) == type(())
#: E721
assert type(res) == type((0,))
#: E721
assert type(res) == type((0))
#: E721
assert type(res) != type((1,))
#: E721
assert type(res) is type((1,))
#: E721
assert type(res) is not type((1,))
#: E721
assert type(res) == type(
    [
        2,
    ]
)
#: E721
assert type(res) == type(())
#: E721
assert type(res) == type((0,))

#: Okay
import types

if isinstance(res, int):
    pass
if isinstance(res, str):
    pass
if isinstance(res, types.MethodType):
    pass
if type(a) != type(b) or type(a) == type(ccc):
    pass
`;
