# Pydantic on Python 3.12

```toml
[environment]
python-version = "3.12"
python-platform = "linux"

[project]
dependencies = ["pydantic==2.13.4"]
```

## Frozen models and variance

Frozen models are covariant in the type of a read-only field. Mutable models are invariant.

```py
from pydantic import BaseModel, ConfigDict

class Animal: ...
class Dog(Animal): ...

class KeywordFrozen[T](BaseModel, frozen=True):
    value: T

class ConfigFrozen[T](BaseModel):
    model_config = ConfigDict(frozen=True)
    value: T

class Mutable[T](BaseModel):
    value: T

def check(keyword: KeywordFrozen[Dog], config: ConfigFrozen[Dog], mutable: Mutable[Dog]):
    keyword_animal: KeywordFrozen[Animal] = keyword  # ok
    config_animal: ConfigFrozen[Animal] = config  # ok
    mutable_animal: Mutable[Animal] = mutable  # error: [invalid-assignment]
```
