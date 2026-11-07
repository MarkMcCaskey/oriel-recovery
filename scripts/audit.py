"""Audit Git's publication candidate list without reading private authoring files."""
from pathlib import Path
import re
import subprocess
root = Path(__file__).resolve().parent.parent
files = subprocess.check_output(['git', 'ls-files', '--cached', '--others', '--exclude-standard', '-z'], cwd=root).decode().split('\0')
files = sorted(set(f for f in files if f))
allowed_dirs = {'src', 'tests', 'archive', 'assets', 'web', 'scripts', 'docs', '.github'}
allowed_root = {'Cargo.toml', 'Cargo.lock', 'build.rs', 'README.md', 'LICENSE', '.gitignore'}
errors = []
for name in files:
    p = Path(name)
    if (len(p.parts) == 1 and name not in allowed_root) or (len(p.parts) > 1 and p.parts[0] not in allowed_dirs):
        errors.append(f'Unexpected publication path: {name}')
    if any(part in {'.private', '.local', 'target', 'saves', 'node_modules', '.env'} for part in p.parts) or p.suffix in {'.sav', '.session'}:
        errors.append(f'Local state selected for publication: {name}')
    path = root / p
    if path.is_file() and path.suffix not in {'.bin', '.orb'}:
        data = path.read_text(errors='replace')
        if re.search(r'(?:gh[pousr]_[A-Za-z0-9]{30,}|AKIA[A-Z0-9]{16}|-----BEGIN (?:RSA |OPENSSH )?PRIVATE KEY-----)', data):
            errors.append(f'Potential credential in {name}')
for private in ('.private/', '.local/', 'target/', 'dist/'):
    r = subprocess.run(['git', 'check-ignore', '--quiet', private], cwd=root)
    if r.returncode != 0:
        errors.append(f'Missing exclusion: {private}')
if errors:
    raise SystemExit('\n'.join(errors))
print(f'Publication audit passed: {len(files)} allowed files; local authoring and session data excluded.')
