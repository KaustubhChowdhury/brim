import type { ReactNode } from "react";

export function Section({ title, description, children }: { title: string; description?: ReactNode; children: ReactNode }) {
  return (
    <section className="mb-7">
      <h2 className="text-base font-semibold">{title}</h2>
      {description && <p className="mt-0.5 text-xs text-muted">{description}</p>}
      <div className="mt-3 divide-y divide-line rounded-xl border border-line bg-[var(--surface-solid)] px-4">{children}</div>
    </section>
  );
}
