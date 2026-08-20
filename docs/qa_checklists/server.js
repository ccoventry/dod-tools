#!/usr/bin/env node
// Tiny local server for the QA checklist manifest — no dependencies, just
// Node's built-ins. Serves manifest.html plus a /api/checklists endpoint
// that lists and parses every *.json file in this folder. Dropping a new
// checklist file in here and refreshing the page is genuinely all it takes
// — no rebuild step, unlike the published Claude artifact this mirrors.
//
// Run: node server.js [port]   (defaults to 5680)

const http = require('http');
const fs = require('fs');
const path = require('path');

const DIR = __dirname;
const PORT = process.argv[2] ? parseInt(process.argv[2], 10) : 5680;

function listChecklists() {
  return fs.readdirSync(DIR)
    .filter((f) => f.endsWith('.json'))
    .map((f) => {
      try {
        return JSON.parse(fs.readFileSync(path.join(DIR, f), 'utf8'));
      } catch (e) {
        console.error(`Skipping ${f}: ${e.message}`);
        return null;
      }
    })
    .filter(Boolean);
}

const MIME = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.json': 'application/json; charset=utf-8',
};

const server = http.createServer((req, res) => {
  if (req.url === '/api/checklists') {
    res.writeHead(200, { 'Content-Type': 'application/json; charset=utf-8' });
    res.end(JSON.stringify(listChecklists()));
    return;
  }

  const urlPath = req.url === '/' ? '/manifest.html' : req.url.split('?')[0];
  const filePath = path.join(DIR, decodeURIComponent(urlPath));

  // Refuse anything that resolves outside this folder (e.g. ../../secret).
  if (!filePath.startsWith(DIR)) {
    res.writeHead(403);
    res.end('Forbidden');
    return;
  }

  fs.readFile(filePath, (err, data) => {
    if (err) {
      res.writeHead(404);
      res.end('Not found');
      return;
    }
    const ext = path.extname(filePath);
    res.writeHead(200, { 'Content-Type': MIME[ext] || 'application/octet-stream' });
    res.end(data);
  });
});

server.listen(PORT, () => {
  console.log(`QA manifest running at http://localhost:${PORT}/`);
});
