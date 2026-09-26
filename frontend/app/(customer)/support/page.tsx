"use client";

import { Suspense } from "react";

import { SupportPage } from "@/components/customer/SupportPage";
import { RequireRole } from "@/lib/session";

export default function Support() {
  return (
    <Suspense>
      <RequireRole role="customer">{(principal) => <SupportPage principal={principal} />}</RequireRole>
    </Suspense>
  );
}
