with open('crates/crabide-git/src/lib.rs', 'r') as f:
    content = f.read()
depth = 0
for i, ch in enumerate(content):
    if ch == '{':
        depth += 1
    elif ch == '}':
        depth -= 1
    if depth < 0:
        print(f'Extra closing brace at position {i} (line {content[:i].count(chr(10))+1})')
        break
print(f'Final depth: {depth}')
