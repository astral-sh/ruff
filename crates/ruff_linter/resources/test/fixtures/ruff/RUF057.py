foo = 1
bar = 0
qux = 0.


### Errors

round(1, 0)
round(1, ndigits=0)
round(number=1, ndigits=0)

round(1, None)
round(1, ndigits=None)
round(number=1, ndigits=None)

round(1., None)
round(1., ndigits=None)
round(number=1., ndigits=None)

round(foo, None)
round(foo, ndigits=None)
round(number=foo, ndigits=None)

### No errors

round(1., 0)
round(1., ndigits=0)
round(number=1., ndigits=0)

round(1, 1)
round(1, bar)
round(1., bar)
round(1., 0)
round(foo, bar)
round(0, 3.14)
round(0, 0, extra=keyword)
