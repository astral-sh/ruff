# Below is black stable style
# In preview style, black always breaks the right side first

if True:
    aaaaaaaaaaaaaaaaaaaaaaaaaaaaaa[
        bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
    ] = cccccccc.ccccccccccccc.cccccccc

    aaaaaaaaaaaaaaaaaaaaaaaaaaaaaa[
        bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
    ] = cccccccc.ccccccccccccc().cccccccc

    aaaaaaaaaaaaaaaaaaaaaaaaaaaaaa[
        bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
    ] = cccccccc.ccccccccccccc(d).cccccccc

    aaaaaaaaaaaaaaaaaaaaaaaaaaaaaa[bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb] = (
        cccccccc.ccccccccccccc(d).cccccccc + e
    )

    aaaaaaaaaaaaaaaaaaaaaaaaaaaaaa[bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb] = (
        cccccccc.ccccccccccccc.cccccccc + e
    )
    aaaaaaaaaaaaaaaaaaaaaaaaaaaaaa[bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb] = (
        cccccccc.ccccccccccccc.cccccccc
        + eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee
    )

    self._cache: dict[
        DependencyCacheKey, list[list[DependencyPackage]]
    ] = collections.defaultdict(list)
    self._cached_dependencies_by_level: dict[
        int, list[DependencyCacheKey]
    ] = collections.defaultdict(list)
