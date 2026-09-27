"use client";

import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useRouter, useSearchParams } from "next/navigation";
import { useEffect, useRef, useState, useSyncExternalStore, type FormEvent, type ReactNode } from "react";

import { StatusBadge } from "@/components/ui/Badge";
import { Button } from "@/components/ui/Button";
import { Field, TextInput, describedBy } from "@/components/ui/Field";
import { BrandMark, Icon } from "@/components/ui/Icon";
import { Alert } from "@/components/ui/Surface";
import type { DemoAccount, Principal } from "@/lib/api-types";
import { ApiError, api, isApiError } from "@/lib/bff";
import { cn } from "@/lib/cn";
import { formatClock } from "@/lib/format";
import { HOME, useSession } from "@/lib/session";

/** Every seeded account shares this password (listed for reviewers). */
const DEMO_PASSWORD = "demo-2026";
const LAST_EMAIL_KEY = "lastEmail";
const EMAIL = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;

type Banner =
  | { kind: "invalid"; attemptsLeft: number | null }
  | { kind: "expired" }
  | { kind: "locked"; until: number }
  | { kind: "unavailable"; message: string };

type Phase = "idle" | "submitting" | "success";

const noSubscription = () => () => {};

function readLastEmail(): string {
  try {
    return sessionStorage.getItem(LAST_EMAIL_KEY) ?? "";
  } catch {
    return "";
  }
}

function safeNext(next: string | null, principal: Principal): string {
  const home = HOME[principal.kind];
  if (!next || !next.startsWith("/") || next.startsWith("//")) return home;
  const prefix = principal.kind === "admin" ? "/admin" : "/support";
  return next.startsWith(prefix) ? next : home;
}

