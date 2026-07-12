from http.server import SimpleHTTPRequestHandler, HTTPServer
import os

class OKXHandler(SimpleHTTPRequestHandler):
    def do_GET(self):
        if self.path in ['', '/', '/okx', '/okx/']:
            self.send_response(200)
            self.send_header('Content-Type', 'text/html; charset=utf-8')
            self.end_headers()
            
            try:
                with open('/root/bittrade-v2-strategi/dashboard_okx.html', 'r', encoding='utf-8') as f:
                    html_content = f.read()
                
                header = ""
                try:
                    with open('/root/bittrade-v2-strategi/includes/header.html', 'r', encoding='utf-8') as f:
                        header = f.read()
                except Exception:
                    pass
                
                header = header.replace("BitTrade Engine", "BitTrade Bot OKX")
                header = header.replace("BitTrade Menu", "BitTrade Menu OKX")
                
                footer = ""
                try:
                    with open('/root/bittrade-v2-strategi/includes/footer.html', 'r', encoding='utf-8') as f:
                        footer = f.read()
                except Exception:
                    pass
                
                html_content = html_content.replace("<!-- INCLUDE HEADER -->", header)
                html_content = html_content.replace("<!-- INCLUDE FOOTER -->", footer)
                
                self.wfile.write(html_content.encode('utf-8'))
            except Exception as e:
                self.send_response(500)
                self.end_headers()
                self.wfile.write(f"Error: {str(e)}".encode('utf-8'))
                
        elif self.path == '/js/dashboard_okx.js':
            self.send_response(200)
            self.send_header('Content-Type', 'application/javascript')
            self.end_headers()
            try:
                with open('/root/bittrade-v2-strategi/js/dashboard_okx.js', 'rb') as f:
                    self.wfile.write(f.read())
            except Exception as e:
                self.wfile.write(b"")
        else:
            self.send_response(404)
            self.end_headers()

if __name__ == '__main__':
    server = HTTPServer(('127.0.0.1', 8091), OKXHandler)
    print("Serving OKX static files on port 8091...")
    server.serve_forever()
