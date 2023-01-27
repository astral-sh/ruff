spam(ham[1], {eggs: 2})
spam( ham[1], {eggs: 2})  # E201
spam(ham[ 1], {eggs: 2})  # E201
spam(ham[1], { eggs: 2})  # E201
spam( ham[1 ], {eggs: 2})  # E201

( x, y, ) == (3 , 4 )  # E201

if x := 2 and y := 3:
    pass
