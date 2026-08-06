import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';

import App from './App';
import { ApplicationErrorBoundary } from './app/ApplicationErrorBoundary';
import { installFrontendDiagnostics } from './app/frontend-diagnostics';
import './styles.css';

installFrontendDiagnostics();

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <ApplicationErrorBoundary>
      <App />
    </ApplicationErrorBoundary>
  </StrictMode>,
);
