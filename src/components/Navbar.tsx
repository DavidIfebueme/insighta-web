'use client';

import Link from 'next/link';
import { useAuth } from '@/lib/useAuth';

export default function Navbar() {
  const { user, logout } = useAuth();

  if (!user) return null;

  return (
    <nav className="bg-white border-b border-gray-200 px-6 py-3 flex items-center justify-between">
      <div className="flex items-center gap-6">
        <Link href="/dashboard" className="text-xl font-bold text-indigo-600">
          Insighta Labs+
        </Link>
        <div className="flex gap-4">
          <Link href="/dashboard" className="text-gray-600 hover:text-gray-900">Dashboard</Link>
          <Link href="/profiles" className="text-gray-600 hover:text-gray-900">Profiles</Link>
          <Link href="/search" className="text-gray-600 hover:text-gray-900">Search</Link>
        </div>
      </div>
      <div className="flex items-center gap-4">
        <Link href="/account" className="flex items-center gap-2 text-gray-600 hover:text-gray-900">
          {user.avatar_url && (
            <img src={user.avatar_url} alt="" className="w-7 h-7 rounded-full" />
          )}
          <span>@{user.username}</span>
          <span className="text-xs bg-indigo-100 text-indigo-700 px-2 py-0.5 rounded">{user.role}</span>
        </Link>
        <button onClick={logout} className="text-gray-400 hover:text-gray-600 text-sm">Logout</button>
      </div>
    </nav>
  );
}
