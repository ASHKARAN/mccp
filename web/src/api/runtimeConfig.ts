const LS_HTTP = 'mccp.httpBaseUrl';
const LS_WS = 'mccp.wsUrl';

export function getHttpBaseUrl(): string {
  const v = localStorage.getItem(LS_HTTP) || import.meta.env.VITE_MCCP_HTTP_URL;
  return (v || 'http://localhost:7422').replace(/\/+$/, '');
}

export function setHttpBaseUrl(v: string) {
  localStorage.setItem(LS_HTTP, v.replace(/\/+$/, ''));
}

function deriveWsFromHttp(httpBaseUrl: string): string {
  const wsBase = httpBaseUrl
    .replace(/^https:/, 'wss:')
    .replace(/^http:/, 'ws:');
  return `${wsBase.replace(/\/+$/, '')}/ws`;
}

export function getWsUrl(): string {
  const explicit = localStorage.getItem(LS_WS) || import.meta.env.VITE_MCCP_WS_URL;
  if (explicit && explicit.trim().length > 0) return explicit;
  return deriveWsFromHttp(getHttpBaseUrl());
}

export function setWsUrl(v: string) {
  localStorage.setItem(LS_WS, v);
}
