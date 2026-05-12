use insta_cmd::{assert_cmd_snapshot, get_cargo_bin};
use std::path::Path;
use std::process::{Command, Output};
use tempfile::tempdir;

fn ty_cmd() -> Command {
    let mut cmd = Command::new(get_cargo_bin("ty"));
    cmd.env_clear();
    cmd
}

fn python_cmd() -> Command {
    let mut cmd = Command::new("python3");
    cmd.env_clear();
    cmd
}

fn assert_successful_program_output(
    output: Output,
    runner: &str,
    example: &str,
    expected_stdout: &str,
) -> anyhow::Result<()> {
    assert!(
        output.status.success(),
        "{runner} failed for {example}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout)?,
        expected_stdout,
        "unexpected stdout from {runner} for {example}"
    );
    assert_eq!(
        String::from_utf8(output.stderr)?,
        "",
        "unexpected stderr from {runner} for {example}"
    );
    Ok(())
}

fn assert_matches_cpython(
    example_path: &Path,
    example: &str,
    expected_stdout: &str,
) -> anyhow::Result<()> {
    let cpython_output = python_cmd().arg(example_path).output()?;
    assert_successful_program_output(cpython_output, "CPython", example, expected_stdout)?;

    let ty_output = ty_cmd().arg("run").arg(example_path).output()?;
    assert_successful_program_output(ty_output, "`ty run`", example, expected_stdout)
}

#[test]
fn run_typed_loop_program() -> anyhow::Result<()> {
    let temp_dir = tempdir()?;
    let program = temp_dir.path().join("main.py");
    std::fs::write(
        &program,
        r#"
total: int = 0
index: int = 0
running: bool = index < 5

while running:
    total += index
    index += 1
    running = index < 5

print(total)
"#,
    )?;

    assert_cmd_snapshot!(
        ty_cmd().current_dir(temp_dir.path()).args(["run", "main.py"]),
        @"
    success: true
    exit_code: 0
    ----- stdout -----
    10

    ----- stderr -----
    "
    );

    Ok(())
}

#[test]
fn run_infers_unannotated_locals() -> anyhow::Result<()> {
    let temp_dir = tempdir()?;
    let program = temp_dir.path().join("main.py");
    std::fs::write(
        &program,
        r#"
total = 0
index = 0
running = index < 5

while running:
    total += index
    index += 1
    running = index < 5

print(total)
"#,
    )?;

    assert_cmd_snapshot!(
        ty_cmd().current_dir(temp_dir.path()).args(["run", "main.py"]),
        @"
    success: true
    exit_code: 0
    ----- stdout -----
    10

    ----- stderr -----
    "
    );

    Ok(())
}

#[test]
fn run_rejects_ty_diagnostics_before_codegen() -> anyhow::Result<()> {
    let temp_dir = tempdir()?;
    let program = temp_dir.path().join("main.py");
    std::fs::write(
        &program,
        r#"
value: int = "not an int"
print(value)
"#,
    )?;

    assert_cmd_snapshot!(
        ty_cmd().current_dir(temp_dir.path()).args(["run", "main.py"]),
        @r#"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    ty failed
      Cause: Failed to type-check `main.py` before compilation: found 1 diagnostic
    "#
    );

    Ok(())
}

#[test]
fn run_recursive_function_program() -> anyhow::Result<()> {
    let temp_dir = tempdir()?;
    let program = temp_dir.path().join("main.py");
    std::fs::write(
        &program,
        r#"
def factorial(value: int) -> int:
    if value <= 1:
        return 1
    else:
        return value * factorial(value - 1)

print(factorial(6))
"#,
    )?;

    assert_cmd_snapshot!(
        ty_cmd().current_dir(temp_dir.path()).args(["run", "main.py"]),
        @"
    success: true
    exit_code: 0
    ----- stdout -----
    720

    ----- stderr -----
    "
    );

    Ok(())
}

#[test]
fn run_elif_function_program() -> anyhow::Result<()> {
    let temp_dir = tempdir()?;
    let program = temp_dir.path().join("main.py");
    std::fs::write(
        &program,
        r#"
def classify(score: int) -> int:
    if score >= 90:
        return 2
    elif score >= 60:
        return 1
    else:
        return 0

print(classify(73))
"#,
    )?;

    assert_cmd_snapshot!(
        ty_cmd().current_dir(temp_dir.path()).args(["run", "main.py"]),
        @"
    success: true
    exit_code: 0
    ----- stdout -----
    1

    ----- stderr -----
    "
    );

    Ok(())
}

