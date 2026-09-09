import { AdminWebsocket, AppWebsocket } from "@holochain/client";

const APP_ID = import.meta.env.VITE_HC_APP_ID ?? "p2p_art_marketplace";

/**
 * Open an app websocket.
 *
 * - **Dev** (hc-spin / hc-launcher): the environment is injected, so
 *   `AppWebsocket.connect()` with no arguments discovers the port and token.
 * - **Packaged** (`VITE_HC_ADMIN_PORT` set): mint a single-use app token via the
 *   admin interface, then connect the app socket with it (see client.md).
 */
export async function connectHolochain(): Promise<AppWebsocket> {
  const adminPort = import.meta.env.VITE_HC_ADMIN_PORT;
  const appPort = import.meta.env.VITE_HC_PORT;

  if (adminPort) {
    const admin = await AdminWebsocket.connect({
      url: new URL(`ws://localhost:${adminPort}`),
    });
    const { token } = await admin.issueAppAuthenticationToken({
      installed_app_id: APP_ID,
      single_use: true,
      expiry_seconds: 30,
    });
    return AppWebsocket.connect({
      url: appPort ? new URL(`ws://localhost:${appPort}`) : undefined,
      token,
    });
  }

  return AppWebsocket.connect();
}
