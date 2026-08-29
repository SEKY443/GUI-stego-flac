import type { ReactNode } from "react";

export function Section({ title, children, defaultOpen = true }: {
  title: string;
  children: ReactNode;
  defaultOpen?: boolean;
}) {
  return (
    <details
      open={defaultOpen}
      className="rounded-lg border border-neutral-300 bg-neutral-50 dark:border-neutral-800 dark:bg-neutral-900/50"
    >
      <summary className="cursor-pointer select-none px-4 py-2.5 text-sm font-medium text-neutral-700 hover:text-black dark:text-neutral-200 dark:hover:text-white">
        {title}
      </summary>
      <div className="space-y-3 border-t border-neutral-300 px-4 py-3 dark:border-neutral-800">
        {children}
      </div>
    </details>
  );
}

export function Field({ label, hint, children }: { label: string; hint?: string; children: ReactNode }) {
  return (
    <label className="block space-y-1">
      <span className="text-xs font-medium uppercase tracking-wide text-neutral-500 dark:text-neutral-400">
        {label}
      </span>
      {children}
      {hint && (
        <span className="block text-xs text-neutral-500 dark:text-neutral-500">{hint}</span>
      )}
    </label>
  );
}

export function TextInput(props: React.InputHTMLAttributes<HTMLInputElement>) {
  return (
    <input
      {...props}
      className={
        "w-full rounded-md border border-neutral-300 bg-white px-3 py-1.5 text-sm text-black " +
        "placeholder:text-neutral-400 focus:border-teal-500 focus:outline-none focus:ring-1 focus:ring-teal-500 " +
        "dark:border-neutral-700 dark:bg-black dark:text-white dark:placeholder:text-neutral-600 " +
        (props.className ?? "")
      }
    />
  );
}

export function Select({
  value,
  onChange,
  children,
}: {
  value: string;
  onChange: (value: string) => void;
  children: ReactNode;
}) {
  return (
    <select
      value={value}
      onChange={(e) => onChange(e.target.value)}
      className="w-full rounded-md border border-neutral-300 bg-white px-3 py-1.5 text-sm text-black focus:border-teal-500 focus:outline-none focus:ring-1 focus:ring-teal-500 dark:border-neutral-700 dark:bg-black dark:text-white"
    >
      {children}
    </select>
  );
}

export function Checkbox({
  checked,
  onChange,
  label,
}: {
  checked: boolean;
  onChange: (checked: boolean) => void;
  label: string;
}) {
  return (
    <label className="flex items-center gap-2 text-sm text-neutral-800 dark:text-neutral-200">
      <input
        type="checkbox"
        checked={checked}
        onChange={(e) => onChange(e.target.checked)}
        className="h-4 w-4 rounded border-neutral-400 bg-white text-teal-500 focus:ring-teal-500 dark:border-neutral-600 dark:bg-black"
      />
      {label}
    </label>
  );
}

export function Button({
  onClick,
  children,
  variant = "primary",
  disabled,
  type = "button",
}: {
  onClick?: () => void;
  children: ReactNode;
  variant?: "primary" | "secondary" | "danger";
  disabled?: boolean;
  type?: "button" | "submit";
}) {
  const styles = {
    primary:
      "bg-teal-500 text-black hover:bg-teal-400 disabled:bg-teal-900 disabled:text-neutral-500",
    secondary:
      "border border-neutral-300 text-neutral-800 hover:border-neutral-500 hover:text-black " +
      "dark:border-neutral-700 dark:text-neutral-200 dark:hover:border-neutral-500 dark:hover:text-white disabled:opacity-40",
    danger: "bg-red-500/90 text-white hover:bg-red-500 disabled:bg-red-900 disabled:text-neutral-500",
  }[variant];
  return (
    <button
      type={type}
      onClick={onClick}
      disabled={disabled}
      className={`rounded-md px-3 py-1.5 text-sm font-medium transition-colors disabled:cursor-not-allowed ${styles}`}
    >
      {children}
    </button>
  );
}

export function FilePicker({
  value,
  placeholder,
  onPick,
  onClear,
}: {
  value: string | null;
  placeholder: string;
  onPick: () => void;
  onClear?: () => void;
}) {
  return (
    <div className="flex items-center gap-2">
      <TextInput readOnly value={value ?? ""} placeholder={placeholder} onClick={onPick} />
      <Button variant="secondary" onClick={onPick}>
        Browse…
      </Button>
      {value && onClear && (
        <Button variant="secondary" onClick={onClear}>
          Clear
        </Button>
      )}
    </div>
  );
}

export function Banner({ kind, children }: { kind: "error" | "warning" | "info"; children: ReactNode }) {
  const styles = {
    error:
      "border-red-300 bg-red-50 text-red-800 dark:border-red-800 dark:bg-red-950/60 dark:text-red-200",
    warning:
      "border-amber-300 bg-amber-50 text-amber-800 dark:border-amber-800 dark:bg-amber-950/60 dark:text-amber-200",
    info: "border-neutral-300 bg-neutral-50 text-neutral-700 dark:border-neutral-700 dark:bg-neutral-900 dark:text-neutral-300",
  }[kind];
  return <div className={`rounded-md border px-3 py-2 text-sm ${styles}`}>{children}</div>;
}

export function Row({ label, value }: { label: string; value: ReactNode }) {
  return (
    <div className="flex justify-between gap-4 border-b border-neutral-200 py-1.5 text-sm last:border-0 dark:border-neutral-800/70">
      <span className="text-neutral-500 dark:text-neutral-400">{label}</span>
      <span className="text-right text-black dark:text-white">{value}</span>
    </div>
  );
}

export function ProgressBar({ stage }: { stage: string | null }) {
  if (!stage) return null;
  return (
    <div className="space-y-1.5">
      <div className="h-1.5 w-full overflow-hidden rounded-full bg-neutral-200 dark:bg-neutral-800">
        <div className="h-full w-1/3 animate-[indeterminate_1.2s_ease-in-out_infinite] rounded-full bg-teal-500" />
      </div>
      <p className="text-xs text-neutral-500 dark:text-neutral-400">{stage}…</p>
      <style>{`
        @keyframes indeterminate {
          0% { margin-left: -33%; }
          100% { margin-left: 100%; }
        }
      `}</style>
    </div>
  );
}

export function humanBytes(bytes: number): string {
  const units = ["B", "KiB", "MiB", "GiB", "TiB"];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return unit === 0 ? `${bytes} B` : `${value.toFixed(1)} ${units[unit]}`;
}

export function humanDuration(seconds: number): string {
  if (seconds < 60) return `${seconds.toFixed(1)} s`;
  const total = Math.round(seconds);
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  return h > 0
    ? `${h} h ${String(m).padStart(2, "0")} m ${String(s).padStart(2, "0")} s`
    : `${m} m ${String(s).padStart(2, "0")} s`;
}