#[test]
fn run_infers_unannotated_function_and_method_returns() -> anyhow::Result<()> {
    let temp_dir = tempdir()?;
    let program = temp_dir.path().join("main.py");
    std::fs::write(
        &program,
        r#"
def twice(value: int):
    return value * 2

class Counter:
    def __init__(self, value: int) -> None:
        self.value = value

    def bumped(self, amount: int):
        return self.value + amount

counter = Counter(5)
print(twice(4))
print(counter.bumped(3))
"#,
    )?;

    assert_cmd_snapshot!(
        ty_cmd().current_dir(temp_dir.path()).args(["run", "main.py"]),
        @"
    success: true
    exit_code: 0
    ----- stdout -----
    8
    8

    ----- stderr -----
    "
    );

    Ok(())
}

#[test]
fn run_infers_unannotated_parameter_types_when_ty_is_concrete() -> anyhow::Result<()> {
    let temp_dir = tempdir()?;
    let program = temp_dir.path().join("main.py");
    std::fs::write(
        &program,
        r#"
def constant(value):
    return value

print(constant(4))
"#,
    )?;

    assert_cmd_snapshot!(
        ty_cmd().current_dir(temp_dir.path()).args(["run", "main.py"]),
        @r#"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    ty failed
      Cause: Failed to compile `main.py`
      Cause: `ty run` cannot compile dynamic type information for parameter `value` in function `constant`
    "#
    );

    Ok(())
}

#[test]
fn run_float_program() -> anyhow::Result<()> {
    let temp_dir = tempdir()?;
    let program = temp_dir.path().join("main.py");
    std::fs::write(
        &program,
        r#"
base: float = 3.5
factor: float = 2.0
result: float = base * factor
print(result)
"#,
    )?;

    assert_cmd_snapshot!(
        ty_cmd().current_dir(temp_dir.path()).args(["run", "main.py"]),
        @"
    success: true
    exit_code: 0
    ----- stdout -----
    7.0

    ----- stderr -----
    "
    );

    Ok(())
}

#[test]
fn run_string_program() -> anyhow::Result<()> {
    let temp_dir = tempdir()?;
    let program = temp_dir.path().join("main.py");
    std::fs::write(
        &program,
        r#"
prefix: str = "Hello, "
name: str = "ty"
message: str = prefix + name
print(message)
"#,
    )?;

    assert_cmd_snapshot!(
        ty_cmd().current_dir(temp_dir.path()).args(["run", "main.py"]),
        @"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello, ty

    ----- stderr -----
    "
    );

    Ok(())
}

#[test]
fn run_collection_program() -> anyhow::Result<()> {
    let temp_dir = tempdir()?;
    let program = temp_dir.path().join("main.py");
    std::fs::write(
        &program,
        r#"
values: list[int] = [4, 5, 6]
pair: tuple[int, int] = (7, 8)
scores: dict[str, int] = {"ty": 9}

print(values[1])
print(pair[0])
print(scores["ty"])
"#,
    )?;

    assert_cmd_snapshot!(
        ty_cmd().current_dir(temp_dir.path()).args(["run", "main.py"]),
        @"
    success: true
    exit_code: 0
    ----- stdout -----
    5
    7
    9

    ----- stderr -----
    "
    );

    Ok(())
}

#[test]
fn run_mutable_collection_program() -> anyhow::Result<()> {
    let temp_dir = tempdir()?;
    let program = temp_dir.path().join("main.py");
    std::fs::write(
        &program,
        r#"
values = [1, 2, 3]
labels = ["ty", "wasm"]
scores = {"ty": 1}
names = {"lang": "py"}

values[1] = 9
labels[0] = "ruff"
scores["ty"] = 7
names["lang"] = "wasm"

print(values[1])
print(labels[0])
print(scores["ty"])
print(names["lang"])
"#,
    )?;

    assert_cmd_snapshot!(
        ty_cmd().current_dir(temp_dir.path()).args(["run", "main.py"]),
        @"
    success: true
    exit_code: 0
    ----- stdout -----
    9
    ruff
    7
    wasm

    ----- stderr -----
    "
    );

    Ok(())
}

#[test]
fn run_mutable_collection_example_matches_cpython() -> anyhow::Result<()> {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let example = manifest_dir.join("examples/wasm_backend/mutable_collections/main.py");

    assert_matches_cpython(
        &example,
        "mutable_collections/main.py",
        "9\nruff\n7\nwasm\n",
    )
}

#[test]
fn run_for_range_len_and_append_program() -> anyhow::Result<()> {
    let temp_dir = tempdir()?;
    let program = temp_dir.path().join("main.py");
    std::fs::write(
        &program,
        r#"
values = [1]
values.append(2)
values.append(3)

total = 0
for index in range(len(values)):
    total += values[index]

print(len(values))
print(total)
"#,
    )?;

    assert_cmd_snapshot!(
        ty_cmd().current_dir(temp_dir.path()).args(["run", "main.py"]),
        @"
    success: true
    exit_code: 0
    ----- stdout -----
    3
    6

    ----- stderr -----
    "
    );

    Ok(())
}

