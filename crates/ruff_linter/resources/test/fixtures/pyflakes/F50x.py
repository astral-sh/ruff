'%(foo)' % {'foo': 'bar'}  # F501
'%s %(foo)s' % {'foo': 'bar'}  # F506
'%(foo)s %s' % {'foo': 'bar'}  # F506
'%j' % (1,)  # F509
'%s %s' % (1,)  # F507
'%s %s' % (1, 2, 3)  # F507
'%(bar)s' % {}  # F505
'%(bar)s' % {'bar': 1, 'baz': 2}  # F504
'%(bar)s' % (1, 2, 3)  # F502
'%s %s' % {'k': 'v'}  # F503
'%(bar)*s' % {'bar': 'baz'}  # F506, F508

# ok: single %s with mapping
'%s' % {'foo': 'bar', 'baz': 'womp'}
# ok: %% should not count towards placeholder count
'%% %s %% %s' % (1, 2)
# ok: * consumes one positional argument
'%.*f' % (2, 1.1234)
'%*.*f' % (5, 2, 3.1234)
# ok *args and **kwargs
a = []
'%s %s' % [*a]
'%s %s' % (*a,)
k = {}
'%(k)s' % {**k}
'%s' % [1, 2, 3]
'%s' % {1, 2, 3}
# F507: literal non-tuple RHS with multiple positional placeholders
'%s %s' % 42  # F507
'%s %s' % 3.14  # F507
'%s %s' % "hello"  # F507
'%s %s' % b"hello"  # F507
'%s %s' % True  # F507
'%s %s' % None  # F507
'%s %s' % ...  # F507
'%s %s' % f"hello {name}"  # F507
# F507: ResolvedPythonType catches compound expressions with known types
'%s %s' % -1  # F507 (unary op on int → int)
'%s %s' % (1 + 2)  # F507 (int + int → int)
'%s %s' % (not x)  # F507 (not → bool)
'%s %s' % ("a" + "b")  # F507 (str + str → str)
'%s %s' % (1 if True else 2)  # F507 (int if ... else int → int)
# ok: single placeholder with literal RHS
'%s' % 42
'%s' % "hello"
'%s' % True
# ok: variables/expressions could be tuples at runtime
'%s %s' % banana
'%s %s' % obj.attr
'%s %s' % arr[0]
'%s %s' % get_args()
# ok: ternary/binop where one branch could be a tuple → Unknown
'%s %s' % (a if cond else b)
'%s %s' % (a + b)

# F507: zero placeholders with literal non-tuple RHS
'hello' % 42  # F507
'' % 42  # F507
'hello' % (1,)  # F507
# F507: zero placeholders with variable RHS (intentional use is very unlikely)
banana = 42
'hello' % banana  # F507
'' % banana  # F507
'hello' % unknown_var  # F507
'hello' % get_value()  # F507
'hello' % obj.attr  # F507
# ok: zero placeholders with empty tuple
'hello' % ()
