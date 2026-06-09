with open(r"C:\Users\ericl\.cargo\registry\src\index.crates.io-1949cf8c6b5b557f\git2-0.21.0\src\submodule.rs", "r") as f:
    lines = f.readlines()
for i in range(233, 248):
    if i < len(lines):
        print(f"{i+1:4d}: {repr(lines[i])}")
