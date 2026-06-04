import { Pill } from "@/components/ui/pill";

export interface StatusPillProps {
  status: "ok" | "failed";
  errorKind?: string | null;
}

export function StatusPill({ status, errorKind }: StatusPillProps) {
  if (status === "ok") return <Pill tone="ok">ok</Pill>;
  return <Pill tone="danger">{errorKind ?? "failed"}</Pill>;
}
