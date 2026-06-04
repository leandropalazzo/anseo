import type { ButtonHTMLAttributes, ReactNode } from "react";

export type ButtonVariant = "primary" | "secondary" | "ghost" | "danger";
export type ButtonSize = "sm" | "md";

export interface ButtonProps
  extends Omit<ButtonHTMLAttributes<HTMLButtonElement>, "children"> {
  variant?: ButtonVariant;
  size?: ButtonSize;
  /** Optional icon node (e.g. a Lucide icon) rendered before children. */
  leadingIcon?: ReactNode;
  children?: ReactNode;
}

const VARIANT: Readonly<Record<ButtonVariant, string>> = {
  primary:
    "bg-[color:var(--accent)] text-[color:var(--accent-ink)] border-[color:var(--accent)] hover:opacity-90",
  secondary:
    "bg-[color:var(--bg-elev-2)] text-[color:var(--text)] border-[color:var(--border)] hover:bg-[color:var(--bg-elev)]",
  ghost:
    "bg-transparent text-[color:var(--text-muted)] border-transparent hover:text-[color:var(--text)] hover:bg-[color:var(--bg-elev-2)]",
  danger:
    "bg-[color:var(--danger)] text-[color:var(--bg)] border-[color:var(--danger)] hover:opacity-90",
};

const SIZE: Readonly<Record<ButtonSize, string>> = {
  sm: "px-[8px] py-[3px] text-[length:var(--font-size-xs)]",
  md: "px-[10px] py-[5px] text-[length:var(--font-size-sm)]",
};

/**
 * Standard Signal-direction button. Inherits the `box-sizing: border-box`
 * reset from globals so the 1px border doesn't shift sibling layout.
 */
export function Button({
  variant = "secondary",
  size = "md",
  leadingIcon,
  children,
  className,
  type = "button",
  ...rest
}: ButtonProps) {
  return (
    <button
      type={type}
      className={[
        "inline-flex cursor-pointer items-center justify-center gap-[6px]",
        "border font-[family-name:var(--font-body)] leading-tight",
        "disabled:cursor-not-allowed disabled:opacity-50",
        VARIANT[variant],
        SIZE[size],
        className,
      ]
        .filter(Boolean)
        .join(" ")}
      {...rest}
    >
      {leadingIcon}
      {children}
    </button>
  );
}
