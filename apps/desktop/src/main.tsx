// SPDX-License-Identifier: GPL-3.0-or-later
import './styles/index.css';

import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';

import { App } from './app/App';
import { initI18n } from './lib/i18n';
import { isTauri } from './lib/ipc';

async function bootstrap() {
  // Outside Tauri (browser dev server, Playwright, screenshots) use the in-memory backend.
  if (!isTauri() && (import.meta.env.DEV || import.meta.env.VITE_MOCK_IPC === '1')) {
    const { installMockIpc } = await import('./lib/ipc/mock');
    installMockIpc();
  }
  await initI18n();
  const el = document.getElementById('root');
  if (!el) throw new Error('missing #root');
  createRoot(el).render(
    <StrictMode>
      <App />
    </StrictMode>,
  );
}

void bootstrap();
