import { AlertTriangle, X } from 'lucide-react';
import { useEffect, useId, useRef, type ReactNode } from 'react';

export type ConfirmDialogTone = 'default' | 'danger';

type ConfirmDialogProps = {
  cancelLabel?: string;
  children?: ReactNode;
  confirmLabel?: string;
  loading?: boolean;
  message: string;
  onCancel: () => void;
  onConfirm: () => void;
  open: boolean;
  title: string;
  tone?: ConfirmDialogTone;
};

export default function ConfirmDialog({
  cancelLabel = '取消',
  children,
  confirmLabel = '确认',
  loading = false,
  message,
  onCancel,
  onConfirm,
  open,
  title,
  tone = 'default',
}: ConfirmDialogProps) {
  const titleId = useId();
  const messageId = useId();
  const cancelButtonRef = useRef<HTMLButtonElement | null>(null);

  useEffect(() => {
    if (!open) {
      return;
    }

    cancelButtonRef.current?.focus();
  }, [open]);

  useEffect(() => {
    if (!open) {
      return;
    }

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape' && !loading) {
        onCancel();
      }
    };

    window.addEventListener('keydown', handleKeyDown);

    return () => {
      window.removeEventListener('keydown', handleKeyDown);
    };
  }, [loading, onCancel, open]);

  if (!open) {
    return null;
  }

  return (
    <div className="confirm-backdrop" role="presentation">
      <section
        aria-describedby={messageId}
        aria-labelledby={titleId}
        aria-modal="true"
        className={`confirm-dialog tone-${tone}`}
        role="dialog"
      >
        <header className="confirm-dialog-head">
          <span className="confirm-dialog-icon" aria-hidden="true">
            <AlertTriangle size={20} />
          </span>
          <div>
            <h3 id={titleId}>{title}</h3>
            <p id={messageId}>{message}</p>
          </div>
          <button aria-label="关闭确认面板" className="icon-button" disabled={loading} onClick={onCancel} type="button">
            <X size={17} />
          </button>
        </header>
        {children && <div className="confirm-dialog-body">{children}</div>}
        <footer className="confirm-dialog-actions">
          <button className="secondary-action" disabled={loading} onClick={onCancel} ref={cancelButtonRef} type="button">
            {cancelLabel}
          </button>
          <button
            className={tone === 'danger' ? 'danger-action' : 'primary-action'}
            disabled={loading}
            onClick={onConfirm}
            type="button"
          >
            {loading ? '处理中' : confirmLabel}
          </button>
        </footer>
      </section>
    </div>
  );
}
