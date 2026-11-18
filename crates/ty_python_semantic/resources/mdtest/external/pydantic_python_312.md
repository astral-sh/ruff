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

class Animal(BaseModel): ...
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

A custom `__setattr__` can bypass Pydantic's enforcement of `frozen=True`. The model's field remains
writable, so the model is invariant.

```py
class FrozenWithCustomSetattr[T](BaseModel, frozen=True):
    value: T

    def __setattr__(self, name: str, value: object) -> None:
        object.__setattr__(self, name, value)

def check_custom_setattr(dogs: FrozenWithCustomSetattr[Dog]):
    dogs.value = Dog()  # ok
    animals: FrozenWithCustomSetattr[Animal] = dogs  # error: [invalid-assignment]
```

## Thawed subclasses

A subclass that disables freezing can write inherited fields. In this case, the subclass should be
treated as invariant:

```py
from pydantic import BaseModel

class Frozen[T](BaseModel, frozen=True):
    value: T

class Thawed[T](Frozen[T], frozen=False): ...

def overwrite(box: Thawed[object]):
    box.value = 0

def check(box: Thawed[str]):
    # TODO: Reject this call because the inherited writable field should make Thawed invariant.
    overwrite(box)
```