#[test]
fn run_for_collection_program() -> anyhow::Result<()> {
    let temp_dir = tempdir()?;
    let program = temp_dir.path().join("main.py");
    std::fs::write(
        &program,
        r#"
values = [2, 4, 6]
list_total = 0
for value in values:
    list_total += value

pair = (3, 5)
tuple_total = 0
for value in pair:
    tuple_total += value

print(list_total)
print(tuple_total)
"#,
    )?;

    assert_cmd_snapshot!(
        ty_cmd().current_dir(temp_dir.path()).args(["run", "main.py"]),
        @"
    success: true
    exit_code: 0
    ----- stdout -----
    12
    8

    ----- stderr -----
    "
    );

    Ok(())
}

#[test]
fn run_nested_for_program() -> anyhow::Result<()> {
    let temp_dir = tempdir()?;
    let program = temp_dir.path().join("main.py");
    std::fs::write(
        &program,
        r#"
weights = [1, 2]
total = 0

for left in range(3):
    for weight in weights:
        total += left * weight

print(total)
"#,
    )?;

    assert_cmd_snapshot!(
        ty_cmd().current_dir(temp_dir.path()).args(["run", "main.py"]),
        @"
    success: true
    exit_code: 0
    ----- stdout -----
    9

    ----- stderr -----
    "
    );

    Ok(())
}

#[test]
fn run_for_dict_keys_program() -> anyhow::Result<()> {
    let temp_dir = tempdir()?;
    let program = temp_dir.path().join("main.py");
    std::fs::write(
        &program,
        r#"
scores = {"ty": 9, "wasm": 4}
key_chars = 0
total = 0

for key in scores:
    key_chars += len(key)
    total += scores[key]

print(key_chars)
print(total)
"#,
    )?;

    assert_cmd_snapshot!(
        ty_cmd().current_dir(temp_dir.path()).args(["run", "main.py"]),
        @"
    success: true
    exit_code: 0
    ----- stdout -----
    6
    13

    ----- stderr -----
    "
    );

    Ok(())
}

#[test]
fn run_string_collection_program() -> anyhow::Result<()> {
    let temp_dir = tempdir()?;
    let program = temp_dir.path().join("main.py");
    std::fs::write(
        &program,
        r#"
words = ["ty"]
words.append("wasm")

labels = {"tool": "ty", "target": "wasm"}
chars = 0
for word in words:
    chars += len(word)

print(words[1])
print(labels["target"])
print(chars)
"#,
    )?;

    assert_cmd_snapshot!(
        ty_cmd().current_dir(temp_dir.path()).args(["run", "main.py"]),
        @"
    success: true
    exit_code: 0
    ----- stdout -----
    wasm
    wasm
    6

    ----- stderr -----
    "
    );

    Ok(())
}

#[test]
fn run_string_tuple_program() -> anyhow::Result<()> {
    let temp_dir = tempdir()?;
    let program = temp_dir.path().join("main.py");
    std::fs::write(
        &program,
        r#"
pair: tuple[str, str] = ("ty", "wasm")
chars = 0
for word in pair:
    chars += len(word)

print(pair[1])
print(len(pair))
print(chars)
"#,
    )?;

    assert_cmd_snapshot!(
        ty_cmd().current_dir(temp_dir.path()).args(["run", "main.py"]),
        @"
    success: true
    exit_code: 0
    ----- stdout -----
    wasm
    2
    6

    ----- stderr -----
    "
    );

    Ok(())
}

#[test]
fn run_reads_local_text_files() -> anyhow::Result<()> {
    let temp_dir = tempdir()?;
    let program = temp_dir.path().join("main.py");
    let note = temp_dir.path().join("note.txt");
    std::fs::write(note, "hello from disk")?;
    std::fs::write(
        &program,
        r#"
from ty_extensions import read_text

message = read_text("note.txt")
print(message)
"#,
    )?;

    assert_cmd_snapshot!(
        ty_cmd().current_dir(temp_dir.path()).args(["run", "main.py"]),
        @"
    success: true
    exit_code: 0
    ----- stdout -----
    hello from disk

    ----- stderr -----
    "
    );

    Ok(())
}

