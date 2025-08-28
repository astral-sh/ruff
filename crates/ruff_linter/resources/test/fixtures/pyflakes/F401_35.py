"""
Test: allowed-unused-imports-top-level-module
"""

# No errors

def f():
    import hvplot
    import hvplot.pandas
    import hvplot.pandas.plots
    from hvplot.pandas import scatter_matrix
    from hvplot.pandas.plots import scatter_matrix
