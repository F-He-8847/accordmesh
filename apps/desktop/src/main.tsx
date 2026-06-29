import React from "react";
import ReactDOM from "react-dom/client";
import { App } from "./app/App";
import { OverlayWindow } from "./components/OverlayWindow";
import "./app/styles.css";

const overlay = new URLSearchParams(window.location.search).get("overlay") === "1";
ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    {overlay ? <OverlayWindow /> : <App />}
  </React.StrictMode>,
);
