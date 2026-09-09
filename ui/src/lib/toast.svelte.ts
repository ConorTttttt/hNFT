export type ToastKind = "info" | "success" | "error";

export interface Toast {
  id: number;
  kind: ToastKind;
  message: string;
}

let seq = 0;
export const toasts = $state<Toast[]>([]);

export function pushToast(kind: ToastKind, message: string, ttlMs = 4000): void {
  const id = ++seq;
  toasts.push({ id, kind, message });
  if (ttlMs > 0) setTimeout(() => dismissToast(id), ttlMs);
}

export function dismissToast(id: number): void {
  const i = toasts.findIndex((t) => t.id === id);
  if (i !== -1) toasts.splice(i, 1);
}

export const toast = {
  info: (m: string) => pushToast("info", m),
  success: (m: string) => pushToast("success", m),
  /** Errors linger longer. */
  error: (m: string) => pushToast("error", m, 7000),
};
