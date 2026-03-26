/**
 * gui/serve.mjs — dev server for mediamtx-viewer.html
 *
 * Serves static files from gui/ and transparently proxies requests to any
 * upstream host/port under the path /proxy/{host}/{port}/...
 *
 * This eliminates browser CORS errors when connecting to a LAN mediamtx
 * server from the viewer page.
 *
 * Usage:
 *   node gui/serve.mjs          # listens on :8081
 *   PORT=3000 node gui/serve.mjs
 */

import http from 'node:http';
import fs   from 'node:fs';
import path from 'node:path';
import { URL, fileURLToPath } from 'node:url';

const PORT = parseInt(process.env.PORT ?? '8081', 10);
const DIR  = path.dirname(fileURLToPath(import.meta.url));

const MIME = {
  '.html': 'text/html; charset=utf-8',
  '.js':   'application/javascript',
  '.css':  'text/css',
  '.json': 'application/json',
  '.m3u8': 'application/vnd.apple.mpegurl',
  '.ts':   'video/mp2t',
  '.mp4':  'video/mp4',
};

function addCors(res) {
  res.setHeader('Access-Control-Allow-Origin',  '*');
  res.setHeader('Access-Control-Allow-Methods', 'GET, OPTIONS');
  res.setHeader('Access-Control-Allow-Headers', '*');
}

const server = http.createServer(async (req, res) => {
  // Pre-flight
  if (req.method === 'OPTIONS') {
    addCors(res);
    res.writeHead(204);
    res.end();
    return;
  }

  const reqUrl = new URL(req.url, `http://localhost:${PORT}`);

  // ── Transparent reverse proxy ──────────────────────────────────────────
  // Pattern: /proxy/{host}/{port}/{...rest}
  // e.g.   /proxy/toonca.local/9997/v3/paths/list
  //        /proxy/toonca.local/8889/cam1/whep      (WHEP POST)
  //        /proxy/toonca.local/8888/cam1/index.m3u8
  const proxyMatch = reqUrl.pathname.match(/^\/proxy\/([^/]+)\/(\d+)(\/.*)?$/);
  if (proxyMatch) {
    const host = proxyMatch[1];
    const port = proxyMatch[2];
    const rest = proxyMatch[3] ?? '/';
    const upstream = `http://${host}:${port}${rest}${reqUrl.search}`;

    // Read request body for POST/PUT (e.g. WHEP SDP offer)
    let body;
    if (req.method === 'POST' || req.method === 'PUT' || req.method === 'PATCH') {
      body = await new Promise((resolve, reject) => {
        const chunks = [];
        req.on('data', c => chunks.push(c));
        req.on('end',  () => resolve(Buffer.concat(chunks)));
        req.on('error', reject);
      });
    }

    try {
      const r = await fetch(upstream, {
        method:  req.method,
        headers: {
          ...(req.headers['content-type'] && { 'Content-Type': req.headers['content-type'] }),
        },
        body,
      });
      addCors(res);
      // Forward relevant response headers
      const ct       = r.headers.get('content-type') ?? 'application/octet-stream';
      const location = r.headers.get('location');
      const extraHdrs = location ? { Location: location } : {};
      res.writeHead(r.status, { 'Content-Type': ct, ...extraHdrs });
      const buf = await r.arrayBuffer();
      res.end(Buffer.from(buf));
    } catch (err) {
      addCors(res);
      res.writeHead(502, { 'Content-Type': 'text/plain' });
      res.end(`Proxy error: ${err.message}`);
    }
    return;
  }

  // ── Static files ───────────────────────────────────────────────────────
  const pathname  = reqUrl.pathname === '/' ? '/mediamtx-viewer.html' : reqUrl.pathname;
  const filePath  = path.join(DIR, pathname);

  // Safety: don't escape gui/ directory
  if (!filePath.startsWith(DIR)) {
    res.writeHead(403);
    res.end('Forbidden');
    return;
  }

  try {
    const data = fs.readFileSync(filePath);
    const ext  = path.extname(filePath).toLowerCase();
    addCors(res);
    res.writeHead(200, { 'Content-Type': MIME[ext] ?? 'application/octet-stream' });
    res.end(data);
  } catch {
    res.writeHead(404, { 'Content-Type': 'text/plain' });
    res.end('Not found');
  }
});

server.listen(PORT, () => {
  console.log(`mediamtx viewer  →  http://localhost:${PORT}`);
  console.log(`proxy endpoint   →  http://localhost:${PORT}/proxy/{host}/{port}/{path}`);
});
