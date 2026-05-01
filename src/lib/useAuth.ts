'use client';

import { useEffect, useState } from 'react';
import { useRouter } from 'next/navigation';
import { apiFetch } from '@/lib/api';
import { getAccessToken, getStoredUser, clearAuth } from '@/lib/auth';

interface User {
  id: string;
  username: string;
  email: string;
  avatar_url: string;
  role: string;
  is_active: boolean;
}

export function useAuth() {
  const [user, setUser] = useState<User | null>(null);
  const [loading, setLoading] = useState(true);
  const router = useRouter();

  useEffect(() => {
    checkAuth();
  }, []);

  async function checkAuth() {
    const token = getAccessToken();
    const stored = getStoredUser();
    if (!token) {
      setUser(null);
      setLoading(false);
      return;
    }
    try {
      const data = await apiFetch('/auth/me');
      setUser(data.data);
    } catch {
      if (stored) {
        setUser({ id: '', username: stored.username, email: '', avatar_url: '', role: stored.role, is_active: true } as User);
      } else {
        setUser(null);
        clearAuth();
      }
    } finally {
      setLoading(false);
    }
  }

  async function logout() {
    try {
      await apiFetch('/auth/logout', { method: 'POST' });
    } finally {
      clearAuth();
      setUser(null);
      router.push('/login');
    }
  }

  return { user, loading, logout, checkAuth };
}
