import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import ts from 'typescript';

const helperSource = await readFile(new URL('../src/utils/fileDownload.ts', import.meta.url), 'utf8');
const helperModuleSource = ts.transpileModule(helperSource, {
  compilerOptions: {
    module: ts.ModuleKind.ESNext,
    target: ts.ScriptTarget.ES2020,
  },
}).outputText;
const helperUrl = `data:text/javascript;base64,${Buffer.from(helperModuleSource).toString('base64')}`;
const { downloadTextFile } = await import(helperUrl);

await runMissingDocumentCase();
await runCase('success', false);
await runCase('failure', true);

console.log('downloadTextFile cleanup probe passed');

async function runCase(label, shouldThrowOnClick) {
  const env = createEnvironment({ shouldThrowOnClick });
  try {
    env.install();

    const filename = `${label}.txt`;
    const content = `payload:${label}`;
    let capturedError = null;

    try {
      downloadTextFile(filename, content);
    } catch (error) {
      capturedError = error;
    }

    assert.equal(env.createdBlobs.length, 1);
    assert.equal(env.createdAnchors.length, 1);
    assert.equal(env.timers.length, 1);
    assert.equal(env.bodyChildren.length, 0);
    assert.deepEqual(env.log.slice(0, 6), [
      'createObjectURL',
      'createElement:a',
      'appendChild',
      'click',
      'remove',
      'setTimeout:1000',
    ]);

    const blob = env.createdBlobs[0];
    assert.equal(await blob.text(), content);
    assert.equal(blob.type, 'text/plain;charset=utf-8');

    const anchor = env.createdAnchors[0];
    assert.equal(anchor.download, filename);
    assert.equal(anchor.rel, 'noopener');
    assert.equal(anchor.removed, true);
    assert.equal(anchor.clickCount, 1);

    if (shouldThrowOnClick) {
      assert.ok(capturedError instanceof Error);
      assert.equal(capturedError.message, 'click failed');
    } else {
      assert.equal(capturedError, null);
    }

    env.runTimers();
    assert.equal(env.revokedUrls.length, 1);
    assert.equal(env.timers.length, 0);
    assert.deepEqual(env.revokedUrls, [env.createdUrls[0]]);
    assert.deepEqual(env.log.at(-1), `revokeObjectURL:${env.createdUrls[0]}`);
  } finally {
    env.restore();
  }
}

async function runMissingDocumentCase() {
  const originalDocument = globalThis.document;
  const originalWindow = globalThis.window;
  const originalCreateObjectURL = URL.createObjectURL;
  const originalRevokeObjectURL = URL.revokeObjectURL;
  let createObjectUrlCalls = 0;
  let revokeObjectUrlCalls = 0;

  try {
    Reflect.deleteProperty(globalThis, 'document');
    Reflect.deleteProperty(globalThis, 'window');
    URL.createObjectURL = () => {
      createObjectUrlCalls += 1;
      return 'blob:unused';
    };
    URL.revokeObjectURL = () => {
      revokeObjectUrlCalls += 1;
    };

    let capturedError = null;
    try {
      downloadTextFile('missing-document.txt', 'payload:missing');
    } catch (error) {
      capturedError = error;
    }

    assert.ok(capturedError instanceof Error);
    assert.match(capturedError.message, /导出文件/);
    assert.equal(createObjectUrlCalls, 0);
    assert.equal(revokeObjectUrlCalls, 0);
  } finally {
    if (originalDocument === undefined) {
      Reflect.deleteProperty(globalThis, 'document');
    } else {
      globalThis.document = originalDocument;
    }

    if (originalWindow === undefined) {
      Reflect.deleteProperty(globalThis, 'window');
    } else {
      globalThis.window = originalWindow;
    }

    URL.createObjectURL = originalCreateObjectURL;
    URL.revokeObjectURL = originalRevokeObjectURL;
  }
}

function createEnvironment({ shouldThrowOnClick }) {
  const log = [];
  const createdBlobs = [];
  const createdUrls = [];
  const revokedUrls = [];
  const createdAnchors = [];
  const bodyChildren = [];
  const timers = [];

  const originalDocument = globalThis.document;
  const originalWindow = globalThis.window;
  const originalCreateObjectURL = URL.createObjectURL;
  const originalRevokeObjectURL = URL.revokeObjectURL;

  const body = {
    appendChild(node) {
      log.push('appendChild');
      bodyChildren.push(node);
      node.parentNode = body;
      return node;
    },
  };

  function createAnchor() {
    const anchor = {
      href: '',
      download: '',
      rel: '',
      parentNode: null,
      removed: false,
      clickCount: 0,
      removeCount: 0,
      click() {
        log.push('click');
        this.clickCount += 1;
        if (shouldThrowOnClick) {
          throw new Error('click failed');
        }
      },
      remove() {
        log.push('remove');
        this.removeCount += 1;
        this.removed = true;
        const index = bodyChildren.indexOf(this);
        if (index !== -1) {
          bodyChildren.splice(index, 1);
        }
        this.parentNode = null;
      },
    };
    createdAnchors.push(anchor);
    return anchor;
  }

  const fakeDocument = {
    body,
    createElement(tagName) {
      log.push(`createElement:${tagName}`);
      assert.equal(tagName, 'a');
      return createAnchor();
    },
  };

  function fakeSetTimeout(callback, delay) {
    log.push(`setTimeout:${delay}`);
    timers.push(callback);
    return timers.length;
  }

  function fakeCreateObjectURL(blob) {
    log.push('createObjectURL');
    createdBlobs.push(blob);
    const url = `blob:download-cleanup-${createdUrls.length + 1}`;
    createdUrls.push(url);
    return url;
  }

  function fakeRevokeObjectURL(url) {
    log.push(`revokeObjectURL:${url}`);
    revokedUrls.push(url);
  }

  return {
    bodyChildren,
    createdAnchors,
    createdBlobs,
    createdUrls,
    install() {
      globalThis.document = fakeDocument;
      globalThis.window = { setTimeout: fakeSetTimeout };
      URL.createObjectURL = fakeCreateObjectURL;
      URL.revokeObjectURL = fakeRevokeObjectURL;
    },
    log,
    revokedUrls,
    restore() {
      if (originalDocument === undefined) {
        Reflect.deleteProperty(globalThis, 'document');
      } else {
        globalThis.document = originalDocument;
      }

      if (originalWindow === undefined) {
        Reflect.deleteProperty(globalThis, 'window');
      } else {
        globalThis.window = originalWindow;
      }

      URL.createObjectURL = originalCreateObjectURL;
      URL.revokeObjectURL = originalRevokeObjectURL;
    },
    runTimers() {
      while (timers.length > 0) {
        const callback = timers.shift();
        callback?.();
      }
    },
    timers,
  };
}
