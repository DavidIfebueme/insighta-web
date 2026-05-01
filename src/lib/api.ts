const API_URL = process.env.NEXT_PUBLIC_API_URL || 'http://localhost:8080';

import { getAccessToken } from '@/lib/auth';

export async function apiFetch(path: string, options: RequestInit = {}) {
  const token = getAccessToken();
  const headers: Record<string, string> = {
    'Content-Type': 'application/json',
    'X-API-Version': '1',
    ...(options.headers as Record<string, string> || {}),
  };
  if (token) {
    headers['Authorization'] = `Bearer ${token}`;
  }

  const res = await fetch(`${API_URL}${path}`, {
    ...options,
    headers,
  });

  const data = await res.json();

  if (data.status === 'error') {
    throw new Error(data.message || 'API error');
  }

  return data;
}

export function getLoginURL() {
  return `${API_URL}/auth/github?web=true`;
}

export function getLogoutURL() {
  return `${API_URL}/auth/logout`;
}

export { API_URL };
