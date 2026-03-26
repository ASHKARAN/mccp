import {
  Alert,
  AlertDescription,
  AlertTitle,
  Box,
  Heading,
  Progress,
  SimpleGrid,
  Stat,
  StatHelpText,
  StatLabel,
  StatNumber,
  Text,
} from '@chakra-ui/react';
import { useEffect, useState } from 'react';
import { mccp, type HealthResponse, type IndexingStatus } from '../api/mccp';
import { useMccpWs } from '../ws/MccpWsProvider';

function bytes(n: number) {
  if (!Number.isFinite(n)) return '-';
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  let v = n;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v.toFixed(i === 0 ? 0 : 1)} ${units[i]}`;
}

export function DashboardPage() {
  const ws = useMccpWs();
  const [health, setHealth] = useState<HealthResponse | null>(null);
  const [indexStatus, setIndexStatus] = useState<IndexingStatus | null>(null);
  const [err, setErr] = useState<string | null>(null);

  useEffect(() => {
    let mounted = true;
    Promise.all([mccp.health(), mccp.indexStatus()])
      .then(([h, s]) => {
        if (!mounted) return;
        setHealth(h);
        setIndexStatus(s);
        setErr(null);
      })
      .catch((e) => {
        if (!mounted) return;
        setErr(String(e?.message || e));
      });
    return () => {
      mounted = false;
    };
  }, [ws.lastMessageAt]);

  const activeTasks = ws.tasks.filter((t) => t.state === 'running' || t.state === 'queued').length;
  const finishedTasks = ws.tasks.filter((t) => t.state === 'finished' || t.state === 'failed' || t.state === 'canceled').length;

  return (
    <Box>
      <Heading size="lg" mb={4}>
        Dashboard
      </Heading>

      {err ? (
        <Alert status="warning" mb={4}>
          <Box>
            <AlertTitle>Backend not reachable</AlertTitle>
            <AlertDescription>{err}</AlertDescription>
          </Box>
        </Alert>
      ) : null}

      <SimpleGrid columns={{ base: 1, md: 2, lg: 4 }} spacing={4}>
        <Stat p={4} bg="white" borderWidth="1px" borderRadius="lg">
          <StatLabel>Version</StatLabel>
          <StatNumber>{health?.version ?? '-'}</StatNumber>
          <StatHelpText>{health?.status ? `health: ${health.status}` : '—'}</StatHelpText>
        </Stat>

        <Stat p={4} bg="white" borderWidth="1px" borderRadius="lg">
          <StatLabel>CPU</StatLabel>
          <StatNumber>{ws.systemMetrics ? `${ws.systemMetrics.cpu_percent.toFixed(1)}%` : '-'}</StatNumber>
          <StatHelpText>System metrics (WS)</StatHelpText>
        </Stat>

        <Stat p={4} bg="white" borderWidth="1px" borderRadius="lg">
          <StatLabel>RAM</StatLabel>
          <StatNumber>
            {ws.systemMetrics
              ? `${bytes(ws.systemMetrics.ram_used_bytes)} / ${bytes(ws.systemMetrics.ram_total_bytes)}`
              : '-'}
          </StatNumber>
          <StatHelpText>System metrics (WS)</StatHelpText>
        </Stat>

        <Stat p={4} bg="white" borderWidth="1px" borderRadius="lg">
          <StatLabel>Uptime</StatLabel>
          <StatNumber>
            {ws.systemStatus ? `${Math.floor(ws.systemStatus.uptime_ms / 1000)}s` : '-'}
          </StatNumber>
          <StatHelpText>System status (WS)</StatHelpText>
        </Stat>
      </SimpleGrid>

      <SimpleGrid columns={{ base: 1, md: 2 }} spacing={4} mt={6}>
        <Box p={4} bg="white" borderWidth="1px" borderRadius="lg">
          <Heading size="sm" mb={3}>
            Indexing
          </Heading>
          <Text fontSize="sm" color="gray.600">
            Status endpoint: {indexStatus ? `${indexStatus.indexed_files}/${indexStatus.file_count} indexed` : '-'}
          </Text>
          <Box mt={3}>
            <Text fontSize="sm" mb={1}>
              Reindex progress
            </Text>
            <Progress value={ws.indexProgress?.percentage ?? 0} />
            <Text mt={2} fontSize="sm" color="gray.600">
              {ws.indexProgress
                ? `${ws.indexProgress.phase} — ${ws.indexProgress.current}/${ws.indexProgress.total} (${ws.indexProgress.percentage}%)`
                : 'Waiting for index.progress (WS)'}
            </Text>
          </Box>
        </Box>

        <Box p={4} bg="white" borderWidth="1px" borderRadius="lg">
          <Heading size="sm" mb={3}>
            Tasks
          </Heading>
          <SimpleGrid columns={2} spacing={4}>
            <Stat>
              <StatLabel>Active</StatLabel>
              <StatNumber>{activeTasks}</StatNumber>
              <StatHelpText>queued/running</StatHelpText>
            </Stat>
            <Stat>
              <StatLabel>Finished</StatLabel>
              <StatNumber>{finishedTasks}</StatNumber>
              <StatHelpText>finished/failed/canceled</StatHelpText>
            </Stat>
          </SimpleGrid>
          <Text mt={3} fontSize="sm" color="gray.600">
            Task stream is expected via WebSocket (`tasks.*` events).
          </Text>
        </Box>
      </SimpleGrid>
    </Box>
  );
}
