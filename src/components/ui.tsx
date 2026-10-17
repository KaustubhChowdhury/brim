import {
  Component,
  forwardRef,
  useEffect,
  useId,
  useLayoutEffect,
  useRef,
  useState,
  type ButtonHTMLAttributes,
  type ErrorInfo,
  type ReactNode,
} from "react";
import { LoaderCircle, TriangleAlert, type LucideIcon } from "lucide-react";
import type { AppError } from "../lib/types";

const cx = (...c: Array<string | false | null | undefined>) => c.filter(Boolean).join(" ");
export { cx };

type Variant = "primary" | "subtle" | "ghost" | "danger";

const variants: Record<Variant, string> = {
  primary: "bg-accent text-accent-contrast hover:brightness-110",
  subtle: "bg-field text-fg hover:bg-hover",
  ghost: "text-fg hover:bg-hover",
  danger: "bg-field text-danger hover:bg-hover",
};

export const Button = forwardRef<
  HTMLButtonElement,
  ButtonHTMLAttributes<HTMLButtonElement> & { variant?: Variant; busy?: boolean; icon?: LucideIcon; small?: boolean }
>(function Button({ variant = "subtle", busy, icon: Icon, small, className, children, disabled, ...rest }, ref) {
  return (
    <button
      ref={ref}
      type="button"
      disabled={disabled || busy}
      className={cx(
        "press inline-flex items-center justify-center gap-1.5 rounded-lg font-medium disabled:opacity-50",
        small ? "h-7 px-2.5 text-xs" : "h-8 px-3 text-[13px]",
        variants[variant],
        className,
      )}
      {...rest}
    >
      {busy ? <LoaderCircle className="size-3.5 animate-spin" aria-hidden /> : Icon ? <Icon className="size-3.5" aria-hidden /> : null}
      {children}
    </button>
  );
});

export const IconButton = forwardRef<
  HTMLButtonElement,
  Omit<ButtonHTMLAttributes<HTMLButtonElement>, "children"> & { label: string; icon: LucideIcon; active?: boolean; size?: "sm" | "md" }
>(function IconButton({ label, icon: Icon, active, size = "md", className, ...rest }, ref) {
  return (
    <button
      ref={ref}
      type="button"
      aria-label={label}
      title={label}
      className={cx(
        "press inline-flex shrink-0 items-center justify-center rounded-lg disabled:opacity-40",
        size === "sm" ? "size-7" : "size-8",
        active ? "bg-accent/20 text-accent" : "text-muted hover:bg-hover hover:text-fg",
        className,
      )}
      {...rest}
    >
      <Icon className={size === "sm" ? "size-3.5" : "size-4"} aria-hidden />
    </button>
  );
});

export function Toggle({
  checked,
  onChange,
  label,
  description,
  disabled,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
  label: string;
  description?: ReactNode;
  disabled?: boolean;
}) {
  const id = useId();
  return (
    <div className="flex items-start justify-between gap-4 py-2">
      <div className="min-w-0">
        <label htmlFor={id} className="block text-[13px] font-medium">
          {label}
        </label>
        {description && <p className="mt-0.5 text-xs text-muted">{description}</p>}
      </div>
      <button
        id={id}
        type="button"
        role="switch"
        aria-checked={checked}
        disabled={disabled}
        onClick={() => onChange(!checked)}
        className={cx("toggle relative mt-0.5 h-[22px] w-10 shrink-0 rounded-full disabled:opacity-50", checked ? "bg-accent" : "bg-line")}
      >
        <span className="toggle-knob absolute left-[3px] top-[3px] h-4 w-4 rounded-full bg-white shadow-[0_1px_3px_rgb(0_0_0/0.3)]" />
      </button>
    </div>
  );
}

