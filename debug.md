# Type Checker Debug Analysis

## Command Executed

```bash
cargo run --bin ty -- check --project ~/renkai-lab/playty -vv
```

## Key Functions Called During Type Checking

### 1. Entry Point and Setup

- `main()` in `crates/ty/src/main.rs` - Entry point that calls `run()`
- `run()` in `crates/ty/src/lib.rs` - Main function that:
    - Sets up Rayon for parallel processing
    - Parses CLI arguments
    - Routes to appropriate command handler

### 2. Check Command Processing

- `run_check()` in `crates/ty/src/lib.rs` - Handles the check command:
    - Sets up tracing/logging with verbosity level
    - Discovers project configuration
    - Creates `ProjectDatabase`
    - Initializes and runs the `MainLoop`

### 3. Main Loop and Orchestration

- `MainLoop::run()` - Starts the main checking loop
- `MainLoop::main_loop()` - Core event loop that:
    - Sends `CheckWorkspace` message
    - Spawns checking tasks on Rayon thread pool
    - Handles check completion and displays results
    - Manages watch mode events (if enabled)

### 4. Project-Level Checking

- `ProjectDatabase::check_with_reporter()` - Entry point for checking
- `Project::check()` in `crates/ty_project/src/lib.rs` - Orchestrates file checking:
    - Collects project files based on check mode
    - Spawns parallel tasks for each file using Rayon
    - Aggregates diagnostics
    - Reports progress via `IndicatifReporter`

### 5. File-Level Type Checking

- `check_file_impl()` in `crates/ty_project/src/lib.rs` - Checks individual files:
    - Reads source text
    - Parses the module
    - Collects parse errors and unsupported syntax errors
    - Calls type checking
    - Sorts diagnostics by position

### 6. Type Inference and Checking

- `check_types()` in `crates/ty_python_semantic/src/types.rs` - Main type checking entry:

    - Gets semantic index for the file
    - Iterates through all scopes in the file
    - Calls type inference for each scope
    - Collects semantic syntax errors
    - Checks suppressions

- `infer_scope_types()` in `crates/ty_python_semantic/src/types/infer.rs` - Core type inference:

    - Performs type inference for a specific scope
    - Returns diagnostics for type errors

### 7. Diagnostic Handling

- `IndicatifReporter` - Progress reporting during checking
- Diagnostic collection and sorting throughout the pipeline
- Final output formatting based on verbosity and output format settings
- Color output support with ANSI colors

## Key Features Observed

### Parallel Processing

- Uses Rayon for parallel file checking
- Each file is checked in a separate task
- Results are collected asynchronously

### Incremental Computation

- Uses Salsa database for caching/incremental computation
- Supports cancellation of outdated queries
- Memory usage can be reported with `TY_MEMORY_REPORT` environment variable

### Cancellation Support

- Supports Ctrl+C signal handling
- Query cancellation propagates through Salsa
- Graceful shutdown of pending tasks

### Watch Mode

- Can monitor file changes (not used in this example)
- Automatic re-checking on file modifications
- Clear screen between checks in watch mode

### Memory Management

- AST clearing for non-open files after checking
- Optional memory usage reporting
- Efficient handling of large codebases

## Execution Flow Summary

1. **CLI Parsing**: Parse command-line arguments and determine action
1. **Project Discovery**: Find project root and configuration files
1. **Database Setup**: Initialize Salsa database with project metadata
1. **Main Loop Start**: Begin event-driven checking process
1. **Parallel Checking**: Check all files in parallel using Rayon
1. **Type Inference**: Run type inference on each scope within files
1. **Diagnostic Collection**: Gather all errors, warnings, and info messages
1. **Result Display**: Format and display results based on verbosity settings
1. **Exit Status**: Return appropriate exit code based on findings

## Debug Output Analysis

From the verbose output, we can see:

- Version information and architecture details
- Project discovery process
- Python version detection (3.13)
- File indexing (1 file found)
- Checking duration (0.117s)
- Diagnostic output with source locations
- Rule information (undefined-reveal, revealed-type)
