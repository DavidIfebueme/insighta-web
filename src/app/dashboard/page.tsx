'use client';

import { useEffect, useState } from 'react';
import { useRouter } from 'next/navigation';
import { useAuth } from '@/lib/useAuth';
import { apiFetch } from '@/lib/api';
import Navbar from '@/components/Navbar';

export default function DashboardPage() {
  const { user, loading } = useAuth();
  const router = useRouter();
  const [stats, setStats] = useState({ total: 0, male: 0, female: 0, countries: 0 });

  useEffect(() => {
    if (!loading && !user) router.push('/login');
  }, [user, loading, router]);

  useEffect(() => {
    if (user) loadStats();
  }, [user]);

  async function loadStats() {
    try {
      const data = await apiFetch('/api/profiles?limit=1');
      const total = data.total || 0;
      const maleData = await apiFetch('/api/profiles?gender=male&limit=1');
      const femaleData = await apiFetch('/api/profiles?gender=female&limit=1');
      setStats({
        total,
        male: maleData.total || 0,
        female: femaleData.total || 0,
        countries: 0,
      });
    } catch {}
  }

  if (loading) return <div className="flex items-center justify-center min-h-screen">Loading...</div>;
  if (!user) return null;

  return (
    <div>
      <Navbar />
      <div className="max-w-7xl mx-auto px-6 py-8">
        <h1 className="text-3xl font-bold text-gray-900 mb-6">Dashboard</h1>
        <div className="grid grid-cols-1 md:grid-cols-3 gap-6">
          <div className="bg-white rounded-xl shadow p-6">
            <h3 className="text-gray-500 text-sm font-medium">Total Profiles</h3>
            <p className="text-3xl font-bold text-gray-900">{stats.total}</p>
          </div>
          <div className="bg-white rounded-xl shadow p-6">
            <h3 className="text-gray-500 text-sm font-medium">Male Profiles</h3>
            <p className="text-3xl font-bold text-blue-600">{stats.male}</p>
          </div>
          <div className="bg-white rounded-xl shadow p-6">
            <h3 className="text-gray-500 text-sm font-medium">Female Profiles</h3>
            <p className="text-3xl font-bold text-pink-600">{stats.female}</p>
          </div>
        </div>
      </div>
    </div>
  );
}