#[test]
fn run_writes_local_text_files() -> anyhow::Result<()> {
    let temp_dir = tempdir()?;
    let program = temp_dir.path().join("main.py");
    std::fs::write(
        &program,
        r#"
from ty_extensions import read_text, write_text

write_text("note.txt", "hello from wasm")
print(read_text("note.txt"))
"#,
    )?;

    assert_cmd_snapshot!(
        ty_cmd().current_dir(temp_dir.path()).args(["run", "main.py"]),
        @"
    success: true
    exit_code: 0
    ----- stdout -----
    hello from wasm

    ----- stderr -----
    "
    );

    assert_eq!(
        std::fs::read_to_string(temp_dir.path().join("note.txt"))?,
        "hello from wasm"
    );

    Ok(())
}

#[test]
fn run_rejects_web_artifacts_for_local_filesystem_intrinsics() -> anyhow::Result<()> {
    let temp_dir = tempdir()?;
    let program = temp_dir.path().join("main.py");
    let note = temp_dir.path().join("note.txt");
    std::fs::write(note, "hello from disk")?;
    std::fs::write(
        &program,
        r#"
from ty_extensions import read_text

print(read_text("note.txt"))
"#,
    )?;

    assert_cmd_snapshot!(
        ty_cmd()
            .current_dir(temp_dir.path())
            .args(["run", "main.py", "--emit-web", "build/web", "--no-execute"]),
        @r#"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    ty failed
      Cause: Browser artifacts are not available for programs that call local filesystem intrinsics yet
    "#
    );

    Ok(())
}

#[test]
fn run_rejects_web_artifacts_for_local_filesystem_writes() -> anyhow::Result<()> {
    let temp_dir = tempdir()?;
    let program = temp_dir.path().join("main.py");
    std::fs::write(
        &program,
        r#"
from ty_extensions import write_text

write_text("note.txt", "hello from wasm")
"#,
    )?;

    assert_cmd_snapshot!(
        ty_cmd()
            .current_dir(temp_dir.path())
            .args(["run", "main.py", "--emit-web", "build/web", "--no-execute"]),
        @r#"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    ty failed
      Cause: Browser artifacts are not available for programs that call local filesystem intrinsics yet
    "#
    );

    Ok(())
}

#[test]
fn run_class_program() -> anyhow::Result<()> {
    let temp_dir = tempdir()?;
    let program = temp_dir.path().join("main.py");
    std::fs::write(
        &program,
        r#"
class Point:
    def __init__(self, x: int, y: int) -> None:
        self.x = x
        self.y = y

    def total(self) -> int:
        return self.x + self.y

class Measurement:
    def __init__(self, value: float) -> None:
        self.value = value

    def scaled(self, factor: float) -> float:
        return self.value * factor

point: Point = Point(3, 4)
measurement: Measurement = Measurement(2.5)
print(point.x + point.y)
print(measurement.value)
print(point.total())
print(measurement.scaled(2.0))
"#,
    )?;

    assert_cmd_snapshot!(
        ty_cmd().current_dir(temp_dir.path()).args(["run", "main.py"]),
        @"
    success: true
    exit_code: 0
    ----- stdout -----
    7
    2.5
    7
    5.0

    ----- stderr -----
    "
    );

    Ok(())
}

#[test]
fn run_mutable_class_program() -> anyhow::Result<()> {
    let temp_dir = tempdir()?;
    let program = temp_dir.path().join("main.py");
    std::fs::write(
        &program,
        r#"
class Point:
    def __init__(self, x: int, y: int) -> None:
        self.x = x
        self.y = y

    def total(self) -> int:
        return self.x + self.y

class Measurement:
    def __init__(self, value: float) -> None:
        self.value = value

    def scaled(self, factor: float) -> float:
        return self.value * factor

point = Point(3, 4)
measurement = Measurement(2.5)

point.x = 8
measurement.value = 3.5

print(point.total())
print(measurement.scaled(2.0))
"#,
    )?;

    assert_cmd_snapshot!(
        ty_cmd().current_dir(temp_dir.path()).args(["run", "main.py"]),
        @"
    success: true
    exit_code: 0
    ----- stdout -----
    12
    7.0

    ----- stderr -----
    "
    );

    Ok(())
}

#[test]
fn run_mutable_class_example_matches_cpython() -> anyhow::Result<()> {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let example = manifest_dir.join("examples/wasm_backend/mutable_classes/main.py");

    assert_matches_cpython(&example, "mutable_classes/main.py", "12\n7.0\n")
}

