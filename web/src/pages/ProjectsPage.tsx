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
  Modal,
  ModalBody,
  ModalCloseButton,
  ModalContent,
  ModalFooter,
  ModalHeader,
  ModalOverlay,
  Table,
  Tbody,
  Td,
  Text,
  Th,
  Thead,
  Tr,
  useDisclosure,
  useToast,
} from '@chakra-ui/react';
import { useEffect, useMemo, useState } from 'react';
import { admin } from '../api/admin';
import { useMccpWs } from '../ws/MccpWsProvider';
import type { ProjectInfo } from '../ws/types';

export function ProjectsPage() {
  const ws = useMccpWs();
  const toast = useToast();
  const addDlg = useDisclosure();

  const [restProjects, setRestProjects] = useState<ProjectInfo[]>([]);
  const [err, setErr] = useState<string | null>(null);

  const [name, setName] = useState('');
  const [rootPath, setRootPath] = useState('');
  const [watch, setWatch] = useState(true);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    let mounted = true;
    admin
      .listProjects()
      .then((p) => {
        if (!mounted) return;
        setRestProjects(p);
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

  const projects = useMemo(() => {
    return ws.projects.length > 0 ? ws.projects : restProjects;
  }, [ws.projects, restProjects]);

  const createProject = async () => {
    setSaving(true);
    try {
      const res = await admin.createProject({ name, root_path: rootPath, watch, index_immediately: false });
      toast({
        title: 'Project created',
        description: `id: ${res.id}`,
        status: 'success',
        duration: 3000,
        isClosable: true,
      });
      addDlg.onClose();
      setName('');
      setRootPath('');
      setWatch(true);
    } catch (e: any) {
      toast({
        title: 'Failed to create project',
        description: String(e?.message || e),
        status: 'error',
        duration: 5000,
        isClosable: true,
      });
    } finally {
      setSaving(false);
    }
  };

  const reindex = async (projectId: string) => {
    try {
      const res = await admin.reindexProject(projectId, true);
      toast({
        title: 'Reindex started',
        description: `task: ${res.task_id}`,
        status: 'success',
        duration: 3000,
        isClosable: true,
      });
    } catch (e: any) {
      toast({
        title: 'Failed to start reindex',
        description: String(e?.message || e),
        status: 'error',
        duration: 5000,
        isClosable: true,
      });
    }
  };

  const remove = async (projectId: string) => {
    try {
      await admin.deleteProject(projectId);
      toast({
        title: 'Project deleted',
        status: 'success',
        duration: 2000,
        isClosable: true,
      });
    } catch (e: any) {
      toast({
        title: 'Failed to delete project',
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
        <Heading size="lg">Projects</Heading>
        <Button onClick={addDlg.onOpen}>Add Project</Button>
      </HStack>

      {err ? (
        <Alert status="warning" mb={4}>
          <Box>
            <AlertTitle>Projects API not available</AlertTitle>
            <AlertDescription>{err}</AlertDescription>
          </Box>
        </Alert>
      ) : null}

      <Box bg="white" borderWidth="1px" borderRadius="lg" overflowX="auto">
        <Table size="sm">
          <Thead>
            <Tr>
              <Th>ID</Th>
              <Th>Name</Th>
              <Th>Root</Th>
              <Th>Status</Th>
              <Th isNumeric>Files</Th>
              <Th isNumeric>Chunks</Th>
              <Th>Actions</Th>
            </Tr>
          </Thead>
          <Tbody>
            {projects.length === 0 ? (
              <Tr>
                <Td colSpan={7}>
                  <Text fontSize="sm" color="gray.600">
                    No projects yet.
                  </Text>
                </Td>
              </Tr>
            ) : (
              projects.map((p) => (
                <Tr key={p.id}>
                  <Td>{p.id}</Td>
                  <Td>{p.name}</Td>
                  <Td>{p.root_path}</Td>
                  <Td>{p.status}</Td>
                  <Td isNumeric>{p.file_count ?? '-'}</Td>
                  <Td isNumeric>{p.chunk_count ?? '-'}</Td>
                  <Td>
                    <HStack>
                      <Button size="xs" onClick={() => reindex(p.id)}>
                        Reindex
                      </Button>
                      <Button size="xs" variant="outline" colorScheme="red" onClick={() => remove(p.id)}>
                        Delete
                      </Button>
                    </HStack>
                  </Td>
                </Tr>
              ))
            )}
          </Tbody>
        </Table>
      </Box>

      <Text mt={3} fontSize="sm" color="gray.600">
        Realtime project updates are expected via WS (`projects.snapshot` / `projects.updated`).
      </Text>

      <Modal isOpen={addDlg.isOpen} onClose={addDlg.onClose} isCentered>
        <ModalOverlay />
        <ModalContent>
          <ModalHeader>Add Project</ModalHeader>
          <ModalCloseButton />
          <ModalBody>
            <FormControl mb={3}>
              <FormLabel>Name</FormLabel>
              <Input value={name} onChange={(e) => setName(e.target.value)} placeholder="my-repo" />
            </FormControl>
            <FormControl mb={3}>
              <FormLabel>Root path</FormLabel>
              <Input value={rootPath} onChange={(e) => setRootPath(e.target.value)} placeholder="/path/to/repo" />
            </FormControl>
            <Checkbox isChecked={watch} onChange={(e) => setWatch(e.target.checked)}>
              Watch for changes
            </Checkbox>
            <Text mt={3} fontSize="sm" color="gray.600">
              Requires REST API: POST /api/v1/projects (see apis.md).
            </Text>
          </ModalBody>
          <ModalFooter>
            <Button variant="ghost" mr={3} onClick={addDlg.onClose}>
              Cancel
            </Button>
            <Button colorScheme="blue" onClick={createProject} isLoading={saving} isDisabled={!name || !rootPath}>
              Create
            </Button>
          </ModalFooter>
        </ModalContent>
      </Modal>
    </Box>
  );
}
