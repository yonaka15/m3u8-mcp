# Browser Automation MCP Server

A desktop application that combines browser automation capabilities with an MCP (Model Context Protocol) server, built with Tauri, React, and Rust.

## 🚀 Features

- **MCP Server**: Runs a Model Context Protocol server on port 37650
- **Browser Automation**: Control browsers programmatically via MCP tools
- **Cross-Platform**: Works on macOS, Windows, and Linux
- **Modern UI**: React-based interface with Tailwind CSS styling
- **Real-time Control**: Start/stop MCP server from the UI

## 📋 Prerequisites

- Node.js (v16 or higher)
- Rust (latest stable)
- npm/pnpm/yarn
- A modern web browser (Chrome, Firefox, Safari, etc.)

## 🛠️ Installation

1. Clone the repository:
```bash
git clone https://github.com/yourusername/browser-automation.git
cd browser-automation
```

2. Install dependencies:
```bash
npm install
```

3. Start the development server:
```bash
npm run tauri dev
```

## 🎮 Usage

### Starting the Application

1. Run the application in development mode:
```bash
npm run tauri dev
```

2. Click the "Run MCP Server" button in the UI to start the MCP server on port 37650

### Using the MCP Server

The MCP server implements the Streamable HTTP Transport protocol (2025-03-26 specification) and can be accessed by any MCP-compatible client.

#### Connect with Claude Desktop:
```bash
claude mcp add --transport http browser-automation http://localhost:37650/mcp
```

#### Initialize a session:
```bash
curl -X POST http://localhost:37650/mcp \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}}'
```

#### List available tools:
```bash
curl -X POST http://localhost:37650/mcp \
  -H "Content-Type: application/json" \
  -H "Mcp-Session-Id: YOUR_SESSION_ID" \
  -d '{"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}}'
```

#### Open a URL in browser:
```bash
curl -X POST http://localhost:37650/mcp \
  -H "Content-Type: application/json" \
  -H "Mcp-Session-Id: YOUR_SESSION_ID" \
  -d '{
    "jsonrpc": "2.0",
    "id": 3,
    "method": "tools/call",
    "params": {
      "name": "browser_navigate",
      "arguments": {
        "url": "https://www.google.com"
      }
    }
  }'
```

## 🔧 Available MCP Tools

### browser_navigate
Opens a URL in the system's default browser.

**Parameters:**
- `url` (string, required): The URL to open

**Example:**
```json
{
  "name": "browser_navigate",
  "arguments": {
    "url": "https://example.com"
  }
}
```

## 🏗️ Tech Stack

### Frontend
- **React 19** - UI framework
- **TypeScript** - Type safety
- **Tailwind CSS v4** - Styling
- **Vite** - Build tool

### Backend
- **Rust** - Core backend language
- **Tauri v2** - Desktop application framework
- **Axum** - Web framework for MCP server
- **Tokio** - Async runtime

### Protocol
- **MCP Streamable HTTP Transport** - Communication protocol
- **JSON-RPC 2.0** - Message format
- **Server-Sent Events (SSE)** - Real-time streaming

## 📁 Project Structure

```
browser-automation/
├── src/                      # React frontend
│   ├── App.tsx              # Main UI component with MCP controls
│   ├── App.css              # Tailwind CSS styles
│   └── main.tsx             # Application entry point
├── src-tauri/               # Rust backend
│   ├── src/
│   │   ├── lib.rs           # Tauri commands and server lifecycle
│   │   ├── mcp_server.rs    # MCP server implementation
│   │   └── simple_browser.rs # Browser automation logic
│   ├── Cargo.toml           # Rust dependencies
│   └── tauri.conf.json      # Tauri configuration
├── .claude/                 # Claude AI documentation
│   └── CLAUDE.md           # Project context for AI assistance
├── package.json            # Node.js dependencies
├── vite.config.ts          # Vite configuration
└── README.md              # This file
```

## 🔨 Build

Build the application for production:

```bash
npm run tauri build
```

The built application will be available in:
- **macOS**: `src-tauri/target/release/bundle/dmg/`
- **Windows**: `src-tauri/target/release/bundle/msi/`
- **Linux**: `src-tauri/target/release/bundle/appimage/`

## 🧪 Testing

Test the MCP server endpoints:

```bash
# Run the test script
./test-mcp-server.sh
```

Or manually test with curl commands as shown in the Usage section.

## 🚧 Roadmap

- [x] Basic MCP server implementation
- [x] Browser navigation tool
- [x] Session management
- [x] UI controls for server
- [ ] WebDriver/Selenium integration
- [ ] Advanced browser automation tools
  - [ ] Click elements
  - [ ] Type text
  - [ ] Take screenshots
  - [ ] Execute JavaScript
- [ ] X.com (Twitter) automation features
- [ ] Chrome extension integration
- [ ] Recording and playback of browser actions

## 🤝 Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/AmazingFeature`)
3. Commit your changes (`git commit -m 'Add some AmazingFeature'`)
4. Push to the branch (`git push origin feature/AmazingFeature`)
5. Open a Pull Request

## 📝 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## 🙏 Acknowledgments

- [Tauri](https://tauri.app/) - For the amazing desktop framework
- [MCP Specification](https://spec.modelcontextprotocol.io/) - For the protocol documentation
- [x-auto-liker](https://github.com/yonaka15/x-auto-liker) - Reference implementation for browser automation

## 📞 Support

For issues and questions:
- Open an issue on GitHub
- Check the [CLAUDE.md](.claude/CLAUDE.md) file for detailed technical documentation

---

**Note**: This is an MVP (Minimum Viable Product) implementation. Advanced browser automation features are planned for future releases.