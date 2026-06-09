with open('crates/crabide-git/src/lib.rs', 'r') as f:
    lines = f.readlines()
depth = 0
for lineno, line in enumerate(lines, 1):
    opens = line.count('{')
    closes = line.count('}')
    depth += opens - closes
    if depth < 0:
        print(f'Line {lineno}: depth={depth} EXTRA CLOSE')
        break
    if opens != 0 or closes != 0:
        print(f'Line {lineno}: +{opens} -{closes} = depth {depth}')
print(f'Final depth: {depth}')
