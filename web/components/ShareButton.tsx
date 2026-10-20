import React, { useState } from 'react';
import { Share2, Copy, Check } from 'lucide-react';
import { Toast } from './Toast';

interface ShareButtonProps {
  resultId?: string;
  functionName?: string;
  data: any; // The profiling result to share
  onShareSuccess?: (shareUrl: string) => void;
}

export const ShareButton: React.FC<ShareButtonProps> = ({
  resultId,
  functionName,
  data,
  onShareSuccess,
}) => {
  const [isOpen, setIsOpen] = useState(false);
  const [isLoading, setIsLoading] = useState(false);
  const [shareUrl, setShareUrl] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const [ttlDays, setTtlDays] = useState(30);
  const [error, setError] = useState<string | null>(null);

  const handleGenerateShareLink = async () => {
    setIsLoading(true);
    setError(null);

    try {
      const response = await fetch('/api/shares', {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({
          result: data,
          ttl_days: ttlDays,
          is_public: true,
        }),
      });

      if (!response.ok) {
        throw new Error(`Failed to create share: ${response.statusText}`);
      }

      const result = await response.json();
      setShareUrl(result.share_url);
      onShareSuccess?.(result.share_url);
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Failed to create share';
      setError(message);
    } finally {
      setIsLoading(false);
    }
  };

  const handleCopyToClipboard = async () => {
    if (!shareUrl) return;

    try {
      await navigator.clipboard.writeText(shareUrl);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch (err) {
      setError('Failed to copy to clipboard');
    }
  };

  return (
    <>
      <button
        onClick={() => setIsOpen(!isOpen)}
        className="flex items-center gap-2 px-4 py-2 rounded-lg border border-gray-300 hover:bg-gray-50 transition-colors"
        title="Share this profiling result"
      >
        <Share2 size={18} />
        <span>Share</span>
      </button>

      {isOpen && (
        <div className="fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50">
          <div className="bg-white rounded-lg shadow-lg p-6 max-w-md w-full mx-4">
            <h2 className="text-xl font-semibold mb-4">Share Profiling Result</h2>

            {!shareUrl ? (
              <div className="space-y-4">
                <div>
                  <label className="block text-sm font-medium text-gray-700 mb-2">
                    Expiration (days)
                  </label>
                  <input
                    type="number"
                    min="1"
                    max="90"
                    value={ttlDays}
                    onChange={(e) => setTtlDays(Math.min(90, Math.max(1, parseInt(e.target.value) || 1)))}
                    className="w-full px-3 py-2 border border-gray-300 rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500"
                  />
                  <p className="text-xs text-gray-500 mt-1">Link will expire after this period</p>
                </div>

                {error && (
                  <div className="bg-red-50 border border-red-200 rounded-lg p-3">
                    <p className="text-sm text-red-700">{error}</p>
                  </div>
                )}

                <button
                  onClick={handleGenerateShareLink}
                  disabled={isLoading}
                  className="w-full px-4 py-2 bg-blue-500 text-white rounded-lg hover:bg-blue-600 disabled:opacity-50 transition-colors"
                >
                  {isLoading ? 'Generating...' : 'Generate Share Link'}
                </button>
              </div>
            ) : (
              <div className="space-y-4">
                <div>
                  <label className="block text-sm font-medium text-gray-700 mb-2">
                    Share URL
                  </label>
                  <div className="flex gap-2">
                    <input
                      type="text"
                      value={shareUrl}
                      readOnly
                      className="flex-1 px-3 py-2 border border-gray-300 rounded-lg bg-gray-50"
                    />
                    <button
                      onClick={handleCopyToClipboard}
                      className="px-3 py-2 bg-gray-200 hover:bg-gray-300 rounded-lg transition-colors flex items-center gap-2"
                    >
                      {copied ? <Check size={18} /> : <Copy size={18} />}
                      {copied ? 'Copied!' : 'Copy'}
                    </button>
                  </div>
                </div>

                <button
                  onClick={() => setIsOpen(false)}
                  className="w-full px-4 py-2 bg-gray-200 text-gray-800 rounded-lg hover:bg-gray-300 transition-colors"
                >
                  Close
                </button>
              </div>
            )}
          </div>
        </div>
      )}
    </>
  );
};
