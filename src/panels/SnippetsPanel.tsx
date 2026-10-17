import { Check, Code, Pencil, Plus, Search, Trash2 } from "lucide-react";
import { useMemo, useState } from "react";
import { api, toAppError } from "../lib/ipc";
import { useAction, useLoader, useSettings } from "../lib/hooks";
import type { Snippet } from "../lib/types";
import { Button, EmptyState, ErrorNote, IconButton, Spinner, cx, inputClass } from "../components/ui";
import { useDock } from "../dock/context";

type Draft = { id: number | null; title: string; content: string; category: string };

export function SnippetsPanel() {
  const { settings } = useSettings();
  const { collapse } = useDock();
  const snippets = useLoader(api.snippets.list, ["snippets:changed"]);
  const action = useAction();
  const [query, setQuery] = useState("");
  const [category, setCategory] = useState<string | null>(null);
  const [draft, setDraft] = useState<Draft | null>(null);
  const [copied, setCopied] = useState<number | null>(null);

  const all = snippets.data ?? [];
  const categories = useMemo(
    () => [...new Set(all.map((s) => s.category).filter((c): c is string => !!c))].sort((a, b) => a.localeCompare(b)),
    [all],
  );
  const q = query.trim().toLowerCase();
  const shown = all.filter(
    (s) =>
      (!category || s.category === category) &&
      (!q || s.title.toLowerCase().includes(q) || s.content.toLowerCase().includes(q) || s.category?.toLowerCase().includes(q)),
  );
  const blank = (): Draft => ({ id: null, title: "", content: "", category: category ?? "" });

  const copy = async (s: Snippet) => {
    try {
      await api.snippets.copy(s.id);
    } catch (e) {
      action.setError(toAppError(e));
      return;
    }
    setCopied(s.id);
    window.setTimeout(() => setCopied(null), 900);
    if (settings.collapseAfterCopy) window.setTimeout(() => collapse(true), 250);
  };

  if (draft) {
    return <SnippetEditor draft={draft} categories={categories} onClose={() => setDraft(null)} />;
  }

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center gap-1.5 px-3 pt-3">
        <div className="relative flex-1">
          <Search className="pointer-events-none absolute left-2.5 top-2 size-4 text-subtle" aria-hidden />
          <input
            autoFocus
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Escape" && query) {
                e.preventDefault();
                e.stopPropagation();
                setQuery("");
              } else if (e.key === "Enter" && shown[0]) {
                e.preventDefault();
                copy(shown[0]);
              }
            }}
            placeholder="Search snippets"
            aria-label="Search snippets"
            className={cx(inputClass, "pl-8")}
          />
        </div>
        <IconButton label="New snippet" icon={Plus} onClick={() => setDraft(blank())} />
      </div>
      {categories.length > 0 && (
        <div className="flex gap-1 overflow-x-auto px-3 pt-2" role="group" aria-label="Categories">
          {[null, ...categories].map((c) => (
            <button
              key={c ?? "__all"}
              type="button"
              aria-pressed={category === c}
              onClick={() => setCategory(c)}
              className={cx(
                "h-6 shrink-0 rounded-full px-2.5 text-[11px]",
                category === c ? "bg-accent text-accent-contrast" : "bg-field text-muted hover:text-fg",
              )}
            >
              {c ?? "All"}
            </button>
          ))}
        </div>
      )}
      <div className="px-3 pt-2">
        <ErrorNote error={action.error ?? snippets.error} onDismiss={() => action.setError(null)} />
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto px-2 pb-3 pt-1">
        {snippets.loading && !snippets.data ? (
          <Spinner />
        ) : shown.length === 0 ? (
          all.length === 0 ? (
            <EmptyState icon={Code} title="No snippets yet">
              Save commands, replies or templates you reuse, then copy them with one click.
              <div className="mt-3">
                <Button small variant="primary" icon={Plus} onClick={() => setDraft(blank())}>
                  New snippet
                </Button>
              </div>
            </EmptyState>
          ) : (
            <EmptyState icon={Search} title="No matches" />
          )
        ) : (
          <ul className="space-y-1" aria-label="Snippets">
            {shown.map((s) => (
              <li key={s.id} className="row-in group relative">
                <button
                  type="button"
                  onClick={() => copy(s)}
                  title="Copy to clipboard"
                  className="press press-soft w-full rounded-lg px-2.5 py-2 pr-16 text-left hover:bg-hover"
                >
                  <div className="flex items-center gap-1.5 text-[13px] font-medium">
                    <span className="truncate">{s.title}</span>
                    {copied === s.id && (
                      <span className="pop flex shrink-0 items-center gap-0.5 text-[11px] font-normal text-ok">
                        <Check className="size-3" aria-hidden /> Copied
                      </span>
                    )}
                  </div>
                  <div className="mt-0.5 line-clamp-2 whitespace-pre-wrap break-all font-mono text-[11px] text-muted">{s.content}</div>
                  {s.category && <div className="mt-1 text-[10px] uppercase tracking-wide text-subtle">{s.category}</div>}
                </button>
                <div className="absolute right-1.5 top-1.5 flex gap-0.5">
                  <IconButton
                    size="sm"
                    label={`Edit ${s.title}`}
                    icon={Pencil}
                    className="opacity-60 group-hover:opacity-100 focus-visible:opacity-100"
                    onClick={() => setDraft({ id: s.id, title: s.title, content: s.content, category: s.category ?? "" })}
                  />
                  <IconButton
                    size="sm"
                    label={`Delete ${s.title}`}
                    icon={Trash2}
                    className="opacity-60 group-hover:opacity-100 focus-visible:opacity-100"
                    onClick={() => action.run(() => api.snippets.remove(s.id))}
                  />
                </div>
              </li>
            ))}
          </ul>
        )}
      </div>
    </div>
  );
}

