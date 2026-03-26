import {
  Alert,
  AlertDescription,
  AlertTitle,
  Box,
  Button,
  Checkbox,
  FormControl,
  FormLabel,
  Heading,
  HStack,
  Input,
  Select,
  Table,
  Tbody,
  Td,
  Text,
  Th,
  Thead,
  Tr,
  useToast,
} from '@chakra-ui/react';
import { useMemo, useState } from 'react';
import { admin } from '../api/admin';
import { useMccpWs } from '../ws/MccpWsProvider';
import type { LogLine, LogLevel } from '../ws/types';

function includesCI(hay: string, needle: string) {
  return hay.toLowerCase().includes(needle.toLowerCase());
}

export function LogsPage() {
  const ws = useMccpWs();
  const toast = useToast();

  const [level, setLevel] = useState<LogLevel | 'all'>('all');
  const [q, setQ] = useState('');
  const [onlyMccp, setOnlyMccp] = useState(true);
  const [history, setHistory] = useState<LogLine[]>([]);
  const [historyErr, setHistoryErr] = useState<string | null>(null);

  const logs = useMemo(() => {
    const base = [...ws.logs, ...history];
    return base.filter((l) => {
      if (onlyMccp && l.target && !l.target.startsWith('mccp')) return false;
      if (level !== 'all' && l.level !== level) return false;
      if (q.trim().length === 0) return true;
      const blob = `${l.message} ${l.target ?? ''} ${l.span ?? ''}`;
      return includesCI(blob, q.trim());
    });
  }, [ws.logs, onlyMccp, level, q]);

  const loadHistory = async () => {
    try {
      const res = await admin.listLogs({
        level: level === 'all' ? undefined : level,
        q: q.trim() || undefined,
        target: onlyMccp ? 'mccp' : undefined,
        limit: 1000,
      });
      setHistory(res.items || []);
      setHistoryErr(null);
      toast({
        title: 'Loaded log history',
        description: `${(res.items || []).length} lines`,
        status: 'success',
        duration: 2000,
        isClosable: true,
      });
    } catch (e: any) {
      setHistoryErr(String(e?.message || e));
      toast({
        title: 'Failed to load history',
        description: String(e?.message || e),
        status: 'warning',
        duration: 5000,
        isClosable: true,
      });
    }
  };

  return (
    <Box>
      <HStack justify="space-between" mb={4}>
        <Heading size="lg">Logs</Heading>
        <Button variant="outline" onClick={loadHistory}>
          Load history
        </Button>
      </HStack>

      {ws.status !== 'connected' ? (
        <Alert status="warning" mb={4}>
          <Box>
            <AlertTitle>Realtime logs not connected</AlertTitle>
            <AlertDescription>
              Connect WebSocket to receive `logs.line` events. If you only run MCCP in a terminal today, logs are on stdout.
            </AlertDescription>
          </Box>
        </Alert>
      ) : null}

      {historyErr ? (
        <Alert status="warning" mb={4}>
          <Box>
            <AlertTitle>History not available</AlertTitle>
            <AlertDescription>{historyErr}</AlertDescription>
          </Box>
        </Alert>
      ) : null}

      <HStack spacing={4} align="end" mb={4} flexWrap="wrap">
        <FormControl maxW="220px">
          <FormLabel>Level</FormLabel>
          <Select value={level} onChange={(e) => setLevel(e.target.value as any)}>
            <option value="all">All</option>
            <option value="TRACE">TRACE</option>
            <option value="DEBUG">DEBUG</option>
            <option value="INFO">INFO</option>
            <option value="WARN">WARN</option>
            <option value="ERROR">ERROR</option>
          </Select>
        </FormControl>

        <FormControl minW={{ base: '240px', md: '420px' }}>
          <FormLabel>Filter</FormLabel>
          <Input value={q} onChange={(e) => setQ(e.target.value)} placeholder="text, target, span..." />
        </FormControl>

        <Checkbox isChecked={onlyMccp} onChange={(e) => setOnlyMccp(e.target.checked)}>
          Only mccp targets
        </Checkbox>
      </HStack>

      <Box bg="white" borderWidth="1px" borderRadius="lg" overflowX="auto">
        <Table size="sm">
          <Thead>
            <Tr>
              <Th>Time</Th>
              <Th>Level</Th>
              <Th>Target</Th>
              <Th>Message</Th>
            </Tr>
          </Thead>
          <Tbody>
            {logs.length === 0 ? (
              <Tr>
                <Td colSpan={4}>
                  <Text fontSize="sm" color="gray.600">
                    No logs yet. The server should broadcast logs over WS (`logs.line`) and/or expose REST history.
                  </Text>
                </Td>
              </Tr>
            ) : (
              logs.map((l: LogLine) => (
                <Tr key={l.id}>
                  <Td whiteSpace="nowrap">{new Date(l.ts).toLocaleTimeString()}</Td>
                  <Td>{l.level}</Td>
                  <Td>{l.target ?? '-'}</Td>
                  <Td>{l.message}</Td>
                </Tr>
              ))
            )}
          </Tbody>
        </Table>
      </Box>

      <Text mt={3} fontSize="sm" color="gray.600">
        Realtime: WS `logs.line` (see apis.md). Filtering is done client-side on the received buffer.
      </Text>
    </Box>
  );
}
