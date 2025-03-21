[a for a in range(10)]
[abcd for abcd in range(10) if abcd > 5]
aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa([a for a in range(10)])
aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa([abcd for abcd in range(10) if abcd > 5])

{a: a for a in f(10)}
{abcd: abcd for abcd in f(10) if abcd > 5}
aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa({a: a for a in f(10)})
aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa({abcd: abcd for abcd in f(10) if abcd > 5})

{a for a in a(10)}
{abcd for abcd in a(10) if abcd > 5}
aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa({a for a in a(10)})
aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa({abcd for abcd in a(10) if abcd > 5})

(a for a in a(10))
(abcd for abcd in a(10) if abcd > 5)
aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa((a for a in a(10)))
aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa(all(a for a in a(10)))
aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa((abcd for abcd in a(10) if abcd > 5))
aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa(all(abcd for abcd in a(10) if abcd > 5))

x = a if b else c
x = a if b else c if d else e
x = aaaaaaa if bbbbbbbbb else ccccccccc
x = aaaaaaa if bbbbbbbbb else ccccccccc if ddddddddd else eeeeeeeee
fffffffffffffffffffffff(a if b else c)
fffffffffffffffffffffff(aaaaaaaaa if bbbbbbbbb else ccccccccc)