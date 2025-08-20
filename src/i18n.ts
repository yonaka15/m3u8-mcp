export const translations = {
  en: {
    // App title
    title: "m3u8 MCP Server",
    
    // Menu
    menu: {
      settings: "Settings",
      ffmpegConfig: "FFmpeg Configuration", 
      mcpServerControl: "MCP Server Control",
      cacheStats: "Cache Statistics",
    },
    
    // M3u8 Form
    m3u8Form: {
      title: "m3u8 Stream Parser",
      urlLabel: "m3u8 URL",
      urlPlaceholder: "https://example.com/stream.m3u8",
      parseButton: "Parse Playlist",
      downloadButton: "Download Stream",
      parsing: "Parsing...",
      processing: "Processing...",
      clearUrl: "Clear URL",
      urlHistory: "URL History",
      recentUrls: "Recent URLs",
      clearAll: "Clear All",
      noUrlError: "Please enter a valid m3u8 URL",
      parsedPlaylist: "Parsed Playlist",
      version: "Version",
      targetDuration: "Target Duration",
      variants: "Variants",
      bandwidth: "Bandwidth",
      resolution: "Resolution",
      codecs: "Codecs",
      segments: "Segments",
      duration: "Duration",
      segmentsTotal: "total",
      andMore: "and {count} more segments",
      master: "Master",
      media: "Media",
      downloadCompleted: "Download completed",
      downloadCancelled: "Download cancelled",
      initializingDownload: "Initializing download...",
      cancellingDownload: "Cancelling download...",
      extractSegmentsButton: "Extract Segments",
      extracting: "Extracting...",
      extractedSegments: "Extracted Segments",
      copyUrl: "Copy URL",
      copyAllUrls: "Copy All URLs",
      clearSegments: "Clear",
      showLess: "Show Less",
      disclaimer: "Content Download Disclaimer",
      disclaimerText: "By using this download feature, you acknowledge that:\n• You are responsible for ensuring you have proper rights and permissions to download the content\n• You will comply with all applicable copyright laws and terms of service\n• This tool is provided for legitimate use cases only (personal backups, content you own, etc.)\n• The developers are not responsible for any misuse of this tool",
      iUnderstand: "I understand and agree",
      cancel: "Cancel",
      selectDownloadFolder: "Select Download Folder",
    },
    
    // MCP Server Control
    mcpServer: {
      title: "MCP Server Control",
      ffmpegConfig: "FFmpeg Configuration",
      ffmpegPath: "FFmpeg Path",
      connectWith: "Connect with:",
      claudeCode: "Claude Code",
      claudeDesktop: "Claude Desktop",
      vsCode: "VS Code Extension",
      port: "Port:",
      portError: "Please enter a valid port (1024-65535)",
      startServer: "Connect to AI via MCP",
      stopServer: "Disconnect from AI",
      status: "Status:",
      running: "Running on port",
      stopped: "Stopped",
      copy: "Copy",
      copied: "Copied!",
      error: "Error:",
      saveConfig: "Configuration saved",
    },
    
    // Cache Statistics Modal
    cache: {
      title: "Cache Statistics",
      cachedPlaylists: "Cached Playlists",
      downloadedStreams: "Downloaded Streams",
      probeResults: "Probe Results",
      totalDownloadSize: "Total Download Size",
      latestDownload: "Latest download",
      latestCache: "Latest cache",
      refresh: "Refresh",
      close: "Close",
      databaseNotInitialized: "Database not initialized",
      clearSuccess: "Cache cleared successfully",
      clearError: "Failed to clear cache",
    },
  },
  ja: {
    // App title
    title: "m3u8 MCP サーバー",
    
    // Menu
    menu: {
      settings: "設定",
      ffmpegConfig: "FFmpeg 設定",
      mcpServerControl: "MCP サーバー制御", 
      cacheStats: "キャッシュ統計",
    },
    
    // M3u8 Form
    m3u8Form: {
      title: "m3u8 ストリームパーサー",
      urlLabel: "m3u8 URL",
      urlPlaceholder: "https://example.com/stream.m3u8",
      parseButton: "プレイリストを解析",
      downloadButton: "ストリームをダウンロード",
      parsing: "解析中...",
      processing: "処理中...",
      clearUrl: "URLをクリア",
      urlHistory: "URL履歴",
      recentUrls: "最近のURL",
      clearAll: "すべてクリア",
      noUrlError: "有効なm3u8 URLを入力してください",
      parsedPlaylist: "解析済みプレイリスト",
      version: "バージョン",
      targetDuration: "ターゲット時間",
      variants: "バリアント",
      bandwidth: "帯域幅",
      resolution: "解像度",
      codecs: "コーデック",
      segments: "セグメント",
      duration: "時間",
      segmentsTotal: "合計",
      andMore: "他 {count} セグメント",
      master: "マスター",
      media: "メディア",
      downloadCompleted: "ダウンロード完了",
      downloadCancelled: "ダウンロードをキャンセルしました",
      initializingDownload: "ダウンロードを初期化しています...",
      cancellingDownload: "ダウンロードをキャンセル中...",
      extractSegmentsButton: "セグメント抽出",
      extracting: "抽出中...",
      extractedSegments: "抽出されたセグメント",
      copyUrl: "URLをコピー",
      copyAllUrls: "すべてのURLをコピー",
      clearSegments: "クリア",
      showLess: "折りたたむ",
      disclaimer: "コンテンツダウンロードに関する免責事項",
      disclaimerText: "このダウンロード機能を使用することで、以下に同意したものとみなされます：\n• コンテンツをダウンロードする適切な権限と許可を持っていることを確認する責任があります\n• すべての適用される著作権法およびサービス利用規約を遵守します\n• このツールは正当な用途（個人的なバックアップ、所有するコンテンツなど）にのみ使用されます\n• 開発者はこのツールの誤用について一切の責任を負いません",
      iUnderstand: "理解し、同意します",
      cancel: "キャンセル",
      selectDownloadFolder: "ダウンロード先フォルダを選択",
    },
    
    // MCP Server Control
    mcpServer: {
      title: "MCP サーバー制御",
      ffmpegConfig: "FFmpeg 設定",
      ffmpegPath: "FFmpeg パス",
      connectWith: "接続方法:",
      claudeCode: "Claude Code",
      claudeDesktop: "Claude Desktop",
      vsCode: "VS Code 拡張機能",
      port: "ポート:",
      portError: "有効なポート番号を入力してください (1024-65535)",
      startServer: "MCPでAIに接続",
      stopServer: "AIから切断",
      status: "ステータス:",
      running: "ポートで実行中",
      stopped: "停止中",
      copy: "コピー",
      copied: "コピーしました!",
      error: "エラー:",
      saveConfig: "設定を保存しました",
    },
    
    // Cache Statistics Modal
    cache: {
      title: "キャッシュ統計",
      cachedPlaylists: "キャッシュ済みプレイリスト",
      downloadedStreams: "ダウンロード済みストリーム",
      probeResults: "プローブ結果",
      totalDownloadSize: "合計ダウンロードサイズ",
      latestDownload: "最新のダウンロード",
      latestCache: "最新のキャッシュ",
      refresh: "更新",
      close: "閉じる",
      databaseNotInitialized: "データベースが初期化されていません",
      clearSuccess: "キャッシュを正常にクリアしました",
      clearError: "キャッシュのクリアに失敗しました",
    },
  },
} as const;

export type Language = keyof typeof translations;
export type TranslationKey = keyof typeof translations.en;

// Helper function to get translation
export function t(language: Language, path: string): string {
  const keys = path.split('.');
  let value: any = translations[language];
  
  for (const key of keys) {
    if (value && typeof value === 'object' && key in value) {
      value = value[key];
    } else {
      // Fallback to English if translation not found
      value = translations.en;
      for (const k of keys) {
        if (value && typeof value === 'object' && k in value) {
          value = value[k];
        } else {
          return path; // Return the path if translation not found
        }
      }
      break;
    }
  }
  
  return typeof value === 'string' ? value : path;
}

// Helper function for template strings with placeholders
export function tWithParams(language: Language, path: string, params: Record<string, any>): string {
  let text = t(language, path);
  
  Object.entries(params).forEach(([key, value]) => {
    text = text.replace(`{${key}}`, String(value));
  });
  
  return text;
}