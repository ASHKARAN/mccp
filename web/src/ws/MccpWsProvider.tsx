import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState } from 'react';
import { getWsUrl } from '../api/runtimeConfig';
import type { IndexProgress } from '../api/mccp';
import type { ProjectInfo, SystemMetrics, SystemStatus, TaskInfo, WsEnvelope, WsStatus } from './types';

type WsState = {
  status: WsStatus;
  lastMessageAt: number | null;
  systemStatus: SystemStatus | null;
  systemMetrics: SystemMetrics | null;
  indexProgress: IndexProgress | null;
  tasks: TaskInfo[];
  projects: ProjectInfo[];
  reconnect: () => void;
};

const Ctx = createContext<WsState | null>(null);

export function MccpWsProvider({ children }: { children: React.ReactNode }) {
  const [status, setStatus] = useState<WsStatus>('connecting');
  const [lastMessageAt, setLastMessageAt] = useState<number | null>(null);

  const [systemStatus, setSystemStatus] = useState<SystemStatus | null>(null);
  const [systemMetrics, setSystemMetrics] = useState<SystemMetrics | null>(null);
  const [indexProgress, setIndexProgress] = useState<IndexProgress | null>(null);
  const [tasks, setTasks] = useState<TaskInfo[]>([]);
  const [projects, setProjects] = useState<ProjectInfo[]>([]);

  const wsRef = useRef<WebSocket | null>(null);
  const connectRef = useRef<() => void>(() => {});
  const reconnectTimer = useRef<number | null>(null);
  const backoffMs = useRef<number>(500);

  const cleanup = () => {
    if (reconnectTimer.current) {
      window.clearTimeout(reconnectTimer.current);
      reconnectTimer.current = null;
    }
    if (wsRef.current) {
      wsRef.current.onopen = null;
      wsRef.current.onclose = null;
      wsRef.current.onerror = null;
      wsRef.current.onmessage = null;
      try {
        wsRef.current.close();
      } catch {
        // ignore
      }
      wsRef.current = null;
    }
  };

  const scheduleReconnect = useCallback(() => {
    if (reconnectTimer.current) return;
    const delay = backoffMs.current;
    reconnectTimer.current = window.setTimeout(() => {
      reconnectTimer.current = null;
      connectRef.current();
      backoffMs.current = Math.min(10_000, Math.floor(backoffMs.current * 1.6));
    }, delay);
  }, []);

  const handleEnvelope = useCallback((env: WsEnvelope) => {
    setLastMessageAt(Date.now());

    switch (env.type) {
      case 'system.status':
        setSystemStatus(env.data as SystemStatus);
        return;
      case 'system.metrics':
        setSystemMetrics(env.data as SystemMetrics);
        return;
      case 'index.progress':
        setIndexProgress(env.data as IndexProgress);
        return;
      case 'tasks.snapshot':
        setTasks((env.data as TaskInfo[]) || []);
        return;
      case 'tasks.updated': {
        const updated = env.data as TaskInfo;
        setTasks((prev) => {
          const i = prev.findIndex((t) => t.id === updated.id);
          if (i === -1) return [updated, ...prev];
          const next = prev.slice();
          next[i] = updated;
          return next;
        });
        return;
      }
      case 'projects.snapshot':
        setProjects((env.data as ProjectInfo[]) || []);
        return;
      case 'projects.updated': {
        const updated = env.data as ProjectInfo;
        setProjects((prev) => {
          const i = prev.findIndex((p) => p.id === updated.id);
          if (i === -1) return [updated, ...prev];
          const next = prev.slice();
          next[i] = updated;
          return next;
        });
        return;
      }
      default:
        return;
    }
  }, []);

  const connect = useCallback(() => {
    cleanup();
    setStatus('connecting');

    const url = getWsUrl();
    let ws: WebSocket;
    try {
      ws = new WebSocket(url);
    } catch {
      setStatus('disconnected');
      scheduleReconnect();
      return;
    }

    wsRef.current = ws;

    ws.onopen = () => {
      backoffMs.current = 500;
      setStatus('connected');
      // Optional: subscribe handshake (server can ignore).
      ws.send(
        JSON.stringify({
          type: 'subscribe',
          topics: ['system', 'metrics', 'index', 'tasks', 'projects'],
        })
      );
    };

    ws.onclose = () => {
      setStatus('disconnected');
      scheduleReconnect();
    };

    ws.onerror = () => {
      setStatus('disconnected');
      scheduleReconnect();
    };

    ws.onmessage = (msg) => {
      try {
        const env = JSON.parse(String(msg.data)) as WsEnvelope;
        if (env && typeof env.type === 'string') handleEnvelope(env);
      } catch {
        // ignore
      }
    };
  }, [handleEnvelope, scheduleReconnect]);

  const reconnect = useCallback(() => {
    backoffMs.current = 500;
    connect();
  }, [connect]);

  useEffect(() => {
    connectRef.current = connect;
  }, [connect]);

  useEffect(() => {
    connect();
    return () => cleanup();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const value = useMemo<WsState>(
    () => ({
      status,
      lastMessageAt,
      systemStatus,
      systemMetrics,
      indexProgress,
      tasks,
      projects,
      reconnect,
    }),
    [
      status,
      lastMessageAt,
      systemStatus,
      systemMetrics,
      indexProgress,
      tasks,
      projects,
      reconnect,
    ]
  );

  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}

export function useMccpWs(): WsState {
  const v = useContext(Ctx);
  if (!v) throw new Error('useMccpWs must be used within MccpWsProvider');
  return v;
}
