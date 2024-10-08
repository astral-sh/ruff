"""
Test: allowed-unused-imports
"""

# OK
import hvplot.pandas
import hvplot.pandas.plots
from hvplot.pandas import scatter_matrix
from hvplot.pandas.plots import scatter_matrix

# Errors
from hvplot.pandas_alias import scatter_matrix
