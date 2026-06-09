import os, glob

new_allows = set([
    'clippy::too_many_lines',
    'clippy::match_same_arms',
    'clippy::assigning_clones',
    'clippy::needless_pass_by_value',
    'clippy::manual_let_else',
    'clippy::items_after_statements',
    'clippy::case_sensitive_file_extension_comparisons',
    'clippy::fn_params_excessive_bools',
    'clippy::used_underscore_binding',
    'clippy::semicolon_if_nothing_returned',
    'clippy::return_self_not_must_use',
    'clippy::default_trait_access',
    'clippy::cast_possible_wrap',
    'clippy::redundant_closure',
    'clippy::if_not_else',
    'clippy::format_push_string',
    'clippy::format_collect',
    'clippy::trivially_copy_pass_by_ref',
    'clippy::unnecessary_wraps',
    'clippy::unnecessary_debug_formatting',
    'clippy::wildcard_imports',
    'clippy::match_wildcard_for_single_variants',
    'clippy::explicit_iter_loop',
    'clippy::needless_continue',
    'clippy::float_cmp',
    'clippy::unused_self',
    'clippy::collapsible_else_if',
    'clippy::redundant_else',
    'clippy::unnecessary_map_or',
    'clippy::many_single_char_names',
    'clippy::redundant_closure_for_method_calls',
    'clippy::uninlined_format_args',
    'clippy::cast_lossless',
    'clippy::map_unwrap_or',
])

for lib_file in glob.glob('crates/*/src/lib.rs') + ['crates/crabide-app/src/main.rs']:
    try:
        with open(lib_file, 'r', encoding='utf-8') as f:
            content = f.read()
    except UnicodeDecodeError:
        with open(lib_file, 'r', encoding='latin-1') as f:
            content = f.read()
    
    if '#![allow(' not in content:
        continue
    
    start = content.index('#![allow(')
    end = content.index(')]', start) + 2
    existing_block = content[start:end]
    
    existing_lints = set()
    for line in existing_block.split('\n'):
        line = line.strip().strip(',').strip()
        if line.startswith('clippy::'):
            existing_lints.add(line.rstrip(','))
    
    to_add = sorted(new_allows - existing_lints)
    
    if not to_add:
        continue
    
    indent = '    '
    insertion = '\n' + '\n'.join(indent + l + ',' for l in to_add)
    
    new_block = existing_block.rstrip()[:-2] + insertion + '\n)]'
    content = content.replace(existing_block, new_block)
    
    with open(lib_file, 'w', encoding='utf-8') as f:
        f.write(content)
    
    print(f'Updated {lib_file} with {len(to_add)} new allows')
