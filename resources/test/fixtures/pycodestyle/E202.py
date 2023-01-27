spam(ham[1], {eggs: 2})
spam(ham[1], {eggs: 2} )  # E202
spam(ham[1 ], {eggs: 2})  # E202
spam(ham[1], {eggs: 2 })  # E202
spam( ham[1 ], {eggs: 2})  # E202

( x, y, ) == (3 , 4 )  # E202

if x := 2 and y := 3:
    pass
