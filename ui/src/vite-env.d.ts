/// <reference types="svelte" />
/// <reference types="vite/client" />

interface ImportMetaEnv {
  /** Conductor app-websocket port. Injected by hc-spin / hc-launcher in dev. */
  readonly VITE_HC_PORT?: string;
  /** Admin-websocket port. Only for packaged builds that mint their own app token. */
  readonly VITE_HC_ADMIN_PORT?: string;
  /** Installed app id, for the packaged (admin-token) connection path. */
  readonly VITE_HC_APP_ID?: string;
}
interface ImportMeta {
  readonly env: ImportMetaEnv;
}
