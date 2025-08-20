import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { M3u8Form } from "./components/M3u8Form";
import { t, Language } from "./i18n";
import "./App.css";

function App() {
  const [mcpServerRunning, setMcpServerRunning] = useState(false);
  const [mcpServerMessage, setMcpServerMessage] = useState("");
  const [portInput, setPortInput] = useState<string>("37650");
  const [currentPort, setCurrentPort] = useState<number | null>(null);
  const [copiedCommand, setCopiedCommand] = useState<
    "claude-code" | "claude-desktop" | "vscode" | null
  >(null);
  
  // FFmpeg configuration
  const [ffmpegPath, setFfmpegPath] = useState<string>("ffmpeg");
  
  // Cache state
  const [databaseInitialized, setDatabaseInitialized] = useState(false);
  const [cacheStats, setCacheStats] = useState<{
    cached_playlists: number;
    downloaded_streams: number;
    probe_results: number;
    total_download_size: number;
    latest_download?: string;
    latest_cache?: string;
  } | null>(null);
  const [cacheMessage] = useState<string>("");
  
  // Menu state
  const [menuOpen, setMenuOpen] = useState(false);
  const [statsModalOpen, setStatsModalOpen] = useState(false);
  const [ffmpegConfigModalOpen, setFfmpegConfigModalOpen] = useState(false);
  const [mcpServerModalOpen, setMcpServerModalOpen] = useState(false);

  // Check MCP server status and load config on mount
  useEffect(() => {
    checkMcpServerStatus();
    loadM3u8Configuration();
    initializeDatabase();
  }, []);

  // Close menu when clicking outside
  useEffect(() => {
    const handleClickOutside = (event: MouseEvent) => {
      const target = event.target as HTMLElement;
      if (menuOpen && !target.closest('.menu-container')) {
        setMenuOpen(false);
      }
    };

    document.addEventListener('click', handleClickOutside);
    return () => {
      document.removeEventListener('click', handleClickOutside);
    };
  }, [menuOpen]);

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

  async function loadM3u8Configuration() {
    try {
      const config = await invoke<{ ffmpeg_path: string | null; output_dir: string }>(
        "load_m3u8_config",
      );
      if (config) {
        setFfmpegPath(config.ffmpeg_path || "ffmpeg");
      }
    } catch (error) {
      console.error("Failed to load m3u8 configuration:", error);
    }
  }


  // async function _saveM3u8Configuration() {
  //   try {
  //     await invoke("save_m3u8_config", {
  //       ffmpegPath: ffmpegPath || null,
  //       outputDir: outputDir,
  //     });
  //     setMcpServerMessage(t(language, 'mcpServer.saveConfig'));
  //     setTimeout(() => setMcpServerMessage(""), 2000);
  //   } catch (error) {
  //     setMcpServerMessage(`Failed to save configuration: ${error}`);
  //   }
  // }

  async function toggleMcpServer() {
    try {
      if (mcpServerRunning) {
        await invoke<string>("stop_mcp_server");
        setMcpServerRunning(false);
        setCurrentPort(null);
      } else {
        const port = parseInt(portInput);
        if (isNaN(port) || port < 1024 || port > 65535) {
          setMcpServerMessage(t(language, 'mcpServer.portError'));
          return;
        }
        
        // Start MCP server with m3u8 tools
        await invoke<string>("start_mcp_server", { 
          port,
          enabledTools: [
            "m3u8_set_url",
            "m3u8_get_url",
            "m3u8_parse",
            "m3u8_download",
            "m3u8_convert",
            "m3u8_probe",
            "m3u8_extract_segments",
            "m3u8_cache_list",
            "m3u8_cache_clear"
          ]
        });
        setMcpServerRunning(true);
        setCurrentPort(port);
        setMcpServerMessage("");
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
        textToCopy = `claude mcp add --transport http m3u8 http://localhost:${currentPort}/mcp`;
        break;
      case "claude-desktop":
        const desktopConfig = {
          "m3u8": {
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
          name: "m3u8",
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

  // Database and cache functions
  async function initializeDatabase() {
    try {
      await invoke<string>("init_database");
      setDatabaseInitialized(true);
      await refreshCacheStats();
    } catch (error) {
      console.error("Failed to initialize database:", error);
      setDatabaseInitialized(false);
    }
  }

  async function refreshCacheStats() {
    if (!databaseInitialized) return;
    
    try {
      const stats = await invoke<any>("get_cache_stats");
      setCacheStats(stats);
    } catch (error) {
      console.error("Failed to get cache stats:", error);
    }
  }

  // async function _clearAllCache() {
  //   try {
  //     await invoke<string>("clear_cache");
  //     setCacheMessage("Cache cleared successfully");
  //     await refreshCacheStats();
  //   } catch (error) {
  //     setCacheMessage(`Failed to clear cache: ${error}`);
  //   } finally {
  //     setTimeout(() => setCacheMessage(""), 3000);
  //   }
  // }

  // Language state
  const [language, setLanguage] = useState<Language>('en');

  const toggleLanguage = () => {
    setLanguage((prev) => prev === 'en' ? 'ja' : 'en');
  };

  return (
    <main className="min-h-screen bg-gray-50 dark:bg-gray-900 py-12 px-4 sm:px-6 lg:px-8">
      <div className="max-w-4xl mx-auto">
        {/* Top bar with Menu, MCP Status, and Language Toggle */}
        <div className="flex justify-between items-center mb-4">
          {/* Left side: Menu Button and MCP Status */}
          <div className="flex items-center gap-3">
            <div className="relative menu-container">
              <button
                onClick={() => setMenuOpen(!menuOpen)}
                className="p-2 bg-gray-200 dark:bg-gray-700 hover:bg-gray-300 dark:hover:bg-gray-600 rounded-md text-gray-700 dark:text-gray-300 transition-colors"
              >
                <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 6h16M4 12h16M4 18h16" />
                </svg>
              </button>
            
            {/* Dropdown Menu */}
            {menuOpen && (
              <div className="absolute top-full left-0 mt-1 w-48 bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-md shadow-lg z-50">
                <div className="py-1">
                  <button
                    onClick={() => {
                      setFfmpegConfigModalOpen(true);
                      setMenuOpen(false);
                    }}
                    className="w-full text-left px-4 py-2 text-sm text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700 transition-colors"
                  >
                    {t(language, 'menu.ffmpegConfig')}
                  </button>
                  <button
                    onClick={() => {
                      setMcpServerModalOpen(true);
                      setMenuOpen(false);
                    }}
                    className="w-full text-left px-4 py-2 text-sm text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700 transition-colors"
                  >
                    {t(language, 'menu.mcpServerControl')}
                  </button>
                </div>
              </div>
              )}
            </div>
            
            {/* MCP Status Indicator */}
            <div 
              className="flex items-center gap-2 px-3 py-1.5 bg-gray-100 dark:bg-gray-800 rounded-md cursor-pointer hover:bg-gray-200 dark:hover:bg-gray-700 transition-colors"
              onClick={() => setMcpServerModalOpen(true)}
              title={mcpServerRunning ? `MCP Server: Running on port ${currentPort}` : 'MCP Server: Stopped (Click to configure)'}
            >
              <div className={`w-2 h-2 rounded-full ${
                mcpServerRunning 
                  ? 'bg-green-500 shadow-green-500/50 shadow-lg' 
                  : 'bg-red-500 shadow-red-500/50 shadow-lg'
              }`}></div>
              <span className="text-xs font-medium text-gray-600 dark:text-gray-400">
                MCP {mcpServerRunning ? 'ON' : 'OFF'}
              </span>
              {mcpServerRunning && currentPort && (
                <span className="text-xs text-gray-500 dark:text-gray-500">
                  :{currentPort}
                </span>
              )}
            </div>
          </div>

          {/* Language Toggle Button */}
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
          {t(language, 'title')}
        </h1>

        {/* m3u8 Form Component */}
        <M3u8Form language={language} />

        {/* FFmpeg Configuration Modal */}
        {ffmpegConfigModalOpen && (
          <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50 p-4">
            <div className="bg-white dark:bg-gray-800 rounded-lg shadow-2xl max-w-xl w-full">
              <div className="flex justify-between items-center p-6 border-b border-gray-200 dark:border-gray-700">
                <h2 className="text-xl font-semibold text-gray-800 dark:text-gray-200">
                  {t(language, 'mcpServer.ffmpegConfig')}
                </h2>
                <button
                  onClick={() => setFfmpegConfigModalOpen(false)}
                  className="text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-200"
                >
                  <svg className="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                  </svg>
                </button>
              </div>
              
              <div className="p-6 space-y-4">
                <div>
                  <label className="block text-sm font-medium text-gray-600 dark:text-gray-400 mb-1">
                    {t(language, 'mcpServer.ffmpegPath')}
                  </label>
                  <input
                    type="text"
                    value={ffmpegPath}
                    onChange={(e) => setFfmpegPath(e.target.value)}
                    placeholder="ffmpeg"
                    className="w-full px-3 py-2 rounded-md border border-gray-300 dark:border-gray-600 focus:ring-2 focus:ring-blue-500 focus:outline-none dark:bg-gray-700 dark:text-white"
                  />
                </div>
                
                <div className="flex justify-end gap-3 pt-4">
                  <button
                    onClick={() => setFfmpegConfigModalOpen(false)}
                    className="px-4 py-2 bg-gray-200 dark:bg-gray-700 hover:bg-gray-300 dark:hover:bg-gray-600 text-gray-700 dark:text-gray-300 rounded-lg font-medium transition-colors"
                  >
                    Close
                  </button>
                </div>
              </div>
            </div>
          </div>
        )}

        {/* MCP Server Control Modal */}
        {mcpServerModalOpen && (
          <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50 p-4">
            <div className="bg-white dark:bg-gray-800 rounded-lg shadow-2xl max-w-4xl w-full max-h-[80vh] overflow-y-auto">
              <div className="flex justify-between items-center p-6 border-b border-gray-200 dark:border-gray-700">
                <h2 className="text-xl font-semibold text-gray-800 dark:text-gray-200">
                  {t(language, 'mcpServer.title')}
                </h2>
                <button
                  onClick={() => setMcpServerModalOpen(false)}
                  className="text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-200"
                >
                  <svg className="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                  </svg>
                </button>
              </div>
              
              <div className="p-6">
                {mcpServerRunning && currentPort && (
                  <div className="mb-6 p-4 bg-blue-50 dark:bg-blue-900/20 rounded-lg border border-blue-200 dark:border-blue-800">
                    <p className="text-lg font-semibold mb-4 text-gray-800 dark:text-gray-200">
                      {t(language, 'mcpServer.connectWith')}
                    </p>

                    {/* Claude Code */}
                    <div className="mb-4">
                      <p className="text-sm font-medium mb-2 text-gray-700 dark:text-gray-300">
                        {t(language, 'mcpServer.claudeCode')}
                      </p>
                      <div className="relative">
                        <code className="block p-3 pr-20 bg-gray-900 dark:bg-black text-gray-300 rounded text-sm font-mono whitespace-nowrap overflow-x-auto">
                          claude mcp add --transport http m3u8 http://localhost:{currentPort}/mcp
                        </code>
                        <button
                          onClick={() => copyToClipboard("claude-code")}
                          className={`absolute top-1/2 right-2 -translate-y-1/2 px-3 py-1 text-xs font-medium text-white rounded-lg transition-colors ${
                            copiedCommand === "claude-code"
                              ? "bg-green-500 hover:bg-green-600"
                              : "bg-blue-500 hover:bg-blue-600"
                          }`}
                        >
                          {copiedCommand === "claude-code" ? t(language, 'mcpServer.copied') : t(language, 'mcpServer.copy')}
                        </button>
                      </div>
                    </div>

                    {/* Claude Desktop */}
                    <div className="mb-4">
                      <p className="text-sm font-medium mb-2 text-gray-700 dark:text-gray-300">
                        {t(language, 'mcpServer.claudeDesktop')}
                      </p>
                      <div className="relative">
                        <pre className="block p-3 pr-20 bg-gray-900 dark:bg-black text-gray-300 rounded text-xs font-mono overflow-x-auto max-h-40 overflow-y-auto">
{`"m3u8": {
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
                          className={`absolute top-2 right-2 px-3 py-1 text-xs font-medium text-white rounded-lg transition-colors ${
                            copiedCommand === "claude-desktop"
                              ? "bg-green-500 hover:bg-green-600"
                              : "bg-blue-500 hover:bg-blue-600"
                          }`}
                        >
                          {copiedCommand === "claude-desktop" ? t(language, 'mcpServer.copied') : t(language, 'mcpServer.copy')}
                        </button>
                      </div>
                    </div>

                    {/* VS Code */}
                    <div>
                      <p className="text-sm font-medium mb-2 text-gray-700 dark:text-gray-300">
                        {t(language, 'mcpServer.vsCode')}
                      </p>
                      <div className="relative">
                        <code className="block p-3 pr-20 bg-gray-900 dark:bg-black text-gray-300 rounded text-sm font-mono whitespace-nowrap overflow-x-auto">
                          code --add-mcp "&#123;&quot;name&quot;:&quot;m3u8&quot;,&quot;url&quot;:&quot;http://localhost:{currentPort}/mcp&quot;&#125;"
                        </code>
                        <button
                          onClick={() => copyToClipboard("vscode")}
                          className={`absolute top-1/2 right-2 -translate-y-1/2 px-3 py-1 text-xs font-medium text-white rounded-lg transition-colors ${
                            copiedCommand === "vscode"
                              ? "bg-green-500 hover:bg-green-600"
                              : "bg-blue-500 hover:bg-blue-600"
                          }`}
                        >
                          {copiedCommand === "vscode" ? t(language, 'mcpServer.copied') : t(language, 'mcpServer.copy')}
                        </button>
                      </div>
                    </div>
                  </div>
                )}

                <div className="flex items-center gap-4 mb-4">
                  <label className="font-medium text-gray-700 dark:text-gray-300">
                    {t(language, 'mcpServer.port')}
                  </label>
                  <input
                    type="text"
                    value={portInput}
                    onChange={(e) => setPortInput(e.target.value)}
                    disabled={mcpServerRunning}
                    placeholder="37650"
                    className={`px-3 py-2 rounded-md border ${
                      mcpServerRunning
                        ? "bg-gray-100 dark:bg-gray-700 text-gray-500 cursor-not-allowed border-gray-300 dark:border-gray-600"
                        : "border-gray-300 dark:border-gray-600 focus:ring-blue-500"
                    } focus:outline-none focus:ring-2 transition-colors dark:bg-gray-700 dark:text-white`}
                  />
                </div>

                {!mcpServerRunning && (
                  <div className="flex items-center gap-4 mb-6">
                    <button
                      onClick={toggleMcpServer}
                      className="px-4 py-2 rounded-lg font-medium transition-colors bg-green-500 hover:bg-green-600 text-white"
                    >
                      {t(language, 'mcpServer.startServer')}
                    </button>
                    <span className="font-medium text-gray-500">
                      {t(language, 'mcpServer.status')} {t(language, 'mcpServer.stopped')}
                    </span>
                  </div>
                )}

                {mcpServerRunning && (
                  <>
                    <div className="mb-6">
                      <span className={`font-medium text-green-600 dark:text-green-400`}>
                        {t(language, 'mcpServer.status')} {t(language, 'mcpServer.running')} {currentPort}
                      </span>
                    </div>
                    <div className="flex justify-between gap-3">
                      <button
                        onClick={() => setMcpServerModalOpen(false)}
                        className="px-4 py-2 bg-gray-200 dark:bg-gray-700 hover:bg-gray-300 dark:hover:bg-gray-600 text-gray-700 dark:text-gray-300 rounded-lg font-medium transition-colors"
                      >
                        Close
                      </button>
                      <button
                        onClick={toggleMcpServer}
                        className="px-4 py-2 bg-red-500 hover:bg-red-600 text-white rounded-lg font-medium transition-colors"
                      >
                        {t(language, 'mcpServer.stopServer')}
                      </button>
                    </div>
                  </>
                )}

                {!mcpServerRunning && (
                  <div className="flex justify-end gap-3">
                    <button
                      onClick={() => setMcpServerModalOpen(false)}
                      className="px-4 py-2 bg-gray-200 dark:bg-gray-700 hover:bg-gray-300 dark:hover:bg-gray-600 text-gray-700 dark:text-gray-300 rounded-lg font-medium transition-colors"
                    >
                      Close
                    </button>
                  </div>
                )}

                {mcpServerMessage && (
                  <p className="mt-4 text-sm text-red-600 dark:text-red-400">
                    {mcpServerMessage}
                  </p>
                )}
              </div>
            </div>
          </div>
        )}

        {/* Statistics Modal */}
        {statsModalOpen && (
          <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50 p-4">
            <div className="bg-white dark:bg-gray-800 rounded-lg shadow-2xl max-w-2xl w-full max-h-[80vh] overflow-y-auto">
              <div className="flex justify-between items-center p-6 border-b border-gray-200 dark:border-gray-700">
                <h2 className="text-xl font-semibold text-gray-800 dark:text-gray-200">
                  Cache Statistics
                </h2>
                <button
                  onClick={() => setStatsModalOpen(false)}
                  className="text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-200"
                >
                  <svg className="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                  </svg>
                </button>
              </div>
              
              <div className="p-6">
                {databaseInitialized && cacheStats ? (
                  <div className="space-y-4">
                    <div className="grid grid-cols-2 gap-4">
                      <div className="bg-blue-50 dark:bg-blue-900/20 rounded-lg p-4 text-center">
                        <div className="text-3xl font-bold text-blue-600 dark:text-blue-400">
                          {cacheStats.cached_playlists}
                        </div>
                        <div className="text-sm text-gray-600 dark:text-gray-400 mt-1">
                          Cached Playlists
                        </div>
                      </div>
                      
                      <div className="bg-green-50 dark:bg-green-900/20 rounded-lg p-4 text-center">
                        <div className="text-3xl font-bold text-green-600 dark:text-green-400">
                          {cacheStats.downloaded_streams}
                        </div>
                        <div className="text-sm text-gray-600 dark:text-gray-400 mt-1">
                          Downloaded Streams
                        </div>
                      </div>
                      
                      <div className="bg-purple-50 dark:bg-purple-900/20 rounded-lg p-4 text-center">
                        <div className="text-3xl font-bold text-purple-600 dark:text-purple-400">
                          {cacheStats.probe_results}
                        </div>
                        <div className="text-sm text-gray-600 dark:text-gray-400 mt-1">
                          Probe Results
                        </div>
                      </div>
                      
                      <div className="bg-orange-50 dark:bg-orange-900/20 rounded-lg p-4 text-center">
                        <div className="text-3xl font-bold text-orange-600 dark:text-orange-400">
                          {(cacheStats.total_download_size / 1024 / 1024).toFixed(2)} MB
                        </div>
                        <div className="text-sm text-gray-600 dark:text-gray-400 mt-1">
                          Total Download Size
                        </div>
                      </div>
                    </div>
                    
                    {(cacheStats.latest_download || cacheStats.latest_cache) && (
                      <div className="border-t border-gray-200 dark:border-gray-700 pt-4">
                        {cacheStats.latest_download && (
                          <div className="text-sm text-gray-600 dark:text-gray-400">
                            Latest download: {new Date(cacheStats.latest_download).toLocaleString()}
                          </div>
                        )}
                        {cacheStats.latest_cache && (
                          <div className="text-sm text-gray-600 dark:text-gray-400">
                            Latest cache: {new Date(cacheStats.latest_cache).toLocaleString()}
                          </div>
                        )}
                      </div>
                    )}
                    
                    <div className="flex justify-end gap-3">
                      <button
                        onClick={async () => {
                          await refreshCacheStats();
                        }}
                        className="px-4 py-2 bg-blue-500 hover:bg-blue-600 text-white rounded-lg font-medium transition-colors"
                      >
                        Refresh
                      </button>
                      <button
                        onClick={() => setStatsModalOpen(false)}
                        className="px-4 py-2 bg-gray-200 dark:bg-gray-700 hover:bg-gray-300 dark:hover:bg-gray-600 text-gray-700 dark:text-gray-300 rounded-lg font-medium transition-colors"
                      >
                        Close
                      </button>
                    </div>
                    
                    {cacheMessage && (
                      <p className={`mt-4 text-sm text-center ${
                        cacheMessage.includes("Failed") || cacheMessage.includes("error") 
                          ? "text-red-600 dark:text-red-400" 
                          : "text-green-600 dark:text-green-400"
                      }`}>
                        {cacheMessage}
                      </p>
                    )}
                  </div>
                ) : (
                  <div className="text-center py-8 text-gray-500 dark:text-gray-400">
                    Database not initialized
                  </div>
                )}
              </div>
            </div>
          </div>
        )}
      </div>
    </main>
  );
}

export default App;