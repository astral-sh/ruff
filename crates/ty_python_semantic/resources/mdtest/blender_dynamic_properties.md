# Blender dynamic properties

Tests for dynamically created Blender properties that are assigned to `bpy.types.*` classes
in project files and should be resolved when accessing attributes on instances of those classes.

## Basic dynamic property resolution

A property assigned via `bpy.types.Scene.my_prop = bpy.props.StringProperty()` in one file
should be visible as an attribute on `Scene` instances in another file.

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

`register_props.py`:

```py
import bpy

bpy.types.Scene.my_string = bpy.props.StringProperty()
bpy.types.Scene.my_int = bpy.props.IntProperty()
bpy.types.Object.my_float = bpy.props.FloatProperty()
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

## Basic dynamic property resolution existing class properties

A property assigned via `bpy.types.Scene.my_prop = bpy.props.StringProperty()` in one file
should be visible as an attribute on `Scene` instances in another file.

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

`register_props.py`:

```py
import bpy

bpy.types.Scene.my_string = bpy.props.StringProperty()
bpy.types.Scene.my_int = bpy.props.IntProperty()
bpy.types.Object.my_float = bpy.props.FloatProperty()
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

## Multiple files registering properties for the same class

Properties can be registered across multiple files and all should be visible.

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
def BoolProperty() -> bool: ...
```

`register_a.py`:

```py
import bpy

bpy.types.Scene.prop_a = bpy.props.StringProperty()
```

`register_b.py`:

```py
import bpy

bpy.types.Scene.prop_b = bpy.props.BoolProperty()
```

`consumer.py`:

```py
import bpy

def check(scene: bpy.types.Scene) -> None:
    reveal_type(scene.prop_a)  # revealed: str
    reveal_type(scene.prop_b)  # revealed: bool
```

## Basic dynamic property resolution register unregister

A property assigned via `bpy.types.Scene.my_prop = bpy.props.StringProperty()` in one file
should be visible as an attribute on `Scene` instances in another file.

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

`register_props2.py`:

```py
import bpy

def create_props():
    bpy.types.Scene.my_int = bpy.props.IntProperty()
    bpy.types.Object.my_float = bpy.props.FloatProperty()

def destroy_props():
    del bpy.types.Scene.my_int
    del bpy.types.Object.my_float
```

`register_props.py`:

```py
import bpy
from register_props2 import create_props, destroy_props

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
