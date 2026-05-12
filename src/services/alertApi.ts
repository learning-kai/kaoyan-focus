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

    const oscillator = audioContext.createOscillator();
    const gain = audioContext.createGain();
    const now = audioContext.currentTime;

    oscillator.type = 'sine';
    oscillator.frequency.setValueAtTime(880, now);
    oscillator.frequency.exponentialRampToValueAtTime(1320, now + 0.08);

    gain.gain.setValueAtTime(0.0001, now);
    gain.gain.exponentialRampToValueAtTime(0.24, now + 0.018);
    gain.gain.exponentialRampToValueAtTime(0.0001, now + 0.32);

    oscillator.connect(gain);
    gain.connect(audioContext.destination);
    oscillator.start(now);
    oscillator.stop(now + 0.34);
  } catch {
    // Sound reminders are best-effort. Notification still handles the visible cue.
  }
}

async function showDesktopNotification({ title, body }: ReminderPayload) {
  try {
    const notification = await import('@tauri-apps/plugin-notification');
    let permitted = await notification.isPermissionGranted();

    if (!permitted) {
      const permission = await notification.requestPermission();
      permitted = permission === 'granted';
    }

    if (permitted) {
      notification.sendNotification({ title, body });
    }
  } catch {
    if ('Notification' in window) {
      if (Notification.permission === 'default') {
        await Notification.requestPermission();
      }

      if (Notification.permission === 'granted') {
        new Notification(title, { body });
      }
    }
  }
}

declare global {
  interface Window {
    webkitAudioContext?: typeof AudioContext;
  }
}
