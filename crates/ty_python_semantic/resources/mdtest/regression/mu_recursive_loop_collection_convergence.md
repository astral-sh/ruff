# Recursive loop-carried collection convergence

This is minimized from a Kornia ecosystem failure. The loop-carried empty lists used to keep a
`Divergent` element type and panic with "too many cycle iterations" when the lists were consumed
later in the function.

This test intentionally checks that the recursive collection state converges rather than asserting
the precise inferred element types.

```py
from typing import Any, Iterator

class Tensor:
    def __getitem__(self, key: Any) -> "Tensor":
        raise NotImplementedError

    def __iter__(self) -> Iterator["Tensor"]:
        raise NotImplementedError

    def __len__(self) -> int:
        raise NotImplementedError

    def nonzero(self) -> "Tensor":
        raise NotImplementedError

    def sort(self, descending: bool = False) -> tuple["Tensor", "Tensor"]:
        raise NotImplementedError

def detect(masks: Tensor, scores: Tensor, limit: int, return_extra: bool):
    keypoints = []
    values = []
    extras: list[Tensor] = []

    for mask in masks:
        indices = mask.nonzero()[:, 0]
        if len(indices) > limit:
            sort_idx = indices.sort(descending=True)[1]
            indices = indices[sort_idx[:limit]]
        keypoints.append(indices)

    for indices in keypoints:
        value = scores[indices]
        values.append(value)
        if return_extra:
            extras.append(value)

    if return_extra:
        _ = (keypoints, values, extras)
        return keypoints, values, extras

    _ = (keypoints, values)
    return keypoints, values
```
