import { readFile } from 'node:fs/promises';
import { createServer } from 'node:http';

const assets = new Map([
  ['/', ['index.html', 'text/html']],
  ['/styles.css', ['styles.css', 'text/css']],
  ['/src/main.js', ['src/main.js', 'text/javascript']],
  ['/src/scene.js', ['src/scene.js', 'text/javascript']],
  ['/src/trace.js', ['src/trace.js', 'text/javascript']],
  ['/fixtures/recovered.log', ['fixtures/recovered.log', 'text/plain']],
  ['/vendor/three.module.js', ['node_modules/three/build/three.module.js', 'text/javascript']],
  ['/vendor/three.core.js', ['node_modules/three/build/three.core.js', 'text/javascript']],
]);

const port = Number(process.env.KAI_WEBGL_PORT ?? 4173);

createServer(async (request, response) => {
  if (request.method !== 'GET') {
    response.writeHead(405, { Allow: 'GET' }).end();
    return;
  }
  const asset = assets.get(request.url.split('?')[0]);
  if (!asset) {
    response.writeHead(404).end();
    return;
  }
  const [path, type] = asset;
  try {
    const bytes = await readFile(new URL(path, import.meta.url));
    response.writeHead(200, {
      'Content-Type': type + '; charset=utf-8',
      'Cache-Control': 'no-store',
    }).end(bytes);
  } catch (error) {
    console.error(error.message);
    response.writeHead(500).end('Run npm ci from the Kai WebGL workspace.');
  }
}).listen(port, '127.0.0.1', () => {
  console.log('Kai WebGL is available at http://127.0.0.1:' + port);
});
