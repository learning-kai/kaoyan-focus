export async function copyTextToClipboard(text: string) {
  let asyncCopyError: unknown = null;

  if (typeof navigator !== 'undefined' && navigator.clipboard?.writeText) {
    try {
      await navigator.clipboard.writeText(text);
      return;
    } catch (reason) {
      asyncCopyError = reason;
    }
  }

  if (typeof document === 'undefined') {
    if (asyncCopyError instanceof Error) {
      throw asyncCopyError;
    }

    throw new Error('当前环境不支持复制到剪贴板。');
  }

  const textArea = document.createElement('textarea');
  textArea.value = text;
  textArea.readOnly = true;
  textArea.style.position = 'fixed';
  textArea.style.top = '0';
  textArea.style.left = '0';
  textArea.style.width = '2px';
  textArea.style.height = '2px';
  textArea.style.opacity = '0';
  document.body.appendChild(textArea);
  let copied = false;

  try {
    textArea.focus();
    textArea.select();
    copied = document.execCommand('copy');
  } finally {
    textArea.remove();
  }

  if (!copied) {
    if (asyncCopyError instanceof Error) {
      throw asyncCopyError;
    }

    throw new Error('复制到剪贴板失败。');
  }
}
