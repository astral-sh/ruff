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
