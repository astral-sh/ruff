# Pydantic variance on Python 3.12

```toml
[environment]
python-version = "3.12"
python-platform = "linux"

[project]
dependencies = ["pydantic==2.13.4"]
```

## Frozen models

ty infers covariance when the model's configuration makes its generic field read-only.

```py
from pydantic import BaseModel, ConfigDict

class Animal: ...
class Dog(Animal): ...

class KeywordFrozen[T](BaseModel, frozen=True):
    value: T

class ConfigFrozen[T](BaseModel):
    model_config = ConfigDict(frozen=True)
    value: T

def check(keyword: KeywordFrozen[Dog], config: ConfigFrozen[Dog]):
    keyword_animal: KeywordFrozen[Animal] = keyword  # ok
    config_animal: ConfigFrozen[Animal] = config  # ok
```

## Mutable fields

Model fields remain invariant when the model is mutable or a custom setter allows writes.

```py
from pydantic import BaseModel

class Animal: ...
class Dog(Animal): ...

class Mutable[T](BaseModel):
    value: T

class CustomSetter[T](BaseModel, frozen=True):
    value: T

    def __setattr__(self, name: str, value: object) -> None:
        object.__setattr__(self, name, value)

def check(mutable: Mutable[Dog], custom: CustomSetter[Dog]):
    mutable_animal: Mutable[Animal] = mutable  # error: [invalid-assignment]
    custom_animal: CustomSetter[Animal] = custom  # error: [invalid-assignment]
```
