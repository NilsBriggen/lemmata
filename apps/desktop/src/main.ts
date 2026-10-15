import "@fontsource/young-serif/latin-400.css";
import "@fontsource/young-serif/latin-ext-400.css";
import "@fontsource/ibm-plex-sans/latin-400.css";
import "@fontsource/ibm-plex-sans/latin-ext-400.css";
import "@fontsource/ibm-plex-sans/latin-500.css";
import "@fontsource/ibm-plex-sans/latin-ext-500.css";
import "@fontsource/ibm-plex-sans/latin-600.css";
import "@fontsource/ibm-plex-sans/latin-ext-600.css";
import "@fontsource/ibm-plex-mono/latin-400.css";
import "@fontsource/ibm-plex-mono/latin-ext-400.css";
import "@fontsource/ibm-plex-mono/latin-500.css";
import "@fontsource/ibm-plex-mono/latin-ext-500.css";
import "./app.css";
import { mount } from "svelte";
import App from "./App.svelte";
import PresenterView from "./lib/PresenterView.svelte";

// The presenter window is the same app, opened with ?presenter=1.
const presenter = new URLSearchParams(location.search).has("presenter");
mount(presenter ? PresenterView : App, { target: document.getElementById("app")! });
