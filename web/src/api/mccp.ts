import { fetchJson } from './http';

export type HealthResponse = { status: string; version: string };

export type IndexingStatus = {
  project_id: string;
  file_count: number;
  indexed_files: number;
  queue_depth: number;
  is_watching: boolean;
};

export type IndexProgress = {
  phase: string;
  current: number;
  total: number;
  percentage: number;
  project_id: string;
};

// Existing endpoints today
export const mccp = {
  health: () => fetchJson<HealthResponse>('/health'),
  indexStatus: () => fetchJson<IndexingStatus>('/index/status'),
  listProjects: () => fetchJson<unknown[]>('/projects'),
};
