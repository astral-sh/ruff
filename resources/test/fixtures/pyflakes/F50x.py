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
