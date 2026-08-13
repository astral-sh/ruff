# Fixtures for fluent formatting of call chains
# Note that `fluent.options.json` sets line width to 8


x = a.b()

x = a.b().c()

x = a.b().c().d

x = a.b.c.d().e()

x = a.b.c().d.e().f.g()

# Consecutive calls/subscripts are grouped together
# for the purposes of fluent formatting (though, as 2025.12.15,
# there may be a break inside of one of these
# calls/subscripts, but that is unrelated to the fluent format.)

x = a()[0]().b().c()

x = a.b()[0].c.d()[1]().e

# Parentheses affect both where the root of the call
# chain is and how many calls we require before applying
# fluent formatting (just 1, in the presence of a parenthesized
# root, as of 2025.12.15.)

x = (a).b()

x = (a()).b()

x = (a.b()).d.e()

x = (a.b().d).e()
