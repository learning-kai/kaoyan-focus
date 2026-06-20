import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import ts from 'typescript';

const helperSource = await readFile(new URL('../src/services/alertApi.ts', import.meta.url), 'utf8');
const testableHelperSource = helperSource
  .replace("import { getAppSettings, getCustomReminderSound } from './settingsApi';", [
    'const getAppSettings = async () => ({});',
    'const getCustomReminderSound = async () => null;',
  ].join('\n'))
  .replace("import { showStudyReminder } from './systemApi';", 'const showStudyReminder = async () => undefined;')
  .replace(/import type .*?;\n/gs, '');
const helperModuleSource = ts.transpileModule(testableHelperSource, {
  compilerOptions: {
    module: ts.ModuleKind.ESNext,
    target: ts.ScriptTarget.ES2020,
  },
}).outputText;

const helperUrl = `data:text/javascript;base64,${Buffer.from(helperModuleSource).toString('base64')}`;
const {
  resolveDesktopNotificationSoundId,
} = await import(helperUrl);

const customSettings = createSettings({ reminder_sound_source: 'custom' });
assert.equal(resolveDesktopNotificationSoundId(customSettings, { localSoundPlaying: true }), 'silent');
assert.equal(resolveDesktopNotificationSoundId(customSettings, { localSoundPlaying: false }), 'classic');

const builtinSettings = createSettings({ reminder_sound_source: 'builtin', reminder_sound_id: 'urgent' });
assert.equal(resolveDesktopNotificationSoundId(builtinSettings, { localSoundPlaying: true }), 'silent');
assert.equal(resolveDesktopNotificationSoundId(builtinSettings, { localSoundPlaying: false }), 'urgent');

const mutedSettings = createSettings({ reminder_sound_volume: 0 });
assert.equal(resolveDesktopNotificationSoundId(mutedSettings, { localSoundPlaying: false }), 'silent');

const quietSettings = createSettings({
  reminder_quiet_hours_enabled: true,
  reminder_quiet_hours_start: '00:00',
  reminder_quiet_hours_end: '23:59',
});
assert.equal(
  resolveDesktopNotificationSoundId(quietSettings, {
    localSoundPlaying: false,
    now: new Date('2026-06-20T12:00:00'),
  }),
  'silent',
);

console.log('reminder sound priority probe passed');

function createSettings(patch = {}) {
  return {
    reminder_sound_source: 'builtin',
    reminder_sound_id: 'classic',
    reminder_sound_file_name: null,
    reminder_sound_updated_at: null,
    reminder_sound_volume: 100,
    reminder_sound_duration_seconds: 30,
    reminder_quiet_hours_enabled: false,
    reminder_quiet_hours_start: '22:30',
    reminder_quiet_hours_end: '07:00',
    ...patch,
  };
}
