import { useRouter } from 'next/router';
import { useEffect, useState } from 'react';
import Head from 'next/head';
import { AlertCircle, Loader } from 'lucide-react';
import { ResultViewer } from '../components/Resultviewer';
import { NutritionLabel } from '../components/NutritionLabel';
import { HeaderNav } from '../components/HeaderNav';

interface SharedResultData {
  share_id: string;
  result: any;
  metadata?: any;
  created_at: string;
  access_count: number;
}

export default function SharedResultPage() {
  const router = useRouter();
  const { id } = router.query;
  const [data, setData] = useState<SharedResultData | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!id) return;

    const fetchSharedResult = async () => {
      try {
        setIsLoading(true);
        setError(null);

        const response = await fetch(`/api/shares/${id}`);

        if (response.status === 404) {
          setError('Share not found');
          return;
        }

        if (response.status === 410) {
          setError('This share has expired');
          return;
        }

        if (!response.ok) {
          setError(`Failed to load share: ${response.statusText}`);
          return;
        }

        const result = await response.json();
        setData(result);
      } catch (err) {
        setError(err instanceof Error ? err.message : 'Failed to load shared result');
      } finally {
        setIsLoading(false);
      }
    };

    fetchSharedResult();
  }, [id]);

  if (!id) {
    return <div>Loading...</div>;
  }

  return (
    <>
      <Head>
        <title>Shared Profiling Result - Sky Moon Scope</title>
        <meta name="description" content="View shared Soroban contract profiling results" />
      </Head>

      <div className="min-h-screen bg-gradient-to-br from-slate-900 via-purple-900 to-slate-900">
        <HeaderNav currentTab="share" />

        <main className="container mx-auto px-4 py-8">
          <div className="mb-8">
            <h1 className="text-4xl font-bold text-white mb-2">Shared Profiling Result</h1>
            <p className="text-gray-400">
              Viewing a shared analysis result from the Sky Moon Scope profiler
            </p>
          </div>

          {isLoading && (
            <div className="flex flex-col items-center justify-center py-16">
              <Loader className="animate-spin text-blue-500 mb-4" size={32} />
              <p className="text-gray-400">Loading shared result...</p>
            </div>
          )}

          {error && (
            <div className="bg-red-50 border border-red-200 rounded-lg p-6 mb-6">
              <div className="flex gap-3">
                <AlertCircle className="text-red-600 flex-shrink-0" size={24} />
                <div>
                  <h3 className="font-semibold text-red-900">Unable to Load Result</h3>
                  <p className="text-red-700 mt-1">{error}</p>
                  {error === 'This share has expired' && (
                    <p className="text-sm text-red-600 mt-2">
                      The share link has expired. Please ask the original user to generate a new share link.
                    </p>
                  )}
                </div>
              </div>
            </div>
          )}

          {data && (
            <div className="space-y-8">
              {/* Share Info */}
              <div className="bg-slate-800 rounded-lg border border-slate-700 p-6">
                <div className="grid grid-cols-2 gap-4 md:gap-8">
                  <div>
                    <p className="text-gray-400 text-sm">Share ID</p>
                    <p className="text-white font-mono text-lg">{data.share_id}</p>
                  </div>
                  <div>
                    <p className="text-gray-400 text-sm">Access Count</p>
                    <p className="text-white text-lg font-semibold">{data.access_count}</p>
                  </div>
                  <div>
                    <p className="text-gray-400 text-sm">Created</p>
                    <p className="text-white text-sm">
                      {new Date(data.created_at).toLocaleString()}
                    </p>
                  </div>
                  {data.metadata?.contract && (
                    <div>
                      <p className="text-gray-400 text-sm">Contract</p>
                      <p className="text-white text-sm font-mono truncate">{data.metadata.contract}</p>
                    </div>
                  )}
                </div>
              </div>

              {/* Nutrition Label */}
              {data.result && (
                <>
                  <div>
                    <h2 className="text-2xl font-bold text-white mb-4">Contract Nutrition Label</h2>
                    <NutritionLabel result={data.result} />
                  </div>

                  {/* Detailed Results */}
                  <div>
                    <h2 className="text-2xl font-bold text-white mb-4">Detailed Analysis</h2>
                    <ResultViewer result={data.result} />
                  </div>
                </>
              )}

              {/* Disclaimer */}
              <div className="bg-slate-800 border border-slate-700 rounded-lg p-4">
                <p className="text-gray-400 text-sm">
                  ℹ️ This is a shared read-only view of a profiling result. To run your own analysis,
                  visit the <a href="/" className="text-blue-400 hover:underline">main profiler</a>.
                </p>
              </div>
            </div>
          )}
        </main>
      </div>
    </>
  );
}
