import {
  Alert,
  AlertDescription,
  AlertTitle,
  Box,
  Button,
  Heading,
  HStack,
  Table,
  Tbody,
  Td,
  Text,
  Th,
  Thead,
  Tr,
  useToast,
} from '@chakra-ui/react';
import { useEffect, useMemo, useState } from 'react';
import { admin } from '../api/admin';
import { useMccpWs } from '../ws/MccpWsProvider';
import type { TaskInfo } from '../ws/types';

export function TasksPage() {
  const ws = useMccpWs();
  const toast = useToast();

  const [restTasks, setRestTasks] = useState<TaskInfo[]>([]);
  const [err, setErr] = useState<string | null>(null);

  useEffect(() => {
    let mounted = true;
    admin
      .listTasks({ state: 'all' })
      .then((t) => {
        if (!mounted) return;
        setRestTasks(t);
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

  const tasks = useMemo(() => {
    const base = ws.tasks.length > 0 ? ws.tasks : restTasks;
    return [...base].sort((a, b) => (a.created_at < b.created_at ? 1 : -1));
  }, [ws.tasks, restTasks]);

  const cancel = async (taskId: string) => {
    try {
      await admin.cancelTask(taskId);
      toast({
        title: 'Task canceled',
        status: 'success',
        duration: 2000,
        isClosable: true,
      });
    } catch (e: any) {
      toast({
        title: 'Failed to cancel task',
        description: String(e?.message || e),
        status: 'error',
        duration: 5000,
        isClosable: true,
      });
    }
  };

  return (
    <Box>
      <HStack justify="space-between" mb={4}>
        <Heading size="lg">Tasks</Heading>
      </HStack>

      {err ? (
        <Alert status="warning" mb={4}>
          <Box>
            <AlertTitle>Tasks API not available</AlertTitle>
            <AlertDescription>{err}</AlertDescription>
          </Box>
        </Alert>
      ) : null}

      <Box bg="white" borderWidth="1px" borderRadius="lg" overflowX="auto">
        <Table size="sm">
          <Thead>
            <Tr>
              <Th>ID</Th>
              <Th>Kind</Th>
              <Th>Project</Th>
              <Th>State</Th>
              <Th>Title</Th>
              <Th isNumeric>Progress</Th>
              <Th>Actions</Th>
            </Tr>
          </Thead>
          <Tbody>
            {tasks.length === 0 ? (
              <Tr>
                <Td colSpan={7}>
                  <Text fontSize="sm" color="gray.600">
                    No tasks.
                  </Text>
                </Td>
              </Tr>
            ) : (
              tasks.map((t) => {
                const canCancel = t.state === 'queued' || t.state === 'running';
                return (
                  <Tr key={t.id}>
                    <Td>{t.id}</Td>
                    <Td>{t.kind}</Td>
                    <Td>{t.project_id ?? '-'}</Td>
                    <Td>{t.state}</Td>
                    <Td>{t.title}</Td>
                    <Td isNumeric>{t.progress ? `${t.progress.percentage}%` : '-'}</Td>
                    <Td>
                      <Button size="xs" variant="outline" onClick={() => cancel(t.id)} isDisabled={!canCancel}>
                        Cancel
                      </Button>
                    </Td>
                  </Tr>
                );
              })
            )}
          </Tbody>
        </Table>
      </Box>

      <Text mt={3} fontSize="sm" color="gray.600">
        Realtime task updates are expected via WS (`tasks.snapshot` / `tasks.updated`).
      </Text>
    </Box>
  );
}
