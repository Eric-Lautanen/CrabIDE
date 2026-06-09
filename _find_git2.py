import subprocess, json, sys
result = subprocess.run(['cargo', 'metadata', '--no-deps', '--format-version', '1'], capture_output=True, text=True)
data = json.loads(result.stdout)
for p in data['packages']:
    if p['name'] == 'git2':
        print(p['version'])
        print(p['manifest_path'])
