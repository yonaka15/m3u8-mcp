import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { t, tWithParams, Language } from '../i18n';

interface ParsedPlaylist {
  type: 'master' | 'media';
  version?: number;
  targetDuration?: number;
  segments?: Segment[];
  variants?: Variant[];
  error?: string;
}

interface Segment {
  uri: string;
  duration: number;
  title?: string;
  byteRange?: string;
  key?: any;
}

interface Variant {
  uri: string;
  bandwidth: number;
  resolution?: string;
  codecs?: string;
  frameRate?: number;
}

interface M3u8FormProps {
  language: Language;
}

export function M3u8Form({ language }: M3u8FormProps) {
  const [url, setUrl] = useState('');
  const [loading, setLoading] = useState(false);
  const [parsedData, setParsedData] = useState<ParsedPlaylist | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [downloadStatus, setDownloadStatus] = useState<string | null>(null);
  const [downloadProgress, setDownloadProgress] = useState<string | null>(null);
  const [isDownloading, setIsDownloading] = useState(false);
  const [urlHistory, setUrlHistory] = useState<Array<{url: string; timestamp: string}>>([]);
  const [showHistory, setShowHistory] = useState(false);
  const [extractedSegments, setExtractedSegments] = useState<string[] | null>(null);
  const [extractingSegments, setExtractingSegments] = useState(false);
  const [segmentDisplayCount, setSegmentDisplayCount] = useState(20);
  const [copiedSegmentIndex, setCopiedSegmentIndex] = useState<number | null>(null);
  const [copiedAllSegments, setCopiedAllSegments] = useState(false);
  const [showDisclaimer, setShowDisclaimer] = useState(false);
  const [disclaimerAccepted, setDisclaimerAccepted] = useState(false);

  // Listen for download progress events
  useEffect(() => {
    const unsubscribe = listen<{
      status: string;
      message: string;
      time?: string;
      size?: string;
      speed?: string;
    }>('download-progress', (event) => {
      const { status, message, time, size, speed } = event.payload;
      
      if (status === 'progress') {
        // Format progress message
        let progressMsg = message;
        if (time && size && speed) {
          progressMsg = `Time: ${time} | Size: ${size} | Speed: ${speed}`;
        }
        setDownloadProgress(progressMsg);
        setDownloadStatus(null); // Clear the initial status message
        setIsDownloading(true);
      } else if (status === 'completed') {
        setDownloadStatus(message);
        setDownloadProgress(null);
        setLoading(false);
        setIsDownloading(false);
        // Clear success message after 5 seconds
        setTimeout(() => {
          setDownloadStatus(null);
        }, 5000);
      } else if (status === 'error') {
        setError(message);
        setDownloadProgress(null);
        setDownloadStatus(null);
        setLoading(false);
        setIsDownloading(false);
      } else if (status === 'cancelled') {
        setDownloadStatus(message);
        setDownloadProgress(null);
        setLoading(false);
        setIsDownloading(false);
        // Clear message after 3 seconds
        setTimeout(() => {
          setDownloadStatus(null);
        }, 3000);
      } else if (status === 'starting') {
        setDownloadStatus(message);
        setDownloadProgress(null);
        setIsDownloading(true);
      }
    });

    return () => {
      unsubscribe.then(fn => fn());
    };
  }, []);

  // Load last used URL on mount
  useEffect(() => {
    const loadLastUrl = async () => {
      try {
        const lastUrl = await invoke<string | null>('get_last_used_url');
        if (lastUrl) {
          setUrl(lastUrl);
        }
      } catch (err) {
        console.error('Failed to load last URL:', err);
      }
    };
    loadLastUrl();
  }, []);

  // Load URL history
  useEffect(() => {
    const loadHistory = async () => {
      try {
        const history = await invoke<Array<{url: string; timestamp: string}>>('get_url_history');
        setUrlHistory(history);
      } catch (err) {
        console.error('Failed to load URL history:', err);
      }
    };
    loadHistory();
  }, [url]); // Reload history when URL changes

  // Check for URL updates from MCP
  useEffect(() => {
    const checkUrl = async () => {
      try {
        const currentUrl = await invoke<string | null>('get_current_m3u8_url');
        if (currentUrl && currentUrl !== url) {
          setUrl(currentUrl);
        }
      } catch (err) {
        console.error('Failed to get current URL:', err);
      }
    };

    // Initial check
    checkUrl();

    // Set up interval to check for URL updates from MCP
    const interval = setInterval(checkUrl, 2000);

    return () => clearInterval(interval);
  }, [url]);

  // Sync URL changes to backend
  const handleUrlChange = async (newUrl: string) => {
    setUrl(newUrl);
    try {
      // Always sync to backend, even when empty
      await invoke('set_current_m3u8_url', { url: newUrl });
    } catch (err) {
      console.error('Failed to set URL:', err);
    }
  };

  const handleParse = async () => {
    if (!url.trim()) {
      setError(t(language, 'm3u8Form.noUrlError'));
      return;
    }

    setLoading(true);
    setError(null);
    setParsedData(null);

    try {
      const result = await invoke<ParsedPlaylist>('parse_m3u8_url', { url });
      setParsedData(result);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to parse m3u8');
    } finally {
      setLoading(false);
    }
  };

  const handleDownload = async () => {
    if (!url.trim()) {
      setError(t(language, 'm3u8Form.noUrlError'));
      return;
    }

    // Show disclaimer if not accepted
    if (!disclaimerAccepted) {
      setShowDisclaimer(true);
      return;
    }

    // Show folder selection dialog
    try {
      const { open } = await import('@tauri-apps/plugin-dialog');
      const selectedPath = await open({
        directory: true,
        multiple: false,
        title: t(language, 'm3u8Form.selectDownloadFolder'),
      });
      
      if (!selectedPath || typeof selectedPath !== 'string') {
        // User cancelled the dialog
        return;
      }

      setLoading(true);
      setError(null);
      setDownloadStatus(t(language, 'm3u8Form.initializingDownload'));
      setDownloadProgress(null);

      // Generate filename based on URL
      const urlObj = new URL(url);
      const pathParts = urlObj.pathname.split('/');
      const lastPart = pathParts[pathParts.length - 1] || 'stream';
      const baseName = lastPart.replace(/\.m3u8$/i, '');
      const timestamp = new Date().toISOString().replace(/[:.]/g, '-').slice(0, 19);
      const filename = `${baseName}_${timestamp}.mp4`;
      const fullPath = `${selectedPath}/${filename}`;

      // The actual progress will be handled by the event listener
      await invoke<string>('download_m3u8_stream', { 
        url,
        outputPath: fullPath
      });
      // Success is handled by the event listener
    } catch (err) {
      // Error is also handled by the event listener, but we keep this as fallback
      if (!error) {
        setError(err instanceof Error ? err.message : 'Failed to download stream');
      }
    }
  };

  // const _handleCancelDownload = async () => {
  //   console.log('Cancel button clicked');
  //   try {
  //     setDownloadStatus(t(language, 'm3u8Form.cancellingDownload'));
  //     const result = await invoke('cancel_download');
  //     console.log('Cancel result:', result);
  //   } catch (err) {
  //     console.error('Cancel error:', err);
  //     setError(err instanceof Error ? err.message : 'Failed to cancel download');
  //   }
  // };

  const handleExtractSegments = async () => {
    if (!url.trim()) {
      setError(t(language, 'm3u8Form.noUrlError'));
      return;
    }

    setExtractingSegments(true);
    setError(null);
    setExtractedSegments(null);

    try {
      const segments = await invoke<string[]>('extract_m3u8_segments', { 
        url,
        baseUrl: null
      });
      setExtractedSegments(segments);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to extract segments');
    } finally {
      setExtractingSegments(false);
    }
  };

  const handleClearUrl = () => {
    setUrl('');
    handleUrlChange('');
    setError(null);
    setParsedData(null);
    setDownloadStatus(null);
    setDownloadProgress(null);
    setExtractedSegments(null);
    setSegmentDisplayCount(20);
  };

  const handleSelectHistoryUrl = (historyUrl: string) => {
    setUrl(historyUrl);
    handleUrlChange(historyUrl);
    setShowHistory(false);
  };

  const handleClearHistory = async () => {
    try {
      await invoke('clear_url_history');
      setUrlHistory([]);
    } catch (err) {
      console.error('Failed to clear history:', err);
    }
  };

  return (
    <div className="p-6 bg-white dark:bg-gray-800 rounded-lg shadow-lg">
      <h2 className="text-2xl font-bold mb-4 text-gray-900 dark:text-white">
        {t(language, 'm3u8Form.title')}
      </h2>
      
      <div className="space-y-4">
        <div className="relative">
          <label htmlFor="m3u8-url" className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
            {t(language, 'm3u8Form.urlLabel')}
          </label>
          <div className="relative">
            <input
              id="m3u8-url"
              type="url"
              value={url}
              onChange={(e) => handleUrlChange(e.target.value)}
              placeholder={t(language, 'm3u8Form.urlPlaceholder')}
              className="w-full px-3 py-2 pr-20 border border-gray-300 dark:border-gray-600 rounded-md shadow-sm focus:ring-blue-500 focus:border-blue-500 dark:bg-gray-700 dark:text-white"
              disabled={loading}
            />
            <div className="absolute inset-y-0 right-0 flex items-center">
              <button
                onClick={handleClearUrl}
                title={t(language, 'm3u8Form.clearUrl')}
                className="px-2 py-1 mr-1 text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-200"
                disabled={!url || loading}
              >
                <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                </svg>
              </button>
              <button
                onClick={() => setShowHistory(!showHistory)}
                title={t(language, 'm3u8Form.urlHistory')}
                className="px-2 py-1 mr-1 text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-200"
                disabled={loading}
              >
                <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z" />
                </svg>
              </button>
            </div>
          </div>

          {/* URL History Dropdown */}
          {showHistory && urlHistory.length > 0 && (
            <div className="absolute z-10 mt-1 left-0 right-0 bg-white dark:bg-gray-800 border border-gray-300 dark:border-gray-600 rounded-md shadow-lg max-h-60 overflow-auto">
              <div className="p-2 border-b border-gray-200 dark:border-gray-700 flex justify-between items-center">
                <span className="text-sm font-medium text-gray-700 dark:text-gray-300">{t(language, 'm3u8Form.recentUrls')}</span>
                <button
                  onClick={handleClearHistory}
                  className="px-2 py-1 text-xs bg-red-500 hover:bg-red-600 text-white rounded font-medium transition-colors"
                >
                  {t(language, 'm3u8Form.clearAll')}
                </button>
              </div>
              {urlHistory.map((item, index) => (
                <button
                  key={index}
                  onClick={() => handleSelectHistoryUrl(item.url)}
                  className="w-full px-3 py-2 text-left hover:bg-gray-100 dark:hover:bg-gray-700 border-b border-gray-100 dark:border-gray-700 last:border-b-0"
                >
                  <div className="text-sm text-gray-900 dark:text-white truncate">{item.url}</div>
                  <div className="text-xs text-gray-500 dark:text-gray-400">
                    {new Date(item.timestamp).toLocaleString()}
                  </div>
                </button>
              ))}
            </div>
          )}
        </div>

        <div className="flex gap-3">
          <button
            onClick={handleParse}
            disabled={loading || !url.trim()}
            className="px-4 py-2 bg-blue-500 hover:bg-blue-600 text-white rounded-lg font-medium disabled:bg-gray-400 disabled:cursor-not-allowed transition-colors"
          >
            {t(language, 'm3u8Form.parseButton')}
          </button>
          
          <button
            onClick={handleExtractSegments}
            disabled={extractingSegments || !url.trim()}
            className="px-4 py-2 bg-purple-500 hover:bg-purple-600 text-white rounded-lg font-medium disabled:bg-gray-400 disabled:cursor-not-allowed transition-colors"
          >
            {t(language, 'm3u8Form.extractSegmentsButton')}
          </button>
          
          <button
            onClick={handleDownload}
            disabled={!url.trim() || isDownloading}
            className="px-4 py-2 bg-green-500 hover:bg-green-600 text-white rounded-lg font-medium disabled:bg-gray-400 disabled:cursor-not-allowed transition-colors"
          >
            {t(language, 'm3u8Form.downloadButton')}
          </button>
          
        </div>

        {error && (
          <div className="p-3 bg-red-100 dark:bg-red-900/30 border border-red-400 dark:border-red-600 text-red-700 dark:text-red-400 rounded-md">
            {error}
          </div>
        )}

        {downloadStatus && (
          <div className={`p-3 rounded-md border ${
            downloadStatus.includes('completed') 
              ? 'bg-green-100 dark:bg-green-900/30 border-green-400 dark:border-green-700 text-green-700 dark:text-green-400'
              : downloadStatus.includes('cancelled')
              ? 'bg-yellow-100 dark:bg-yellow-900/30 border-yellow-400 dark:border-yellow-700 text-yellow-700 dark:text-yellow-400'
              : 'bg-blue-100 dark:bg-blue-900/30 border-blue-400 dark:border-blue-700 text-blue-700 dark:text-blue-400'
          }`}>
            {downloadStatus}
          </div>
        )}

        {downloadProgress && (
          <div className="p-3 bg-gray-100 dark:bg-gray-700 rounded-md mt-2">
            <pre className="text-xs text-gray-600 dark:text-gray-300 font-mono">{downloadProgress}</pre>
          </div>
        )}

        {parsedData && (
          <div className="mt-6 p-4 bg-gray-50 dark:bg-gray-900 rounded-md relative">
            <div className="flex justify-between items-start mb-3">
              <h3 className="text-lg font-semibold text-gray-900 dark:text-white">
                {t(language, 'm3u8Form.parsedPlaylist')} ({parsedData.type === 'master' ? t(language, 'm3u8Form.master') : t(language, 'm3u8Form.media')})
              </h3>
              <button
                onClick={() => setParsedData(null)}
                className="text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-200 p-1 rounded hover:bg-gray-200 dark:hover:bg-gray-700 transition-colors"
                title="Close"
              >
                <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                </svg>
              </button>
            </div>
            
            <div className="space-y-2 text-sm">
              {parsedData.version && (
                <div>
                  <span className="font-medium text-gray-600 dark:text-gray-400">{t(language, 'm3u8Form.version')}:</span>{' '}
                  <span className="text-gray-900 dark:text-white">{parsedData.version}</span>
                </div>
              )}
              
              {parsedData.targetDuration && (
                <div>
                  <span className="font-medium text-gray-600 dark:text-gray-400">{t(language, 'm3u8Form.targetDuration')}:</span>{' '}
                  <span className="text-gray-900 dark:text-white">{parsedData.targetDuration}s</span>
                </div>
              )}

              {parsedData.variants && parsedData.variants.length > 0 && (
                <div>
                  <h4 className="font-medium text-gray-600 dark:text-gray-400 mb-2">{t(language, 'm3u8Form.variants')}:</h4>
                  <div className="space-y-2 ml-4">
                    {parsedData.variants.map((variant, idx) => (
                      <div key={idx} className="p-2 bg-white dark:bg-gray-800 rounded border border-gray-200 dark:border-gray-700">
                        <div className="text-gray-900 dark:text-white">
                          <span className="font-medium">{t(language, 'm3u8Form.bandwidth')}:</span> {variant.bandwidth} bps
                        </div>
                        {variant.resolution && (
                          <div className="text-gray-600 dark:text-gray-400">
                            <span className="font-medium">{t(language, 'm3u8Form.resolution')}:</span> {variant.resolution}
                          </div>
                        )}
                        {variant.codecs && (
                          <div className="text-gray-600 dark:text-gray-400">
                            <span className="font-medium">{t(language, 'm3u8Form.codecs')}:</span> {variant.codecs}
                          </div>
                        )}
                        <div className="text-xs text-gray-500 dark:text-gray-500 mt-1 break-all">
                          {variant.uri}
                        </div>
                      </div>
                    ))}
                  </div>
                </div>
              )}

              {parsedData.segments && parsedData.segments.length > 0 && (
                <div>
                  <h4 className="font-medium text-gray-600 dark:text-gray-400 mb-2">
                    {t(language, 'm3u8Form.segments')} ({parsedData.segments.length} {t(language, 'm3u8Form.segmentsTotal')}):
                  </h4>
                  <div className="max-h-64 overflow-y-auto space-y-1 ml-4">
                    {parsedData.segments.slice(0, 10).map((segment, idx) => (
                      <div key={idx} className="p-2 bg-white dark:bg-gray-800 rounded border border-gray-200 dark:border-gray-700">
                        <div className="text-gray-900 dark:text-white">
                          <span className="font-medium">#{idx + 1}</span> - {t(language, 'm3u8Form.duration')}: {segment.duration}s
                        </div>
                        <div className="text-xs text-gray-500 dark:text-gray-500 break-all">
                          {segment.uri}
                        </div>
                      </div>
                    ))}
                    {parsedData.segments.length > 10 && (
                      <div className="text-gray-500 dark:text-gray-400 text-sm italic">
                        ... {tWithParams(language, 'm3u8Form.andMore', { count: parsedData.segments.length - 10 })}
                      </div>
                    )}
                  </div>
                </div>
              )}
            </div>
          </div>
        )}

        {extractedSegments && extractedSegments.length > 0 && (
          <div className="mt-6 p-4 bg-gray-50 dark:bg-gray-900 rounded-md relative">
            <div className="flex justify-between items-start mb-3">
              <h3 className="text-lg font-semibold text-gray-900 dark:text-white">
                {t(language, 'm3u8Form.extractedSegments')} ({extractedSegments.length} {t(language, 'm3u8Form.segmentsTotal')})
              </h3>
              <button
                onClick={() => {
                  setExtractedSegments(null);
                  setSegmentDisplayCount(20);
                  setCopiedSegmentIndex(null);
                  setCopiedAllSegments(false);
                }}
                className="text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-200 p-1 rounded hover:bg-gray-200 dark:hover:bg-gray-700 transition-colors"
                title="Close"
              >
                <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                </svg>
              </button>
            </div>
            
            <div className="max-h-64 overflow-y-auto space-y-1">
              {extractedSegments.slice(0, segmentDisplayCount).map((segment, idx) => (
                <div key={idx} className="p-2 bg-white dark:bg-gray-800 rounded border border-gray-200 dark:border-gray-700">
                  <div className="flex justify-between items-center">
                    <div className="text-xs text-gray-600 dark:text-gray-400 break-all flex-1">
                      #{idx + 1}: {segment}
                    </div>
                    <button
                      onClick={() => {
                        navigator.clipboard.writeText(segment);
                        setCopiedSegmentIndex(idx);
                        setTimeout(() => setCopiedSegmentIndex(null), 2000);
                      }}
                      className={`ml-2 px-2 py-1 text-xs rounded transition-colors ${
                        copiedSegmentIndex === idx 
                          ? 'bg-green-500 hover:bg-green-600 text-white' 
                          : 'bg-gray-100 hover:bg-gray-200 dark:bg-gray-700 dark:hover:bg-gray-600'
                      }`}
                      title={t(language, 'm3u8Form.copyUrl')}
                    >
                      {copiedSegmentIndex === idx ? t(language, 'mcpServer.copied') : t(language, 'mcpServer.copy')}
                    </button>
                  </div>
                </div>
              ))}
              {(extractedSegments.length > segmentDisplayCount || segmentDisplayCount > 20) && (
                <div className="flex justify-between items-center">
                  {segmentDisplayCount > 20 ? (
                    <button
                      onClick={() => setSegmentDisplayCount(20)}
                      className="py-2 px-4 text-sm text-gray-600 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-300 hover:bg-gray-50 dark:hover:bg-gray-800 rounded transition-colors cursor-pointer"
                    >
                      ← {t(language, 'm3u8Form.showLess')}
                    </button>
                  ) : (
                    <div></div>
                  )}
                  
                  {extractedSegments.length > segmentDisplayCount && (
                    <button
                      onClick={() => setSegmentDisplayCount(prev => Math.min(prev + 20, extractedSegments.length))}
                      className="py-2 px-4 text-sm text-blue-600 hover:text-blue-700 dark:text-blue-400 dark:hover:text-blue-300 hover:bg-blue-50 dark:hover:bg-blue-900/20 rounded transition-colors cursor-pointer"
                    >
                      ... {tWithParams(language, 'm3u8Form.andMore', { count: Math.min(20, extractedSegments.length - segmentDisplayCount) })} →
                    </button>
                  )}
                </div>
              )}
            </div>
            
            <div className="mt-4 flex gap-2">
              <button
                onClick={() => {
                  const segmentList = extractedSegments.join('\n');
                  navigator.clipboard.writeText(segmentList);
                  setCopiedAllSegments(true);
                  setTimeout(() => setCopiedAllSegments(false), 2000);
                }}
                className={`px-3 py-1 text-sm text-white rounded-lg font-medium transition-colors ${
                  copiedAllSegments 
                    ? 'bg-green-500 hover:bg-green-600' 
                    : 'bg-blue-500 hover:bg-blue-600'
                }`}
              >
                {copiedAllSegments ? t(language, 'mcpServer.copied') : t(language, 'm3u8Form.copyAllUrls')}
              </button>
              <button
                onClick={() => {
                  setExtractedSegments(null);
                  setSegmentDisplayCount(20);
                  setCopiedSegmentIndex(null);
                  setCopiedAllSegments(false);
                }}
                className="px-3 py-1 text-sm bg-gray-500 hover:bg-gray-600 text-white rounded-lg font-medium transition-colors"
              >
                {t(language, 'm3u8Form.clearSegments')}
              </button>
            </div>
          </div>
        )}
      </div>

      {/* Disclaimer Modal */}
      {showDisclaimer && (
        <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50 p-4">
          <div className="bg-white dark:bg-gray-800 rounded-lg shadow-2xl max-w-2xl w-full max-h-[80vh] overflow-y-auto">
            <div className="flex justify-between items-center p-6 border-b border-gray-200 dark:border-gray-700">
              <h2 className="text-xl font-semibold text-gray-800 dark:text-gray-200">
                {t(language, 'm3u8Form.disclaimer')}
              </h2>
              <button
                onClick={() => setShowDisclaimer(false)}
                className="text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-200"
              >
                <svg className="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                </svg>
              </button>
            </div>
            
            <div className="p-6">
              <div className="mb-6 p-4 bg-yellow-50 dark:bg-yellow-900/20 border-l-4 border-yellow-400 dark:border-yellow-600">
                <div className="text-sm text-gray-700 dark:text-gray-300 whitespace-pre-line">
                  {t(language, 'm3u8Form.disclaimerText')}
                </div>
              </div>
              
              <div className="flex justify-end gap-3">
                <button
                  onClick={() => setShowDisclaimer(false)}
                  className="px-4 py-2 bg-gray-200 dark:bg-gray-700 hover:bg-gray-300 dark:hover:bg-gray-600 text-gray-700 dark:text-gray-300 rounded-lg font-medium transition-colors"
                >
                  {t(language, 'm3u8Form.cancel')}
                </button>
                <button
                  onClick={() => {
                    setDisclaimerAccepted(true);
                    setShowDisclaimer(false);
                    handleDownload();
                  }}
                  className="px-4 py-2 bg-blue-500 hover:bg-blue-600 text-white rounded-lg font-medium transition-colors"
                >
                  {t(language, 'm3u8Form.iUnderstand')}
                </button>
              </div>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}