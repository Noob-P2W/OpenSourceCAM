OpenSourceCAM — shop development package

1. Extract the whole ZIP into a folder on your shop computer.
2. Install Python 3.10 or newer once (python.org). No Rust or Node needed.
3. Windows: double-click Start-Shop.bat.
   Linux/macOS: run sh Start-Shop.sh.
4. Your browser opens automatically. Keep the host window open.
5. For a phone/tablet, connect to the same Wi-Fi and open the LAN address
   shown in the host window. Allow private-network access if the firewall asks.
6. Ctrl+C in the host window stops the server.

This is a development launcher, not yet a standalone installer. Only static app
files are served. Projects/calculations stay on each device. There is no shared
project storage and no CNC connection. This host has no login: use only a trusted
shop LAN, without router port forwarding.

Phone/tablet PWA installation and offline mode require trusted HTTPS. Plain LAN
HTTP is for browser testing. GitHub Pages provides HTTPS for remote tests.
For private HTTPS use an administrator-managed reverse proxy, or launch with
--cert certificate.pem --key private-key.pem using a certificate trusted on the
phone/tablet. Do not package private keys with the application.

Optional: --port 8081, --bind 127.0.0.1 (this computer only), --no-browser.
