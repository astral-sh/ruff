import pandas as pd

x = pd.DataFrame()

x.drop(["a"], axis=1, inplace=True)

x.y.drop(["a"], axis=1, inplace=True)

x["y"].drop(["a"], axis=1, inplace=True)

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

x.drop(["a"], axis=1, **kwargs, inplace=True)
x.drop(["a"], axis=1, inplace=True, **kwargs)
f(x.drop(["a"], axis=1, inplace=True))

x.apply(lambda x: x.sort_values("a", inplace=True))
import torch

torch.m.ReLU(inplace=True)  # safe because this isn't a pandas call

(x.drop(["a"], axis=1, inplace=True))

# This method doesn't take exist in Pandas, so ignore it.
x.rotate_z(45, inplace=True)
