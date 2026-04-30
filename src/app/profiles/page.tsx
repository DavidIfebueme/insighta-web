'use client';

import { useEffect, useState } from 'react';
import { useRouter } from 'next/navigation';
import { useAuth } from '@/lib/useAuth';
import { apiFetch, API_URL } from '@/lib/api';
import Navbar from '@/components/Navbar';
import Link from 'next/link';

interface Profile {
  id: string;
  name: string;
  gender: string;
  age: number;
  age_group: string;
  country_id: string;
  country_name: string;
  gender_probability: number;
}

export default function ProfilesPage() {
  const { user, loading } = useAuth();
  const router = useRouter();
  const [profiles, setProfiles] = useState<Profile[]>([]);
  const [page, setPage] = useState(1);
  const [totalPages, setTotalPages] = useState(1);
  const [total, setTotal] = useState(0);
  const [gender, setGender] = useState('');
  const [ageGroup, setAgeGroup] = useState('');
  const [country, setCountry] = useState('');

  useEffect(() => {
    if (!loading && !user) router.push('/login');
  }, [user, loading, router]);

  useEffect(() => {
    if (user) loadProfiles();
  }, [user, page, gender, ageGroup, country]);

  async function loadProfiles() {
    try {
      let path = `/api/profiles?page=${page}&limit=10`;
      if (gender) path += `&gender=${gender}`;
      if (ageGroup) path += `&age_group=${ageGroup}`;
      if (country) path += `&country_id=${country}`;
      const data = await apiFetch(path);
      setProfiles(data.data || []);
      setTotalPages(data.total_pages || 1);
      setTotal(data.total || 0);
    } catch {}
  }

  if (loading) return <div className="flex items-center justify-center min-h-screen">Loading...</div>;
  if (!user) return null;

  return (
    <div>
      <Navbar />
      <div className="max-w-7xl mx-auto px-6 py-8">
        <div className="flex items-center justify-between mb-6">
          <h1 className="text-3xl font-bold text-gray-900">Profiles</h1>
          <div className="flex gap-2">
            <a href={`${API_URL}/api/profiles/export?format=csv`} className="bg-green-600 text-white px-4 py-2 rounded text-sm hover:bg-green-700">Export CSV</a>
            {user.role === 'admin' && (
              <Link href="/profiles/create" className="bg-indigo-600 text-white px-4 py-2 rounded text-sm hover:bg-indigo-700">Create Profile</Link>
            )}
          </div>
        </div>

        <div className="bg-white rounded-xl shadow p-4 mb-6 flex gap-4 flex-wrap">
          <select value={gender} onChange={e => { setGender(e.target.value); setPage(1); }} className="border rounded px-3 py-2 text-sm">
            <option value="">All Genders</option>
            <option value="male">Male</option>
            <option value="female">Female</option>
          </select>
          <select value={ageGroup} onChange={e => { setAgeGroup(e.target.value); setPage(1); }} className="border rounded px-3 py-2 text-sm">
            <option value="">All Age Groups</option>
            <option value="child">Child</option>
            <option value="teenager">Teenager</option>
            <option value="adult">Adult</option>
            <option value="senior">Senior</option>
          </select>
          <input value={country} onChange={e => { setCountry(e.target.value); setPage(1); }} placeholder="Country code (e.g. NG)" className="border rounded px-3 py-2 text-sm w-40" />
        </div>

        <div className="bg-white rounded-xl shadow overflow-hidden">
          <table className="w-full">
            <thead className="bg-gray-50">
              <tr>
                <th className="px-4 py-3 text-left text-xs font-medium text-gray-500 uppercase">Name</th>
                <th className="px-4 py-3 text-left text-xs font-medium text-gray-500 uppercase">Gender</th>
                <th className="px-4 py-3 text-left text-xs font-medium text-gray-500 uppercase">Age</th>
                <th className="px-4 py-3 text-left text-xs font-medium text-gray-500 uppercase">Age Group</th>
                <th className="px-4 py-3 text-left text-xs font-medium text-gray-500 uppercase">Country</th>
                <th className="px-4 py-3 text-left text-xs font-medium text-gray-500 uppercase">Actions</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-gray-200">
              {profiles.map(p => (
                <tr key={p.id} className="hover:bg-gray-50">
                  <td className="px-4 py-3 text-sm font-medium text-gray-900">{p.name}</td>
                  <td className="px-4 py-3 text-sm text-gray-600">{p.gender}</td>
                  <td className="px-4 py-3 text-sm text-gray-600">{p.age}</td>
                  <td className="px-4 py-3 text-sm text-gray-600">{p.age_group}</td>
                  <td className="px-4 py-3 text-sm text-gray-600">{p.country_name || p.country_id}</td>
                  <td className="px-4 py-3 text-sm">
                    <Link href={`/profiles/${p.id}`} className="text-indigo-600 hover:text-indigo-900">View</Link>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>

        <div className="flex items-center justify-between mt-6">
          <p className="text-sm text-gray-500">Total: {total} profiles</p>
          <div className="flex gap-2">
            <button onClick={() => setPage(Math.max(1, page - 1))} disabled={page === 1} className="px-3 py-1 border rounded text-sm disabled:opacity-50">Previous</button>
            <span className="px-3 py-1 text-sm text-gray-600">Page {page} of {totalPages}</span>
            <button onClick={() => setPage(Math.min(totalPages, page + 1))} disabled={page === totalPages} className="px-3 py-1 border rounded text-sm disabled:opacity-50">Next</button>
          </div>
        </div>
      </div>
    </div>
  );
}
