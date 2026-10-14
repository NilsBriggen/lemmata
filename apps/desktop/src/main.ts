import "@fontsource/young-serif/400.css";
import "@fontsource/ibm-plex-sans/400.css";
import "@fontsource/ibm-plex-sans/500.css";
import "@fontsource/ibm-plex-sans/600.css";
import "@fontsource/ibm-plex-mono/400.css";
import "@fontsource/ibm-plex-mono/500.css";
import "./app.css";
import { mount } from "svelte";
import App from "./App.svelte";
import PresenterView from "./lib/PresenterView.svelte";

// The presenter window is the same app, opened with ?presenter=1.
const presenter = new URLSearchParams(location.search).has("presenter");
mount(presenter ? PresenterView : App, { target: document.getElementById("app")! });
