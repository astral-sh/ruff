# High-Impact Type Changes for LSP Profiling

## Overview
This document contains recommended type changes for profiling LSP performance across 9 Python projects. Each change is designed to create maximum cascading effects across module boundaries, invalidating public interfaces and forcing widespread type re-checking.

## Projects and Recommended Changes

### 1. **black** - AST Node Type
- **File**: `bench/black/src/black/nodes.py:23`
- **Current**: `LN = Union[Leaf, Node]`
- **Change to**: `LN = Union[Leaf, Node, str]`  # Add incompatible type
- **Impact**: 29 uses across 6 core files
- **Why**: `LN` is the central type for all AST operations
- **Affected Files**:
  - `src/black/linegen.py` - all line generation functions
  - `src/black/trans.py` - transformation functions
  - `src/black/debug.py` - debugging utilities
  - `src/black/ranges.py` - range operations
  - `src/black/brackets.py` - bracket matching
  - `src/black/comments.py` - comment handling

### 2. **discord.py** - Sentinel Type
- **File**: `bench/discord.py/discord/utils.py:139`
- **Current**: `MISSING: Any = _MissingSentinel()`
- **Change to**: `MISSING: str = _MissingSentinel()`  # Narrow to incompatible type
- **Impact**: 288 uses across 20+ files
- **Why**: MISSING sentinel is used as default value throughout entire API
- **Affected Files**:
  - `discord/abc.py` - 20 uses in base classes
  - `discord/guild.py` - 114 uses in Guild methods
  - `discord/message.py` - 17 uses in Message class
  - `discord/member.py` - 13 uses in Member class
  - `discord/channel.py` - 17 uses in Channel classes
  - `discord/role.py` - 20 uses in Role class
  - `discord/app_commands/commands.py` - 34 uses
  - All modules using optional parameters

### 3. **homeassistant** - Callback Type
- **File**: `bench/homeassistant/homeassistant/core.py:126`
- **Current**: `type CALLBACK_TYPE = Callable[[], None]`
- **Change to**: `type CALLBACK_TYPE = Callable[[str], None]`  # Add required parameter
- **Impact**: Hundreds of uses across entire codebase
- **Why**: Used for all event listeners, state callbacks, async operations
- **Affected Files**:
  - All components registering callbacks
  - Event system (Event class, listeners)
  - State change handlers
  - Timer callbacks
  - Service callbacks
  - Config entry unload callbacks

### 4. **isort** - File Type Check Return
- **File**: `bench/isort/isort/settings.py`
- **Method**: `Config.is_supported_filetype()`
- **Current**: `def is_supported_filetype(self, file_name: str) -> bool:`
- **Change to**: `def is_supported_filetype(self, file_name: str) -> str:`
- **Impact**: Called before processing every file
- **Why**: Core file filtering logic
- **Affected Files**:
  - `isort/main.py` - file filtering logic
  - `isort/api.py` - public API methods
  - `isort/files.py` - file discovery
  - All file processing code

### 5. **jinja** - Template Render Return
- **File**: `bench/jinja/src/jinja2/environment.py`
- **Method**: `Template.render()`
- **Current**: `def render(self, *args: t.Any, **kwargs: t.Any) -> str:`
- **Change to**: `def render(self, *args: t.Any, **kwargs: t.Any) -> list[str]:`
- **Impact**: Every template usage
- **Why**: Core method of Jinja2
- **Affected Files**:
  - All files calling `template.render()`
  - `src/jinja2/loaders.py` - template loading
  - `src/jinja2/sandbox.py` - sandboxed rendering
  - Any code processing template output

### 6. **pandas** - Axis Type Alias
- **File**: `bench/pandas/pandas/_typing.py:187`
- **Current**: `Axis: TypeAlias = AxisInt | Literal["index", "columns", "rows"]`
- **Change to**: `Axis: TypeAlias = Literal["index", "columns", "rows"]`  # Remove int
- **Impact**: 511 uses across 20+ files
- **Why**: Nearly every DataFrame/Series operation takes axis parameter
- **Affected Files**:
  - `pandas/core/frame.py` - 137 uses in DataFrame
  - `pandas/core/series.py` - 83 uses in Series
  - `pandas/core/generic.py` - 146 uses in NDFrame
  - `pandas/core/indexes/base.py` - 8 uses
  - `pandas/core/nanops.py` - 25 uses
  - `pandas/core/algorithms.py` - 5 uses
  - 14+ more core files

### 7. **pandas-stubs** - DataFrame.loc Type
- **File**: `bench/pandas-stubs/pandas-stubs/core/frame.pyi`
- **Property**: `DataFrame.loc`
- **Current**: `@property def loc(self) -> _LocIndexer:`
- **Change to**: `@property def loc(self) -> str:`  # Completely wrong type
- **Impact**: Thousands of uses
- **Why**: Most frequently used pandas accessor
- **Affected Files**:
  - Every stub file using DataFrame indexing
  - All type-checked code using `.loc[]`

