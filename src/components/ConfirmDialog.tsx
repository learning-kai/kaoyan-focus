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

const focusableSelector = [
  'a[href]',
  'button:not([disabled])',
  'textarea:not([disabled])',
  'input:not([disabled])',
  'select:not([disabled])',
  '[tabindex]:not([tabindex="-1"])',
].join(',');

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
  const dialogRef = useRef<HTMLElement | null>(null);
  const cancelButtonRef = useRef<HTMLButtonElement | null>(null);
  const previousFocusRef = useRef<HTMLElement | null>(null);

  useEffect(() => {
    if (!open) {
      return;
    }

    previousFocusRef.current = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    if (cancelButtonRef.current && !cancelButtonRef.current.disabled) {
      cancelButtonRef.current.focus();
    } else {
      dialogRef.current?.focus();
    }

    return () => {
      if (previousFocusRef.current?.isConnected) {
        previousFocusRef.current.focus();
      }
      previousFocusRef.current = null;
    };
  }, [open]);

  useEffect(() => {
    if (!open) {
      return;
    }

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape' && !loading) {
        onCancel();
        return;
      }

      if (event.key !== 'Tab') {
        return;
      }

      const focusableElements = Array.from(
        dialogRef.current?.querySelectorAll<HTMLElement>(focusableSelector) ?? [],
      ).filter((element) => element.offsetParent !== null || element === document.activeElement);
      if (focusableElements.length === 0) {
        event.preventDefault();
        dialogRef.current?.focus();
        return;
      }

      const firstElement = focusableElements[0];
      const lastElement = focusableElements[focusableElements.length - 1];
      if (event.shiftKey && document.activeElement === firstElement) {
        event.preventDefault();
        lastElement.focus();
      } else if (!event.shiftKey && document.activeElement === lastElement) {
        event.preventDefault();
        firstElement.focus();
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
        ref={dialogRef}
        role="dialog"
        tabIndex={-1}
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
