with open('crates/crabide-git/src/lib.rs', 'r') as f:
    lines = f.readlines()
for i in range(865, 876):
    if i <= len(lines):
        print(f'{i:4d}: {repr(lines[i-1])}')