function SnippetEditor({ draft, categories, onClose }: { draft: Draft; categories: string[]; onClose: () => void }) {
  const [d, setD] = useState(draft);
  const action = useAction();
  const valid = d.title.trim().length > 0 && d.content.trim().length > 0;
  const save = async () => {
    const input = { title: d.title, content: d.content, category: d.category.trim() || null };
    const r = await action.run(() => (d.id == null ? api.snippets.create(input) : api.snippets.update(d.id, input)));
    if (r) onClose();
  };
  return (
    <form
      className="flex h-full flex-col gap-2 p-3"
      onSubmit={(e) => {
        e.preventDefault();
        if (valid) save();
      }}
      onKeyDown={(e) => {
        if (e.key === "Escape") {
          e.preventDefault();
          e.stopPropagation();
          onClose();
        } else if (e.key === "Enter" && e.ctrlKey && valid) {
          e.preventDefault();
          save();
        }
      }}
    >
      <div className="text-[13px] font-semibold">{d.id == null ? "New snippet" : "Edit snippet"}</div>
      <input
        autoFocus
        aria-label="Name"
        placeholder="Name"
        maxLength={80}
        value={d.title}
        onChange={(e) => setD({ ...d, title: e.target.value })}
        className={inputClass}
      />
      <input
        aria-label="Category"
        placeholder="Category (optional)"
        maxLength={40}
        list="snippet-categories"
        value={d.category}
        onChange={(e) => setD({ ...d, category: e.target.value })}
        className={inputClass}
      />
      <datalist id="snippet-categories">
        {categories.map((c) => (
          <option key={c} value={c} />
        ))}
      </datalist>
      <textarea
        aria-label="Snippet text"
        placeholder="Text to copy"
        value={d.content}
        onChange={(e) => setD({ ...d, content: e.target.value })}
        className={cx(inputClass, "h-auto min-h-0 flex-1 resize-none py-2 font-mono text-xs")}
      />
      <ErrorNote error={action.error} />
      <div className="flex items-center justify-between">
        <span className="text-[11px] text-subtle">Ctrl+Enter to save · Esc to cancel</span>
        <div className="flex gap-1.5">
          <Button variant="ghost" onClick={onClose}>
            Cancel
          </Button>
          <Button type="submit" variant="primary" disabled={!valid} busy={action.busy}>
            Save
          </Button>
        </div>
      </div>
    </form>
  );
}
