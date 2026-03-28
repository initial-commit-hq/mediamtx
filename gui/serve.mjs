/**
 * gui/serve.mjs — dev proxy for the Nuxt dev server
 *
 * Forwards all non-proxy requests to the Nuxt dev server (default :3001).
 * Transparently proxies /proxy/{host}/{port}/... to upstream mediamtx servers
 * to avoid CORS errors when connecting to LAN mediamtx instances.
 *
 * Usage:
 *   1. Start the Nuxt dev server:   cd gui && npm run dev   (listens on :3001)
 *   2. Start this proxy:            node gui/serve.mjs      (listens on :8081)
 *
 * Environment variables:
 *   PORT       — port this proxy listens on  (default: 8081)
 *   NUXT_PORT  — port of the Nuxt dev server (default: 3001)
 */

import http from 'node:http';
import { URL } from 'node:url';

const PORT      = parseInt(process.env.PORT      ?? '8081', 10);
const NUXT_PORT = parseInt(process.env.NUXT_PORT ?? '3001', 10);

function addCors(res) {
  res.setHeader('Access-Control-Allow-Origin',  '*');
  res.setHeader('Access-Control-Allow-Methods', 'GET, POST, PATCH, PUT, DELETE, OPTIONS');
  res.setHeader('Access-Control-Allow-Headers', '*');
}

async function readBody(req) {
  if (!['POST', 'PUT', 'PATCH'].includes(req.method)) return undefined;
  return new Promise((resolve, reject) => {
    const chunks = [];
    req.on('data', c => chunks.push(c));
    req.on('end',  () => resolve(Buffer.concat(chunks)));
    req.on('error', reject);
  });
}

const server = http.createServer(async (req, res) => {
  if (req.method === 'OPTIONS') {
    addCors(res);
    res.writeHead(204);
    res.end();
    return;
  }

  const reqUrl = new URL(req.url, `http://localhost:${PORT}`);

  // ── Transparent reverse proxy ──────────────────────────────────────────────
  // Pattern: /proxy/{host}/{port}/{...rest}
  // e.g.   /proxy/toonca.local/9997/v3/paths/list
  //        /proxy/toonca.local/8889/cam1/whep      (WHEP POST)
  //        /proxy/toonca.local/8888/cam1/index.m3u8
  const proxyMatch = reqUrl.pathname.match(/^\/proxy\/([^/]+)\/(\d+)(\/.*)?$/);
  if (proxyMatch) {
    const [, host, port, rest = '/'] = proxyMatch;
    const upstream = `http://${host}:${port}${rest}${reqUrl.search}`;
    const body = await readBody(req);

    try {
      const r = await fetch(upstream, {
        method: req.method,
        headers: {
          ...(req.headers['content-type'] && { 'Content-Type': req.headers['content-type'] }),
        },
        body,
      });
      addCors(res);
      const ct       = r.headers.get('content-type') ?? 'application/octet-stream';
      const location = r.headers.get('location');
      res.writeHead(r.status, { 'Content-Type': ct, ...(location ? { Location: location } : {}) });
      res.end(Buffer.from(await r.arrayBuffer()));
    } catch (err) {
      addCors(res);
      res.writeHead(502, { 'Content-Type': 'text/plain' });
      res.end(`Proxy error: ${err.message}`);
    }
    return;
  }

  // ── Forward everything else to the Nuxt dev server ─────────────────────────
  const upstream = `http://localhost:${NUXT_PORT}${req.url}`;
  const body = await readBody(req);

  try {
    const r = await fetch(upstream, {
      method: req.method,
      headers: Object.fromEntries(
        Object.entries(req.headers).filter(([k]) => k !== 'host'),
      ),
      body,
    });
    const hdrs = {};
    r.headers.forEach((v, k) => { hdrs[k] = v; });
    res.writeHead(r.status, hdrs);
    res.end(Buffer.from(await r.arrayBuffer()));
  } catch (err) {
    res.writeHead(502, { 'Content-Type': 'text/plain' });
    res.end(`Nuxt proxy error: ${err.message}\n\nMake sure the Nuxt dev server is running: cd gui && npm run dev`);
  }
});

server.listen(PORT, () => {
  console.log(`mediamtx dev proxy  →  http://localhost:${PORT}`);
  console.log(`nuxt dev server     →  http://localhost:${NUXT_PORT}  (start with: cd gui && npm run dev)`);
  console.log(`upstream proxy      →  /proxy/{host}/{port}/{path}`);
});
