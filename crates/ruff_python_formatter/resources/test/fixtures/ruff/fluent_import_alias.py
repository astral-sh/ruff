import pandas as pd

# Fixtures for fluent formatting of call chains
# Note that `fluent.options.json` sets line width to 8


x = pd.b()

x = pd.b().c()

x = pd.b().c().d

x = pd.b.c.d().e()

x = pd.b.c().d.e().f.g()

