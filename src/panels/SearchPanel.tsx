import { ArrowUpRight, Clock, Search, X } from "lucide-react";
import { useState } from "react";
import { api } from "../lib/ipc";
import { useTauriEvent } from "../lib/hooks";
import { cx } from "../components/ui";

type Engine = "youtube" | "google";
const RECENT_KEY = "wisp.recentSearches";
const MAX_RECENT = 6;

function loadRecent(): Array<{ q: string; engine: Engine }> {
  try {
    const v = JSON.parse(localStorage.getItem(RECENT_KEY) ?? "[]");
    return Array.isArray(v) ? v.slice(0, MAX_RECENT) : [];
  } catch {
    return [];
  }
}

function saveRecent(items: Array<{ q: string; engine: Engine }>) {
  try {
    localStorage.setItem(RECENT_KEY, JSON.stringify(items));
  } catch {
    // storage unavailable: recents just aren't remembered
  }
}

/** Search YouTube (or Google) in the default browser. Recent searches stay on this PC. */
export function SearchPanel() {
  const [query, setQuery] = useState("");
  const [engine, setEngine] = useState<Engine>("youtube");
  const [recent, setRecent] = useState(loadRecent);
  const [error, setError] = useState<string | null>(null);

  useTauriEvent("data:reset", () => {
    saveRecent([]);
    setRecent([]);
  });

  const run = (q: string, e: Engine) => {
    const text = q.trim();
    if (!text) return;
    setError(null);
    api.search
      .web(e, text)
      .then(() => {
        const next = [{ q: text, engine: e }, ...recent.filter((r) => r.q !== text)].slice(0, MAX_RECENT);
        setRecent(next);
        saveRecent(next);
        setQuery("");
      })
      .catch((err) => setError(err.message));
  };

  return (
    <div className="flex h-full flex-col px-8 pb-5 pt-6">
      <form
        className="search-field flex items-center gap-3 rounded-2xl px-4"
        onSubmit={(e) => {
          e.preventDefault();
          run(query, engine);
        }}
      >
        <Search className="size-[18px] shrink-0 text-muted" aria-hidden />
        <input
          autoFocus
          value={query}
          maxLength={300}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Escape" && query) {
              e.preventDefault();
              e.stopPropagation();
              setQuery("");
            }
          }}
          placeholder={engine === "youtube" ? "Search YouTube" : "Search the web"}
          aria-label={engine === "youtube" ? "Search YouTube" : "Search the web"}
          className="h-12 min-w-0 flex-1 bg-transparent text-[15px] outline-none placeholder:text-subtle"
        />
        <div className="flex shrink-0 gap-1" role="radiogroup" aria-label="Search with">
          {(["youtube", "google"] as const).map((e) => (
            <button
              key={e}
              type="button"
              role="radio"
              aria-checked={engine === e}
              onClick={() => setEngine(e)}
              className={cx(
                "press h-7 rounded-full px-3 text-[11.5px] font-semibold",
                engine === e ? "bg-accent/15 text-accent" : "text-muted hover:text-fg",
              )}
            >
              {e === "youtube" ? "YouTube" : "Google"}
            </button>
          ))}
        </div>
      </form>
      {error && <p className="mt-3 text-xs text-danger">{error}</p>}

      {recent.length > 0 && (
        <div className="mt-6 min-h-0 flex-1">
          <div className="mb-2 flex items-center justify-between">
            <span className="text-[11px] font-semibold uppercase tracking-[0.08em] text-subtle">Recent</span>
            <button
              type="button"
              className="text-[11px] text-subtle hover:text-fg"
              onClick={() => {
                saveRecent([]);
                setRecent([]);
              }}
            >
              Clear
            </button>
          </div>
          <ul className="grid grid-cols-2 gap-1">
            {recent.map((r) => (
              <li key={`${r.engine}:${r.q}`} className="row-in group relative">
                <button
                  type="button"
                  onClick={() => run(r.q, r.engine)}
                  className="press press-soft flex w-full items-center gap-2.5 rounded-xl px-3 py-2.5 text-left hover:bg-hover"
                >
                  <Clock className="size-3.5 shrink-0 text-subtle" aria-hidden />
                  <span className="min-w-0 flex-1 truncate text-[13px]">{r.q}</span>
                  <span className="text-[10.5px] text-subtle">{r.engine === "youtube" ? "YouTube" : "Google"}</span>
                  <ArrowUpRight className="size-3.5 shrink-0 text-subtle opacity-0 transition-opacity group-hover:opacity-100" aria-hidden />
                </button>
                <button
                  type="button"
                  aria-label={`Remove “${r.q}”`}
                  onClick={() => {
                    const next = recent.filter((x) => x !== r);
                    setRecent(next);
                    saveRecent(next);
                  }}
                  className="absolute -right-1 -top-1 hidden size-5 items-center justify-center rounded-full bg-[var(--surface-solid)] text-subtle shadow group-hover:flex"
                >
                  <X className="size-3" aria-hidden />
                </button>
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}
