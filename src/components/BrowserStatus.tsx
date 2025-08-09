import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";

interface BrowserInfo {
  connected: boolean;
  tabs: Array<{
    index: number;
    title: string;
    url: string;
    current: boolean;
  }>;
  console_messages?: Array<{
    level: string;
    text: string;
    timestamp: number;
  }>;
}

export function BrowserStatus() {
  const [browserInfo, setBrowserInfo] = useState<BrowserInfo | null>(null);
  const [loading, setLoading] = useState(false);
  const [showConsole, setShowConsole] = useState(false);

  async function refreshStatus() {
    setLoading(true);
    try {
      // This would call a Tauri command to get browser status
      const info = await invoke<BrowserInfo>("get_browser_status");
      setBrowserInfo(info);
    } catch (error) {
      console.error("Failed to get browser status:", error);
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    refreshStatus();
    const interval = setInterval(refreshStatus, 5000); // Refresh every 5 seconds
    return () => clearInterval(interval);
  }, []);

  if (!browserInfo) {
    return (
      <div className="browser-status" style={{
        padding: "1rem",
        border: "1px solid #e5e7eb",
        borderRadius: "0.5rem",
        marginTop: "1rem"
      }}>
        <h3 style={{ fontSize: "1.125rem", fontWeight: "600", marginBottom: "0.5rem" }}>
          Browser Status
        </h3>
        <p style={{ color: "#6b7280" }}>No browser information available</p>
      </div>
    );
  }

  return (
    <div className="browser-status" style={{
      padding: "1rem",
      border: "1px solid #e5e7eb",
      borderRadius: "0.5rem",
      marginTop: "1rem"
    }}>
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: "1rem" }}>
        <h3 style={{ fontSize: "1.125rem", fontWeight: "600" }}>
          Browser Status
        </h3>
        <div style={{ display: "flex", gap: "0.5rem" }}>
          <button
            onClick={refreshStatus}
            disabled={loading}
            style={{
              padding: "0.25rem 0.5rem",
              fontSize: "0.875rem",
              borderRadius: "0.25rem",
              border: "1px solid #d1d5db",
              backgroundColor: "white",
              cursor: loading ? "not-allowed" : "pointer",
              opacity: loading ? 0.5 : 1
            }}
          >
            {loading ? "..." : "Refresh"}
          </button>
          {browserInfo.console_messages && (
            <button
              onClick={() => setShowConsole(!showConsole)}
              style={{
                padding: "0.25rem 0.5rem",
                fontSize: "0.875rem",
                borderRadius: "0.25rem",
                border: "1px solid #d1d5db",
                backgroundColor: showConsole ? "#eff6ff" : "white",
                cursor: "pointer"
              }}
            >
              Console ({browserInfo.console_messages.length})
            </button>
          )}
        </div>
      </div>

      <div style={{ marginBottom: "0.75rem" }}>
        <span style={{
          display: "inline-flex",
          alignItems: "center",
          padding: "0.25rem 0.5rem",
          borderRadius: "0.25rem",
          fontSize: "0.875rem",
          fontWeight: "500",
          backgroundColor: browserInfo.connected ? "#dcfce7" : "#fee2e2",
          color: browserInfo.connected ? "#166534" : "#991b1b"
        }}>
          {browserInfo.connected ? "🟢 Connected" : "🔴 Disconnected"}
        </span>
      </div>

      {browserInfo.tabs.length > 0 && (
        <div>
          <h4 style={{ fontSize: "0.875rem", fontWeight: "600", marginBottom: "0.5rem", color: "#4b5563" }}>
            Open Tabs ({browserInfo.tabs.length})
          </h4>
          <div style={{ maxHeight: "150px", overflowY: "auto" }}>
            {browserInfo.tabs.map((tab) => (
              <div
                key={tab.index}
                style={{
                  padding: "0.5rem",
                  marginBottom: "0.25rem",
                  borderRadius: "0.25rem",
                  backgroundColor: tab.current ? "#eff6ff" : "#f9fafb",
                  border: tab.current ? "1px solid #3b82f6" : "1px solid #e5e7eb",
                  fontSize: "0.75rem"
                }}
              >
                <div style={{ fontWeight: tab.current ? "600" : "400" }}>
                  {tab.current && "▶ "}{tab.title || "Untitled"}
                </div>
                <div style={{ color: "#6b7280", marginTop: "0.25rem", wordBreak: "break-all" }}>
                  {tab.url}
                </div>
              </div>
            ))}
          </div>
        </div>
      )}

      {showConsole && browserInfo.console_messages && (
        <div style={{ marginTop: "1rem" }}>
          <h4 style={{ fontSize: "0.875rem", fontWeight: "600", marginBottom: "0.5rem", color: "#4b5563" }}>
            Console Messages
          </h4>
          <div style={{
            maxHeight: "200px",
            overflowY: "auto",
            backgroundColor: "#1e293b",
            borderRadius: "0.25rem",
            padding: "0.5rem"
          }}>
            {browserInfo.console_messages.map((msg, idx) => (
              <div
                key={idx}
                style={{
                  fontSize: "0.75rem",
                  fontFamily: "monospace",
                  marginBottom: "0.25rem",
                  color: msg.level === "error" ? "#ef4444" :
                        msg.level === "warn" ? "#f59e0b" :
                        msg.level === "info" ? "#3b82f6" :
                        "#94a3b8"
                }}
              >
                <span style={{ opacity: 0.7 }}>
                  [{new Date(msg.timestamp * 1000).toLocaleTimeString()}]
                </span>
                {" "}
                <span style={{ fontWeight: "600" }}>[{msg.level.toUpperCase()}]</span>
                {" "}
                {msg.text}
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}