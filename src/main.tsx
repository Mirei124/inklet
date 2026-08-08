import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
// vite 预打包 Excalidraw 时内部 SCSS 不注入，需显式引入其样式
import "@excalidraw/excalidraw/index.css";
import "./styles.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
