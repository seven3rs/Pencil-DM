import { mount } from "svelte";
// Bundled locally: the app must not fetch fonts from the network (and its CSP forbids it).
import "@fontsource/inter/latin-400.css";
import "@fontsource/inter/latin-500.css";
import "@fontsource/inter/latin-600.css";
import "@fontsource/inter/latin-800.css";
import "@fontsource/jetbrains-mono/latin-400.css";
import "./app.css";
import App from "./App.svelte";

mount(App, { target: document.getElementById("app")! });
