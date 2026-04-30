'use client';

import { useState } from 'react';
import { useRouter } from 'next/navigation';
import { useAuth } from '@/lib/useAuth';
import { apiFetch } from '@/lib/api';
import Navbar from '@/components/Navbar';
import Link from 'next/link';

export default function SearchPage() {
  const { user, loading } = useAuth();
  const router = useRouter();
  const [query, setQuery] = useState('');
  const [results, setResults] = useState<any[]>([]);
  const [searched, setSearched] = useState(false);
  const [total, setTotal] = useState(0);

  async function handleSearch() {
    if (!query.trim()) return;
    try {
      const data = await apiFetch(`/api/profiles/search?q=${encodeURIComponent(query)}`);
      setResults(data.data || []);
      setTotal(data.total || 0);
      setSearched(true);
    } catch {}
  }

  if (loading) return <div className="flex items-center justify-center min-h-screen">Loading...</div>;
  if (!user) { router.push('/login'); return null; }

  return (
    <div>
      <Navbar />
      <div className="max-w-4xl mx-auto px-6 py-8">
        <h1 className="text-3xl font-bold text-gray-900 mb-6">Search Profiles</h1>
        <div className="flex gap-2 mb-6">
          <input
            value={query}
            onChange={e => setQuery(e.target.value)}
            onKeyDown={e => e.key === 'Enter' && handleSearch()}
            placeholder="e.g. young males from nigeria"
            className="flex-1 border rounded-lg px-4 py-3 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500"
          />
          <button onClick={handleSearch} className="bg-indigo-600 text-white px-6 py-3 rounded-lg hover:bg-indigo-700 text-sm">Search</button>
        </div>

        {searched && (
          <p className="text-sm text-gray-500 mb-4">{total} results found</p>
        )}

        <div className="space-y-3">
          {results.map(p => (
            <Link key={p.id} href={`/profiles/${p.id}`} className="block bg-white rounded-xl shadow p-4 hover:shadow-md transition">
              <div className="flex items-center justify-between">
                <div>
                  <h3 className="font-medium text-gray-900">{p.name}</h3>
                  <p className="text-sm text-gray-500">{p.gender} | Age {p.age} | {p.country_name || p.country_id}</p>
                </div>
                <span className="text-xs bg-gray-100 text-gray-600 px-2 py-1 rounded">{p.age_group}</span>
              </div>
            </Link>
          ))}
        </div>
      </div>
    </div>
  );
}
