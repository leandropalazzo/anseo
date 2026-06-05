"use client";

import { useRouter } from "next/navigation";
import { useEffect, useState, useSyncExternalStore } from "react";
import { Bell, Box, Database, Play, Sparkles } from "lucide-react";

import { ONBOARDED_FLAG } from "@/lib/mock-ops";

import { StepBrand } from "../_components/step-brand";
import { StepConnectProviders } from "../_components/step-connect-providers";
import { StepFirstRun } from "../_components/step-first-run";
import { StepInit } from "../_components/step-init";
import { StepScheduleAlerts } from "../_components/step-schedule-alerts";
import { Stepper, type StepperStep } from "../_components/stepper";

const STEPS: ReadonlyArray<StepperStep> = [
  { id: "init",     label: "Initialize project", icon: Sparkles },
  { id: "provider", label: "Connect providers",  icon: Database },
  { id: "brand",    label: "Configure brand",    icon: Box },
  { id: "first",    label: "First prompt run",   icon: Play },
  { id: "schedule", label: "Schedule & alerts",  icon: Bell },
];

/** SSR-safe localStorage read. Server snapshot returns `undefined` so
 *  the first client paint can branch on "still loading" without a
 *  hydration mismatch. */
function subscribe(callback: () => void): () => void {
  if (typeof window === "undefined") return () => {};
  window.addEventListener("storage", callback);
  return () => window.removeEventListener("storage", callback);
}

function getClientSnapshot(): boolean {
  try {
    return window.localStorage.getItem(ONBOARDED_FLAG) === "true";
  } catch {
    return false;
  }
}

function getServerSnapshot(): undefined {
  return undefined;
}

function useOnboardedFlag(): boolean | undefined {
  return useSyncExternalStore(subscribe, getClientSnapshot, getServerSnapshot);
}

export default function OnboardingPage() {
  const router = useRouter();
  const [step, setStep] = useState(0);
  const onboarded = useOnboardedFlag();

  // Gate: if the operator has already finished onboarding, bounce to /.
  // Settings → "Re-run onboarding" clears the flag before navigating
  // here so re-entry from that path works without a localStorage edit.
  useEffect(() => {
    if (onboarded === true) router.replace("/");
  }, [onboarded, router]);

  const complete = () => {
    try {
      window.localStorage.setItem(ONBOARDED_FLAG, "true");
    } catch {
      /* swallow — flag will re-prompt next visit but UX still proceeds */
    }
    router.push("/");
  };

  if (onboarded === undefined || onboarded === true) return null;

  return (
    <div
      data-testid="onboarding-page"
      className="grid gap-[20px] [grid-template-columns:260px_1fr]"
    >
      <aside className="flex flex-col">
        <div className="mb-[18px]">
          <div className="label-eyebrow text-[color:var(--text-faint)]">
            get started
          </div>
          <h1 className="m-0 mt-[6px] text-[30px] font-normal tracking-[var(--display-tracking)] text-[color:var(--text)]">
            Initialize Anseo
          </h1>
          <p className="mt-[6px] text-[length:var(--font-size-sm)] text-[color:var(--text-muted)]">
            Five steps. Mirrors the CLI:{" "}
            <code className="font-[family-name:var(--font-mono)]">
              ogeo init
            </code>
            .
          </p>
        </div>
        <Stepper steps={STEPS} active={step} onSelect={setStep} />
      </aside>
      <main>
        {step === 0 && <StepInit onNext={() => setStep(1)} />}
        {step === 1 && <StepConnectProviders onNext={() => setStep(2)} />}
        {step === 2 && <StepBrand onNext={() => setStep(3)} />}
        {step === 3 && <StepFirstRun onNext={() => setStep(4)} />}
        {step === 4 && <StepScheduleAlerts onComplete={complete} />}
      </main>
    </div>
  );
}
