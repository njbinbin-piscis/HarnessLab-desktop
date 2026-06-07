import { useEffect, useId, useRef, useState } from "react";

export type SelectOption = { value: string; label: string; disabled?: boolean };

type SelectProps = {
  value: string;
  onChange: (value: string) => void;
  options: SelectOption[];
  placeholder?: string;
  disabled?: boolean;
  className?: string;
  id?: string;
  "aria-label"?: string;
  block?: boolean;
  size?: "md" | "sm";
};

/** Build options from plain string lists. */
export function selectOptions(values: string[], labels?: Record<string, string>): SelectOption[] {
  return values.map((v) => ({ value: v, label: labels?.[v] ?? v }));
}

export function Select({
  value,
  onChange,
  options,
  placeholder,
  disabled,
  className,
  id,
  "aria-label": ariaLabel,
  block,
  size = "md",
}: SelectProps) {
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);
  const autoId = useId();

  const selected = options.find((o) => o.value === value);
  const display = selected?.label ?? placeholder ?? "Select…";

  useEffect(() => {
    if (!open) return;
    const onDoc = (e: MouseEvent) => {
      if (rootRef.current && !rootRef.current.contains(e.target as Node)) setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    document.addEventListener("mousedown", onDoc);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDoc);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  const pick = (v: string) => {
    onChange(v);
    setOpen(false);
  };

  const classes = [
    "hl-select",
    size === "sm" ? "hl-select-sm" : "",
    block ? "hl-select-block" : "",
    disabled ? "disabled" : "",
    className ?? "",
  ]
    .filter(Boolean)
    .join(" ");

  return (
    <div ref={rootRef} className={classes} id={id ?? autoId}>
      <button
        type="button"
        className="hl-select-trigger"
        disabled={disabled}
        aria-label={ariaLabel}
        aria-haspopup="listbox"
        aria-expanded={open}
        onClick={() => !disabled && setOpen((o) => !o)}
      >
        <span className={"hl-select-value" + (!selected && placeholder ? " placeholder" : "")}>{display}</span>
        <span className="hl-select-chevron" aria-hidden>
          ▾
        </span>
      </button>
      {open ? (
        <ul className="hl-select-menu" role="listbox">
          {options.map((o) => (
            <li key={o.value}>
              <button
                type="button"
                role="option"
                aria-selected={o.value === value}
                className={"hl-select-option" + (o.value === value ? " active" : "")}
                disabled={o.disabled}
                onClick={() => !o.disabled && pick(o.value)}
              >
                {o.label}
              </button>
            </li>
          ))}
        </ul>
      ) : null}
    </div>
  );
}
