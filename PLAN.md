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
**Status:** In progress

- [x] Add `CheckOutput` enum with `Diagnostic` and `Hover` variants
- [ ] Update `match_file` to accept `&[CheckOutput]` instead of `&[Diagnostic]`
- [ ] Create `SortedCheckOutputs` similar to `SortedDiagnostics`
- [ ] Update matching logic to extract line numbers from CheckOutput variants

### 4. Add hover checking logic (lib.rs)
**Status:** Not started

- [ ] After type checking, for each hover assertion:
  - [ ] Find the target line (first non-assertion line after hover comment)
  - [ ] Calculate position from down arrow column
  - [ ] Call `hover(db, file, position)` from ty_ide
  - [ ] Create a "hover diagnostic" with the result
  - [ ] Add to diagnostics list for matching

### 5. Update matcher (matcher.rs)
**Status:** In progress

- [x] Add placeholder matching logic for `ParsedAssertion::Hover`
- [ ] Compute column position from comment range when matching
- [ ] Match against hover diagnostics by:
  - [ ] Position (must match exactly)
  - [ ] Type string (must match the assertion body)
- [ ] Handle `@Todo` metadata stripping (line 201-212)

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
