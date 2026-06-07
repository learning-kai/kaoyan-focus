import { useCallback, useEffect, useRef, useState, type ReactNode } from 'react';
import ConfirmDialog, { type ConfirmDialogTone } from '../components/ConfirmDialog';

type ConfirmDialogOptions = {
  cancelLabel?: string;
  children?: ReactNode;
  confirmLabel?: string;
  message: string;
  title: string;
  tone?: ConfirmDialogTone;
};

type PendingConfirm = ConfirmDialogOptions & {
  resolve: (confirmed: boolean) => void;
};

export function useConfirmDialog() {
  const [pendingConfirm, setPendingConfirm] = useState<PendingConfirm | null>(null);
  const pendingConfirmRef = useRef<PendingConfirm | null>(null);

  const confirm = useCallback((options: ConfirmDialogOptions) => {
    return new Promise<boolean>((resolve) => {
      pendingConfirmRef.current?.resolve(false);

      const nextConfirm = { ...options, resolve };
      pendingConfirmRef.current = nextConfirm;
      setPendingConfirm(nextConfirm);
    });
  }, []);

  const settle = useCallback((confirmed: boolean) => {
    const current = pendingConfirmRef.current;
    pendingConfirmRef.current = null;
    current?.resolve(confirmed);
    setPendingConfirm(null);
  }, []);

  const confirmDialog = pendingConfirm ? (
    <ConfirmDialog
      cancelLabel={pendingConfirm.cancelLabel}
      confirmLabel={pendingConfirm.confirmLabel}
      message={pendingConfirm.message}
      onCancel={() => settle(false)}
      onConfirm={() => settle(true)}
      open
      title={pendingConfirm.title}
      tone={pendingConfirm.tone}
    >
      {pendingConfirm.children}
    </ConfirmDialog>
  ) : null;

  useEffect(() => {
    return () => {
      pendingConfirmRef.current?.resolve(false);
      pendingConfirmRef.current = null;
    };
  }, []);

  return { confirm, confirmDialog };
}
