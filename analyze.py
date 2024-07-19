import sys

source_counts = []
lines_counts = []
nodes_counts = []
scopes_counts = []
bindings_counts = []
definitions_counts = []
resolved_references_counts = []
unresolved_references_counts = []
globals_counts = []
resolved_names_counts = []
shadowed_bindings_counts = []

with open(sys.argv[1], 'r') as fp:
    for line in fp:
        if line.startswith('source'):
            source_counts.append(int(line.split()[1]))
        if line.startswith('lines'):
            lines_counts.append(int(line.split()[1]))
        if line.startswith('nodes'):
            nodes_counts.append(int(line.split()[1]))
        if line.startswith('scopes'):
            scopes_counts.append(int(line.split()[1]))
        if line.startswith('bindings'):
            bindings_counts.append(int(line.split()[1]))
        if line.startswith('definitions'):
            definitions_counts.append(int(line.split()[1]))
        if line.startswith('resolved_references'):
            resolved_references_counts.append(int(line.split()[1]))
        if line.startswith('unresolved_references'):
            unresolved_references_counts.append(int(line.split()[1]))
        if line.startswith('globals'):
            globals_counts.append(int(line.split()[1]))
        if line.startswith('resolved_names'):
            resolved_names_counts.append(int(line.split()[1]))
        if line.startswith('shadowed_bindings'):
            shadowed_bindings_counts.append(int(line.split()[1]))

    # Each line represents a file.
    # Let's compute (e.g.) the average number of bindings per line.
    print('Average number of nodes per file:', sum(nodes_counts) / len(lines_counts))
    print('Average number of scopes per file:', sum(scopes_counts) / len(lines_counts))
    print('Average number of bindings per file:', sum(bindings_counts) / len(lines_counts))
    print('Average number of definitions per file:', sum(definitions_counts) / len(lines_counts))
    print('Average number of resolved references per file:', sum(resolved_references_counts) / len(lines_counts))
    print('Average number of unresolved references per file:', sum(unresolved_references_counts) / len(lines_counts))
    print('Average number of globals per file:', sum(globals_counts) / len(lines_counts))
    print('Average number of resolved names per file:', sum(resolved_names_counts) / len(lines_counts))
    print('Average number of shadowed bindings per file:', sum(shadowed_bindings_counts) / len(lines_counts))

    print()

    print('Max nodes per file:', max(nodes_counts))
    print('Max scopes per file:', max(scopes_counts))
    print('Max bindings per file:', max(bindings_counts))
    print('Max definitions per file:', max(definitions_counts))
    print('Max resolved references per file:', max(resolved_references_counts))
    print('Max unresolved references per file:', max(unresolved_references_counts))
    print('Max globals per file:', max(globals_counts))
    print('Max resolved names per file:', max(resolved_names_counts))
    print('Max shadowed bindings per file:', max(shadowed_bindings_counts))

    print()

    # Let's compute (e.g.) the average number of bindings per line.
    print('Average number of nodes per byte:', sum(nodes_counts) / sum(source_counts))
    print('Average number of scopes per byte:', sum(scopes_counts) / sum(source_counts))
    print('Average number of bindings per byte:', sum(bindings_counts) / sum(source_counts))
    print('Average number of definitions per byte:', sum(definitions_counts) / sum(source_counts))
    print('Average number of resolved references per byte:', sum(resolved_references_counts) / sum(source_counts))
    print('Average number of unresolved references per byte:', sum(unresolved_references_counts) / sum(source_counts))
    print('Average number of globals per byte:', sum(globals_counts) / sum(source_counts))
    print('Average number of resolved names per byte:', sum(resolved_names_counts) / sum(source_counts))
    print('Average number of shadowed bindings per byte:', sum(shadowed_bindings_counts) / sum(source_counts))
