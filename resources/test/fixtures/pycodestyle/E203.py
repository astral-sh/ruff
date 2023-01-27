if x == 4: print(x, y); x, y = y, x
if x == 4: print(x, y); x, y = y , x  # E203
if x == 4: print(x, y) ; x, y = y, x  # E203
if x == 4 : print(x, y); x, y = y, x  # E203

( x, y, ) == (3 , 4 )  # E203

if x := 2 and y := 3:
    pass
