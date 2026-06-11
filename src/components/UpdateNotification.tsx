import { Download, Clock, SkipForward, X, Sparkles } from 'lucide-react';
import { useCallback, useEffect, useId, useRef, useState } from 'react';
import { installAppUpdate, type AppUpdate } from '../services/updateApi';
import { skipUpdateVersion, snoozeUpdateReminder } from '../services/settingsApi';
import './UpdateNotification.css';

export type UpdateInfo = {
  version: string;
  body: string | null;
};

type UpdateNotificationProps = {
  update: UpdateInfo | null;
  onDismiss: () => void;
  onUpdateInstalled?: () => void;
};

const SNOOZE_OPTIONS = [
  { label: '1 小时', hours: 1 },
  { label: '4 小时', hours: 4 },
  { label: '24 小时', hours: 24 },
];

export default function UpdateNotification({
  update,
  onDismiss,
  onUpdateInstalled,
}: UpdateNotificationProps) {
  const titleId = useId();
  const dialogRef = useRef<HTMLElement | null>(null);
  const [installing, setInstalling] = useState(false);
  const [downloadProgress, setDownloadProgress] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [showSnoozeOptions, setShowSnoozeOptions] = useState(false);

  useEffect(() => {
    if (!update) {
      return;
    }

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape' && !installing) {
        onDismiss();
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => {
      window.removeEventListener('keydown', handleKeyDown);
    };
  }, [update, installing, onDismiss]);

  const handleInstall = useCallback(async () => {
    if (!update || installing) {
      return;
    }

    try {
      setInstalling(true);
      setError(null);
      setDownloadProgress(0);

      const { check } = await import('@tauri-apps/plugin-updater');
      const updateObj = await check();

      if (!updateObj) {
        setError('更新信息已过期，请重新检查。');
        return;
      }

      await installAppUpdate(
        updateObj,
        ({ downloadedBytes, totalBytes }) => {
          if (totalBytes && totalBytes > 0) {
            setDownloadProgress(Math.round((downloadedBytes / totalBytes) * 100));
          }
        },
      );

      onUpdateInstalled?.();
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
      setInstalling(false);
      setDownloadProgress(null);
    }
  }, [update, installing, onUpdateInstalled]);

  const handleSkipVersion = useCallback(async () => {
    if (!update) {
      return;
    }

    try {
      await skipUpdateVersion(update.version);
      onDismiss();
    } catch {
      // 静默失败
      onDismiss();
    }
  }, [update, onDismiss]);

  const handleSnooze = useCallback(async (hours: number) => {
    try {
      await snoozeUpdateReminder(hours * 60 * 60 * 1000);
      onDismiss();
    } catch {
      // 静默失败
      onDismiss();
    }
  }, [onDismiss]);

  if (!update) {
    return null;
  }

  return (
    <div className="update-backdrop" role="presentation">
      <section
        aria-labelledby={titleId}
        aria-modal="true"
        className="update-dialog"
        ref={dialogRef}
        role="dialog"
        tabIndex={-1}
      >
        <header className="update-dialog-head">
          <span className="update-dialog-icon" aria-hidden="true">
            <Sparkles size={24} />
          </span>
          <div>
            <h3 id={titleId}>发现新版本</h3>
            <p className="update-version">考研专注 {update.version}</p>
          </div>
          <button
            aria-label="关闭"
            className="icon-button"
            disabled={installing}
            onClick={onDismiss}
            type="button"
          >
            <X size={17} />
          </button>
        </header>

        {update.body && (
          <div className="update-dialog-body">
            <p className="update-release-notes-label">更新内容：</p>
            <div className="update-release-notes">
              {update.body.split('\n').map((line, index) => (
                <p key={index}>{line}</p>
              ))}
            </div>
          </div>
        )}

        {error && (
          <p className="update-error" role="alert">
            {error}
          </p>
        )}

        {downloadProgress !== null && (
          <div className="update-progress">
            <div className="update-progress-bar">
              <div
                className="update-progress-fill"
                style={{ width: `${downloadProgress}%` }}
              />
            </div>
            <span className="update-progress-text">
              {downloadProgress === 100 ? '准备安装...' : `下载中 ${downloadProgress}%`}
            </span>
          </div>
        )}

        <footer className="update-dialog-actions">
          {!installing && (
            <>
              <div className="update-secondary-actions">
                <button
                  className="ghost-action"
                  disabled={installing}
                  onClick={handleSkipVersion}
                  type="button"
                >
                  <SkipForward size={15} />
                  跳过此版本
                </button>
                <div className="update-snooze-wrapper">
                  <button
                    className="ghost-action"
                    disabled={installing}
                    onClick={() => setShowSnoozeOptions(!showSnoozeOptions)}
                    type="button"
                  >
                    <Clock size={15} />
                    稍后提醒
                  </button>
                  {showSnoozeOptions && (
                    <div className="update-snooze-options">
                      {SNOOZE_OPTIONS.map((option) => (
                        <button
                          key={option.hours}
                          className="ghost-action"
                          onClick={() => void handleSnooze(option.hours)}
                          type="button"
                        >
                          {option.label}
                        </button>
                      ))}
                    </div>
                  )}
                </div>
              </div>
              <button
                className="primary-action"
                disabled={installing}
                onClick={() => void handleInstall()}
                type="button"
              >
                <Download size={17} />
                {installing ? '下载中...' : '立即更新'}
              </button>
            </>
          )}
        </footer>
      </section>
    </div>
  );
}
