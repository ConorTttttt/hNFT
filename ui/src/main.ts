import "@fontsource-variable/inter";
import { mount } from "svelte";
import App from "./App.svelte";
import "./app.css";
import { initTheme } from "./lib/theme.svelte";

initTheme();

const target = document.getElementById("app");
if (!target) throw new Error("#app not found");

export default mount(App, { target });