export function LoginView() {
  const router = useRouter();
  const params = useSearchParams();
  const queryClient = useQueryClient();
  const session = useSession();
  const expired = params.get("reason") === "expired";

  // After an expired session the email is prefilled (empty during SSR).
  const rememberedEmail = useSyncExternalStore(
    noSubscription,
    () => (expired ? readLastEmail() : ""),
    () => "",
  );
  const [emailInput, setEmail] = useState<string | null>(null);
  const email = emailInput ?? rememberedEmail;
  const [password, setPassword] = useState("");
  const [showPw, setShowPw] = useState(false);
  const [emailErr, setEmailErr] = useState<string | null>(null);
  const [pwErr, setPwErr] = useState<string | null>(null);
  const [banner, setBanner] = useState<Banner | null>(expired ? { kind: "expired" } : null);
  const [phase, setPhase] = useState<Phase>("idle");
  const [picked, setPicked] = useState<string | null>(null);
  const [signedIn, setSignedIn] = useState<Principal | null>(null);
  const [now, setNow] = useState(() => Date.now());

  const emailRef = useRef<HTMLInputElement>(null);
  const pwRef = useRef<HTMLInputElement>(null);
  const submitRef = useRef<HTMLButtonElement>(null);

  const demo = useQuery({
    queryKey: ["demo-accounts"],
    queryFn: () => api<DemoAccount[]>("auth/demo-accounts"),
    staleTime: Infinity,
  });

  // A tab that is already signed in goes straight to its home.
  useEffect(() => {
    if (session.data && !expired && phase === "idle") router.replace(HOME[session.data.kind]);
  }, [session.data, expired, phase, router]);

  // Focus the first field still to fill.
  useEffect(() => {
    (rememberedEmail ? pwRef : emailRef).current?.focus();
  }, [rememberedEmail]);

  const locked = banner?.kind === "locked" && banner.until > now;
  useEffect(() => {
    if (banner?.kind !== "locked") return;
    const timer = window.setInterval(() => {
      const t = Date.now();
      setNow(t);
      if (t >= banner.until) setBanner(null);
    }, 1000);
    return () => window.clearInterval(timer);
  }, [banner]);

  const busy = phase !== "idle";

  function pick(a: DemoAccount) {
    setEmail(a.email);
    setPassword(DEMO_PASSWORD);
    setPicked(a.email);
    setEmailErr(null);
    setPwErr(null);
    if (banner?.kind !== "locked") setBanner(null);
    if (window.matchMedia("(min-width: 640px)").matches) {
      if (!locked) submitRef.current?.focus();
    } else {
      document.getElementById("signin")?.scrollIntoView({ behavior: "smooth", block: "start" });
    }
  }

  async function submit(e?: FormEvent) {
    e?.preventDefault();
    if (busy || locked) return;
    const eErr = !email.trim() ? "Enter your email address." : !EMAIL.test(email.trim()) ? "Enter an email like name@company.com." : null;
    const pErr = !password ? "Enter your password." : null;
    setEmailErr(eErr);
    setPwErr(pErr);
    if (eErr || pErr) {
      (eErr ? emailRef : pwRef).current?.focus();
      return;
    }
    setPhase("submitting");
    try {
      const { principal } = await api<{ principal: Principal }>("auth/login", {
        method: "POST",
        json: { email: email.trim(), password },
      });
      try {
        sessionStorage.setItem(LAST_EMAIL_KEY, principal.email);
      } catch {
        // Prefill is a convenience only.
      }
      queryClient.setQueryData(["me"], principal);
      setSignedIn(principal);
      setPhase("success");
      router.replace(safeNext(params.get("next"), principal));
    } catch (err) {
      setPhase("idle");
      setPassword("");
      if (isApiError(err, 429)) {
        setBanner({ kind: "locked", until: Date.now() + (err.retryAfter ?? 300) * 1000 });
        setNow(Date.now());
      } else if (isApiError(err, 401)) {
        setBanner({ kind: "invalid", attemptsLeft: err.attemptsLeft });
        pwRef.current?.focus();
      } else {
        setBanner({
          kind: "unavailable",
          message: err instanceof ApiError ? err.message : "Check your connection and try again.",
        });
      }
    }
  }

  const invalid = banner?.kind === "invalid";
  const admins = demo.data?.filter((a) => a.role === "admin") ?? [];
  const customers = demo.data?.filter((a) => a.role === "customer") ?? [];
  const pickedAccount = demo.data?.find((a) => a.email === picked);

  return (
    <div className="min-h-dvh bg-surface sm:bg-canvas">
      <header className="flex h-14 items-center px-4 sm:h-[72px] sm:bg-surface sm:px-16">
        <a href="#signin" aria-label="Worknoon Support home" className="flex items-center gap-2.5 rounded-sm">
          <BrandMark />
          <span className="text-title-sm font-semibold tracking-[-0.01em]">Worknoon Support</span>
        </a>
      </header>

      <main className="flex flex-col items-center gap-8 pt-6 pb-20 sm:px-16 sm:pt-16">
        <div className="flex max-w-[560px] flex-col gap-3 px-4 sm:px-0 sm:text-center">
          <h1 className="text-title-lg font-semibold sm:text-display">Refund support for Worknoon orders</h1>
          <p className="text-body text-ink-muted">
            Customers describe the problem in chat and an AI assistant applies the refund policy. Anything it
            can&apos;t decide goes to a support admin, who can also review every decision.
          </p>
        </div>

        <section
          id="signin"
          aria-labelledby="signin-title"
          className="w-full scroll-mt-4 px-4 sm:w-[440px] sm:rounded-xl sm:border sm:border-border sm:bg-surface sm:p-8 sm:shadow-sm"
        >
          <h2 id="signin-title" className="mb-5 text-title font-semibold">
            Sign in
          </h2>

          {phase === "success" && signedIn ? (
            <div role="status" className="flex flex-col gap-1 rounded-lg border border-approved-border bg-approved-bg p-4">
              <p className="flex items-center gap-2 text-body-sm font-semibold text-approved-fg">
                <Icon name="check-circle" size={18} strokeWidth={2.25} className="text-approved-icon" />
                Signed in as {signedIn.name}
              </p>
              <p className="text-body-sm text-ink">
                {signedIn.kind === "admin" ? "Admin" : "Customer"} account. Opening{" "}
                {signedIn.kind === "admin" ? "the admin dashboard" : "your orders and refund chat"}…
              </p>
            </div>
          ) : (
            <form noValidate aria-busy={busy || undefined} onSubmit={submit} className="flex flex-col gap-4">
              <LoginBanner banner={banner} now={now} />

              <Field id="signin-email" label="Email" error={emailErr}>
                <TextInput
                  ref={emailRef}
                  id="signin-email"
                  type="email"
                  autoComplete="username"
                  inputMode="email"
                  spellCheck={false}
                  placeholder="you@company.com"
                  controlSize="lg"
                  value={email}
                  readOnly={busy}
                  invalid={!!emailErr || invalid}
                  aria-describedby={describedBy("signin-email", { error: !!emailErr })}
                  onChange={(e) => {
                    setEmail(e.target.value);
                    setEmailErr(null);
                    setPicked(null);
                  }}
                  className={busy ? "bg-canvas" : undefined}
                />
              </Field>

              <Field id="signin-password" label="Password" error={pwErr}>
                <div className="relative flex">
                  <TextInput
                    ref={pwRef}
                    id="signin-password"
                    type={showPw ? "text" : "password"}
                    autoComplete="current-password"
                    controlSize="lg"
                    value={password}
                    readOnly={busy}
                    invalid={!!pwErr || invalid}
                    aria-describedby={describedBy("signin-password", { error: !!pwErr })}
                    onChange={(e) => {
                      setPassword(e.target.value);
                      setPwErr(null);
                    }}
                    className={cn("pr-16", busy && "bg-canvas")}
                  />
                  <button
                    type="button"
                    aria-label={showPw ? "Hide password" : "Show password"}
                    aria-pressed={showPw}
                    aria-controls="signin-password"
                    onClick={() => setShowPw((v) => !v)}
                    className="absolute top-1 right-1 h-9 rounded-md px-3 text-body-sm font-medium text-ink-muted hover:bg-muted hover:text-ink"
                  >
                    {showPw ? "Hide" : "Show"}
                  </button>
                </div>
              </Field>

              <Button
                ref={submitRef}
                id="signin-submit"
                type="submit"
                variant="primary"
                size="lg"
                className="w-full"
                loading={phase === "submitting"}
                loadingText="Signing in…"
                disabled={locked}
              >
                Sign in
              </Button>
              <p className="text-caption text-ink-subtle">
                Your account decides where you land: customers go to their orders and refund chat, support staff
                to the admin dashboard.
              </p>
              <a href="#demo" className="link inline-flex items-center gap-1.5 self-start text-body-sm font-medium">
                <Icon name="arrow-down" size={16} />
                Reviewing the project? Use a demo account
              </a>
            </form>
          )}
        </section>

        <section
          id="demo"
          aria-labelledby="demo-title"
          className="flex w-full flex-col gap-5 border-t border-border bg-muted px-4 py-8 sm:max-w-[920px] sm:rounded-xl sm:border-0 sm:p-8"
        >
          <div className="flex flex-wrap items-center gap-3">
            <h2 id="demo-title" className="text-title font-semibold">
              Demo accounts
            </h2>
            <span className="rounded-full border border-border-strong px-2.5 py-0.5 text-caption font-medium text-ink-muted">
              For reviewers
            </span>
          </div>
          <p className="text-body-sm text-ink-muted">
            Pick an account to fill the sign-in form, then press Sign in. Every demo account uses the password{" "}
            <code className="rounded-sm bg-surface px-1.5 py-0.5 font-mono text-mono text-ink">{DEMO_PASSWORD}</code>.
          </p>
          <div className="flex items-start gap-3 rounded-lg border border-neutral-border bg-surface px-4 py-3 text-body-sm">
            <Icon name="two-pane" size={18} className="mt-px shrink-0 text-ink-muted" />
            <p>Each browser tab has its own session. Open the customer and admin views side by side in separate tabs.</p>
          </div>

          {demo.isError ? (
            <Alert tone="error" title="Couldn't load the demo accounts" action={<Button size="sm" onClick={() => demo.refetch()}>Try again</Button>}>
              You can still sign in with an email and password.
            </Alert>
          ) : null}

          <DemoGroup title="Admins" columns="sm:grid-cols-2">
            {demo.isPending ? <DemoSkeleton count={2} /> : admins.map((a) => (
              <DemoCard key={a.email} account={a} picked={picked === a.email} disabled={busy} onPick={() => pick(a)} />
            ))}
          </DemoGroup>
          <DemoGroup title="Customers · by scenario" columns="sm:grid-cols-2 lg:grid-cols-3">
            {demo.isPending ? <DemoSkeleton count={6} /> : customers.map((a) => (
              <DemoCard key={a.email} account={a} picked={picked === a.email} disabled={busy} onPick={() => pick(a)} />
            ))}
          </DemoGroup>
          <p role="status" className="sr-only">
            {pickedAccount
              ? `Filled in ${pickedAccount.name} (${pickedAccount.role === "admin" ? "admin" : pickedAccount.title}). Press Sign in.`
              : ""}
          </p>
        </section>
      </main>
    </div>
  );
}

