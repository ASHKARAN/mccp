import {
  Box,
  Divider,
  Heading,
  Link,
  Stack,
  Text,
} from '@chakra-ui/react';
import { NavLink } from 'react-router-dom';

function NavItem({ to, label }: { to: string; label: string }) {
  return (
    <Link
      as={NavLink}
      to={to}
      px={3}
      py={2}
      borderRadius="md"
      _hover={{ textDecoration: 'none', bg: 'gray.50' }}
      _activeLink={{ bg: 'gray.100', fontWeight: 'semibold' }}
    >
      {label}
    </Link>
  );
}

export function SidebarNav() {
  return (
    <Box p={5}>
      <Heading size="md">MCCP</Heading>
      <Text mt={1} fontSize="sm" color="gray.600">
        Web Console
      </Text>

      <Divider my={4} />

      <Stack spacing={1}>
        <NavItem to="/dashboard" label="Dashboard" />
        <NavItem to="/projects" label="Projects" />
        <NavItem to="/tasks" label="Tasks" />
        <NavItem to="/logs" label="Logs" />
        <NavItem to="/config" label="Config" />
      </Stack>
    </Box>
  );
}
