# RustyCode

AI-powered autonomous development framework built in Rust.

## Features

- **Multi-Provider LLM** — Anthropic, OpenAI, Google, Ollama, and more through a unified interface
- **Autonomous Mode** — Structured reasoning, task planning, and multi-step execution strategies
- **Terminal UI** — Full ratatui-based TUI with session management, themes, and skill plugins
- **Tool Framework** — File editing, bash execution, web fetching, LSP integration, and MCP support
- **Security First** — Permission system, path validation, secret sanitization, and pre-commit hooks

## Install

**macOS / Linux:**
RustyCode Installer
===================
Fetching latest release...
Downloading macos-arm64 binary...
Extracting...

Installed to /Users/nat/.local/bin/rustycode

Run 'rustycode --help' to get started.

**Windows (PowerShell):**


**Build from source:**


## Quick Start

[?1049h[?2004h[?1000h[?1002h[?1003h[?1015h[?1006h[2J
✅ Task completed successfully

# Hello World HTTP Server

Here are implementations in a few popular languages:

## Python

```python
from http.server import HTTPServer, BaseHTTPRequestHandler

class HelloHandler(BaseHTTPRequestHandler):
    def do_GET(self):
        self.send_response(200)
        self.send_header("Content-Type", "text/plain")
        self.end_headers()
        self.wfile.write(b"Hello, World!")

if __name__ == "__main__":
    server = HTTPServer(("localhost", 8080), HelloHandler)
    print("Serving on http://localhost:8080")
    server.serve_forever()
```

## Node.js

```javascript
const http = require("http");

const server = http.createServer((req, res) => {
  res.writeHead(200, { "Content-Type": "text/plain" });
  res.end("Hello, World!");
});

server.listen(8080, "localhost", () => {
  console.log("Serving on http://localhost:8080");
});
```

## Go

```go
package main

import (
	"fmt"
	"net/http"
)

func helloHandler(w http.ResponseWriter, r *http.Request) {
	w.Header().Set("Content-Type", "text/plain")
	w.WriteHeader(http.StatusOK)
	fmt.Fprint(w, "Hello, World!")
}

func main() {
	http.HandleFunc("/", helloHandler)
	fmt.Println("Serving on http://localhost:8080")
	http.ListenAndServe(":8080", nil)
}
```

---

### How to run

| Language | Command |
|----------|---------|
| Python | `python server.py` |
| Node.js | `node server.js` |
| Go | `go run server.go` |

### Test it

```bash
curl http://localhost:8080
# Output: Hello, World!
```

All three servers listen on **port 8080** and respond to any GET request with `Hello, World!`.

## Documentation

- [Getting Started](https://rustycode-ai.github.io/getting-started.html)
- [Configuration](https://rustycode-ai.github.io/configuration.html)
- [Manual](https://rustycode-ai.github.io/manual.html)
- [Tips & Tricks](https://rustycode-ai.github.io/tips-and-tricks.html)

## License

MIT