function LoginBanner({ banner, now }: { banner: Banner | null; now: number }) {
  if (!banner) return null;
  switch (banner.kind) {
    case "invalid":
      return (
        <Alert tone="error" title="Email or password is incorrect">
          Check both and try again.
          {banner.attemptsLeft !== null && banner.attemptsLeft <= 2 && banner.attemptsLeft > 0
            ? ` ${banner.attemptsLeft === 1 ? "1 attempt" : `${banner.attemptsLeft} attempts`} left before a 5-minute pause.`
            : null}
        </Alert>
      );
    case "expired":
      return (
        <Alert tone="neutral" icon="clock" title="Your session expired">
          You were signed out. Sign in again to go back to where you were.
        </Alert>
      );
    case "locked":
      return (
        <Alert tone="error" icon="lock" title="Too many sign-in attempts">
          For your security, sign-in is paused. Try again in{" "}
          <span className="font-mono tabular">{formatClock((banner.until - now) / 1000)}</span>.
        </Alert>
      );
    case "unavailable":
      return (
        <Alert tone="error" title="Couldn't sign you in">
          {banner.message}
        </Alert>
      );
  }
}

function DemoGroup({ title, columns, children }: { title: string; columns: string; children: ReactNode }) {
  return (
    <div className="flex flex-col gap-3">
      <h3 className="text-caption font-semibold tracking-[0.06em] text-ink-muted uppercase">{title}</h3>
      <ul className={cn("grid grid-cols-1 gap-3", columns)}>{children}</ul>
    </div>
  );
}

