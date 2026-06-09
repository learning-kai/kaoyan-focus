import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import ts from 'typescript';

const helperSource = await readFile(new URL('../src/utils/clipboard.ts', import.meta.url), 'utf8');
const helperModuleSource = ts.transpileModule(helperSource, {
  compilerOptions: {
    module: ts.ModuleKind.ESNext,
    target: ts.ScriptTarget.ES2020,
  },
}).outputText;
const helperUrl = `data:text/javascript;base64,${Buffer.from(helperModuleSource).toString('base64')}`;
const { copyTextToClipboard } = await import(helperUrl);

await runAsyncClipboardCase();
await runCase('success', false);
await runCase('throws', true);

console.log('copyTextToClipboard cleanup probe passed');

async function runAsyncClipboardCase() {
  const env = createEnvironment({ shouldThrowOnExecCommand: false });
  try {
    env.install({
      clipboardWriteText: async (text) => {
        env.asyncWrites.push(text);
      },
    });

    await copyTextToClipboard('payload:async');

    assert.deepEqual(env.asyncWrites, ['payload:async']);
    assert.equal(env.createdTextAreas.length, 0);
    assert.equal(env.bodyChildren.length, 0);
    assert.deepEqual(env.log, ['writeText']);
  } finally {
    env.restore();
  }
}

async function runCase(label, shouldThrowOnExecCommand) {
  const env = createEnvironment({ shouldThrowOnExecCommand });
  try {
    env.install({
      clipboardWriteText: async () => {
        throw new Error('clipboard denied');
      },
    });

    const text = `payload:${label}`;
    let capturedError = null;

    try {
      await copyTextToClipboard(text);
    } catch (error) {
      capturedError = error;
    }

    assert.equal(env.createdTextAreas.length, 1);
    assert.equal(env.bodyChildren.length, 0);
    assert.equal(env.createdTextAreas[0].removeCount, 1);
    assert.equal(env.createdTextAreas[0].focused, true);
    assert.equal(env.createdTextAreas[0].selected, true);
    assert.equal(env.createdTextAreas[0].value, text);
    assert.deepEqual(env.log.slice(0, 7), [
      'writeText',
      'createElement:textarea',
      'appendChild',
      'focus',
      'select',
      'execCommand:copy',
      'remove',
    ]);

    if (shouldThrowOnExecCommand) {
      assert.ok(capturedError instanceof Error);
      assert.equal(capturedError.message, 'execCommand failed');
    } else {
      assert.equal(capturedError, null);
    }
  } finally {
    env.restore();
  }
}

function createEnvironment({ shouldThrowOnExecCommand }) {
  const log = [];
  const createdTextAreas = [];
  const bodyChildren = [];
  const asyncWrites = [];

  const originalDocument = globalThis.document;
  const originalNavigator = globalThis.navigator;

  const body = {
    appendChild(node) {
      log.push('appendChild');
      bodyChildren.push(node);
      node.parentNode = body;
      return node;
    },
  };

  function createTextArea() {
    const textArea = {
      value: '',
      readOnly: false,
      parentNode: null,
      focused: false,
      selected: false,
      removeCount: 0,
      style: {},
      focus() {
        log.push('focus');
        this.focused = true;
      },
      select() {
        log.push('select');
        this.selected = true;
      },
      remove() {
        log.push('remove');
        this.removeCount += 1;
        const index = bodyChildren.indexOf(this);
        if (index !== -1) {
          bodyChildren.splice(index, 1);
        }
        this.parentNode = null;
      },
    };
    createdTextAreas.push(textArea);
    return textArea;
  }

  const fakeDocument = {
    body,
    createElement(tagName) {
      log.push(`createElement:${tagName}`);
      assert.equal(tagName, 'textarea');
      return createTextArea();
    },
    execCommand(command) {
      log.push(`execCommand:${command}`);
      assert.equal(command, 'copy');
      if (shouldThrowOnExecCommand) {
        throw new Error('execCommand failed');
      }
      return true;
    },
  };

  const fakeNavigator = {
    clipboard: {
      writeText: async () => {
        throw new Error('clipboard denied');
      },
    },
  };

  return {
    bodyChildren,
    asyncWrites,
    createdTextAreas,
    install({ clipboardWriteText } = {}) {
      globalThis.document = fakeDocument;
      Object.defineProperty(globalThis, 'navigator', {
        configurable: true,
        value: clipboardWriteText
          ? {
              clipboard: {
                writeText: async (text) => {
                  log.push('writeText');
                  return clipboardWriteText(text);
                },
              },
            }
          : fakeNavigator,
      });
    },
    log,
    restore() {
      if (originalDocument === undefined) {
        Reflect.deleteProperty(globalThis, 'document');
      } else {
        globalThis.document = originalDocument;
      }

      if (originalNavigator === undefined) {
        Reflect.deleteProperty(globalThis, 'navigator');
      } else {
        Object.defineProperty(globalThis, 'navigator', {
          configurable: true,
          value: originalNavigator,
        });
      }
    },
  };
}
