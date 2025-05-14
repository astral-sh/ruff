d = {(1,2):"a",(3,4):"b",(5,6,7):"c",(8,):"d"}
d[(1,2)]
d[(
    1,
    2
)]
d[
    1,
    2
]
d[(2,4)]
d[(5,6,7)]
d[(8,)]
d[tuple(1,2)]
d[tuple(8)]
d[1,2]
d[3,4]
d[5,6,7]
e = {((1,2),(3,4)):"a"}
e[((1,2),(3,4))]
e[(1,2),(3,4)]

token_features[
    (window_position, feature_name)
] = self._extract_raw_features_from_token

d[1,]
d[(1,)]
d[()] # empty tuples should be ignored
d[:,] # slices in the subscript lead to syntax error if parens are added
d[1,2,:]

# Should keep these parentheses in
# Python <=3.10 to avoid syntax error.
# https://github.com/astral-sh/ruff/issues/12776
d[(*foo,bar)]

x: dict[str, int]  # tuples inside type annotations should never be altered

import typing

type Y = typing.Literal[1, 2]
Z: typing.TypeAlias = dict[int, int]
class Foo(dict[str, int]): pass

# Skip tuples of length one that are single-starred expressions
# https://github.com/astral-sh/ruff/issues/16077
d[*x]
