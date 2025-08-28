"""
Test: allowed-unused-imports-top-level-module
"""

# No errors

def f():
    import hvplot
def f():
    import hvplot.pandas
def f():
    import hvplot.pandas.plots
def f():
    from hvplot.pandas import scatter_matrix
def f():
    from hvplot.pandas.plots import scatter_matrix