### 8. **prefect** - FlowRun ID Type
- **File**: `bench/prefect/src/prefect/server/schemas/core.py`
- **Field**: `FlowRun.flow_id`
- **Current**: `flow_id: UUID = Field(...)`
- **Change to**: `flow_id: str = Field(...)`  # Change from UUID to str
- **Impact**: Database-wide effect
- **Why**: Links runs to flows throughout API
- **Affected Files**:
  - `src/prefect/server/models/flow_runs.py` - database operations
  - `src/prefect/server/api/flow_runs.py` - API endpoints
  - `src/prefect/server/schemas/responses.py` - response serialization
  - Any code querying flow runs

### 9. **pytorch** - Device Type Alias
- **File**: `bench/pytorch/torch/types.py`
- **Current**: `Device: TypeAlias = Union[torch.device, str, int]`
- **Change to**: `Device: TypeAlias = str`  # Remove torch.device support
- **Impact**: Entire framework
- **Why**: Used for specifying tensor locations (CPU/GPU)
- **Affected Files**:
  - `torch/nn/modules/module.py` - all `to(device)` calls
  - `torch/tensor.py` - tensor creation and movement
  - `torch/cuda/__init__.py` - CUDA operations
  - Every neural network module

## Impact Summary

| Project | Type Changed | Usage Count | Files Affected | Impact Level |
|---------|-------------|-------------|----------------|--------------|
| **black** | `LN` type alias | 29 uses | 6 core files | 🔴 EXTREME |
| **discord.py** | `MISSING` sentinel type | 288 uses | 20+ files | 🔴 EXTREME |
| **homeassistant** | `CALLBACK_TYPE` type alias | Hundreds | Entire codebase | 🔴 EXTREME |
| **isort** | `is_supported_filetype()` return | Every file check | 4+ core files | 🟠 HIGH |
| **jinja** | `Template.render()` return | Every template use | All template users | 🔴 EXTREME |
| **pandas** | `Axis` type alias | 511 uses | 20+ core files | 🔴 EXTREME |
| **pandas-stubs** | `DataFrame.loc` type | Thousands | All indexing code | 🔴 EXTREME |
| **prefect** | `FlowRun.flow_id` type | Database-wide | 5+ core files | 🟠 HIGH |
| **pytorch** | `Device` type alias | Entire framework | 10+ core files | 🔴 EXTREME |

## Testing Approach

1. **Baseline**: Measure LSP diagnostic time with unchanged code
2. **Apply Change**: Make the single type change listed above
3. **Measure**: Time how long LSP takes to publish new diagnostics
4. **Compare**: Evaluate performance differences between:
   - Ruff
   - Pyright
   - MyPy
   - Pyrefly

## Key Characteristics

These changes share important properties:
- **Single Line Changes**: Each is a minimal modification
- **Type System Impact**: Affect type aliases, return types, or property types
- **Cross-Module Effects**: Invalidate public interfaces used across modules
- **Realistic**: Mimic actual API evolution patterns
- **Measurable**: Will generate clear, widespread type errors

## Clone Commands

To clone the projects at the specified revisions:

```bash
# Clone all projects
cd bench

# black
git clone https://github.com/psf/black && cd black && git checkout 45b4087976b7880db9dabacc992ee142f2d6c7c7 && cd ..

# discord.py
git clone https://github.com/Rapptz/discord.py.git && cd discord.py && git checkout 9be91cb093402f54a44726c7dc4c04ff3b2c5a63 && cd ..

# homeassistant
git clone https://github.com/home-assistant/core.git homeassistant && cd homeassistant && git checkout 10c12623bfc0b3a06ffaa88bf986f61818cfb8be && cd ..

# isort
git clone https://github.com/pycqa/isort && cd isort && git checkout ed501f10cb5c1b17aad67358017af18cf533c166 && cd ..

# jinja
git clone https://github.com/pallets/jinja && cd jinja && git checkout 5ef70112a1ff19c05324ff889dd30405b1002044 && cd ..

# pandas
git clone https://github.com/pandas-dev/pandas && cd pandas && git checkout 4d8348341bc4de2f0f90782ecef1b092b9418a19 && cd ..

# pandas-stubs
git clone https://github.com/pandas-dev/pandas-stubs && cd pandas-stubs && git checkout ad8cae5bc1f0bc87ce22b4d445e0700976c9dfb4 && cd ..

# prefect
git clone https://github.com/PrefectHQ/prefect.git && cd prefect && git checkout a3db33d4f9ee7a665430ae6017c649d057139bd3 && cd ..

# pytorch
git clone https://github.com/pytorch/pytorch.git && cd pytorch && git checkout be33b7faf685560bb618561b44b751713a660337 && cd ..
```