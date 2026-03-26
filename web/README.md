# MCCP Web

Static React (Vite) web console for managing and configuring MCCP.

## Local dev
```bash
cd web
npm install
npm run dev
```

## Production build (static)
```bash
cd web
npm install
npm run build
# serve ./dist with any static server
```

## Configuration
You can set the backend URLs either via env vars at build-time or via the in-app **Connection** dialog:
- `VITE_MCCP_HTTP_URL` (default: `http://localhost:7422`)
- `VITE_MCCP_WS_URL` (default: derived from HTTP + `/ws`)
