export type Theme = "light" | "dark";

const KEY = "p2p-art:theme";

function initial(): Theme {
  try {
    const stored = localStorage.getItem(KEY);
    if (stored === "light" || stored === "dark") return stored;
  } catch {
    /* private mode */
  }
  return typeof matchMedia === "function" &&
    matchMedia("(prefers-color-scheme: light)").matches
    ? "light"
    : "dark";
}

export const theme = $state<{ value: Theme }>({ value: initial() });

function apply(t: Theme): void {
  document.documentElement.dataset.theme = t;
}

/** Call once before mount so there's no flash of the wrong theme. */
export function initTheme(): void {
  apply(theme.value);
}

export function toggleTheme(): void {
  theme.value = theme.value === "dark" ? "light" : "dark";
  apply(theme.value);
  try {
    localStorage.setItem(KEY, theme.value);
  } catch {
    /* ignore */
  }
}
