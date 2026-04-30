'use client';

import { useRouter } from 'next/navigation';
import { useAuth } from '@/lib/useAuth';
import Navbar from '@/components/Navbar';

export default function AccountPage() {
  const { user, loading, logout } = useAuth();
  const router = useRouter();

  if (loading) return <div className="flex items-center justify-center min-h-screen">Loading...</div>;
  if (!user) { router.push('/login'); return null; }

  return (
    <div>
      <Navbar />
      <div className="max-w-3xl mx-auto px-6 py-8">
        <h1 className="text-3xl font-bold text-gray-900 mb-6">Account</h1>
        <div className="bg-white rounded-xl shadow p-6">
          <div className="flex items-center gap-4 mb-6">
            {user.avatar_url && (
              <img src={user.avatar_url} alt="" className="w-16 h-16 rounded-full" />
            )}
            <div>
              <h2 className="text-xl font-bold text-gray-900">@{user.username}</h2>
              <span className="text-xs bg-indigo-100 text-indigo-700 px-2 py-1 rounded">{user.role}</span>
            </div>
          </div>
          <dl className="space-y-4">
            <div><dt className="text-sm text-gray-500">ID</dt><dd className="text-sm font-medium text-gray-900">{user.id}</dd></div>
            <div><dt className="text-sm text-gray-500">Email</dt><dd className="text-sm font-medium text-gray-900">{user.email}</dd></div>
            <div><dt className="text-sm text-gray-500">Role</dt><dd className="text-sm font-medium text-gray-900">{user.role}</dd></div>
            <div><dt className="text-sm text-gray-500">Active</dt><dd className="text-sm font-medium text-gray-900">{user.is_active ? 'Yes' : 'No'}</dd></div>
          </dl>
          <div className="mt-6 pt-6 border-t">
            <button onClick={logout} className="bg-red-600 text-white px-4 py-2 rounded hover:bg-red-700 text-sm">Logout</button>
          </div>
        </div>
      </div>
    </div>
  );
}
