import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { translations, type Language } from "./i18n";
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
  const [language, setLanguage] = useState<Language>("en");
  
  // Tool selection state
  const [selectedTools, setSelectedTools] = useState<Set<string>>(new Set([
    "redmine_configure",
    "redmine_test_connection",
    "redmine_list_issues",
    "redmine_get_issue",
    "redmine_create_issue",
    "redmine_update_issue",
    "redmine_list_projects",
    "redmine_get_project",
    "redmine_list_users",
    "redmine_get_current_user",
  ]));
  
  // Redmine configuration
  const [redmineHost, setRedmineHost] = useState<string>("");
  const [redmineApiKey, setRedmineApiKey] = useState<string>("");
  const [redmineConfigured, setRedmineConfigured] = useState(false);
  const [testingConnection, setTestingConnection] = useState(false);

  const t = translations[language];

  // Check MCP server status and load Redmine config on mount
  useEffect(() => {
    checkMcpServerStatus();
    loadRedmineConfiguration();
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

  async function loadRedmineConfiguration() {
    try {
      const config = await invoke<{ host: string; api_key: string } | null>(
        "load_redmine_config",
      );
      if (config && config.host && config.api_key) {
        setRedmineHost(config.host);
        setRedmineApiKey(config.api_key);
        // Auto-test connection after loading
        setTestingConnection(true);
        try {
          const user = await invoke<any>("test_redmine_connection");
          setRedmineConfigured(true);
          setMcpServerMessage(`Connected to Redmine as: ${user.user?.login || "unknown"}`);
        } catch (error) {
          setRedmineConfigured(false);
          // Don't show error message on auto-test failure
        } finally {
          setTestingConnection(false);
          setMcpServerMessage("");
        }
      }
    } catch (error) {
      console.error("Failed to load Redmine configuration:", error);
    }
  }

  const checkPortAvailability = useCallback(
    async (portToCheck: number | null) => {
      // If port is null or NaN, mark as invalid
      if (portToCheck === null || isNaN(portToCheck)) {
        setPortAvailable(false);
        setMcpServerMessage(t.portError);
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
        setMcpServerMessage(t.portNotAllowed);
        return;
      }

      if (portToCheck < 1024 || portToCheck > 65535) {
        setPortAvailable(false);
        setMcpServerMessage(t.portRange);
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
    [mcpServerRunning, currentPort, t],
  );

  async function testRedmineConnection() {
    if (!redmineHost || !redmineApiKey) {
      setMcpServerMessage("Please enter Redmine host and API key");
      return;
    }

    setTestingConnection(true);
    setMcpServerMessage("");

    try {
      // Configure Redmine client
      await invoke<string>("configure_redmine", {
        host: redmineHost,
        apiKey: redmineApiKey,
      });

      // Test connection
      const user = await invoke<any>("test_redmine_connection");
      setRedmineConfigured(true);
      setMcpServerMessage(`Connected to Redmine as: ${user.user?.login || "unknown"}`);
    } catch (error) {
      setRedmineConfigured(false);
      setMcpServerMessage(`Failed to connect to Redmine: ${error}`);
    } finally {
      setTestingConnection(false);
    }
  }

  async function toggleMcpServer() {
    try {
      if (mcpServerRunning) {
        await invoke<string>("stop_mcp_server");
        setMcpServerMessage(""); // Clear message instead of showing redundant stop message
        setMcpServerRunning(false);
        setCurrentPort(null);
      } else {
        // Check if Redmine is configured
        if (!redmineConfigured) {
          setMcpServerMessage("Please configure and test Redmine connection first");
          return;
        }

        const port = parseInt(portInput);
        if (isNaN(port)) {
          setMcpServerMessage(t.portError);
          return;
        }
        
        // Start MCP server with selected tools
        await invoke<string>("start_mcp_server", { 
          port,
          enabledTools: Array.from(selectedTools) 
        });
        setMcpServerMessage(""); // Clear message since status indicator shows the running state
        setMcpServerRunning(true);
        setCurrentPort(port);
      }
    } catch (error) {
      setMcpServerMessage(`${t.error} ${error}`);
    }
  }

  async function copyToClipboard(
    type: "claude-code" | "claude-desktop" | "vscode",
  ) {
    let textToCopy = "";

    switch (type) {
      case "claude-code":
        textToCopy = `claude mcp add --transport http redmine http://localhost:${currentPort}/mcp`;
        break;
      case "claude-desktop":
        const desktopConfig = {
          "redmine": {
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
          name: "redmine",
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

  function toggleLanguage() {
    setLanguage(language === "en" ? "ja" : "en");
  }

  // Tool categories for better organization
  const toolCategories = {
    basic: ["redmine_configure", "redmine_test_connection"],
    issues: ["redmine_list_issues", "redmine_get_issue", "redmine_create_issue", "redmine_update_issue", "redmine_delete_issue"],
    projects: ["redmine_list_projects", "redmine_get_project", "redmine_create_project"],
    users: ["redmine_list_users", "redmine_get_current_user"],
    timeEntries: ["redmine_list_time_entries", "redmine_create_time_entry"]
  };

  const toggleTool = (tool: string) => {
    const newSelected = new Set(selectedTools);
    if (newSelected.has(tool)) {
      newSelected.delete(tool);
    } else {
      newSelected.add(tool);
    }
    setSelectedTools(newSelected);
  };

  const selectAll = () => {
    const allTools = Object.values(toolCategories).flat();
    setSelectedTools(new Set(allTools));
  };

  const deselectAll = () => {
    setSelectedTools(new Set());
  };

  return (
    <main className="min-h-screen bg-gray-50 dark:bg-gray-900 py-12 px-4 sm:px-6 lg:px-8">
      <div className="max-w-3xl mx-auto">
        {/* Language Toggle Button */}
        <div className="flex justify-end mb-4">
          <button
            onClick={toggleLanguage}
            className="px-3 py-1.5 bg-gray-200 dark:bg-gray-700 hover:bg-gray-300 dark:hover:bg-gray-600 rounded-md font-medium text-sm text-gray-700 dark:text-gray-300 transition-colors"
          >
            <span className="font-mono">{language === "en" ? "EN" : "JP"}</span>
            <span className="mx-1.5 text-gray-400">|</span>
            <span className="font-mono text-gray-400">{language === "en" ? "JP" : "EN"}</span>
          </button>
        </div>

        <h1 className="text-4xl font-bold text-center text-blue-600 dark:text-blue-400 mb-8">
          Redmine MCP Server
        </h1>

        <div className="bg-white dark:bg-gray-800 rounded-lg shadow-lg p-6">
          {/* Redmine Configuration Section */}
          {!mcpServerRunning && (
            <div className="mb-6 p-4 bg-yellow-50 dark:bg-yellow-900/20 rounded-lg border border-yellow-200 dark:border-yellow-800">
              <h2 className="text-lg font-semibold mb-4 text-gray-800 dark:text-gray-200">
                Redmine Configuration
              </h2>
              
              <div className="space-y-4">
                <div>
                  <label
                    htmlFor="redmine-host"
                    className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1"
                  >
                    Redmine Host URL
                  </label>
                  <input
                    id="redmine-host"
                    type="text"
                    value={redmineHost}
                    onChange={(e) => setRedmineHost(e.target.value)}
                    placeholder="https://redmine.example.com"
                    className="w-full px-3 py-2 rounded-md border border-gray-300 dark:border-gray-600 focus:ring-2 focus:ring-blue-500 focus:outline-none"
                  />
                </div>

                <div>
                  <label
                    htmlFor="redmine-api-key"
                    className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1"
                  >
                    Redmine API Key
                  </label>
                  <input
                    id="redmine-api-key"
                    type="password"
                    value={redmineApiKey}
                    onChange={(e) => setRedmineApiKey(e.target.value)}
                    placeholder="Your API key from Redmine account settings"
                    className="w-full px-3 py-2 rounded-md border border-gray-300 dark:border-gray-600 focus:ring-2 focus:ring-blue-500 focus:outline-none"
                  />
                </div>

                <div className="flex items-center gap-4">
                  <button
                    onClick={testRedmineConnection}
                    disabled={testingConnection || !redmineHost || !redmineApiKey}
                    className={`px-4 py-2 rounded-lg font-medium transition-colors ${
                      testingConnection || !redmineHost || !redmineApiKey
                        ? "bg-gray-400 cursor-not-allowed text-gray-200"
                        : redmineConfigured
                          ? "bg-gray-500 hover:bg-gray-600 text-white"
                          : "bg-blue-500 hover:bg-blue-600 text-white"
                    }`}
                  >
                    {testingConnection ? "Testing..." : "Check Connection"}
                  </button>
                  {redmineConfigured && (
                    <span className="text-green-600 dark:text-green-400 font-medium">
                      ✓ Ready to start server
                    </span>
                  )}
                </div>
              </div>
            </div>
          )}

          {/* Tool Selection Section */}
          {!mcpServerRunning && redmineConfigured && (
            <div className="mb-6 p-4 bg-gray-50 dark:bg-gray-700/30 rounded-lg border border-gray-200 dark:border-gray-700">
              <div className="flex justify-between items-center mb-4">
                <h2 className="text-lg font-semibold text-gray-800 dark:text-gray-200">
                  {t.selectTools}
                </h2>
                <div className="flex gap-2">
                  <button
                    onClick={selectAll}
                    className="px-3 py-1 text-sm bg-blue-500 hover:bg-blue-600 text-white rounded transition-colors"
                  >
                    {t.selectAll}
                  </button>
                  <button
                    onClick={deselectAll}
                    className="px-3 py-1 text-sm bg-gray-500 hover:bg-gray-600 text-white rounded transition-colors"
                  >
                    {t.deselectAll}
                  </button>
                </div>
              </div>
              
              <div className="space-y-4">
                {Object.entries(toolCategories).map(([category, tools]) => (
                  <div key={category} className="border-l-4 border-blue-400 pl-4">
                    <h3 className="text-sm font-semibold text-gray-700 dark:text-gray-300 mb-2">
                      {t.toolCategories[category as keyof typeof t.toolCategories]}
                    </h3>
                    <div className="grid grid-cols-1 sm:grid-cols-2 gap-2">
                      {tools.map(tool => (
                        <label
                          key={tool}
                          className="flex items-center space-x-2 cursor-pointer hover:bg-gray-100 dark:hover:bg-gray-600/30 p-1 rounded"
                        >
                          <input
                            type="checkbox"
                            checked={selectedTools.has(tool)}
                            onChange={() => toggleTool(tool)}
                            className="w-4 h-4 text-blue-600 bg-gray-100 border-gray-300 rounded focus:ring-blue-500 dark:focus:ring-blue-600 dark:ring-offset-gray-800 focus:ring-2 dark:bg-gray-700 dark:border-gray-600"
                          />
                          <span className="text-sm text-gray-700 dark:text-gray-300">
                            {t.tools[tool as keyof typeof t.tools]}
                          </span>
                        </label>
                      ))}
                    </div>
                  </div>
                ))}
              </div>
              
              <div className="mt-4 text-sm text-gray-600 dark:text-gray-400">
                Selected: {selectedTools.size} / {Object.values(toolCategories).flat().length} tools
              </div>
            </div>
          )}

          {mcpServerRunning && currentPort && (
            <div className="mb-6 p-4 bg-blue-50 dark:bg-blue-900/20 rounded-lg border border-blue-200 dark:border-blue-800">
              <p className="text-lg font-semibold mb-4 text-gray-800 dark:text-gray-200">
                {t.connectWith}
              </p>

              {/* Claude Code */}
              <div className="mb-4">
                <p className="text-sm font-medium mb-2 text-gray-700 dark:text-gray-300">
                  {t.claudeCode}
                </p>
                <div className="relative">
                  <code className="block p-3 pr-20 bg-gray-900 dark:bg-black text-gray-300 rounded text-sm font-mono whitespace-nowrap overflow-x-auto">
                    claude mcp add --transport http redmine
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
                    {copiedCommand === "claude-code" ? t.copied : t.copy}
                  </button>
                </div>
              </div>

              {/* Claude Desktop */}
              <div className="mb-4">
                <p className="text-sm font-medium mb-2 text-gray-700 dark:text-gray-300">
                  {t.claudeDesktop}
                </p>
                <div className="relative">
                  <pre className="block p-3 pr-20 bg-gray-900 dark:bg-black text-gray-300 rounded text-xs font-mono overflow-x-auto max-h-40 overflow-y-auto">
{`"redmine": {
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
                    {copiedCommand === "claude-desktop" ? t.copied : t.copy}
                  </button>
                </div>
              </div>

              {/* VS Code */}
              <div>
                <p className="text-sm font-medium mb-2 text-gray-700 dark:text-gray-300">
                  {t.vsCode}
                </p>
                <div className="relative">
                  <code className="block p-3 pr-20 bg-gray-900 dark:bg-black text-gray-300 rounded text-sm font-mono whitespace-nowrap overflow-x-auto">
                    code --add-mcp "&#123;&quot;name&quot;:&quot;redmine&quot;,&quot;url&quot;:&quot;http://localhost:{currentPort}/mcp&quot;&#125;"
                  </code>
                  <button
                    onClick={() => copyToClipboard("vscode")}
                    className={`absolute top-1/2 right-2 -translate-y-1/2 px-3 py-1 text-xs font-medium text-white rounded transition-colors ${
                      copiedCommand === "vscode"
                        ? "bg-green-600 hover:bg-green-700"
                        : "bg-blue-600 hover:bg-blue-700"
                    }`}
                  >
                    {copiedCommand === "vscode" ? t.copied : t.copy}
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
              {t.port}
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
            {!mcpServerRunning && (
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
                  ? t.checking
                  : portAvailable === false
                    ? t.inUse
                    : portAvailable === true
                      ? t.available
                      : ""}
              </span>
            )}
          </div>

          <div className="flex items-center gap-4">
            <button
              onClick={toggleMcpServer}
              disabled={
                !mcpServerRunning &&
                (portAvailable === false || checkingPort || portInput === "" || !redmineConfigured)
              }
              className={`px-6 py-2 rounded-lg font-medium transition-colors ${
                mcpServerRunning
                  ? "bg-red-500 hover:bg-red-600 text-white"
                  : portAvailable === false || checkingPort || portInput === "" || !redmineConfigured
                    ? "bg-gray-400 cursor-not-allowed text-gray-200"
                    : "bg-green-500 hover:bg-green-600 text-white"
              }`}
            >
              {mcpServerRunning ? t.stopServer : t.startServer}
            </button>
            <span
              className={`font-medium ${
                mcpServerRunning ? "text-green-600 dark:text-green-400" : "text-gray-500"
              }`}
            >
              {t.status}{" "}
              {mcpServerRunning
                ? `${t.running} ${currentPort}`
                : t.stopped}
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