import pandas as pd

x = pd.DataFrame()

x.drop(["a"], axis=1, inplace=True)

x.drop(["a"], axis=1, inplace=True)

x.drop(
    inplace=True,
    columns=["a"],
    axis=1,
)

if True:
    x.drop(
        inplace=True,
        columns=["a"],
        axis=1,
    )