#[test]
fn run_keyword_call_program() -> anyhow::Result<()> {
    let temp_dir = tempdir()?;
    let program = temp_dir.path().join("main.py");
    std::fs::write(
        &program,
        r#"
def combine(left: int, right: int) -> int:
    return left * 10 + right

class Offset:
    def __init__(self, base: int, delta: int) -> None:
        self.base = base
        self.delta = delta

    def total(self, scale: int, extra: int) -> int:
        return (self.base + self.delta) * scale + extra

offset = Offset(delta=4, base=3)
print(combine(right=2, left=1))
print(offset.total(extra=5, scale=2))
"#,
    )?;

    assert_cmd_snapshot!(
        ty_cmd().current_dir(temp_dir.path()).args(["run", "main.py"]),
        @"
    success: true
    exit_code: 0
    ----- stdout -----
    12
    19

    ----- stderr -----
    "
    );

    Ok(())
}

#[test]
fn run_keyword_call_example_matches_cpython() -> anyhow::Result<()> {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let example = manifest_dir.join("examples/wasm_backend/keyword_calls/main.py");

    assert_matches_cpython(&example, "keyword_calls/main.py", "12\n19\n")
}

#[test]
fn run_default_argument_program() -> anyhow::Result<()> {
    let temp_dir = tempdir()?;
    let program = temp_dir.path().join("main.py");
    std::fs::write(
        &program,
        r#"
def weighted(value: int, factor: int = 2) -> int:
    return value * factor

class Offset:
    def __init__(self, base: int, delta: int = 4) -> None:
        self.base = base
        self.delta = delta

    def total(self, scale: int = 2, extra: int = 1) -> int:
        return (self.base + self.delta) * scale + extra

offset = Offset(3)
print(weighted(5))
print(weighted(value=5, factor=3))
print(offset.total())
print(offset.total(extra=5))
"#,
    )?;

    assert_cmd_snapshot!(
        ty_cmd().current_dir(temp_dir.path()).args(["run", "main.py"]),
        @"
    success: true
    exit_code: 0
    ----- stdout -----
    10
    15
    15
    19

    ----- stderr -----
    "
    );

    Ok(())
}

#[test]
fn run_default_argument_example_matches_cpython() -> anyhow::Result<()> {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let example = manifest_dir.join("examples/wasm_backend/default_arguments/main.py");

    assert_matches_cpython(&example, "default_arguments/main.py", "10\n15\n15\n19\n")
}

#[test]
fn run_string_comparison_program() -> anyhow::Result<()> {
    let temp_dir = tempdir()?;
    let program = temp_dir.path().join("main.py");
    std::fs::write(
        &program,
        r#"
left = "apple"
right = "banana"
score = 0

if left < right:
    score += 1
if left != right:
    score += 10
if right >= "banana":
    score += 100
if left == "apple":
    score += 1000

print(score)
"#,
    )?;

    assert_cmd_snapshot!(
        ty_cmd().current_dir(temp_dir.path()).args(["run", "main.py"]),
        @"
    success: true
    exit_code: 0
    ----- stdout -----
    1111

    ----- stderr -----
    "
    );

    Ok(())
}

#[test]
fn run_string_comparison_example_matches_cpython() -> anyhow::Result<()> {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let example = manifest_dir.join("examples/wasm_backend/string_comparisons/main.py");

    assert_matches_cpython(&example, "string_comparisons/main.py", "1111\n")
}

#[test]
fn run_reordered_class_constructor_program() -> anyhow::Result<()> {
    let temp_dir = tempdir()?;
    let program = temp_dir.path().join("main.py");
    std::fs::write(
        &program,
        r#"
class Pair:
    def __init__(self, left: int, right: int) -> None:
        self.right = right
        self.left = left
        self.alias = left

pair = Pair(4, 9)
print(pair.left)
print(pair.right)
print(pair.alias)
"#,
    )?;

    assert_cmd_snapshot!(
        ty_cmd().current_dir(temp_dir.path()).args(["run", "main.py"]),
        @"
    success: true
    exit_code: 0
    ----- stdout -----
    4
    9
    4

    ----- stderr -----
    "
    );

    Ok(())
}

#[test]
fn run_imported_function_program() -> anyhow::Result<()> {
    let temp_dir = tempdir()?;
    std::fs::write(
        temp_dir.path().join("helpers.py"),
        r#"
def scale(value: int) -> int:
    return value * 3
"#,
    )?;
    std::fs::write(
        temp_dir.path().join("main.py"),
        r#"
from helpers import scale

print(scale(4))
"#,
    )?;

    assert_cmd_snapshot!(
        ty_cmd().current_dir(temp_dir.path()).args(["run", "main.py"]),
        @"
    success: true
    exit_code: 0
    ----- stdout -----
    12

    ----- stderr -----
    "
    );

    Ok(())
}

