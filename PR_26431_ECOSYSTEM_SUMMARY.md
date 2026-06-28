# [PR #26431](https://github.com/astral-sh/ruff/pull/26431) ecosystem summary

The report contains nine added diagnostics and two changed diagnostics, all in Home Assistant. The nine additions are false positives caused by synthesizing the narrower `Flag.__or__(Self) -> Self` contract for `IntFlag` subclasses: mixed `IntFlag | int` expressions fall through to `int` even though the runtime operation returns the `IntFlag` subclass. The two message-only changes improve the precision of pure flag unions by retaining every participating enum member instead of only the leftmost member.

## Mixed `IntFlag` and `int` operations produce false positives

**Report entries:**

- [homeassistant/components/freebox/alarm_control_panel.py:74](https://github.com/home-assistant/core/blob/9d34e83f71af0e8f7466ca18fa316ca1281c9ba3/homeassistant/components/freebox/alarm_control_panel.py#L74)
- [homeassistant/components/group/cover.py:340](https://github.com/home-assistant/core/blob/9d34e83f71af0e8f7466ca18fa316ca1281c9ba3/homeassistant/components/group/cover.py#L340)
- [homeassistant/components/mqtt/vacuum.py:257](https://github.com/home-assistant/core/blob/9d34e83f71af0e8f7466ca18fa316ca1281c9ba3/homeassistant/components/mqtt/vacuum.py#L257)
- [homeassistant/components/aprilaire/climate.py:115](https://github.com/home-assistant/core/blob/9d34e83f71af0e8f7466ca18fa316ca1281c9ba3/homeassistant/components/aprilaire/climate.py#L115)
- [homeassistant/components/devialet/media_player.py:127](https://github.com/home-assistant/core/blob/9d34e83f71af0e8f7466ca18fa316ca1281c9ba3/homeassistant/components/devialet/media_player.py#L127)
- [homeassistant/components/zwave_js/cover.py:99](https://github.com/home-assistant/core/blob/9d34e83f71af0e8f7466ca18fa316ca1281c9ba3/homeassistant/components/zwave_js/cover.py#L99)
- [homeassistant/components/zwave_js/cover.py:112](https://github.com/home-assistant/core/blob/9d34e83f71af0e8f7466ca18fa316ca1281c9ba3/homeassistant/components/zwave_js/cover.py#L112)
- [homeassistant/components/zwave_js/cover.py:267](https://github.com/home-assistant/core/blob/9d34e83f71af0e8f7466ca18fa316ca1281c9ba3/homeassistant/components/zwave_js/cover.py#L267)
- [homeassistant/components/zwave_js/cover.py:280](https://github.com/home-assistant/core/blob/9d34e83f71af0e8f7466ca18fa316ca1281c9ba3/homeassistant/components/zwave_js/cover.py#L280)

On the merge base, enum member lookup retains the `IntFlag.__or__(int) -> Self` signature for these expressions. On the PR, class-creation synthesis exposes the `Flag` signature instead, so any branch containing an integer fallback such as `0` is inferred as `int`. This causes the assignment and return errors; the errors at Z-Wave lines 112 and 280 are cascades because the preceding invalid assignments no longer narrow the optional attributes.

```python
# Merge base: no diagnostics
# PR: error[invalid-return-type] Return type does not match returned value: expected `Feature`, found `int`
# PR: error[invalid-assignment] Object of type `int` is not assignable to attribute `features` of type `Feature | None`
# PR: error[unsupported-operator] Operator `|=` is not supported between objects of type `None` and `Literal[Feature.B]`
from enum import IntFlag

class Feature(IntFlag):
    A = 1
    B = 2

def supported_features(condition: bool) -> Feature:
    features = Feature.A | (Feature.B if condition else 0)
    return features

class Entity:
    features: Feature | None

    def update(self, condition: bool) -> None:
        self.features = (self.features or 0) | Feature.A | Feature.B
        if condition:
            self.features |= Feature.B
```

## Pure flag unions retain all enum members

**Report entries:**

- [homeassistant/components/hunterdouglas_powerview/cover.py:368](https://github.com/home-assistant/core/blob/9d34e83f71af0e8f7466ca18fa316ca1281c9ba3/homeassistant/components/hunterdouglas_powerview/cover.py#L368)
- [homeassistant/components/velux/cover.py:242](https://github.com/home-assistant/core/blob/9d34e83f71af0e8f7466ca18fa316ca1281c9ba3/homeassistant/components/velux/cover.py#L242)

Both revisions diagnose the optional left operand. The PR improves the right operand from only the first enum literal to the union of every flag member in the expression, which explains the longer but more accurate messages in both entries.

```python
# Merge base: error[unsupported-operator] Operator `|=` is not supported between objects of type `None` and `Literal[Feature.A]`
# PR: error[unsupported-operator] Operator `|=` is not supported between objects of type `None` and `Literal[Feature.A, Feature.B]`
from enum import IntFlag

class Feature(IntFlag):
    A = 1
    B = 2

def update(features: Feature | None) -> None:
    features |= Feature.A | Feature.B
```

## Reproduction

- Detailed report: [ecosystem-analyzer report](https://32d753d8.ty-ecosystem-ext.pages.dev/diff)
- Actions run: [run 28294734198](https://github.com/astral-sh/ruff/actions/runs/28294734198)
- Ruff comparison: [`0da3f45792522418655b5081ef78fd34ba9bf48f`](https://github.com/astral-sh/ruff/commit/0da3f45792522418655b5081ef78fd34ba9bf48f) to [`ab0765f2e469bf1a84eff480062e0012b3485393`](https://github.com/astral-sh/ruff/commit/ab0765f2e469bf1a84eff480062e0012b3485393)
- `ecosystem-analyzer`: [`e2c5b76149b147fae104a7d8fa0997a9eb7f7754`](https://github.com/astral-sh/ecosystem-analyzer/commit/e2c5b76149b147fae104a7d8fa0997a9eb7f7754)
- `mypy-primer`: [`ff3d9531335dad605d43fcc208c9c159bb09fa81`](https://github.com/hauntsaninja/mypy_primer/commit/ff3d9531335dad605d43fcc208c9c159bb09fa81)
- Project Python: `core: 3.11`
- Dependency cutoff: `2026-06-27T16:17:05Z`
- Comparison method: Set up `core` at revision `9d34e83f71af0e8f7466ca18fa316ca1281c9ba3` with the pinned `mypy-primer` helper and dependency cutoff, then ran copied binaries from both Ruff revisions with `ty check homeassistant --python .venv --output-format concise` and the PR's `.github/ty-ecosystem.toml`.
