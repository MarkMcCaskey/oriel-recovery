"""Explicit allowlists keep local authoring data out of player packages."""
import hashlib
import pathlib
import shutil
import sys
root = pathlib.Path(__file__).resolve().parent.parent
out = root / 'dist'
out.mkdir(exist_ok=True)
if sys.argv[1] == 'web':
    for name in ('index.html', 'style.css', 'app.js', 'worker.js', 'icon.svg', 'manifest.webmanifest'):
        shutil.copy2(root / 'web' / name, out / name)
    shutil.copy2(root / 'target/wasm32-unknown-unknown/release/orbital_recovery.wasm', out / 'core.wasm')
    shutil.copy2(root / 'docs/model.md', out / 'README.txt')
    digest = hashlib.sha256(b''.join((out / name).read_bytes() for name in ('core.wasm', 'app.js', 'worker.js', 'index.html', 'style.css', 'README.txt'))).hexdigest()[:16]
    (out / 'sw.js').write_text((root / 'web/sw.js').read_text().replace('__BUILD__', digest))
    (out / '.nojekyll').touch()
    print(f'Static player built: {out}')
elif sys.argv[1] == 'native':
    player = out / 'player'
    player.mkdir(exist_ok=True)
    shutil.copy2(root / 'target/release/ground', player / 'ground')
    for name in ('archive', 'docs'):
        shutil.copytree(root / name, player / name, dirs_exist_ok=True)
    shutil.copy2(root / 'README.md', player / 'README.md')
    print(f'Native player built: {player}')
else:
    raise SystemExit('usage: package.py web|native')
