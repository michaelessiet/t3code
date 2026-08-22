import React, { type ReactNode } from "react";
import ReactDOM from "react-dom/client";
import { createHashHistory, createBrowserHistory } from "@tanstack/react-router";

import "@fontsource-variable/dm-sans/index.css";
import "@fontsource/jetbrains-mono/400.css";
import "@fontsource/jetbrains-mono/500.css";
import "@xterm/xterm/css/xterm.css";
import "./index.css";

import { isElectron } from "./env";
import { ManagedRelayAuthProvider } from "./cloud/managedAuth";
import { hasCloudPublicConfig } from "./cloud/publicConfig";
import { getRouter } from "./router";
import {
  syncDocumentElectronPlatformClasses,
  syncDocumentWindowControlsOverlayClass,
} from "./lib/windowControlsOverlay";
import { AppRoot } from "./AppRoot";

// Electron loads the app from a file-backed shell, so hash history avoids path resolution issues.
const history = isElectron ? createHashHistory() : createBrowserHistory();

const router = getRouter(history);

if (isElectron) {
  syncDocumentElectronPlatformClasses(navigator.platform);
  syncDocumentWindowControlsOverlayClass();
}

const clerkPublishableKey = import.meta.env.VITE_CLERK_PUBLISHABLE_KEY as string | undefined;

type AuthProviderComponent = (props: { children: ReactNode }) => ReactNode;

// Only one Clerk provider variant ever runs, so import it dynamically: this
// keeps @clerk/clerk-js (pulled by the Electron provider) and the unused
// variant out of the entry chunk on both runtimes.
async function resolveAuthProvider(publishableKey: string): Promise<AuthProviderComponent> {
  if (isElectron) {
    const [{ ClerkProvider: ElectronClerkProvider }, { passkeys }] = await Promise.all([
      import("@clerk/electron/react"),
      import("@clerk/electron/passkeys"),
    ]);
    return ({ children }) => (
      <ElectronClerkProvider publishableKey={publishableKey} passkeys={passkeys}>
        {children}
      </ElectronClerkProvider>
    );
  }
  const { ClerkProvider } = await import("@clerk/react");
  return ({ children }) => (
    <ClerkProvider publishableKey={publishableKey}>{children}</ClerkProvider>
  );
}

async function renderApp(): Promise<void> {
  const app = <AppRoot router={router} />;
  const root = ReactDOM.createRoot(document.getElementById("root") as HTMLElement);
  const AuthProvider =
    clerkPublishableKey && hasCloudPublicConfig()
      ? await resolveAuthProvider(clerkPublishableKey)
      : null;

  root.render(
    <React.StrictMode>
      {AuthProvider ? (
        <AuthProvider>
          <ManagedRelayAuthProvider>{app}</ManagedRelayAuthProvider>
        </AuthProvider>
      ) : (
        app
      )}
    </React.StrictMode>,
  );
}

void renderApp();
