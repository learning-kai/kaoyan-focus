import { invokeCommand } from './tauriInvoke';
import { listenTauriEvent } from './tauriEvents';

export type FocusWidgetDockMode = 'floating' | 'collapsed' | 'peek';
export type FocusWidgetDockEdge = 'left' | 'right' | 'top' | 'bottom';

export type FocusWidgetDockState = {
  mode: FocusWidgetDockMode;
  edge: FocusWidgetDockEdge | null;
};

export const FOCUS_WIDGET_DOCK_STATE_CHANGED_EVENT = 'focus-widget-dock-state-changed';

export const defaultFocusWidgetDockState: FocusWidgetDockState = {
  mode: 'floating',
  edge: null,
};

export function returnFocusWidgetToMain(): Promise<void> {
  return invokeCommand<void>('focus_widget_return_to_main');
}

export function hideFocusWidget(): Promise<void> {
  return invokeCommand<void>('hide_focus_widget');
}

export function getFocusWidgetAlwaysOnTop(): Promise<boolean> {
  return invokeCommand<boolean>('focus_widget_get_always_on_top');
}

export function toggleFocusWidgetAlwaysOnTop(): Promise<boolean> {
  return invokeCommand<boolean>('focus_widget_toggle_always_on_top');
}

export function getFocusWidgetDockState(): Promise<FocusWidgetDockState> {
  return invokeCommand<FocusWidgetDockState>('focus_widget_get_dock_state');
}

export function peekFocusWidgetFromEdge(): Promise<FocusWidgetDockState> {
  return invokeCommand<FocusWidgetDockState>('focus_widget_peek_from_edge');
}

export function collapseFocusWidgetToEdge(): Promise<FocusWidgetDockState> {
  return invokeCommand<FocusWidgetDockState>('focus_widget_collapse_to_edge');
}

export function listenFocusWidgetDockState(
  callback: (state: FocusWidgetDockState) => void,
): Promise<() => void> {
  return listenTauriEvent<FocusWidgetDockState>(FOCUS_WIDGET_DOCK_STATE_CHANGED_EVENT, (event) => {
    callback(event.payload);
  });
}