function DemoCard({
  account,
  picked,
  disabled,
  onPick,
}: {
  account: DemoAccount;
  picked: boolean;
  disabled: boolean;
  onPick: () => void;
}) {
  const admin = account.role === "admin";
  return (
    <li className="flex">
      <button
        type="button"
        aria-pressed={picked}
        disabled={disabled}
        onClick={onPick}
        className={cn(
          "flex min-h-11 w-full flex-col gap-2 rounded-lg border bg-surface p-4 text-left transition-colors duration-120 ease-standard",
          "hover:border-border-strong disabled:cursor-not-allowed disabled:opacity-60",
          picked ? "border-[1.5px] border-ink" : "border-border",
        )}
      >
        <span className="flex flex-wrap items-center justify-between gap-2">
          <span className="text-lead font-semibold">{account.title}</span>
          {admin ? (
            <span className="rounded-full bg-primary px-2.5 py-0.5 text-caption font-medium text-white">Admin</span>
          ) : account.expected_verdict ? (
            <StatusBadge state={account.expected_verdict} size="sm" />
          ) : null}
        </span>
        <span className="text-body-sm text-ink-muted">{account.description}</span>
        <span className="flex flex-wrap items-center justify-between gap-2 text-meta text-ink-subtle">
          <span>
            {account.name} ·{" "}
            <span className="font-mono">{admin ? account.email : (account.order_ref ?? account.email)}</span>
          </span>
          {picked ? (
            <span className="inline-flex items-center gap-1 font-medium text-approved-fg">
              <Icon name="check" size={14} strokeWidth={2.5} />
              Filled
            </span>
          ) : null}
        </span>
      </button>
    </li>
  );
}

function DemoSkeleton({ count }: { count: number }) {
  return (
    <>
      {Array.from({ length: count }, (_, i) => (
        <li key={i} aria-hidden="true" className="h-[118px] animate-wn-pulse rounded-lg bg-skeleton" />
      ))}
    </>
  );
}
