import { fetchJson } from './http';
import type { ModuleInfo, ProjectInfo, SystemMetrics, SystemStatus, TaskInfo } from '../ws/types';

export type ConfigGetResponse = {
  path?: string;
  toml: string;
  parsed?: unknown;
};

export const admin = {
  systemStatus: () => fetchJson<SystemStatus>('/api/v1/system/status'),
  systemMetrics: () => fetchJson<SystemMetrics>('/api/v1/system/metrics'),

  getConfig: () => fetchJson<ConfigGetResponse>('/api/v1/config'),
  putConfig: (toml: string) => fetchJson<{ ok: boolean; restart_required?: boolean }>('/api/v1/config', { method: 'PUT', body: { toml } }),

  listLogs: (params: { level?: string; q?: string; target?: string; since?: string; until?: string; limit?: number; cursor?: string } = {}) => {
    const usp = new URLSearchParams();
    for (const [k, v] of Object.entries(params)) {
      if (v === undefined || v === null || v === '') continue;
      usp.set(k, String(v));
    }
    const qs = usp.toString();
    return fetchJson<{ items: import('../ws/types').LogLine[]; next_cursor?: string }>(`/api/v1/logs${qs ? `?${qs}` : ''}`);
  },

  listProjects: () => fetchJson<ProjectInfo[]>('/api/v1/projects'),
  createProject: (req: { name: string; root_path: string; watch?: boolean; index_immediately?: boolean; languages?: string[]; modules?: ModuleInfo[]; description?: string }) =>
    fetchJson<{ id: string }>('/api/v1/projects', { method: 'POST', body: req }),
  patchProject: (projectId: string, req: Partial<{ name: string; watch: boolean; languages: string[]; modules: ModuleInfo[]; description: string | null }>) =>
    fetchJson<{ ok: boolean }>(`/api/v1/projects/${encodeURIComponent(projectId)}`, { method: 'PATCH', body: req }),
  deleteProject: (projectId: string) =>
    fetchJson<{ ok: boolean }>(`/api/v1/projects/${encodeURIComponent(projectId)}`, { method: 'DELETE' }),
  reindexProject: (projectId: string, force = true) =>
    fetchJson<{ task_id: string }>(`/api/v1/projects/${encodeURIComponent(projectId)}/reindex`, {
      method: 'POST',
      body: { force },
    }),
  detectLanguages: (projectId: string) =>
    fetchJson<{ languages: string[]; modules: ModuleInfo[] }>(
      `/api/v1/projects/${encodeURIComponent(projectId)}/detect-languages`
    ),

  listTasks: (params: { state?: 'active' | 'finished' | 'all'; project_id?: string } = {}) => {
    const usp = new URLSearchParams();
    if (params.state) usp.set('state', params.state);
    if (params.project_id) usp.set('project_id', params.project_id);
    const q = usp.toString();
    return fetchJson<TaskInfo[]>(`/api/v1/tasks${q ? `?${q}` : ''}`);
  },
  cancelTask: (taskId: string) =>
    fetchJson<{ ok: boolean }>(`/api/v1/tasks/${encodeURIComponent(taskId)}/cancel`, { method: 'POST' }),
};
