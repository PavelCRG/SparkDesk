import React from "react";
import ReactDOM from "react-dom/client";
import { Theme } from "@radix-ui/themes";
import "@radix-ui/themes/styles.css";
import "./styles.css";
import { App } from "./ui/App";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <Theme appearance="dark" accentColor="jade" grayColor="slate" panelBackground="solid" radius="large">
      <App />
    </Theme>
  </React.StrictMode>,
);