#[test]
fn run_imported_function_alias_program() -> anyhow::Result<()> {
    let temp_dir = tempdir()?;
    std::fs::write(
        temp_dir.path().join("helpers.py"),
        r#"
def scale(value: int) -> int:
    return value * 3
"#,
    )?;
    std::fs::write(
        temp_dir.path().join("main.py"),
        r#"
from helpers import scale as boost

print(boost(4))
"#,
    )?;

    assert_cmd_snapshot!(
        ty_cmd().current_dir(temp_dir.path()).args(["run", "main.py"]),
        @"
    success: true
    exit_code: 0
    ----- stdout -----
    12

    ----- stderr -----
    "
    );

    Ok(())
}

#[test]
fn run_imported_function_example_matches_cpython() -> anyhow::Result<()> {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let example_dir = manifest_dir.join("examples/wasm_backend/imported_function");

    let cpython_output = python_cmd()
        .current_dir(&example_dir)
        .arg("main.py")
        .output()?;
    assert_successful_program_output(
        cpython_output,
        "CPython",
        "imported_function/main.py",
        "12\n",
    )?;

    let ty_output = ty_cmd()
        .current_dir(&example_dir)
        .args(["run", "main.py"])
        .output()?;
    assert_successful_program_output(ty_output, "`ty run`", "imported_function/main.py", "12\n")
}

#[test]
fn run_imported_function_alias_example_matches_cpython() -> anyhow::Result<()> {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let example_dir = manifest_dir.join("examples/wasm_backend/imported_function_alias");

    let cpython_output = python_cmd()
        .current_dir(&example_dir)
        .arg("main.py")
        .output()?;
    assert_successful_program_output(
        cpython_output,
        "CPython",
        "imported_function_alias/main.py",
        "12\n",
    )?;

    let ty_output = ty_cmd()
        .current_dir(&example_dir)
        .args(["run", "main.py"])
        .output()?;
    assert_successful_program_output(
        ty_output,
        "`ty run`",
        "imported_function_alias/main.py",
        "12\n",
    )
}

#[test]
fn run_relative_imported_function_program() -> anyhow::Result<()> {
    let temp_dir = tempdir()?;
    std::fs::create_dir(temp_dir.path().join("pkg"))?;
    std::fs::write(temp_dir.path().join("pkg/__init__.py"), "")?;
    std::fs::write(
        temp_dir.path().join("pkg/helpers.py"),
        r#"
def scale(value: int) -> int:
    return value * 3
"#,
    )?;
    std::fs::write(
        temp_dir.path().join("pkg/main.py"),
        r#"
from .helpers import scale

print(scale(4))
"#,
    )?;

    assert_cmd_snapshot!(
        ty_cmd()
            .current_dir(temp_dir.path())
            .args(["run", "pkg/main.py"]),
        @"
    success: true
    exit_code: 0
    ----- stdout -----
    12

    ----- stderr -----
    "
    );

    Ok(())
}

#[test]
fn run_relative_import_example_matches_cpython() -> anyhow::Result<()> {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let example_dir = manifest_dir.join("examples/wasm_backend/relative_imports");

    let cpython_output = python_cmd()
        .current_dir(&example_dir)
        .args(["-m", "pkg.main"])
        .output()?;
    assert_successful_program_output(
        cpython_output,
        "CPython",
        "relative_imports/pkg/main.py",
        "12\n",
    )?;

    let ty_output = ty_cmd()
        .current_dir(&example_dir)
        .args(["run", "pkg/main.py"])
        .output()?;
    assert_successful_program_output(
        ty_output,
        "`ty run`",
        "relative_imports/pkg/main.py",
        "12\n",
    )
}

#[test]
fn run_imported_scalar_constant_program() -> anyhow::Result<()> {
    let temp_dir = tempdir()?;
    std::fs::write(
        temp_dir.path().join("config.py"),
        r#"
LIMIT = 7
LABEL = "ty"
"#,
    )?;
    std::fs::write(
        temp_dir.path().join("main.py"),
        r#"
from config import LABEL, LIMIT

print(LIMIT)
print(LABEL)
"#,
    )?;

    assert_cmd_snapshot!(
        ty_cmd().current_dir(temp_dir.path()).args(["run", "main.py"]),
        @"
    success: true
    exit_code: 0
    ----- stdout -----
    7
    ty

    ----- stderr -----
    "
    );

    Ok(())
}

#[test]
fn run_imported_scalar_constant_example_matches_cpython() -> anyhow::Result<()> {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let example_dir = manifest_dir.join("examples/wasm_backend/imported_constants");

    let cpython_output = python_cmd()
        .current_dir(&example_dir)
        .arg("main.py")
        .output()?;
    assert_successful_program_output(
        cpython_output,
        "CPython",
        "imported_constants/main.py",
        "7\nty\n",
    )?;

    let ty_output = ty_cmd()
        .current_dir(&example_dir)
        .args(["run", "main.py"])
        .output()?;
    assert_successful_program_output(
        ty_output,
        "`ty run`",
        "imported_constants/main.py",
        "7\nty\n",
    )
}

