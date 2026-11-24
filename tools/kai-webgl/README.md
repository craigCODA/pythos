# Kai WebGL workspace

Run the development preview from this directory with Node.js 24.

```powershell
npm ci
npm test
npm start
```

Open http://127.0.0.1:4173. The project serves its pinned Three.js dependency
locally. Set `KAI_WEBGL_PORT` to choose another loopback port. On the PythOS
workstation, this checkout also has a workspace-local npm cache for an offline
install.

```powershell
npm ci --offline --cache D:\PythOS-Workspace\.cache\kai-webgl\npm
```

The default trace comes from the QEMU boot that recovered checkpoint 3 and
finished at checkpoint 6. Use **Open trace** to load a serial log from another
`vault-pulse-view` boot. JavaScript reads the selected file inside the browser.
The slider steps through the nodes, links, and checkpoint positions in order.
The browser must provide a WebGL 2 context. [Three.js renderer requirements](https://threejs.org/docs/pages/WebGLRenderer.html)

`src/trace.js` turns the native serial observations into scene data.
`src/scene.js` draws those scenes with Three.js. The browser currently provides
the JavaScript and WebGL 2 host. The native boot ISO uses the framebuffer view
from [ADR 0111](../../docs/decisions/0111-vault-pulse-boot-view.md).
