import { getContext, setContext } from "svelte";
import type { AppState } from "./appState.svelte";

const KEY = Symbol("nft-app-state");

export function setAppStateContext(state: AppState): void {
  setContext(KEY, state);
}

export function getAppStateContext(): AppState {
  const state = getContext<AppState | undefined>(KEY);
  if (!state) {
    throw new Error("AppState is not in context — render inside <App/>");
  }
  return state;
}
