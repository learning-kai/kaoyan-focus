type ReminderPayload = {
  title: string;
  body: string;
};

let audioContext: AudioContext | null = null;

export async function notifyStudyReminder(payload: ReminderPayload) {
  playDing();
  await showDesktopNotification(payload);
}

function playDing() {
  try {
    const AudioContextClass = window.AudioContext || window.webkitAudioContext;
    if (!AudioContextClass) {
      return;
    }

    audioContext ??= new AudioContextClass();
    if (audioContext.state === 'suspended') {
      void audioContext.resume();
    }

    const now = audioContext.currentTime;
    const phrase = [
      { offset: 0, frequency: 880, length: 0.28, type: 'sine' as OscillatorType, gain: 0.2 },
      { offset: 0.34, frequency: 1174, length: 0.3, type: 'triangle' as OscillatorType, gain: 0.22 },
      { offset: 0.74, frequency: 988, length: 0.34, type: 'sine' as OscillatorType, gain: 0.18 },
      { offset: 1.18, frequency: 1318, length: 0.4, type: 'triangle' as OscillatorType, gain: 0.24 },
    ];

    for (const note of phrase) {
      const oscillator = audioContext.createOscillator();
      const gain = audioContext.createGain();
      const startAt = now + note.offset;
      const peakAt = startAt + 0.035;
      const endAt = startAt + note.length;

      oscillator.type = note.type;
      oscillator.frequency.setValueAtTime(note.frequency, startAt);
      oscillator.frequency.exponentialRampToValueAtTime(note.frequency * 1.08, endAt);

      gain.gain.setValueAtTime(0.0001, startAt);
      gain.gain.exponentialRampToValueAtTime(note.gain, peakAt);
      gain.gain.exponentialRampToValueAtTime(0.0001, endAt);

      oscillator.connect(gain);
      gain.connect(audioContext.destination);
      oscillator.start(startAt);
      oscillator.stop(endAt + 0.02);
    }
  } catch {
    // Sound reminders are best-effort. Notification still handles the visible cue.
  }
}

async function showDesktopNotification({ title, body }: ReminderPayload) {
  try {
    const { showStudyReminder } = await import('./systemApi');
    await showStudyReminder(title, body);
    return;
  } catch {
    // Continue to plugin/browser notification fallback.
  }

  try {
    const notification = await import('@tauri-apps/plugin-notification');
    let permitted = await notification.isPermissionGranted();

    if (!permitted) {
      const permission = await notification.requestPermission();
      permitted = permission === 'granted';
    }

    if (permitted) {
      notification.sendNotification({
        title,
        body,
        sound: 'C:\\Windows\\Media\\Alarm02.wav',
      });
      return;
    }
  } catch {
    // Fall through to browser notification.
  }

  if ('Notification' in window) {
    if (Notification.permission === 'default') {
      await Notification.requestPermission();
    }

    if (Notification.permission === 'granted') {
      new Notification(title, { body });
    }
  }
}

declare global {
  interface Window {
    webkitAudioContext?: typeof AudioContext;
  }
}
