"""Small static-only shop development host. No project uploads or machine control."""
import argparse
import functools
import http.server
import ipaddress
from pathlib import Path
import socket
import ssl
import sys
import threading
import webbrowser


class Handler(http.server.SimpleHTTPRequestHandler):
    extensions_map = {**http.server.SimpleHTTPRequestHandler.extensions_map, '.wasm': 'application/wasm'}

    def translate_path(self, path):
        candidate = Path(super().translate_path(path)).resolve()
        root = Path(self.directory).resolve()
        # Do not serve files reached through links outside the site root.
        if not candidate.is_relative_to(root):
            return str(root / '.not-found')
        return str(candidate)

    def list_directory(self, path):
        self.send_error(404)
        return None

    def end_headers(self):
        self.send_header('Cache-Control', 'no-cache')
        self.send_header('X-Content-Type-Options', 'nosniff')
        super().end_headers()


def main():
    base = Path(sys.executable).parent if getattr(sys, 'frozen', False) else Path(__file__).resolve().parent
    parser = argparse.ArgumentParser(description='OpenSourceCAM shop development host')
    parser.add_argument('--site', type=Path, default=base / 'site')
    parser.add_argument('--bind', default='0.0.0.0')
    parser.add_argument('--port', type=int, default=8080)
    parser.add_argument('--no-browser', action='store_true')
    parser.add_argument('--cert', type=Path, help='Optional PEM certificate trusted by client devices')
    parser.add_argument('--key', type=Path, help='PEM private key for --cert')
    args = parser.parse_args()
    if bool(args.cert) != bool(args.key):
        parser.error('--cert and --key must be supplied together')
    if not (args.site / 'pkg/opensourcecam_bg.wasm').is_file():
        parser.error('Built website is missing. Extract the complete shop package first.')
    try:
        server = http.server.ThreadingHTTPServer((args.bind, args.port), functools.partial(Handler, directory=str(args.site.resolve())))
        if args.cert:
            context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
            context.load_cert_chain(args.cert, args.key)
            server.socket = context.wrap_socket(server.socket, server_side=True)
    except (OSError, ssl.SSLError) as error:
        parser.exit(1, f'Cannot start shop host: {error}\nClose another host using this port, or use --port 8081.\n')
    scheme = 'https' if args.cert else 'http'
    port = server.server_port
    print('\nOpenSourceCAM shop host is running.', flush=True)
    print(f'On this computer: {scheme}://localhost:{port}/', flush=True)
    if args.bind not in ('127.0.0.1', 'localhost'):
        addresses = {a[4][0] for a in socket.getaddrinfo(socket.gethostname(), None, socket.AF_INET)}
        for address in sorted(addresses):
            if not ipaddress.ip_address(address).is_loopback:
                print(f'On the same shop Wi-Fi: {scheme}://{address}:{port}/', flush=True)
        print('Use this computer’s LAN IP if no suitable Wi-Fi address is shown.', flush=True)
    if not args.cert:
        print('LAN HTTP supports browser testing. Phone installation/offline mode needs trusted HTTPS.', flush=True)
    print('Keep this window open. Press Ctrl+C to stop. This host has no login; use a trusted shop network.', flush=True)
    if not args.no_browser:
        threading.Timer(0.5, lambda: webbrowser.open(f'{scheme}://localhost:{port}/')).start()
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()


if __name__ == '__main__':
    main()
