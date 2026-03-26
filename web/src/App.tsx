import { Box, Flex } from '@chakra-ui/react';
import { Navigate, Route, Routes } from 'react-router-dom';
import { MccpWsProvider } from './ws/MccpWsProvider';
import { SidebarNav } from './components/SidebarNav';
import { TopBar } from './components/TopBar';
import { DashboardPage } from './pages/DashboardPage';
import { ProjectsPage } from './pages/ProjectsPage';
import { TasksPage } from './pages/TasksPage';
import { ConfigPage } from './pages/ConfigPage';
import { LogsPage } from './pages/LogsPage';

export default function App() {
  return (
    <MccpWsProvider>
      <Flex minH="100vh" bg="gray.50">
        <Box w="280px" borderRightWidth="1px" bg="white">
          <SidebarNav />
        </Box>

        <Flex direction="column" flex="1">
          <TopBar />
          <Box p={6}>
            <Routes>
              <Route path="/" element={<Navigate to="/dashboard" replace />} />
              <Route path="/dashboard" element={<DashboardPage />} />
              <Route path="/projects" element={<ProjectsPage />} />
              <Route path="/tasks" element={<TasksPage />} />
              <Route path="/logs" element={<LogsPage />} />
              <Route path="/config" element={<ConfigPage />} />
              <Route path="*" element={<Navigate to="/dashboard" replace />} />
            </Routes>
          </Box>
        </Flex>
      </Flex>
    </MccpWsProvider>
  );
}