#[test]
fn run_imported_class_program() -> anyhow::Result<()> {
    let temp_dir = tempdir()?;
    std::fs::write(
        temp_dir.path().join("shapes.py"),
        r#"
class Point:
    def __init__(self, x: int, y: int) -> None:
        self.x = x
        self.y = y

    def total(self) -> int:
        return self.x + self.y
"#,
    )?;
    std::fs::write(
        temp_dir.path().join("main.py"),
        r#"
from shapes import Point

point = Point(3, 4)
print(point.x + point.y)
print(point.total())
"#,
    )?;

    assert_cmd_snapshot!(
        ty_cmd().current_dir(temp_dir.path()).args(["run", "main.py"]),
        @"
    success: true
    exit_code: 0
    ----- stdout -----
    7
    7

    ----- stderr -----
    "
    );

    Ok(())
}

#[test]
fn run_imported_class_alias_program() -> anyhow::Result<()> {
    let temp_dir = tempdir()?;
    std::fs::write(
        temp_dir.path().join("shapes.py"),
        r#"
class Point:
    def __init__(self, x: int, y: int) -> None:
        self.x = x
        self.y = y

    def total(self) -> int:
        return self.x + self.y
"#,
    )?;
    std::fs::write(
        temp_dir.path().join("main.py"),
        r#"
from shapes import Point as Vec2

point = Vec2(3, 4)
print(point.x + point.y)
print(point.total())
"#,
    )?;

    assert_cmd_snapshot!(
        ty_cmd().current_dir(temp_dir.path()).args(["run", "main.py"]),
        @"
    success: true
    exit_code: 0
    ----- stdout -----
    7
    7

    ----- stderr -----
    "
    );

    Ok(())
}

#[test]
fn run_imported_class_example_matches_cpython() -> anyhow::Result<()> {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let example_dir = manifest_dir.join("examples/wasm_backend/imported_classes");

    let cpython_output = python_cmd()
        .current_dir(&example_dir)
        .arg("main.py")
        .output()?;
    assert_successful_program_output(
        cpython_output,
        "CPython",
        "imported_classes/main.py",
        "7\n7\n",
    )?;

    let ty_output = ty_cmd()
        .current_dir(&example_dir)
        .args(["run", "main.py"])
        .output()?;
    assert_successful_program_output(ty_output, "`ty run`", "imported_classes/main.py", "7\n7\n")
}

#[test]
fn run_imported_class_alias_example_matches_cpython() -> anyhow::Result<()> {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let example_dir = manifest_dir.join("examples/wasm_backend/imported_class_alias");

    let cpython_output = python_cmd()
        .current_dir(&example_dir)
        .arg("main.py")
        .output()?;
    assert_successful_program_output(
        cpython_output,
        "CPython",
        "imported_class_alias/main.py",
        "7\n7\n",
    )?;

    let ty_output = ty_cmd()
        .current_dir(&example_dir)
        .args(["run", "main.py"])
        .output()?;
    assert_successful_program_output(
        ty_output,
        "`ty run`",
        "imported_class_alias/main.py",
        "7\n7\n",
    )
}

#[test]
fn run_transitively_imported_function_program() -> anyhow::Result<()> {
    let temp_dir = tempdir()?;
    std::fs::write(
        temp_dir.path().join("math_utils.py"),
        r#"
def triple(value: int) -> int:
    return value * 3
"#,
    )?;
    std::fs::write(
        temp_dir.path().join("helpers.py"),
        r#"
from math_utils import triple

def scale(value: int) -> int:
    return triple(value) + 1
"#,
    )?;
    std::fs::write(
        temp_dir.path().join("main.py"),
        r#"
from helpers import scale

print(scale(4))
"#,
    )?;

    assert_cmd_snapshot!(
        ty_cmd().current_dir(temp_dir.path()).args(["run", "main.py"]),
        @"
    success: true
    exit_code: 0
    ----- stdout -----
    13

    ----- stderr -----
    "
    );

    Ok(())
}

#[test]
fn run_transitive_import_example_matches_cpython() -> anyhow::Result<()> {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let example_dir = manifest_dir.join("examples/wasm_backend/transitive_imports");

    let cpython_output = python_cmd()
        .current_dir(&example_dir)
        .arg("main.py")
        .output()?;
    assert_successful_program_output(
        cpython_output,
        "CPython",
        "transitive_imports/main.py",
        "13\n",
    )?;

    let ty_output = ty_cmd()
        .current_dir(&example_dir)
        .args(["run", "main.py"])
        .output()?;
    assert_successful_program_output(ty_output, "`ty run`", "transitive_imports/main.py", "13\n")
}

