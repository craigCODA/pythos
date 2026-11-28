const PREFIX = 'PYTHOS:VAULT_VIEW:';

const BOOT_NODES = {
  CORE: ['PythCore', 'Starting vault runtime'],
  WORKER: ['Vault worker', 'Mapping kernel memory'],
  MEMORY: ['Memory mappings', 'Checking Virtio storage'],
  VAULT: ['Marked vault disk', 'Recovering vault checkpoint'],
  STORAGE: ['Virtio storage', 'Checking Virtio storage'],
};

const STOP_MESSAGES = {
  VIRTIO_BLOCK: 'Virtio test disk unavailable',
  PROOF_DEVICE: 'Marked Virtio test disk required',
  PROOF_IMAGE: 'Marked Virtio test disk required',
  PROGRAM_REVISION: 'Saved program unavailable',
  ARITHMETIC: 'Arithmetic error',
  IDLE: 'Waiting for a task',
  YIELDED: 'Ready to resume',
};

const STATE = /^STATE (admitted|restored|pulse) generation:(\d+) x:(\d+) cursor:([0-7]) status:(active|waiting|finished)$/;

export function framesFromTrace(text) {
  const frames = [];
  let phase = 'boot';
  let nodes = [];
  let links = [];
  let checkpoint = null;
  let pendingLinks = false;
  let focus = null;

  function frame(message, blocked = false) {
    frames.push({ phase, nodes: [...nodes], links: [...links], focus, checkpoint, message, blocked });
  }

  function flushLinks() {
    if (pendingLinks) {
      frame('Instruction links ready');
      pendingLinks = false;
    }
  }

  function addNode(id, name) {
    if (nodes.some(node => node.id === id)) {
      throw new Error('Open the trace from one boot at a time.');
    }
    nodes.push({ id, name });
    focus = id;
  }

  for (const line of text.split(/\r?\n/)) {
    if (!line.startsWith(PREFIX)) continue;
    const record = line.slice(PREFIX.length).trimEnd();
    if (!record.startsWith('LINK ')) flushLinks();

    const bootNode = /^NODE (CORE|WORKER|MEMORY|VAULT|STORAGE)$/.exec(record);
    const address = /^ADDRESS ([0-7])$/.exec(record);
    const link = /^LINK ([0-7]) ([0-7])$/.exec(record);
    const saved = STATE.exec(record);

    if (bootNode && phase === 'boot') {
      const id = bootNode[1];
      const [name, message] = BOOT_NODES[id];
      addNode(id, name);
      frame(message, id === 'STORAGE');
    } else if (record === 'BOOT_LINKS' && phase === 'boot' && nodes.length) {
      links = nodes.slice(1).map((node, index) => [nodes[index].id, node.id]);
      frame('Boot links ready', focus === 'STORAGE');
    } else if (address) {
      if (phase === 'boot') {
        phase = 'program';
        nodes = [];
        links = [];
      }
      if (links.length || checkpoint) {
        throw new Error('The trace introduced an instruction after the links were drawn.');
      }
      addNode(address[1], 'Address ' + address[1]);
      frame('Revealing instructions');
    } else if (link && phase === 'program') {
      const endpoints = link.slice(1);
      if (!endpoints.every(id => nodes.some(node => node.id === id))) {
        throw new Error('The trace links to an unseen instruction.');
      }
      links.push(endpoints);
      pendingLinks = true;
    } else if (saved && phase === 'program') {
      const [, origin, generation, x, cursor, status] = saved;
      if (!nodes.some(node => node.id === cursor)) {
        throw new Error('The checkpoint points to an unseen instruction.');
      }
      focus = cursor;
      checkpoint = { generation, x, cursor, status };
      const message = origin === 'restored' ? 'Checkpoint restored' : {
        active: origin === 'admitted' ? 'Task admitted' : 'Pulse committed',
        waiting: 'Waiting for an event',
        finished: 'Goal reached',
      }[status];
      frame(message, status === 'waiting');
    } else if (record.startsWith('STOP ') && nodes.length) {
      const code = record.slice(5);
      frame(Object.hasOwn(STOP_MESSAGES, code) ? STOP_MESSAGES[code] : 'Vault boot stopped',
        code !== 'IDLE' && code !== 'YIELDED');
    } else {
      throw new Error('The trace contains an unsupported or incomplete Kai record.');
    }
  }
  flushLinks();

  if (!frames.length) throw new Error('No Kai boot-view records were found.');
  return frames;
}
