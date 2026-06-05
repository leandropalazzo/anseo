import { ConnectForm } from "./_components/connect-form";

export default function ConnectPage() {
  return (
    <div
      data-testid="ch-connect-page"
      className="flex flex-col gap-[16px]"
    >
      <div>
        <div className="label-eyebrow text-[color:var(--text-faint)]">
          infrastructure
        </div>
        <h1 className="m-0 text-[length:var(--font-size-base)] font-medium text-[color:var(--text)]">
          Connect Remote ClickHouse
        </h1>
        <p className="mt-[4px] max-w-[60ch] text-[length:var(--font-size-sm)] text-[color:var(--text-muted)]">
          Docker isn&rsquo;t available for a local install, so point Anseo at
          a managed ClickHouse instead. Pick a provider preset, fill in the
          connection details, and we&rsquo;ll probe it before saving.
        </p>
      </div>

      <div className="max-w-[520px]">
        <ConnectForm />
      </div>
    </div>
  );
}
