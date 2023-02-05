#: E201:1:6
spam( ham[1], {eggs: 2})
#: E201:1:10
spam(ham[ 1], {eggs: 2})
#: E201:1:15
spam(ham[1], { eggs: 2})
#: E201:1:6
spam(	ham[1], {eggs: 2})
#: E201:1:10
spam(ham[	1], {eggs: 2})
#: E201:1:15
spam(ham[1], {	eggs: 2})
#: Okay
spam(ham[1], {eggs: 2})
#:


#: E202:1:23
spam(ham[1], {eggs: 2} )
#: E202:1:22
spam(ham[1], {eggs: 2 })
#: E202:1:11
spam(ham[1 ], {eggs: 2})
#: E202:1:23
spam(ham[1], {eggs: 2}	)
#: E202:1:22
spam(ham[1], {eggs: 2	})
#: E202:1:11
spam(ham[1	], {eggs: 2})
#: Okay
spam(ham[1], {eggs: 2})

result = func(
    arg1='some value',
    arg2='another value',
)

result = func(
    arg1='some value',
    arg2='another value'
)

result = [
    item for item in items
    if item > 5
]
#:


#: E203:1:10
if x == 4 :
    print x, y
    x, y = y, x
#: E203:1:10
if x == 4	:
    print x, y
    x, y = y, x
#: E203:2:15 E702:2:16
if x == 4:
    print x, y ; x, y = y, x
#: E203:2:15 E702:2:16
if x == 4:
    print x, y	; x, y = y, x
#: E203:3:13
if x == 4:
    print x, y
    x, y = y , x
#: E203:3:13
if x == 4:
    print x, y
    x, y = y	, x
#: Okay
if x == 4:
    print x, y
    x, y = y, x
a[b1, :] == a[b1, ...]
b = a[:, b1]
#:
