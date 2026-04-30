'use client';

import { useEffect, useState } from 'react';
import { useRouter, useParams } from 'next/navigation';
import { useAuth } from '@/lib/useAuth';
import { apiFetch } from '@/lib/api';
import Navbar from '@/components/Navbar';

export default function ProfileDetailPage() {
  const { user, loading } = useAuth();
  const router = useRouter();
  const params = useParams();
  const [profile, setProfile] = useState<any>(null);

  useEffect(() => {
    if (!loading && !user) router.push('/login');
  }, [user, loading, router]);

  useEffect(() => {
    if (user && params.id) loadProfile();
  }, [user, params.id]);

  async function loadProfile() {
    try {
      const data = await apiFetch(`/api/profiles/${params.id}`);
      setProfile(data.data);
    } catch {
      router.push('/profiles');
    }
  }

  if (loading || !profile) return <div className="flex items-center justify-center min-h-screen">Loading...</div>;
  if (!user) return null;

  return (
    <div>
      <Navbar />
      <div className="max-w-3xl mx-auto px-6 py-8">
        <button onClick={() => router.push('/profiles')} className="text-indigo-600 hover:text-indigo-900 mb-4 inline-block">&larr; Back to Profiles</button>
        <div className="bg-white rounded-xl shadow p-6">
          <h1 className="text-2xl font-bold text-gray-900 mb-4">{profile.name}</h1>
          <dl className="grid grid-cols-2 gap-4">
            <div><dt className="text-sm text-gray-500">Gender</dt><dd className="text-sm font-medium text-gray-900">{profile.gender}</dd></div>
            <div><dt className="text-sm text-gray-500">Gender Probability</dt><dd className="text-sm font-medium text-gray-900">{profile.gender_probability}</dd></div>
            <div><dt className="text-sm text-gray-500">Age</dt><dd className="text-sm font-medium text-gray-900">{profile.age}</dd></div>
            <div><dt className="text-sm text-gray-500">Age Group</dt><dd className="text-sm font-medium text-gray-900">{profile.age_group}</dd></div>
            <div><dt className="text-sm text-gray-500">Country</dt><dd className="text-sm font-medium text-gray-900">{profile.country_name} ({profile.country_id})</dd></div>
            <div><dt className="text-sm text-gray-500">Country Probability</dt><dd className="text-sm font-medium text-gray-900">{profile.country_probability}</dd></div>
            <div><dt className="text-sm text-gray-500">Created At</dt><dd className="text-sm font-medium text-gray-900">{profile.created_at}</dd></div>
          </dl>
        </div>
      </div>
    </div>
  );
}
