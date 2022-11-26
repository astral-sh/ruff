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
'%(bar)*s' % {'bar': 'baz'}  # F508

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

# As above, but with bytes
b'%(foo)' % {'foo': 'bar'}  # F501
b'%s %(foo)s' % {'foo': 'bar'}  # F506
b'%(foo)s %s' % {'foo': 'bar'}  # F506
b'%j' % (1,)  # F509
b'%s %s' % (1,)  # F507
b'%s %s' % (1, 2, 3)  # F507
b'%(bar)s' % {}  # F505
b'%(bar)s' % {'bar': 1, 'baz': 2}  # F504
b'%(bar)s' % (1, 2, 3)  # F502
b'%s %s' % {'k': 'v'}  # F503
b'%(bar)*s' % {'bar': 'baz'}  # F508

# ok: single %s with mapping
b'%s' % {'foo': 'bar', 'baz': 'womp'}
# ok: %% should not count towards placeholder count
b'%% %s %% %s' % (1, 2)
# ok: * consumes one positional argument
b'%.*f' % (2, 1.1234)
b'%*.*f' % (5, 2, 3.1234)
# ok *args and **kwargs
a = []
b'%s %s' % [*a]
b'%s %s' % (*a,)
k = {}
b'%(k)s' % {**k}
