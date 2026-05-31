import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { ThemeProvider } from "./theme/ThemeProvider";

// Bundle theme font options so the picker can switch among them without
// needing network access at runtime.
import "@fontsource-variable/dm-sans";
import "@fontsource-variable/inter";
import "@fontsource-variable/geist";
import "@fontsource-variable/jetbrains-mono";

import "./index.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <ThemeProvider>
      <App />
    </ThemeProvider>
  </React.StrictMode>,
);
