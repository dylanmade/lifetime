import { useEffect, useState } from "react";
import { HashRouter, Route, Routes } from "react-router-dom";
import { TooltipProvider } from "@/components/ui/tooltip";
import {
  SidebarInset,
  SidebarProvider,
  SidebarTrigger,
} from "@/components/ui/sidebar";
import { Separator } from "@/components/ui/separator";
import { getAppState, type AppStateInfo } from "./api";
import { AppSidebar } from "./AppSidebar";
import { Summary } from "./Summary";
import { Timeline } from "./Timeline";
import { Settings } from "./Settings";
import { Appearance } from "./Appearance";
import { UnlockModal } from "./UnlockModal";

function App() {
  const [appState, setAppState] = useState<AppStateInfo | null>(null);

  const refresh = () => {
    getAppState()
      .then(setAppState)
      .catch(() => setAppState(null));
  };

  useEffect(() => {
    refresh();
  }, []);

  if (appState === null) {
    return (
      <div className="text-muted-foreground flex h-screen items-center justify-center text-sm">
        Loading…
      </div>
    );
  }

  if (appState.status === "locked") {
    return (
      <TooltipProvider>
        <UnlockModal fingerprint={appState.fingerprint} onUnlocked={refresh} />
      </TooltipProvider>
    );
  }

  return (
    <TooltipProvider>
      <HashRouter>
        <SidebarProvider>
          <AppSidebar />
          <SidebarInset>
            <header className="flex h-12 items-center gap-2 border-b px-4">
              <SidebarTrigger className="-ml-1" />
              <Separator orientation="vertical" className="h-4" />
            </header>
            <main className="flex-1 p-6">
              <div className="mx-auto w-full max-w-6xl">
                <Routes>
                  <Route path="/" element={<Summary />} />
                  <Route path="/timeline" element={<Timeline />} />
                  <Route
                    path="/settings"
                    element={
                      <Settings appState={appState} onStateChanged={refresh} />
                    }
                  />
                  <Route path="/appearance" element={<Appearance />} />
                </Routes>
              </div>
            </main>
          </SidebarInset>
        </SidebarProvider>
      </HashRouter>
    </TooltipProvider>
  );
}

export default App;
