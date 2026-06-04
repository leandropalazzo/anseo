"use client";

/** A selectable prompt. Derived from the live runs/trend data (declared
 *  prompt names); `text` is optional context for the dropdown label. */
export interface PromptOption {
  id: string;
  name: string;
  text?: string;
}

export interface PromptPickerProps {
  prompts: ReadonlyArray<PromptOption>;
  value: PromptOption;
  onChange: (next: PromptOption) => void;
}

export function PromptPicker({ prompts, value, onChange }: PromptPickerProps) {
  return (
    <select
      aria-label="Prompt"
      value={value.id}
      onChange={(e) => {
        const next = prompts.find((p) => p.id === e.target.value);
        if (next) onChange(next);
      }}
      className="cursor-pointer appearance-none border border-[color:var(--border)] bg-[color:var(--bg-sunken)] py-[5px] pl-[10px] pr-[28px] font-[family-name:var(--font-mono)] text-[length:var(--font-size-sm)] text-[color:var(--text)]"
      style={{
        backgroundImage: `url("data:image/svg+xml;utf8,<svg xmlns='http://www.w3.org/2000/svg' width='10' height='10' viewBox='0 0 24 24' fill='none' stroke='%23999' stroke-width='2'><path d='M6 9l6 6 6-6'/></svg>")`,
        backgroundRepeat: "no-repeat",
        backgroundPosition: "right 8px center",
      }}
    >
      {prompts.map((p) => (
        <option key={p.id} value={p.id}>
          {p.text ? `${p.name} — ${p.text.slice(0, 40)}` : p.name}
        </option>
      ))}
    </select>
  );
}
