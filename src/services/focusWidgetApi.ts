import { invokeCommand } from './tauriInvoke';

export function returnFocusWidgetToMain(): Promise<void> {
  return invokeCommand<void>('focus_widget_return_to_main');
}

export function hideFocusWidget(): Promise<void> {
  return invokeCommand<void>('hide_focus_widget');
}
