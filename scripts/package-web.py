"""Package the already-built PWA for static hosting and shop development."""
from pathlib import Path
import shutil
import hashlib

ROOT = Path(__file__).resolve().parent.parent
out = ROOT / 'dist'
site = out / 'pwa'
site.mkdir(parents=True, exist_ok=True)
files = ['index.html', 'style.css', 'app.js', 'worker.js', 'sw.js', 'manifest.webmanifest',
         'icon.svg', 'icon-192.png', 'icon-512.png']
if not (ROOT / 'web/pkg/opensourcecam_bg.wasm').is_file():
    raise SystemExit('Run scripts/build-web.sh first.')
for name in files:
    shutil.copy2(ROOT / 'web' / name, site / name)
shutil.copytree(ROOT / 'web/pkg', site / 'pkg', dirs_exist_ok=True)
# Content-derived versions change whenever a deployed asset changes.
digest = hashlib.sha256()
for path in sorted(site.rglob('*')):
    if path.is_file() and path.name != 'build.js':
        digest.update(str(path.relative_to(site)).encode())
        digest.update(path.read_bytes())
version = digest.hexdigest()[:12]
(site / 'build.js').write_text(f"export const BUILD_ID = '{version}';\n")
worker = site / 'sw.js'
worker.write_text(worker.read_text().replace('__BUILD_ID__', version))
shop = out / 'shop'
shop.mkdir(exist_ok=True)
shutil.copytree(site, shop / 'site', dirs_exist_ok=True)
for name in ['serve.py', 'Start-Shop.bat', 'Start-Shop.sh', 'README.txt']:
    shutil.copy2(ROOT / 'shop' / name, shop / name)
shutil.make_archive(str(out / 'opensourcecam-pwa'), 'zip', site)
shutil.make_archive(str(out / 'opensourcecam-shop'), 'zip', shop)
print('Created dist/opensourcecam-pwa.zip and dist/opensourcecam-shop.zip')