/** iOS-style segmented control with a thumb that springs between options. */
export function Segmented<T extends string | number>({
  value,
  options,
  onChange,
  label,
}: {
  value: T;
  options: Array<{ value: T; label: string }>;
  onChange: (v: T) => void;
  label: string;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [thumb, setThumb] = useState<{ x: number; w: number } | null>(null);
  const [animate, setAnimate] = useState(false);
  const index = options.findIndex((o) => o.value === value);
  const labels = options.map((o) => o.label).join("|");

  useLayoutEffect(() => {
    const root = ref.current;
    if (!root) return;
    const measure = () => {
      const el = root.querySelectorAll<HTMLElement>('[role="radio"]')[index];
      setThumb(el ? { x: el.offsetLeft, w: el.offsetWidth } : null);
    };
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(root);
    return () => ro.disconnect();
  }, [index, labels]);

  // Only start animating after the first placement, so the thumb never slides in from 0.
  useEffect(() => {
    if (thumb && !animate) {
      const id = requestAnimationFrame(() => setAnimate(true));
      return () => cancelAnimationFrame(id);
    }
  }, [thumb, animate]);

  return (
    <div ref={ref} role="radiogroup" aria-label={label} className="relative inline-flex rounded-lg bg-field p-0.5">
      {thumb && (
        <span
          aria-hidden
          className={cx("absolute bottom-0.5 left-0 top-0.5 rounded-md bg-accent shadow-sm", animate && "slide-indicator")}
          style={{ width: thumb.w, transform: `translateX(${thumb.x}px)` }}
        />
      )}
      {options.map((o) => (
        <button
          key={String(o.value)}
          type="button"
          role="radio"
          aria-checked={o.value === value}
          onClick={() => onChange(o.value)}
          className={cx(
            "press press-soft relative h-7 rounded-md px-2.5 text-xs font-medium",
            o.value === value ? "text-accent-contrast" : "text-muted hover:text-fg",
          )}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}

export function SettingRow({ label, description, children }: { label: string; description?: ReactNode; children: ReactNode }) {
  return (
    <div className="flex items-center justify-between gap-4 py-2">
      <div className="min-w-0">
        <div className="text-[13px] font-medium">{label}</div>
        {description && <p className="mt-0.5 text-xs text-muted">{description}</p>}
      </div>
      <div className="shrink-0">{children}</div>
    </div>
  );
}

export const inputClass =
  "h-8 w-full rounded-lg border border-line bg-field px-2.5 text-[13px] outline-none placeholder:text-subtle focus:border-accent";

export function EmptyState({ icon: Icon, title, children }: { icon: LucideIcon; title: string; children?: ReactNode }) {
  return (
    <div className="flex flex-col items-center justify-center gap-2 px-6 py-8 text-center">
      <div className="flex size-10 items-center justify-center rounded-full bg-field text-muted">
        <Icon className="size-5" aria-hidden />
      </div>
      <div className="text-[13px] font-semibold">{title}</div>
      {children && <div className="text-xs text-muted">{children}</div>}
    </div>
  );
}

export function ErrorNote({ error, onDismiss }: { error: AppError | null; onDismiss?: () => void }) {
  if (!error) return null;
  return (
    <div role="alert" className="flex items-start gap-2 rounded-lg bg-danger/10 px-2.5 py-2 text-xs text-danger">
      <TriangleAlert className="mt-px size-3.5 shrink-0" aria-hidden />
      <span className="min-w-0 flex-1">{error.message}</span>
      {onDismiss && (
        <button type="button" className="font-medium underline" onClick={onDismiss}>
          Dismiss
        </button>
      )}
    </div>
  );
}

export function Spinner({ label = "Loading" }: { label?: string }) {
  return (
    <div className="flex items-center justify-center p-6 text-muted" role="status" aria-label={label}>
      <LoaderCircle className="size-5 animate-spin" aria-hidden />
    </div>
  );
}

/** Isolates a panel so a rendering bug cannot take down the dock (FR-DCK-014). */
export class ErrorBoundary extends Component<{ children: ReactNode; name: string }, { failed: boolean }> {
  state = { failed: false };
  static getDerivedStateFromError() {
    return { failed: true };
  }
  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error(`[${this.props.name}]`, error, info.componentStack);
  }
  render() {
    if (!this.state.failed) return this.props.children;
    return (
      <div className="flex flex-col items-center gap-3 p-6 text-center">
        <TriangleAlert className="size-6 text-warn" aria-hidden />
        <div className="text-[13px] font-semibold">{this.props.name} ran into a problem</div>
        <p className="text-xs text-muted">The rest of the dock still works.</p>
        <Button small onClick={() => this.setState({ failed: false })}>
          Try again
        </Button>
      </div>
    );
  }
}
