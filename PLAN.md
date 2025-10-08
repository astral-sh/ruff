# Hover Assertions in Mdtest Framework

## Goal

Add support for hover assertions in the mdtest framework. These assertions will verify the inferred type of an expression at a specific position, similar to how hover works in a language server.

## Current Architecture

### ty_ide hover tests
- Use `<CURSOR>` marker in test source code (hover.rs:161-167)
- Call `hover(db, file, offset)` to get type at cursor position (hover.rs:12-50)
- Use `find_goto_target()` to find the covering AST node (hover.rs:14)
- Return the inferred type via `SemanticModel` (hover.rs:22-23)

### mdtest framework
- Parses markdown files with Python code blocks (parser.rs)
- Supports two assertion types via comments (assertion.rs:242-248):
  - `# error:` - matches diagnostics
  - `# revealed:` - matches revealed-type diagnostics
- Assertions can be:
  - End-of-line: `x: int = "foo"  # error: [invalid-assignment]`
  - Preceding line: `# error: [invalid-assignment]\nx: int = "foo"`
- Matcher logic in matcher.rs compares assertions against diagnostics

## Implementation Plan

### 1. Add new assertion type (assertion.rs)
**Status:** ✅ Completed

- [x] Add `Hover(&'a str)` variant to `UnparsedAssertion` enum (line 242)
- [x] Update `from_comment()` to recognize `# hover:` and `# ↓ hover:` (line 253)
- [x] Add `Hover(HoverAssertion<'a>)` to `ParsedAssertion` enum (line 294)
- [x] Add `HoverAssertion` struct with column and expected_type fields
- [x] Add `HoverAssertionParseError` enum
- [x] Update parsing logic to validate hover assertions (line 267)

### 2. Extend assertion parsing to capture column position
**Status:** ✅ Simplified approach

- [x] Hover assertions MUST be preceding-line only (not end-of-line)
- [x] Down arrow must appear immediately before `hover` keyword (e.g., `# ↓ hover:`)
- [x] Column position determined by whitespace before `#` in comment
- [x] Calculate TextSize offset from: (target_line_start + down_arrow_column)

### 3. Create CheckOutput enum (matcher.rs)
**Status:** ✅ Completed

- [x] Add `CheckOutput` enum with `Diagnostic` and `Hover` variants
- [x] Update `match_file` to accept `&[CheckOutput]` instead of `&[Diagnostic]`
- [x] Create `SortedCheckOutputs` similar to `SortedDiagnostics`
- [x] Update matching logic to extract line numbers from CheckOutput variants
- [x] Implement `Unmatched` trait for `CheckOutput`
- [x] Update lib.rs to convert diagnostics to `CheckOutput` before matching

### 4. Add hover checking logic (lib.rs)
**Status:** ✅ Completed

- [x] Add find_covering_node() to locate AST nodes at positions
- [x] Add infer_type_at_position() using ty_python_semantic (NOT ty_ide)
- [x] Add generate_hover_outputs() to scan for hover assertions
- [x] Calculate position from down arrow column in comment
- [x] Create CheckOutput::Hover with inferred type
- [x] Integrate into check flow before match_file()

### 5. Update matcher (matcher.rs)
**Status:** ✅ Completed

- [x] Add placeholder matching logic for `ParsedAssertion::Hover`
- [x] Implement actual hover matching logic
- [x] Match hover outputs by comparing inferred type with expected type
- [x] Handle `@Todo` metadata stripping in hover assertions

### 6. Add tests
**Status:** Not started

- [ ] Add unit tests for hover assertion parsing
- [ ] Create mdtest file with hover assertion examples
- [ ] Test edge cases (no hover content, wrong position, etc.)

## Key Design Decisions

1. **Preceding-line only**: Hover assertions make sense only as preceding-line comments, since we need to identify both line and column via the down arrow.

2. **Down arrow syntax**: `# ↓ hover: int` where the arrow column identifies the hover position. This is intuitive and visual.

3. **Reuse diagnostic infrastructure**: By converting hover results to diagnostics, we leverage the existing matcher framework rather than creating parallel logic.

4. **Similar to revealed assertions**: The implementation will closely mirror the `revealed:` assertion logic, as both check inferred types.

## Example Usage

```python
# Test basic type inference
a = 10
    # ↓ hover: Literal[10]
    a

# Test function type
def foo() -> int: ...
       # ↓ hover: def foo() -> int
       foo
```

## Files to Modify

1. `crates/ty_test/src/assertion.rs` - Add `Hover` assertion type
2. `crates/ty_test/src/lib.rs` - Add hover checking logic
3. `crates/ty_test/src/matcher.rs` - Add hover matching logic
4. `crates/ty_test/src/diagnostic.rs` - Add hover diagnostic type (if needed)
5. Tests in existing mdtest files to validate

## Progress Log

- **2025-10-08**: Initial plan created based on codebase analysis
- **2025-10-08**: Completed step 1 - Added hover assertion type to assertion.rs
  - Added `Hover` variant to `UnparsedAssertion` and `ParsedAssertion` enums
  - Created `HoverAssertion` struct and `HoverAssertionParseError` enum
  - Updated `from_comment()` to recognize `# hover:` and `# ↓ hover:` patterns
  - Simplified approach: down arrow must appear immediately before `hover` keyword
  - Added placeholder matching logic in matcher.rs (TODO: implement once diagnostics ready)
  - ty_test compiles successfully with warnings (unused code, expected at this stage)
- **2025-10-08**: Completed step 3 - Created CheckOutput enum infrastructure
  - Decided NOT to add HoverType to DiagnosticId (keep test logic separate)
  - Created `CheckOutput` enum with `Diagnostic` and `Hover` variants
  - Implemented `SortedCheckOutputs` to handle sorting/grouping by line
  - Updated entire matcher module to work with `CheckOutput` instead of `Diagnostic`
  - Updated lib.rs to convert diagnostics to `CheckOutput` before matching
  - All changes compile successfully
- **2025-10-08**: Completed steps 4 & 5 - Implemented hover type inference and matching
  - Avoided adding ty_ide dependency; used ty_python_semantic directly
  - Implemented find_covering_node() using AST visitor pattern
  - Implemented infer_type_at_position() using HasType trait on AST nodes
  - Implemented generate_hover_outputs() to scan comments and generate hover results
  - Integrated hover outputs into check flow
  - Implemented hover matching logic comparing inferred vs expected types
  - **All core functionality now complete and compiling!**
