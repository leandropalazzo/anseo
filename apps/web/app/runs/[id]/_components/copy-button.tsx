"use client";

import { useState, type ReactNode } from "react";
import { Check, Copy } from "lucide-react";

import { Button } from "@/components/ui/button";

export interface CopyButtonProps {
  value: string;
  children: ReactNode;
}

/** Ghost button that copies a string to the clipboard with inline feedback. */
export function CopyButton({ value, children }: CopyButtonProps) {
  const [copied, setCopied] = useState(false);
  const onClick = async () => {
    try {
      await navigator.clipboard?.writeText(value);
      setCopied(true);
      setTimeout(() => setCopied(false), 1200);
    } catch {
      /* clipboard unavailable */
    }
  };
  return (
    <Button
      variant="ghost"
      size="sm"
      onClick={onClick}
      leadingIcon={
        copied ? (
          <Check size={11} strokeWidth={1.5} color="var(--ok)" />
        ) : (
          <Copy size={11} strokeWidth={1.5} />
        )
      }
      aria-label={copied ? "Copied" : "Copy"}
    >
      {copied ? "copied" : children}
    </Button>
  );
}
