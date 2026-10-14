import "./styles/app.css";

import { mount } from "svelte";
import App from "./App.svelte";
import { adoptToken } from "./lib/api";

adoptToken();

const target = document.getElementById("app");
if (target) mount(App, { target });
