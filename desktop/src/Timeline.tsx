import { useEffect, useState } from "react";
import { getRecentObservations, type Observation } from "./api";

const POLL_INTERVAL_MS = 2000;
const LIMIT = 50;

function formatTime(iso: string): string {
  return new Date(iso).toLocaleTimeString();
}

function describe(obs: Observation): string {
  if (obs.kind === "app_usage") {
    const name = obs.app_name ?? "(unknown app)";
    return obs.is_active === false ? `${name} · idle` : name;
  }
  if (obs.kind === "idle") return `idle ${obs.idle_seconds}s`;
  return obs.kind;
}

export function Timeline() {
  const [observations, setObservations] = useState<Observation[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;

    async function poll() {
      try {
        const data = await getRecentObservations(LIMIT);
        if (!cancelled) {
          setObservations(data);
          setError(null);
        }
      } catch (e) {
        if (!cancelled) setError(String(e));
      }
    }

    poll();
    const id = setInterval(poll, POLL_INTERVAL_MS);
    return () => {
      cancelled = true;
      clearInterval(id);
    };
  }, []);

  return (
    <main className="container">
      <h1>Timeline</h1>
      <p className="subtitle">
        Sampling every 5s · {observations.length} recent observations
      </p>
      {error && <p className="error">{error}</p>}
      <ul className="observation-list">
        {observations.map((obs) => (
          <li
            key={obs.id}
            className={`observation${obs.is_active === false ? " inactive" : ""}`}
          >
            <span className="time">{formatTime(obs.recorded_at)}</span>
            <span className="kind">{obs.kind}</span>
            <span className="detail">{describe(obs)}</span>
          </li>
        ))}
      </ul>
    </main>
  );
}
