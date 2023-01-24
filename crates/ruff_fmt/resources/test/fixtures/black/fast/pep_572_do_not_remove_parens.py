# Most of the following examples are really dumb, some of them aren't even accepted by Python,
# we're fixing them only so fuzzers (which follow the grammar which actually allows these
# examples matter of fact!) don't yell at us :p

del (a := [1])

try:
    pass
except (a := 1) as (b := why_does_this_exist):
    pass

for (z := 124) in (x := -124):
    pass

with (y := [3, 2, 1]) as (funfunfun := indeed):
    pass


@(please := stop)
def sigh():
    pass


for (x := 3, y := 4) in y:
    pass
