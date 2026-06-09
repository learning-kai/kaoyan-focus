import React from 'react';
import ReactDOM from 'react-dom/client';
import './styles.css';
import './components.css';
import './theme-light.css';
import './professional-ui.css';
import './theme-variants.css';
import './motion.css';

async function renderCurrentWindow() {
  const root = document.getElementById('root');
  if (!root) {
    throw new Error('Root element is missing.');
  }

  const isFocusWidgetWindow = new URLSearchParams(window.location.search).get('windowLabel') === 'focus-widget';
  if (isFocusWidgetWindow) {
    document.documentElement.classList.add('focus-widget-document');
    document.body.classList.add('focus-widget-body');
  }

  const EntryApp = isFocusWidgetWindow ? (await import('./pages/FocusWidgetPage')).default : (await import('./App')).default;

  ReactDOM.createRoot(root).render(
    <React.StrictMode>
      <EntryApp />
    </React.StrictMode>,
  );
}

void renderCurrentWindow();
