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
    <main className="container">
      <h1 className="text-3xl font-bold text-blue-600">
        CDP-MCP Server Control
      </h1>

      <div
        className="mcp-server-section"
        style={{
          margin: "2rem 0",
          padding: "1rem",
          border: "1px solid #ccc",
          borderRadius: "8px",
        }}
      >
        {mcpServerRunning && currentPort && (
          <div
            style={{
              marginBottom: "1.5rem",
              padding: "1rem",
              backgroundColor: "#f0f9ff",
              borderRadius: "0.5rem",
              border: "1px solid #0284c7",
            }}
          >
            <p
              style={{
                fontWeight: "600",
                marginBottom: "1rem",
                color: "#0c4a6e",
                fontSize: "1.1rem",
              }}
            >
              Connect with your preferred client:
            </p>

            {/* Claude Code */}
            <div style={{ marginBottom: "1rem" }}>
              <p
                style={{
                  fontWeight: "500",
                  marginBottom: "0.5rem",
                  color: "#0c4a6e",
                  fontSize: "0.9rem",
                }}
              >
                Claude Code:
              </p>
              <div style={{ position: "relative" }}>
                <code
                  style={{
                    display: "block",
                    padding: "0.75rem",
                    paddingRight: "3.5rem",
                    backgroundColor: "#1e293b",
                    color: "#94a3b8",
                    borderRadius: "0.25rem",
                    fontSize: "0.875rem",
                    fontFamily: "monospace",
                    whiteSpace: "nowrap",
                    overflowX: "auto",
                    textAlign: "left",
                  }}
                >
                  claude mcp add --transport http browser-automation
                  http://localhost:{currentPort}/mcp
                </code>
                <button
                  onClick={() => copyToClipboard("claude-code")}
                  style={{
                    position: "absolute",
                    top: "50%",
                    right: "0.5rem",
                    transform: "translateY(-50%)",
                    padding: "0.25rem 0.5rem",
                    backgroundColor:
                      copiedCommand === "claude-code" ? "#10b981" : "#3b82f6",
                    color: "white",
                    border: "none",
                    borderRadius: "0.25rem",
                    fontSize: "0.75rem",
                    cursor: "pointer",
                    transition: "background-color 0.2s",
                  }}
                >
                  {copiedCommand === "claude-code" ? "✓ Copied" : "Copy"}
                </button>
              </div>
            </div>

            {/* Claude Desktop */}
            <div style={{ marginBottom: "1rem" }}>
              <p
                style={{
                  fontWeight: "500",
                  marginBottom: "0.5rem",
                  color: "#0c4a6e",
                  fontSize: "0.9rem",
                }}
              >
                Claude Desktop (add to mcpServers in claude_desktop_config.json):
              </p>
              <div style={{ position: "relative" }}>
                <pre
                  style={{
                    display: "block",
                    padding: "0.75rem",
                    paddingRight: "3.5rem",
                    backgroundColor: "#1e293b",
                    color: "#94a3b8",
                    borderRadius: "0.25rem",
                    fontSize: "0.75rem",
                    fontFamily: "monospace",
                    overflowX: "auto",
                    margin: 0,
                    maxHeight: "150px",
                    overflowY: "auto",
                    textAlign: "left",
                  }}
                >
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
                  style={{
                    position: "absolute",
                    top: "0.5rem",
                    right: "0.5rem",
                    padding: "0.25rem 0.5rem",
                    backgroundColor:
                      copiedCommand === "claude-desktop"
                        ? "#10b981"
                        : "#3b82f6",
                    color: "white",
                    border: "none",
                    borderRadius: "0.25rem",
                    fontSize: "0.75rem",
                    cursor: "pointer",
                    transition: "background-color 0.2s",
                  }}
                >
                  {copiedCommand === "claude-desktop" ? "✓ Copied" : "Copy"}
                </button>
              </div>
            </div>

            {/* VS Code */}
            <div>
              <p
                style={{
                  fontWeight: "500",
                  marginBottom: "0.5rem",
                  color: "#0c4a6e",
                  fontSize: "0.9rem",
                }}
              >
                VS Code:
              </p>
              <div style={{ position: "relative" }}>
                <code
                  style={{
                    display: "block",
                    padding: "0.75rem",
                    paddingRight: "3.5rem",
                    backgroundColor: "#1e293b",
                    color: "#94a3b8",
                    borderRadius: "0.25rem",
                    fontSize: "0.875rem",
                    fontFamily: "monospace",
                    whiteSpace: "nowrap",
                    overflowX: "auto",
                    textAlign: "left",
                  }}
                >
                  code --add-mcp "&#123;&quot;name&quot;:&quot;browser-automation&quot;,&quot;url&quot;:&quot;http://localhost:{currentPort}/mcp&quot;&#125;"
                </code>
                <button
                  onClick={() => copyToClipboard("vscode")}
                  style={{
                    position: "absolute",
                    top: "0.5rem",
                    right: "0.5rem",
                    padding: "0.25rem 0.5rem",
                    backgroundColor:
                      copiedCommand === "vscode" ? "#10b981" : "#3b82f6",
                    color: "white",
                    border: "none",
                    borderRadius: "0.25rem",
                    fontSize: "0.75rem",
                    cursor: "pointer",
                    transition: "background-color 0.2s",
                  }}
                >
                  {copiedCommand === "vscode" ? "✓ Copied" : "Copy"}
                </button>
              </div>
            </div>
          </div>
        )}
        <div
          className="row"
          style={{ gap: "1rem", alignItems: "center", marginBottom: "1rem" }}
        >
          <label
            htmlFor="port-input"
            style={{
              fontWeight: "500",
              color: mcpServerRunning ? "#6b7280" : "#000",
            }}
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
            style={{
              padding: "0.5rem",
              borderRadius: "0.25rem",
              border: `1px solid ${
                mcpServerRunning
                  ? "#9ca3af"
                  : checkingPort
                    ? "#fbbf24"
                    : portAvailable === false
                      ? "#ef4444"
                      : portAvailable === true
                        ? "#10b981"
                        : "#ccc"
              }`,
              width: "100px",
              backgroundColor: mcpServerRunning ? "#e5e7eb" : "white",
              color: mcpServerRunning ? "#6b7280" : "#000",
              cursor: mcpServerRunning ? "not-allowed" : "text",
              opacity: mcpServerRunning ? 0.6 : 1,
              transition: "all 0.2s ease",
            }}
          />
          {!mcpServerRunning && !mcpServerMessage && (
            <span
              style={{
                fontSize: "0.875rem",
                color: checkingPort
                  ? "#fbbf24"
                  : portAvailable === false
                    ? "#ef4444"
                    : portAvailable === true
                      ? "#10b981"
                      : "#6b7280",
              }}
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
        <div className="row" style={{ gap: "1rem", alignItems: "center" }}>
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
            style={{
              backgroundColor: mcpServerRunning
                ? "#ef4444"
                : portAvailable === false || checkingPort || portInput === ""
                  ? "#9ca3af"
                  : "#10b981",
              color: "white",
              padding: "0.5rem 1.5rem",
              borderRadius: "0.5rem",
              fontWeight: "500",
              cursor:
                !mcpServerRunning &&
                (portAvailable === false || checkingPort || portInput === "")
                  ? "not-allowed"
                  : "pointer",
            }}
          >
            {mcpServerRunning ? "Stop MCP Server" : "Start MCP Server"}
          </button>
          <span
            className={`status-indicator ${mcpServerRunning ? "text-green-600" : "text-gray-500"}`}
          >
            Status:{" "}
            {mcpServerRunning
              ? `🟢 Running on port ${currentPort}`
              : "⭕ Stopped"}
          </span>
        </div>
        {mcpServerMessage && (
          <p className="mt-2 text-sm" style={{ marginTop: "0.5rem" }}>
            {mcpServerMessage}
          </p>
        )}
      </div>
    </main>
  );
}

export default App;