"use client";

import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useCallback, useEffect, useRef, useState } from "react";

import type { ConversationDetail } from "@/lib/api-types";
import { api, isApiError } from "@/lib/bff";
import { postMessage } from "@/lib/sse";

/** What the thread is doing between a send and the stored reply. */
export type Phase = "idle" | "sending" | "reviewing" | "replying";

export type Notice =
  | { kind: "rate"; until: number }
  | { kind: "expired" }
  | { kind: "error"; message: string; retry: Outgoing };

export type Outgoing = {
  text: string;
  clientMsgId: string;
  orderId: string | null;
  /** True once the API confirmed it stored the message (`message_saved`). */
  saved: boolean;
};

const draftKey = (conversationId: string | null) => `draft:${conversationId ?? "new"}`;

export function readDraft(conversationId: string | null): string {
  try {
    return sessionStorage.getItem(draftKey(conversationId)) ?? "";
  } catch {
    return "";
  }
}

function writeDraft(conversationId: string | null, text: string) {
  try {
    if (text) sessionStorage.setItem(draftKey(conversationId), text);
    else sessionStorage.removeItem(draftKey(conversationId));
  } catch {
    // Keeping a draft across sign-in is a convenience only.
  }
}

/**
 * One conversation's messages plus the in-flight send. The reply is not
 * rendered from the stream: the API commits the reply before streaming it,
 * so on `done` the thread re-reads the conversation and the verdict card
 * animates the stored text (badge and reference first, as designed).
 */
export function useChatThread(initialConversationId: string | null, onConversationCreated: (id: string) => void) {
  const queryClient = useQueryClient();
  const [conversationId, setConversationId] = useState(initialConversationId);
  const [phase, setPhase] = useState<Phase>("idle");
  const [outgoing, setOutgoing] = useState<Outgoing | null>(null);
  const [notice, setNotice] = useState<Notice | null>(null);
  const [startedAt, setStartedAt] = useState<number | null>(null);
  const [animateId, setAnimateId] = useState<string | null>(null);
  const [draft, setDraftState] = useState(() => readDraft(initialConversationId));
  const abort = useRef<AbortController | null>(null);

  useEffect(() => () => abort.current?.abort(), []);

  const detail = useQuery({
    queryKey: ["conversation", conversationId],
    queryFn: () => api<ConversationDetail>(`conversations/${conversationId}`),
    enabled: conversationId !== null,
  });

  const setDraft = useCallback(
    (text: string) => {
      setDraftState(text);
      writeDraft(conversationId, text);
    },
    [conversationId],
  );

  const refresh = useCallback(
    async (id: string) => {
      await queryClient.invalidateQueries({ queryKey: ["conversation", id] });
      void queryClient.invalidateQueries({ queryKey: ["conversations"] });
      void queryClient.invalidateQueries({ queryKey: ["orders"] });
    },
    [queryClient],
  );

  const send = useCallback(
    async (msg: Outgoing) => {
      setNotice(null);
      setDraftState("");
      setOutgoing(msg);
      setPhase("sending");
      setStartedAt(Date.now());
      let id = conversationId;
      let replyId: string | null = null;
      let verdict = false;
      let saved = false;
      const controller = new AbortController();
      abort.current = controller;
      try {
        if (!id) {
          const created = await api<{ id: string }>("conversations", { method: "POST" });
          id = created.id;
          setConversationId(id);
          onConversationCreated(id);
        }
        const target = id;
        await postMessage(
          target,
          { client_msg_id: msg.clientMsgId, body: msg.text, order_id: msg.orderId },
          (e) => {
            switch (e.event) {
              case "message_saved":
                saved = true;
                setOutgoing((o) => (o ? { ...o, saved: true } : o));
                setPhase("reviewing");
                break;
              case "reply_start":
                verdict = e.data.kind === "verdict";
                setPhase("replying");
                break;
              case "reply_done":
                replyId = e.data.message_id;
                break;
              case "error":
                setNotice({ kind: "error", message: e.data.message, retry: { ...msg, saved: true } });
                break;
            }
          },
          controller.signal,
        );
        writeDraft(target, "");
        // A first message was typed before the conversation existed.
        if (!conversationId) writeDraft(null, "");
        await refresh(target);
        if (verdict && replyId) setAnimateId(replyId);
        setOutgoing(null);
      } catch (err) {
        if (controller.signal.aborted) return;
        if (isApiError(err, 429)) {
          setNotice({ kind: "rate", until: Date.now() + (err.retryAfter ?? 30) * 1000 });
          setDraftState(msg.text);
          setOutgoing(null);
        } else if (isApiError(err, 401)) {
          writeDraft(id, msg.text);
          setNotice({ kind: "expired" });
          setDraftState(msg.text);
          setOutgoing(null);
        } else if (isApiError(err, 409) && err.code === "request_closed" && id) {
          // Decided in another tab: re-read so the closed footer replaces the composer.
          await refresh(id);
          setDraftState(msg.text);
          setOutgoing(null);
        } else {
          setNotice({
            kind: "error",
            message: isApiError(err) ? err.message : "The connection dropped before the reply arrived.",
            retry: { ...msg, saved },
          });
          if (id) await refresh(id);
          setOutgoing(null);
        }
      } finally {
        setPhase("idle");
        setStartedAt(null);
      }
    },
    [conversationId, onConversationCreated, refresh],
  );

  /** Resends after a failure: the same id if it never reached the API, else a new message. */
  const retry = useCallback(
    (msg: Outgoing) => void send({ ...msg, clientMsgId: msg.saved ? crypto.randomUUID() : msg.clientMsgId, saved: false }),
    [send],
  );

  return {
    conversationId,
    detail,
    phase,
    outgoing,
    notice,
    setNotice,
    startedAt,
    animateId,
    draft,
    setDraft,
    send,
    retry,
  };
}
