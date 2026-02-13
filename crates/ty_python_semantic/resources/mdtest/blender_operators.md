# Blender custom operators

Tests for custom Blender operators registered via `bpy.utils.register_class()` in the `register()`
function of the project root `__init__.py`. When an operator class with `bl_idname = "module.name"`
is registered, `bpy.ops.module.name(...)` should resolve to a callable with a synthesized signature.

## Basic operator registration and call

A simple operator with annotated properties should be callable via `bpy.ops.<module>.<name>`.

```toml
[environment]
extra-paths = ["/stubs"]
```

`/stubs/bpy/__init__.pyi`:

```pyi
from bpy import ops as ops
from bpy import props as props
from bpy import types as types
from bpy import utils as utils
```

`/stubs/bpy/types.pyi`:

```pyi
class Operator:
    bl_idname: str
    bl_label: str
```

`/stubs/bpy/props.pyi`:

```pyi
def IntProperty() -> int: ...
def FloatProperty() -> float: ...
def StringProperty() -> str: ...
def BoolProperty() -> bool: ...
```

`/stubs/bpy/utils.pyi`:

```pyi
def register_class(cls: type) -> None: ...
```

`/stubs/bpy/ops/__init__.pyi`:

```pyi
```

`my_addon/__init__.py`:

```py
import bpy

class SimpleMouseOperator(bpy.types.Operator):
    bl_idname = "wm.mouse_position"
    bl_label = "Mouse Position"

    x: bpy.props.IntProperty()
    y: bpy.props.IntProperty()

    def execute(self, context):
        return {"FINISHED"}

def register():
    bpy.utils.register_class(SimpleMouseOperator)

def unregister():
    pass
```

`use_operator.py`:

```py
import bpy

def test():
    reveal_type(bpy.ops.wm.mouse_position)  # revealed: (execution_context: int | str | None = None, undo: bool | None = None, *, /, x: int | None = None, y: int | None = None) -> set[str]
    bpy.ops.wm.mouse_position(x=10, y=20)
```

## Multiple operators in different modules

Multiple operators registered in the same `register()` function should all be resolvable.

```toml
[environment]
extra-paths = ["/stubs"]
```

`/stubs/bpy/__init__.pyi`:

```pyi
from bpy import ops as ops
from bpy import props as props
from bpy import types as types
from bpy import utils as utils
```

`/stubs/bpy/types.pyi`:

```pyi
class Operator:
    bl_idname: str
    bl_label: str
```

`/stubs/bpy/props.pyi`:

```pyi
def IntProperty() -> int: ...
def FloatProperty() -> float: ...
def StringProperty() -> str: ...
def BoolProperty() -> bool: ...
```

`/stubs/bpy/utils.pyi`:

```pyi
def register_class(cls: type) -> None: ...
```

`/stubs/bpy/ops/__init__.pyi`:

```pyi
```

`my_addon/__init__.py`:

```py
import bpy

class MoveOperator(bpy.types.Operator):
    bl_idname = "mesh.custom_move"
    bl_label = "Custom Move"

    distance: bpy.props.FloatProperty()

    def execute(self, context):
        return {"FINISHED"}

class InfoOperator(bpy.types.Operator):
    bl_idname = "wm.show_info"
    bl_label = "Show Info"

    message: bpy.props.StringProperty()

    def execute(self, context):
        return {"FINISHED"}

def register():
    bpy.utils.register_class(MoveOperator)
    bpy.utils.register_class(InfoOperator)

def unregister():
    pass
```

`use_operators.py`:

```py
import bpy

def test():
    reveal_type(bpy.ops.mesh.custom_move)  # revealed: (execution_context: int | str | None = None, undo: bool | None = None, *, /, distance: int | float | None = None) -> set[str]
    reveal_type(bpy.ops.wm.show_info)  # revealed: (execution_context: int | str | None = None, undo: bool | None = None, *, /, message: str | None = None) -> set[str]
```

## Operator registered via helper function

Operators can be registered from helper functions that `register()` calls, including
functions imported from other modules.

```toml
[environment]
extra-paths = ["/stubs"]
```

`/stubs/bpy/__init__.pyi`:

```pyi
from bpy import ops as ops
from bpy import props as props
from bpy import types as types
from bpy import utils as utils
```

`/stubs/bpy/types.pyi`:

```pyi
class Operator:
    bl_idname: str
    bl_label: str
```

`/stubs/bpy/props.pyi`:

```pyi
def IntProperty() -> int: ...
def FloatProperty() -> float: ...
def StringProperty() -> str: ...
def BoolProperty() -> bool: ...
```

`/stubs/bpy/utils.pyi`:

```pyi
def register_class(cls: type) -> None: ...
```

`/stubs/bpy/ops/__init__.pyi`:

```pyi
```

`operators.py`:

```py
import bpy

class HelperOperator(bpy.types.Operator):
    bl_idname = "wm.helper_op"
    bl_label = "Helper Op"

    name: bpy.props.StringProperty()
    count: bpy.props.IntProperty()

    def execute(self, context):
        return {"FINISHED"}

def register_operators():
    bpy.utils.register_class(HelperOperator)
```

`my_addon/__init__.py`:

```py
from operators import register_operators

def register():
    register_operators()

def unregister():
    pass
```

`use_helper_op.py`:

```py
import bpy

def test():
    reveal_type(bpy.ops.wm.helper_op)  # revealed: (execution_context: int | str | None = None, undo: bool | None = None, *, /, name: str | None = None, count: int | None = None) -> set[str]
```

## Operator with no custom properties

An operator without any custom property annotations should still be callable with
just the standard `execution_context` and `undo` parameters.

```toml
[environment]
extra-paths = ["/stubs"]
```

`/stubs/bpy/__init__.pyi`:

```pyi
from bpy import ops as ops
from bpy import props as props
from bpy import types as types
from bpy import utils as utils
```

`/stubs/bpy/types.pyi`:

```pyi
class Operator:
    bl_idname: str
    bl_label: str
```

`/stubs/bpy/props.pyi`:

```pyi
def IntProperty() -> int: ...
```

`/stubs/bpy/utils.pyi`:

```pyi
def register_class(cls: type) -> None: ...
```

`/stubs/bpy/ops/__init__.pyi`:

```pyi
```

`my_addon/__init__.py`:

```py
import bpy

class SimpleOperator(bpy.types.Operator):
    bl_idname = "wm.simple"
    bl_label = "Simple"

    def execute(self, context):
        return {"FINISHED"}

def register():
    bpy.utils.register_class(SimpleOperator)

def unregister():
    pass
```

`use_simple.py`:

```py
import bpy

def test():
    reveal_type(bpy.ops.wm.simple)  # revealed: (execution_context: int | str | None = None, undo: bool | None = None, /) -> set[str]
```
