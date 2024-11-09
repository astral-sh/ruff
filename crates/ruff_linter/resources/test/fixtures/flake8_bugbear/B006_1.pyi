# we disable this rule for pyi files

def mquantiles(
    a: _ArrayLikeFloat_co,
    prob: _ArrayLikeFloat_co = [0.25, 0.5, 0.75],
    alphap: AnyReal = 0.4,
    betap: AnyReal = 0.4,
    axis: CanIndex | None = None,
    limit: tuple[AnyReal, AnyReal] | tuple[()] = (),
) -> _MArrayND: ...