#[test]
fn run_emits_web_artifacts_without_executing() -> anyhow::Result<()> {
    let temp_dir = tempdir()?;
    let program = temp_dir.path().join("main.py");
    std::fs::write(
        &program,
        r#"
value: int = 42
print(value)
"#,
    )?;

    assert_cmd_snapshot!(
        ty_cmd()
            .current_dir(temp_dir.path())
            .args(["run", "main.py", "--emit-wasm", "build/program.wasm", "--emit-web", "build/web", "--no-execute"]),
        @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    "
    );

    assert!(temp_dir.path().join("build/program.wasm").is_file());
    assert!(temp_dir.path().join("build/web/program.wasm").is_file());

    let runtime = std::fs::read_to_string(temp_dir.path().join("build/web/runtime.js"))?;
    assert!(runtime.contains("WebAssembly.instantiate"));
    assert!(runtime.contains("print_i64"));
    assert!(runtime.contains("print_f64"));
    assert!(runtime.contains("str_const"));
    assert!(runtime.contains("dict_get_str_i64"));
    assert!(runtime.contains("object_get_i64"));
    assert!(runtime.contains("ref_len"));

    let index = std::fs::read_to_string(temp_dir.path().join("build/web/index.html"))?;
    assert!(index.contains("<pre id=\"output\"></pre>"));

    Ok(())
}

#[test]
fn run_pretty_prints_generated_wasm() -> anyhow::Result<()> {
    let temp_dir = tempdir()?;
    let program = temp_dir.path().join("main.py");
    std::fs::write(
        &program,
        r#"
value: int = 42
print(value)
"#,
    )?;

    let output = ty_cmd()
        .current_dir(temp_dir.path())
        .args(["run", "main.py", "--print-wasm", "--no-execute"])
        .output()?;
    assert!(
        output.status.success(),
        "`ty run --print-wasm` failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout)?;
    assert!(stdout.starts_with("(module"));
    assert!(stdout.contains("(import \"ty\" \"print_i64\""));
    assert!(stdout.contains("(export \"_start\""));
    assert_eq!(String::from_utf8(output.stderr)?, "");

    Ok(())
}

#[test]
fn run_wasm_backend_example_corpus() -> anyhow::Result<()> {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let examples = [
        ("class_point_sum.py", "7\n2.5\n7\n5.0\n"),
        ("classify_score.py", "1\n"),
        ("collections_indexing.py", "5\n7\n9\n"),
        ("factorial_six.py", "720\n"),
        ("float_scaling.py", "7.0\n"),
        ("for_collection_totals.py", "12\n8\n"),
        ("for_dict_keys.py", "6\n13\n"),
        ("nested_for_product_sum.py", "9\n"),
        (
            "object_list_average.py",
            "56\n2\nAverage age across 2 people: 28.0\n",
        ),
        ("for_range_len_append.py", "3\n6\n"),
        ("inferred_function_returns.py", "8\n8\n"),
        ("inferred_locals.py", "10\nHello, ty\n5\n"),
        ("recursive_factorial.py", "720\n"),
        ("reordered_constructor_fields.py", "4\n9\n4\n"),
        ("string_greeting.py", "Hello, ty\n"),
        ("string_collections.py", "wasm\nwasm\n6\n"),
        ("string_tuple_iteration.py", "wasm\n2\n6\n"),
        ("sum_to_ten.py", "55\n"),
    ];

    for (example, expected_stdout) in examples {
        assert_matches_cpython(
            &manifest_dir.join("examples/wasm_backend").join(example),
            example,
            expected_stdout,
        )?;
    }

    Ok(())
}

#[test]
fn run_wasm_backend_showcase_example_matches_cpython() -> anyhow::Result<()> {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let example_dir = manifest_dir.join("examples/wasm_backend/showcase");

    let cpython_output = python_cmd().arg(example_dir.join("main.py")).output()?;
    assert_successful_program_output(
        cpython_output,
        "CPython",
        "showcase/main.py",
        "ty wasm release\nty:ready-distinct\n36\n3.0\ncompiler\n15\n9\n24\n",
    )?;

    let ty_output = ty_cmd()
        .arg("run")
        .arg(example_dir.join("main.py"))
        .output()?;
    assert_successful_program_output(
        ty_output,
        "`ty run`",
        "showcase/main.py",
        "ty wasm release\nty:ready-distinct\n36\n3.0\ncompiler\n15\n9\n24\n",
    )
}
