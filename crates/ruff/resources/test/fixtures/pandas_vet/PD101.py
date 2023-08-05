import pandas as pd


data = pd.Series(range(1000))

# PD101
data.nunique() <= 1
data.nunique(dropna=True) <= 1
data.nunique(dropna=False) <= 1
data.nunique() == 1
data.nunique(dropna=True) == 1
data.nunique(dropna=False) == 1
data.nunique() != 1
data.nunique(dropna=True) != 1
data.nunique(dropna=False) != 1
data.nunique() > 1
data.dropna().nunique() == 1
data[data.notnull()].nunique() == 1

# No violation of this rule
data.nunique() == 0  # empty
data.nunique() >= 1  # not-empty
data.nunique() < 1  # empty
data.nunique() == 2  # not constant
data.unique() == 1  # not `nunique`

{"hello": "world"}.nunique() == 1  # no pd.Series
