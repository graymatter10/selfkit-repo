import "./app.css";
import { mount } from "svelte";
import App from "./App.svelte";
import Capture from "./Capture.svelte";

const view = new URLSearchParams(location.search).get("view");
mount(view === "capture" ? Capture : App, { target: document.getElementById("app")! });
