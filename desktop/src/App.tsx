import { useEffect, useState } from "react";
import { HashRouter, Routes, Route, NavLink } from "react-router-dom";
import { getAppState, type AppStateInfo } from "./api";
import { Timeline } from "./Timeline";
import { Settings } from "./Settings";
import { UnlockModal } from "./UnlockModal";
import "./App.css";

function App() {
  const [appState, setAppState] = useState<AppStateInfo | null>(null);

  const refresh = () => {
    getAppState().then(setAppState).catch(() => setAppState(null));
  };

  useEffect(() => {
    refresh();
  }, []);

  if (appState === null) {
    return <div className="loading">Loading…</div>;
  }

  if (appState.status === "locked") {
    return (
      <UnlockModal
        fingerprint={appState.fingerprint}
        onUnlocked={refresh}
      />
    );
  }

  return (
    <HashRouter>
      <header className="app-header">
        <span className="brand">Lifetime</span>
        <nav>
          <NavLink to="/" end>Timeline</NavLink>
          <NavLink to="/settings">Settings</NavLink>
        </nav>
      </header>
      <Routes>
        <Route path="/" element={<Timeline />} />
        <Route
          path="/settings"
          element={<Settings appState={appState} onStateChanged={refresh} />}
        />
      </Routes>
    </HashRouter>
  );
}

export default App;
