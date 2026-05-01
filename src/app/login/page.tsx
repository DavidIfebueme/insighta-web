'use client';

import { useState } from 'react';
import { getLoginURL, apiFetch } from '@/lib/api';
import { storeAuth } from '@/lib/auth';
import { useRouter } from 'next/navigation';

export default function LoginPage() {
  const [devLoading, setDevLoading] = useState(false);
  const router = useRouter();

  async function handleDevLogin(role: string) {
    setDevLoading(true);
    try {
      const data = await apiFetch('/auth/dev-login', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', 'X-API-Version': '' },
        body: JSON.stringify({ username: `${role}_demo`, role }),
      });
      storeAuth(data.access_token, data.refresh_token, data.user.username, data.user.role);
      router.push('/dashboard');
    } catch (err) {
      console.error('Dev login failed:', err);
    } finally {
      setDevLoading(false);
    }
  }

  return (
    <div className="min-h-screen flex items-center justify-center bg-gray-50">
      <div className="bg-white p-8 rounded-xl shadow-lg max-w-md w-full text-center">
        <h1 className="text-3xl font-bold text-gray-900 mb-2">Welcome Back</h1>
        <p className="text-gray-500 mb-6">Sign in to access Insighta Labs+</p>
        <a
          href={getLoginURL()}
          className="inline-flex items-center gap-2 bg-gray-900 text-white px-6 py-3 rounded-lg hover:bg-gray-800 transition w-full justify-center mb-4"
        >
          <svg className="w-5 h-5" fill="currentColor" viewBox="0 0 24 24"><path d="M12 0c-6.626 0-12 5.373-12 12 0 5.302 3.438 9.8 8.207 11.387.599.111.793-.261.793-.577v-2.234c-3.338.726-4.033-1.416-4.033-1.416-.546-1.387-1.333-1.756-1.333-1.756-1.089-.745.083-.729.083-.729 1.205.084 1.839 1.237 1.839 1.237 1.07 1.834 2.807 1.304 3.492.997.107-.775.418-1.305.762-1.604-2.665-.305-5.467-1.334-5.467-5.931 0-1.311.469-2.381 1.236-3.221-.124-.303-.535-1.524.117-3.176 0 0 1.008-.322 3.301 1.23.957-.266 1.983-.399 3.003-.404 1.02.005 2.047.138 3.006.404 2.291-1.552 3.297-1.23 3.297-1.23.653 1.653.242 2.874.118 3.176.77.84 1.235 1.911 1.235 3.221 0 4.609-2.807 5.624-5.479 5.921.43.372.823 1.102.823 2.222v3.293c0 .319.192.694.801.576 4.765-1.589 8.199-6.086 8.199-11.386 0-6.627-5.373-12-12-12z"/></svg>
          Continue with GitHub
        </a>
        <div className="border-t pt-4 mt-4">
          <p className="text-xs text-gray-400 mb-3">Quick access (demo)</p>
          <div className="flex gap-2">
            <button
              onClick={() => handleDevLogin('admin')}
              disabled={devLoading}
              className="flex-1 bg-indigo-600 text-white px-4 py-2 rounded-lg hover:bg-indigo-700 transition text-sm disabled:opacity-50"
            >
              Admin
            </button>
            <button
              onClick={() => handleDevLogin('analyst')}
              disabled={devLoading}
              className="flex-1 bg-gray-600 text-white px-4 py-2 rounded-lg hover:bg-gray-700 transition text-sm disabled:opacity-50"
            >
              Analyst
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
