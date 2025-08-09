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
      const status = await invoke<{ running: boolean; port: number | null }>("get_mcp_server_status");
      setMcpServerRunning(status.running);
      setCurrentPort(status.port);
      if (status.port) {
        setPortInput(status.port.toString());
      }
    } catch (error) {
      console.error("Failed to check MCP server status:", error);
    }
  }

  const checkPortAvailability = useCallback(async (portToCheck: number | null) => {
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
      const available = await invoke<boolean>("check_port_availability", { port: portToCheck });
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
  }, [mcpServerRunning, currentPort]);

  async function toggleMcpServer() {
    try {
      if (mcpServerRunning) {
        const message = await invoke<string>("stop_mcp_server");
        setMcpServerMessage("");  // Clear message instead of showing redundant stop message
        setMcpServerRunning(false);
        setCurrentPort(null);
      } else {
        const port = parseInt(portInput);
        if (isNaN(port)) {
          setMcpServerMessage("Please enter a valid port number");
          return;
        }
        const message = await invoke<string>("start_mcp_server", { port });
        setMcpServerMessage("");  // Clear message since status indicator shows the running state
        setMcpServerRunning(true);
        setCurrentPort(port);
      }
    } catch (error) {
      setMcpServerMessage(`Error: ${error}`);
    }
  }

  return (
    <main className="container">
      <h1 className="text-3xl font-bold text-blue-600">MCP Server Control</h1>

      <div className="mcp-server-section" style={{ margin: "2rem 0", padding: "1rem", border: "1px solid #ccc", borderRadius: "8px" }}>
        <div className="row" style={{ gap: "1rem", alignItems: "center", marginBottom: "1rem" }}>
          <label htmlFor="port-input" style={{ fontWeight: "500", color: mcpServerRunning ? "#6b7280" : "#000" }}>
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
                mcpServerRunning ? "#9ca3af" :
                checkingPort ? "#fbbf24" : 
                portAvailable === false ? "#ef4444" : 
                portAvailable === true ? "#10b981" : 
                "#ccc"
              }`,
              width: "100px",
              backgroundColor: mcpServerRunning ? "#e5e7eb" : "white",
              color: mcpServerRunning ? "#6b7280" : "#000",
              cursor: mcpServerRunning ? "not-allowed" : "text",
              opacity: mcpServerRunning ? 0.6 : 1,
              transition: "all 0.2s ease"
            }}
          />
          {!mcpServerRunning && !mcpServerMessage && (
            <span style={{ 
              fontSize: "0.875rem", 
              color: checkingPort ? "#fbbf24" : 
                     portAvailable === false ? "#ef4444" : 
                     portAvailable === true ? "#10b981" : 
                     "#6b7280"
            }}>
              {checkingPort ? "Checking..." : 
               portAvailable === false ? "❌ In use" : 
               portAvailable === true ? "✅ Available" : ""}
            </span>
          )}
        </div>
        <div className="row" style={{ gap: "1rem", alignItems: "center" }}>
          <button 
            onClick={toggleMcpServer}
            disabled={!mcpServerRunning && (portAvailable === false || checkingPort || portInput === "")}
            className={`px-6 py-2 rounded-lg font-medium transition-colors ${
              mcpServerRunning 
                ? "bg-red-500 hover:bg-red-600 text-white" 
                : portAvailable === false || checkingPort || portInput === ""
                ? "bg-gray-400 cursor-not-allowed text-gray-200"
                : "bg-green-500 hover:bg-green-600 text-white"
            }`}
            style={{ 
              backgroundColor: mcpServerRunning ? "#ef4444" : 
                              (portAvailable === false || checkingPort || portInput === "") ? "#9ca3af" : "#10b981",
              color: "white",
              padding: "0.5rem 1.5rem",
              borderRadius: "0.5rem",
              fontWeight: "500",
              cursor: (!mcpServerRunning && (portAvailable === false || checkingPort || portInput === "")) ? "not-allowed" : "pointer"
            }}
          >
            {mcpServerRunning ? "Stop MCP Server" : "Start MCP Server"}
          </button>
          <span className={`status-indicator ${mcpServerRunning ? "text-green-600" : "text-gray-500"}`}>
            Status: {mcpServerRunning ? `🟢 Running on port ${currentPort}` : "⭕ Stopped"}
          </span>
        </div>
        {mcpServerMessage && (
          <p className="mt-2 text-sm" style={{ marginTop: "0.5rem" }}>{mcpServerMessage}</p>
        )}
      </div>
    </main>
  );
}

export default App;
