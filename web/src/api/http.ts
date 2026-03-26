import { getHttpBaseUrl } from './runtimeConfig';

export type HttpMethod = 'GET' | 'POST' | 'PUT' | 'PATCH' | 'DELETE';

export async function fetchJson<T>(
  path: string,
  opts: { method?: HttpMethod; body?: unknown; headers?: Record<string, string> } = {}
): Promise<T> {
  const base = getHttpBaseUrl();
  const url = path.startsWith('http') ? path : `${base}${path}`;

  const res = await fetch(url, {
    method: opts.method || 'GET',
    headers: {
      'Content-Type': 'application/json',
      ...(opts.headers || {}),
    },
    body: opts.body === undefined ? undefined : JSON.stringify(opts.body),
  });

  if (!res.ok) {
    const text = await res.text().catch(() => '');
    throw new Error(`${res.status} ${res.statusText}${text ? `: ${text}` : ''}`);
  }

  // Some endpoints may return empty bodies.
  const text = await res.text();
  if (!text) return undefined as T;
  return JSON.parse(text) as T;
}
