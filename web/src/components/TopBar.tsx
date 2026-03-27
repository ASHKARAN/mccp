import {
  Badge,
  Box,
  Button,
  Flex,
  HStack,
  Modal,
  ModalBody,
  ModalCloseButton,
  ModalContent,
  ModalFooter,
  ModalHeader,
  ModalOverlay,
  Spacer,
  Text,
  Tooltip,
  useDisclosure,
  useToast,
  FormControl,
  FormLabel,
  Input,
  VStack,
} from '@chakra-ui/react';
import { useMemo, useState } from 'react';
import { getHttpBaseUrl, getWsUrl, setHttpBaseUrl, setWsUrl } from '../api/runtimeConfig';
import { useMccpWs } from '../ws/MccpWsProvider';

export function TopBar() {
  const ws = useMccpWs();
  const { isOpen, onOpen, onClose } = useDisclosure();
  const toast = useToast();

  const [httpUrl, setHttpUrl] = useState(getHttpBaseUrl());
  const [wsUrl, setWsUrlState] = useState(getWsUrl());

  const wsBadge = useMemo(() => {
    switch (ws.status) {
      case 'connected':
        return <Badge colorScheme="green">WS Connected</Badge>;
      case 'connecting':
        return <Badge colorScheme="yellow">WS Connecting</Badge>;
      default:
        return <Badge colorScheme="red">WS Disconnected</Badge>;
    }
  }, [ws.status]);

  const save = () => {
    setHttpBaseUrl(httpUrl);
    setWsUrl(wsUrl);
    ws.reconnect();
    toast({
      title: 'Connection settings saved',
      status: 'success',
      duration: 2000,
      isClosable: true,
    });
    onClose();
  };

  return (
    <Box px={6} py={3} bg="white" borderBottomWidth="1px">
      <Flex align="center" gap={4}>
        <HStack spacing={3}>
          {wsBadge}
          <Tooltip label={`HTTP: ${getHttpBaseUrl()}\nWS: ${getWsUrl()}`}>
            <Text fontSize="sm" color="gray.600">
              {ws.lastMessageAt ? `Last update: ${new Date(ws.lastMessageAt).toLocaleTimeString()}` : 'No updates yet'}
            </Text>
          </Tooltip>
        </HStack>

        <Spacer />

        <Button size="sm" onClick={onOpen}>
          Connection
        </Button>
      </Flex>

      <Modal isOpen={isOpen} onClose={onClose} isCentered>
        <ModalOverlay />
        <ModalContent>
          <ModalHeader>Connection</ModalHeader>
          <ModalCloseButton />
          <ModalBody>
            <VStack spacing={4} align="stretch">
              <FormControl>
                <FormLabel>HTTP Base URL</FormLabel>
                <Input value={httpUrl} onChange={(e) => setHttpUrl(e.target.value)} placeholder="http://localhost:7425" />
              </FormControl>
              <FormControl>
                <FormLabel>WebSocket URL</FormLabel>
                <Input value={wsUrl} onChange={(e) => setWsUrlState(e.target.value)} placeholder="ws://localhost:7425/ws" />
              </FormControl>
              <Text fontSize="sm" color="gray.600">
                These are stored in localStorage and used immediately.
              </Text>
            </VStack>
          </ModalBody>
          <ModalFooter>
            <Button variant="ghost" mr={3} onClick={onClose}>
              Cancel
            </Button>
            <Button colorScheme="blue" onClick={save}>
              Save & Reconnect
            </Button>
          </ModalFooter>
        </ModalContent>
      </Modal>
    </Box>
  );
}
