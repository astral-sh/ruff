[x for x in (y := range(3))]
[a for a in [(b := 1) for b in [1]]]
xs = [1]
[x for x in xs if [z for z in (x := xs)]]
