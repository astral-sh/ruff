l = []


### Errors
l.extend([list])
l.extend((tuple,   ))
l.extend({  set})
l.extend({dict:
              multiline})


l.extend({dict:            # Unsafe
              multiline})


### No errors
l.extend([])
l.extend(())
l.extend({})

l.extend([*[1]])
l.extend({*1})
l.extend("a")

l.extend((*(),))
l.extend({**{}})

l.extend(foo)
l.extend([bar], lorem=ipsum)
l.extend(*baz)
l.extend(**qux)
