import {
  Alert,
  AlertDescription,
  AlertTitle,
  Box,
  Button,
  Heading,
  HStack,
  Text,
  Textarea,
  useToast,
} from '@chakra-ui/react';
import { useEffect, useState } from 'react';
import { admin } from '../api/admin';

export function ConfigPage() {
  const toast = useToast();
  const [toml, setToml] = useState('');
  const [err, setErr] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  const load = async () => {
    setLoading(true);
    try {
      const res = await admin.getConfig();
      setToml(res.toml);
      setErr(null);
    } catch (e: any) {
      setErr(String(e?.message || e));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const save = async () => {
    setLoading(true);
    try {
      const res = await admin.putConfig(toml);
      toast({
        title: res.restart_required ? 'Config saved (restart required)' : 'Config applied',
        status: 'success',
        duration: 2500,
        isClosable: true,
      });
      setErr(null);
    } catch (e: any) {
      toast({
        title: 'Failed to apply config',
        description: String(e?.message || e),
        status: 'error',
        duration: 5000,
        isClosable: true,
      });
      setErr(String(e?.message || e));
    } finally {
      setLoading(false);
    }
  };

  return (
    <Box>
      <HStack justify="space-between" mb={4}>
        <Heading size="lg">Config</Heading>
        <HStack>
          <Button variant="outline" onClick={load} isLoading={loading}>
            Reload
          </Button>
          <Button colorScheme="blue" onClick={save} isLoading={loading}>
            Apply
          </Button>
        </HStack>
      </HStack>

      {err ? (
        <Alert status="warning" mb={4}>
          <Box>
            <AlertTitle>Config API not available</AlertTitle>
            <AlertDescription>{err}</AlertDescription>
          </Box>
        </Alert>
      ) : null}

      <Text fontSize="sm" color="gray.600" mb={2}>
        This editor uses GET/PUT /api/v1/config (see apis.md).
      </Text>

      <Textarea value={toml} onChange={(e) => setToml(e.target.value)} minH="420px" fontFamily="mono" bg="white" />
    </Box>
  );
}
