export type ToastKind = "info" | "success" | "error";

export interface Toast {
  id: number;
  kind: ToastKind;
  message: string;
  /** If set, the toast body is clickable; running this also dismisses it. */
  onClick?: () => void;
}

let seq = 0;
export const toasts = $state<Toast[]>([]);

export function pushToast(
  kind: ToastKind,
  message: string,
  opts: { ttlMs?: number; onClick?: () => void } = {},
): void {
  const id = ++seq;
  toasts.push({ id, kind, message, onClick: opts.onClick });
  const ttl = opts.ttlMs ?? (opts.onClick ? 8000 : 4000);
  if (ttl > 0) setTimeout(() => dismissToast(id), ttl);
}

export function dismissToast(id: number): void {
  const i = toasts.findIndex((t) => t.id === id);
  if (i !== -1) toasts.splice(i, 1);
}

export const toast = {
  info: (m: string, onClick?: () => void) => pushToast("info", m, { onClick }),
  success: (m: string, onClick?: () => void) =>
    pushToast("success", m, { onClick }),
  /** Errors linger longer. */
  error: (m: string) => pushToast("error", m, { ttlMs: 7000 }),
};
