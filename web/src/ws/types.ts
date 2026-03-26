export type WsStatus = 'connecting' | 'connected' | 'disconnected';

export type WsEnvelope<T = unknown> = {
  type: string;
  ts?: string; // RFC3339
  data: T;
};

export type SystemStatus = {
  version: string;
  started_at: string;
  uptime_ms: number;
  pid?: number;
  http_addr?: string;
};

export type SystemMetrics = {
  cpu_percent: number;
  ram_used_bytes: number;
  ram_total_bytes: number;
  load_avg_1?: number;
};

export type TaskState = 'queued' | 'running' | 'finished' | 'failed' | 'canceled';

export type TaskInfo = {
  id: string;
  kind: 'index' | 'reindex' | 'watch' | 'query' | 'other';
  project_id?: string;
  state: TaskState;
  title: string;
  created_at: string;
  started_at?: string;
  finished_at?: string;
  progress?: { current: number; total: number; percentage: number; phase?: string };
  error?: string;
};

export type ProjectInfo = {
  id: string;
  name: string;
  root_path: string;
  status: 'not_indexed' | 'indexing' | 'indexed' | 'error';
  last_indexed_at?: string;
  file_count?: number;
  chunk_count?: number;
};
