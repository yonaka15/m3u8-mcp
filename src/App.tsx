import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./App.css";

function App() {
  const [mcpServerRunning, setMcpServerRunning] = useState(false);
  const [mcpServerMessage, setMcpServerMessage] = useState("");
  const [portInput, setPortInput] = useState<string>("37650");
  const [currentPort, setCurrentPort] = useState<number | null>(null);
  const [portAvailable, setPortAvailable] = useState<boolean | null>(null);
  const [checkingPort, setCheckingPort] = useState(false);
  const [copiedCommand, setCopiedCommand] = useState<
    "claude-code" | "claude-desktop" | "vscode" | null
  >(null);

  // Check MCP server status on mount
  useEffect(() => {
    checkMcpServerStatus();
    const port = parseInt(portInput);
    if (!isNaN(port)) {
      checkPortAvailability(port);
    }
  }, []);

  async function checkMcpServerStatus() {
    try {
      const status = await invoke<{ running: boolean; port: number | null }>(
        "get_mcp_server_status",
      );
      setMcpServerRunning(status.running);
      setCurrentPort(status.port);
      if (status.port) {
        setPortInput(status.port.toString());
      }
    } catch (error) {
      console.error("Failed to check MCP server status:", error);
    }
  }

  const checkPortAvailability = useCallback(
    async (portToCheck: number | null) => {
      // If port is null or NaN, mark as invalid
      if (portToCheck === null || isNaN(portToCheck)) {
        setPortAvailable(false);
        setMcpServerMessage("Please enter a valid port number");
        return;
      }

      // Don't check if server is running and using this port
      if (mcpServerRunning && currentPort === portToCheck) {
        setPortAvailable(true);
        setMcpServerMessage("");
        return;
      }

      // Validate port range (port 0 is not allowed)
      if (portToCheck === 0) {
        setPortAvailable(false);
        setMcpServerMessage("Port 0 is not allowed");
        return;
      }

      if (portToCheck < 1024 || portToCheck > 65535) {
        setPortAvailable(false);
        setMcpServerMessage("Port must be between 1024 and 65535");
        return;
      }

      setCheckingPort(true);
      setMcpServerMessage(""); // Clear message while checking

      try {
        const available = await invoke<boolean>("check_port_availability", {
          port: portToCheck,
        });
        setPortAvailable(available);
        // Don't set message here - let the status indicator show the availability
        setMcpServerMessage("");
      } catch (error) {
        console.error("Failed to check port availability:", error);
        setPortAvailable(null);
        setMcpServerMessage("");
      } finally {
        setCheckingPort(false);
      }
    },
    [mcpServerRunning, currentPort],
  );

  async function toggleMcpServer() {
    try {
      if (mcpServerRunning) {
        await invoke<string>("stop_mcp_server");
        setMcpServerMessage(""); // Clear message instead of showing redundant stop message
        setMcpServerRunning(false);
        setCurrentPort(null);
      } else {
        const port = parseInt(portInput);
        if (isNaN(port)) {
          setMcpServerMessage("Please enter a valid port number");
          return;
        }
        await invoke<string>("start_mcp_server", { port });
        setMcpServerMessage(""); // Clear message since status indicator shows the running state
        setMcpServerRunning(true);
        setCurrentPort(port);
      }
    } catch (error) {
      setMcpServerMessage(`Error: ${error}`);
    }
  }

  async function copyToClipboard(
    type: "claude-code" | "claude-desktop" | "vscode",
  ) {
    let textToCopy = "";

    switch (type) {
      case "claude-code":
        textToCopy = `claude mcp add --transport http browser-automation http://localhost:${currentPort}/mcp`;
        break;
      case "claude-desktop":
        const desktopConfig = {
          "browser-automation": {
            command: "npx",
            args: [
              "-y",
              "mcp-remote",
              `http://localhost:${currentPort}/mcp`,
            ],
          },
        };
        textToCopy = JSON.stringify(desktopConfig, null, 2);
        break;
      case "vscode":
        const vscodeConfig = {
          name: "browser-automation",
          url: `http://localhost:${currentPort}/mcp`
        };
        textToCopy = `code --add-mcp "${JSON.stringify(vscodeConfig).replace(/"/g, '\\"')}"`;
        break;
    }

    try {
      await navigator.clipboard.writeText(textToCopy);
      setCopiedCommand(type);
      setTimeout(() => setCopiedCommand(null), 2000);
    } catch (error) {
      console.error("Failed to copy to clipboard:", error);
    }
  }

  return (
    <main className="min-h-screen bg-gray-50 dark:bg-gray-900 py-12 px-4 sm:px-6 lg:px-8">
      <div className="max-w-3xl mx-auto">
        <h1 className="text-4xl font-bold text-center text-blue-600 dark:text-blue-400 mb-8">
          CDP-MCP Server Control
        </h1>

        <div className="bg-white dark:bg-gray-800 rounded-lg shadow-lg p-6">
          {mcpServerRunning && currentPort && (
            <div className="mb-6 p-4 bg-blue-50 dark:bg-blue-900/20 rounded-lg border border-blue-200 dark:border-blue-800">
              <p className="text-lg font-semibold mb-4 text-gray-800 dark:text-gray-200">
                Connect with your preferred client:
              </p>

              {/* Claude Code */}
              <div className="mb-4">
                <p className="text-sm font-medium mb-2 text-gray-700 dark:text-gray-300">
                  Claude Code:
                </p>
                <div className="relative">
                  <code className="block p-3 pr-20 bg-gray-900 dark:bg-black text-gray-300 rounded text-sm font-mono whitespace-nowrap overflow-x-auto">
                    claude mcp add --transport http browser-automation
                    http://localhost:{currentPort}/mcp
                  </code>
                  <button
                    onClick={() => copyToClipboard("claude-code")}
                    className={`absolute top-1/2 right-2 -translate-y-1/2 px-3 py-1 text-xs font-medium text-white rounded transition-colors ${
                      copiedCommand === "claude-code"
                        ? "bg-green-600 hover:bg-green-700"
                        : "bg-blue-600 hover:bg-blue-700"
                    }`}
                  >
                    {copiedCommand === "claude-code" ? "✓ Copied" : "Copy"}
                  </button>
                </div>
              </div>

              {/* Claude Desktop */}
              <div className="mb-4">
                <p className="text-sm font-medium mb-2 text-gray-700 dark:text-gray-300">
                  Claude Desktop (add to mcpServers in claude_desktop_config.json):
                </p>
                <div className="relative">
                  <pre className="block p-3 pr-20 bg-gray-900 dark:bg-black text-gray-300 rounded text-xs font-mono overflow-x-auto max-h-40 overflow-y-auto">
{`"browser-automation": {
  "command": "npx",
  "args": [
    "-y",
    "mcp-remote",
    "http://localhost:${currentPort}/mcp"
  ]
}`}
                  </pre>
                  <button
                    onClick={() => copyToClipboard("claude-desktop")}
                    className={`absolute top-2 right-2 px-3 py-1 text-xs font-medium text-white rounded transition-colors ${
                      copiedCommand === "claude-desktop"
                        ? "bg-green-600 hover:bg-green-700"
                        : "bg-blue-600 hover:bg-blue-700"
                    }`}
                  >
                    {copiedCommand === "claude-desktop" ? "✓ Copied" : "Copy"}
                  </button>
                </div>
              </div>

              {/* VS Code */}
              <div>
                <p className="text-sm font-medium mb-2 text-gray-700 dark:text-gray-300">
                  VS Code:
                </p>
                <div className="relative">
                  <code className="block p-3 pr-20 bg-gray-900 dark:bg-black text-gray-300 rounded text-sm font-mono whitespace-nowrap overflow-x-auto">
                    code --add-mcp "&#123;&quot;name&quot;:&quot;browser-automation&quot;,&quot;url&quot;:&quot;http://localhost:{currentPort}/mcp&quot;&#125;"
                  </code>
                  <button
                    onClick={() => copyToClipboard("vscode")}
                    className={`absolute top-1/2 right-2 -translate-y-1/2 px-3 py-1 text-xs font-medium text-white rounded transition-colors ${
                      copiedCommand === "vscode"
                        ? "bg-green-600 hover:bg-green-700"
                        : "bg-blue-600 hover:bg-blue-700"
                    }`}
                  >
                    {copiedCommand === "vscode" ? "✓ Copied" : "Copy"}
                  </button>
                </div>
              </div>
            </div>
          )}

          <div className="flex items-center gap-4 mb-4">
            <label
              htmlFor="port-input"
              className={`font-medium ${
                mcpServerRunning ? "text-gray-500" : "text-gray-700 dark:text-gray-300"
              }`}
            >
              Port:
            </label>
            <input
              id="port-input"
              type="text"
              value={portInput}
              onChange={(e) => {
                const value = e.target.value;
                setPortInput(value);

                // Parse the port and check availability
                const newPort = value === "" ? null : parseInt(value);
                checkPortAvailability(newPort);
              }}
              disabled={mcpServerRunning}
              placeholder="37650"
              className={`px-3 py-2 rounded-md border ${
                mcpServerRunning
                  ? "bg-gray-100 dark:bg-gray-700 text-gray-500 cursor-not-allowed border-gray-300 dark:border-gray-600"
                  : checkingPort
                    ? "border-yellow-500 focus:ring-yellow-500"
                    : portAvailable === false
                      ? "border-red-500 focus:ring-red-500"
                      : portAvailable === true
                        ? "border-green-500 focus:ring-green-500"
                        : "border-gray-300 dark:border-gray-600 focus:ring-blue-500"
              } focus:outline-none focus:ring-2 transition-colors`}
            />
            {!mcpServerRunning && !mcpServerMessage && (
              <span
                className={`text-sm ${
                  checkingPort
                    ? "text-yellow-600"
                    : portAvailable === false
                      ? "text-red-600"
                      : portAvailable === true
                        ? "text-green-600"
                        : "text-gray-600"
                }`}
              >
                {checkingPort
                  ? "Checking..."
                  : portAvailable === false
                    ? "❌ In use"
                    : portAvailable === true
                      ? "✅ Available"
                      : ""}
              </span>
            )}
          </div>

          <div className="flex items-center gap-4">
            <button
              onClick={toggleMcpServer}
              disabled={
                !mcpServerRunning &&
                (portAvailable === false || checkingPort || portInput === "")
              }
              className={`px-6 py-2 rounded-lg font-medium transition-colors ${
                mcpServerRunning
                  ? "bg-red-500 hover:bg-red-600 text-white"
                  : portAvailable === false || checkingPort || portInput === ""
                    ? "bg-gray-400 cursor-not-allowed text-gray-200"
                    : "bg-green-500 hover:bg-green-600 text-white"
              }`}
            >
              {mcpServerRunning ? "Stop MCP Server" : "Start MCP Server"}
            </button>
            <span
              className={`font-medium ${
                mcpServerRunning ? "text-green-600 dark:text-green-400" : "text-gray-500"
              }`}
            >
              Status:{" "}
              {mcpServerRunning
                ? `🟢 Running on port ${currentPort}`
                : "⭕ Stopped"}
            </span>
          </div>

          {mcpServerMessage && (
            <p className="mt-4 text-sm text-red-600 dark:text-red-400">
              {mcpServerMessage}
            </p>
          )}
        </div>
      </div>
    </main>
  );
}

export default App;