"use client";

import { useQueryClient } from "@tanstack/react-query";
import { useEffect } from "react";

import { api } from "./bff";

/**
 * Tells the API the customer has seen a conversation up to `lastSeq` while it
 * is on screen with unread replies, then refreshes the unread counts.
 */
export function useMarkRead(conversationId: string | null, lastSeq: number | undefined, unread: boolean) {
  const queryClient = useQueryClient();
  useEffect(() => {
    if (!conversationId || lastSeq === undefined || !unread) return;
    api<void>(`conversations/${conversationId}/read`, { method: "POST", json: { seq: lastSeq } })
      .then(() => queryClient.invalidateQueries({ queryKey: ["conversations"] }))
      // A missed read marker only leaves a badge up until the next view.
      .catch(() => {});
  }, [conversationId, lastSeq, unread, queryClient]);
}
