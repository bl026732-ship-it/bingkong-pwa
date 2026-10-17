/* 饼控自律 PWA — 零依赖本地静态服务器（node server.js，然后浏览器打开 http://localhost:8520） */
const http = require('http');
const fs = require('fs');
const path = require('path');

const ROOT = __dirname;
const PORT = 8520;
const MIME = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'application/javascript; charset=utf-8',
  '.json': 'application/json; charset=utf-8',
  '.png': 'image/png',
  '.svg': 'image/svg+xml',
  '.css': 'text/css; charset=utf-8'
};

/* 悬浮窗状态桥：主应用推送当前专注状态，桌面悬浮窗轮询读取（跨域开放给本机悬浮窗） */
let focusState = null;
const CORS = {
  'Access-Control-Allow-Origin': '*',
  'Access-Control-Allow-Methods': 'GET,POST,OPTIONS',
  'Access-Control-Allow-Headers': 'Content-Type'
};

http.createServer((req, res) => {
  if (req.method === 'OPTIONS') { res.writeHead(204, CORS); res.end(); return; }

  let p = decodeURIComponent(req.url.split('?')[0]);

  if (p === '/api/focus') {
    if (req.method === 'GET') {
      res.writeHead(200, Object.assign({ 'Content-Type': 'application/json; charset=utf-8' }, CORS));
      res.end(focusState || 'null');
    } else if (req.method === 'POST') {
      let body = '';
      req.on('data', c => { body += c; });
      req.on('end', () => { focusState = body; res.writeHead(200, CORS); res.end('ok'); });
    } else { res.writeHead(405, CORS); res.end(); }
    return;
  }

  if (p === '/' || p === '') p = '/index.html';
  const file = path.join(ROOT, p);
  if (!file.startsWith(ROOT)) { res.writeHead(403); res.end(); return; }
  fs.readFile(file, (err, data) => {
    if (err) { res.writeHead(404); res.end('Not Found'); return; }
    res.writeHead(200, { 'Content-Type': MIME[path.extname(file)] || 'application/octet-stream' });
    res.end(data);
  });
}).listen(PORT, () => console.log(`饼控自律 PWA 已启动: http://localhost:${PORT}`));
