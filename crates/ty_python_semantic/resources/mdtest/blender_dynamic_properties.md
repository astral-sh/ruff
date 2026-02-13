# Blender dynamic properties

Tests for dynamically created Blender properties that are assigned to `bpy.types.*` classes
in project files. Properties must be registered from the `register()` function in the project
root `__init__.py` or functions it transitively calls.

## Dynamic property resolution from register()

Properties assigned inside the `register()` function in the root `__init__.py` should be
resolved when accessing attributes on instances of those classes.

```toml
[environment]
extra-paths = ["/stubs"]
```

`/stubs/bpy/__init__.pyi`:

```pyi
from bpy import props as props
from bpy import types as types
```

`/stubs/bpy/types.pyi`:

```pyi
class Scene:
    pass

class Object:
    pass
```

`/stubs/bpy/props.pyi`:

```pyi
def StringProperty() -> str: ...
def IntProperty() -> int: ...
def FloatProperty() -> float: ...
def BoolProperty() -> bool: ...
```

`my_addon/__init__.py`:

```py
import bpy

def register():
    bpy.types.Scene.my_string = bpy.props.StringProperty()
    bpy.types.Scene.my_int = bpy.props.IntProperty()
    bpy.types.Object.my_float = bpy.props.FloatProperty()

def unregister():
    del bpy.types.Scene.my_string
    del bpy.types.Scene.my_int
    del bpy.types.Object.my_float
```

`use_props.py`:

```py
import bpy

def use_scene(scene: bpy.types.Scene) -> None:
    reveal_type(scene.my_string)  # revealed: str
    reveal_type(scene.my_int)  # revealed: int

def use_object(obj: bpy.types.Object) -> None:
    reveal_type(obj.my_float)  # revealed: int | float
```

## Properties registered via functions called from register()

Properties can be registered from helper functions that `register()` calls, including
functions imported from other modules.

```toml
[environment]
extra-paths = ["/stubs"]
```

`/stubs/bpy/__init__.pyi`:

```pyi
from bpy import props as props
from bpy import types as types
```

`/stubs/bpy/types/__init__.pyi`:

```pyi
class Scene:
    my_existing_float: float

class Object:
    my_existing_bool: bool
```

`/stubs/bpy/props/__init__.pyi`:

```pyi
def StringProperty() -> str: ...
def IntProperty() -> int: ...
def FloatProperty() -> float: ...
def BoolProperty() -> bool: ...
```

`register_helpers.py`:

```py
import bpy

def create_props():
    bpy.types.Scene.my_int = bpy.props.IntProperty()
    bpy.types.Object.my_float = bpy.props.FloatProperty()

def destroy_props():
    del bpy.types.Scene.my_int
    del bpy.types.Object.my_float
```

`my_addon/__init__.py`:

```py
import bpy
from register_helpers import create_props, destroy_props

def register():
    bpy.types.Scene.my_string = bpy.props.StringProperty()
    create_props()

def unregister():
    del bpy.types.Scene.my_string
    destroy_props()
```

`use_props.py`:

```py
import bpy

def use_scene(scene: bpy.types.Scene) -> None:
    reveal_type(scene.my_string)  # revealed: str
    reveal_type(scene.my_int)  # revealed: int
    reveal_type(scene.my_existing_float)  # revealed: int | float

def use_object(obj: bpy.types.Object) -> None:
    reveal_type(obj.my_float)  # revealed: int | float
    reveal_type(obj.my_existing_bool)  # revealed: bool
```

## Properties outside register() scope emit error

Properties defined outside the `register()` function scope (e.g., at module top level)
should emit an error and not be resolved.

```toml
[environment]
extra-paths = ["/stubs"]
```

`/stubs/bpy/__init__.pyi`:

```pyi
from bpy import props as props
from bpy import types as types
```

`/stubs/bpy/types.pyi`:

```pyi
class Scene:
    pass
```

`/stubs/bpy/props.pyi`:

```pyi
def StringProperty() -> str: ...
def IntProperty() -> int: ...
```

`my_addon/__init__.py`:

```py
import bpy

def register():
    bpy.types.Scene.my_valid = bpy.props.StringProperty()

def unregister():
    del bpy.types.Scene.my_valid
```

`bad_register.py`:

```py
import bpy

# error: [blender-property-outside-register] "Blender properties can only be registered from the `register()` function or functions it calls in the project root `__init__.py`"
bpy.types.Scene.my_invalid = bpy.props.IntProperty()
```
