# Errors
1 in []
1 not in []
_ in ()
_ not in ()
'a' in set()
'a' not in set()
'b' in {}
'b' not in {}

# OK
1 in [2]
1 in [1, 2, 3]
_ in ('a')
_ not in ('a')
'a' in set('a', 'b')
'a' not in set('b', 'c')
'b' in {1: 2}
'b' not in {3: 4}
