import { useState, useEffect } from "react";
import reactLogo from "./assets/react.svg";
import { invoke } from "@tauri-apps/api/core";
import "./App.css";

function App() {
  const [greetMsg, setGreetMsg] = useState("");
  const [name, setName] = useState("");
  const [mcpServerRunning, setMcpServerRunning] = useState(false);
  const [mcpServerMessage, setMcpServerMessage] = useState("");

  async function greet() {
    // Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
    setGreetMsg(await invoke("greet", { name }));
  }

  // Check MCP server status on mount
  useEffect(() => {
    checkMcpServerStatus();
  }, []);

  async function checkMcpServerStatus() {
    try {
      const status = await invoke<boolean>("get_mcp_server_status");
      setMcpServerRunning(status);
    } catch (error) {
      console.error("Failed to check MCP server status:", error);
    }
  }

  async function toggleMcpServer() {
    try {
      if (mcpServerRunning) {
        const message = await invoke<string>("stop_mcp_server");
        setMcpServerMessage(message);
        setMcpServerRunning(false);
      } else {
        const message = await invoke<string>("start_mcp_server");
        setMcpServerMessage(message);
        setMcpServerRunning(true);
      }
    } catch (error) {
      setMcpServerMessage(`Error: ${error}`);
    }
  }

  return (
    <main className="container">
      <h1 className="text-3xl font-bold text-blue-600">Browser Automation with MCP</h1>

      <div className="row">
        <a href="https://vite.dev" target="_blank">
          <img src="/vite.svg" className="logo vite" alt="Vite logo" />
        </a>
        <a href="https://tauri.app" target="_blank">
          <img src="/tauri.svg" className="logo tauri" alt="Tauri logo" />
        </a>
        <a href="https://react.dev" target="_blank">
          <img src={reactLogo} className="logo react" alt="React logo" />
        </a>
      </div>
      <p>Click on the Tauri, Vite, and React logos to learn more.</p>

      <div className="mcp-server-section" style={{ margin: "2rem 0", padding: "1rem", border: "1px solid #ccc", borderRadius: "8px" }}>
        <h2 className="text-2xl font-semibold mb-4">MCP Server Control</h2>
        <div className="row" style={{ gap: "1rem", alignItems: "center" }}>
          <button 
            onClick={toggleMcpServer}
            className={`px-6 py-2 rounded-lg font-medium transition-colors ${
              mcpServerRunning 
                ? "bg-red-500 hover:bg-red-600 text-white" 
                : "bg-green-500 hover:bg-green-600 text-white"
            }`}
            style={{ 
              backgroundColor: mcpServerRunning ? "#ef4444" : "#10b981",
              color: "white",
              padding: "0.5rem 1.5rem",
              borderRadius: "0.5rem",
              fontWeight: "500"
            }}
          >
            {mcpServerRunning ? "Stop MCP Server" : "Run MCP Server"}
          </button>
          <span className={`status-indicator ${mcpServerRunning ? "text-green-600" : "text-gray-500"}`}>
            Status: {mcpServerRunning ? "🟢 Running on port 37650" : "⭕ Stopped"}
          </span>
        </div>
        {mcpServerMessage && (
          <p className="mt-2 text-sm" style={{ marginTop: "0.5rem" }}>{mcpServerMessage}</p>
        )}
      </div>

      <form
        className="row"
        onSubmit={(e) => {
          e.preventDefault();
          greet();
        }}
      >
        <input
          id="greet-input"
          onChange={(e) => setName(e.currentTarget.value)}
          placeholder="Enter a name..."
        />
        <button type="submit">Greet</button>
      </form>
      <p>{greetMsg}</p>
    </main>
  );
}

export default App;
