import os

def refactor_rust_code(code):
    pattern_start = 'if settings.rules.should_fix(diagnostic.kind.rule()) {'
    i = 0
    output = []
    while i < len(code):
        start_idx = code.find(pattern_start, i)
        # If pattern is not found, append the rest of the code and break
        if start_idx == -1:
            output.append(code[i:])
            break
        output.append(code[i:start_idx])  # Append content before the pattern
        
        i = start_idx + len(pattern_start)
        brace_count = 1
        content_start_idx = i
        while i < len(code) and brace_count > 0:
            if code[i] == '{':
                brace_count += 1
            elif code[i] == '}':
                brace_count -= 1
            i += 1
        
        if brace_count == 0:  # Found a matching closing brace
            output.append(code[content_start_idx:i-1].strip())  # Append the body content without the closing brace

    return ''.join(output)

def refactor_codebase(directory):
    # Traverse through all files in the directory
    for root, _, files in os.walk(directory):
        for file in files:
            # Only process Rust files
            if file.endswith(".rs"):
                file_path = os.path.join(root, file)

                with open(file_path, 'r') as f:
                    code = f.read()

                refactored_code = refactor_rust_code(code)

                # Save the refactored code back to the file
                with open(file_path, 'w') as f:
                    f.write(refactored_code)

if __name__ == "__main__":
    directory = "./crates/ruff_linter/src/rules"
    refactor_codebase(directory)
