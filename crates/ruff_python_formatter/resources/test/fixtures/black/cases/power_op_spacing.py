def function(**kwargs):
    t = a**2 + b**3
    return t ** 2


def function_replace_spaces(**kwargs):
    t = a **2 + b** 3 + c ** 4


def function_dont_replace_spaces():
    {**a, **b, **c}


a = 5**~4
b = 5 ** f()
c = -(5**2)
d = 5 ** f["hi"]
e = lazy(lambda **kwargs: 5)
f = f() ** 5
g = a.b**c.d
h = 5 ** funcs.f()
i = funcs.f() ** 5
j = super().name ** 5
k = [(2**idx, value) for idx, value in pairs]
l = mod.weights_[0] == pytest.approx(0.95**100, abs=0.001)
m = [([2**63], [1, 2**63])]
n = count <= 10**5
o = settings(max_examples=10**6)
p = {(k, k**2): v**2 for k, v in pairs}
q = [10**i for i in range(6)]
r = x**y
s = 1 ** 1
t = (
    1
    ** 1
    **1
    ** 1
)

a = 5.0**~4.0
b = 5.0 ** f()
c = -(5.0**2.0)
d = 5.0 ** f["hi"]
e = lazy(lambda **kwargs: 5)
f = f() ** 5.0
g = a.b**c.d
h = 5.0 ** funcs.f()
i = funcs.f() ** 5.0
j = super().name ** 5.0
k = [(2.0**idx, value) for idx, value in pairs]
l = mod.weights_[0] == pytest.approx(0.95**100, abs=0.001)
m = [([2.0**63.0], [1.0, 2**63.0])]
n = count <= 10**5.0
o = settings(max_examples=10**6.0)
p = {(k, k**2): v**2.0 for k, v in pairs}
q = [10.5**i for i in range(6)]
s = 1.0 ** 1.0
t = (
    1.0
    ** 1.0
    **1.0
    ** 1.0
)


# WE SHOULD DEFINITELY NOT EAT THESE COMMENTS (https://github.com/psf/black/issues/2873)
if hasattr(view, "sum_of_weights"):
    return np.divide(  # type: ignore[no-any-return]
        view.variance,  # type: ignore[union-attr]
        view.sum_of_weights,  # type: ignore[union-attr]
        out=np.full(view.sum_of_weights.shape, np.nan),  # type: ignore[union-attr]
        where=view.sum_of_weights**2 > view.sum_of_weights_squared,  # type: ignore[union-attr]
    )

return np.divide(
    where=view.sum_of_weights_of_weight_long**2 > view.sum_of_weights_squared,  # type: ignore
)
