import { useQuery } from "@tanstack/react-query";
import { useEffect, useRef, useState } from "react";
import { fetchApi } from "./api";

// ── Types ────────────────────────────────────────────────────────────────────

export interface TimeSeriesRow {
  date: string;
  kind?: string;
  count: number;
}

export interface CategoryRow {
  type: string;
  count: number;
}

export interface GraphNode {
  id: string;
  memory_type: string;
  access_count: number;
  content: string;
  created_at: string;
  [key: string]: unknown;
}

export interface GraphEdge {
  source: string;
  target: string;
  relation_type: string;
  weight: number;
  [key: string]: unknown;
}

export interface GraphData {
  nodes: GraphNode[];
  links: GraphEdge[];
}

export interface StatusData {
  status: string;
  bus_receivers: number;
  has_events: boolean;
  interfaces: number;
  nodes: number;
}

export interface SseEvent {
  id: string;
  timestamp: string;
  payload: Record<string, unknown> & { type: string };
}

// ── Live-updating queries (refetch every 10s) ────────────────────────────────

const LIVE_INTERVAL = 10_000;

export function useMemoryActions() {
  return useQuery({
    queryKey: ["memory-actions"],
    queryFn: () => fetchApi<{ data: TimeSeriesRow[] }>("/api/memory-actions"),
    select: (d) => d.data,
    refetchInterval: LIVE_INTERVAL,
  });
}

export function useAgentSpawns() {
  return useQuery({
    queryKey: ["agent-spawns"],
    queryFn: () => fetchApi<{ data: TimeSeriesRow[] }>("/api/agent-spawns"),
    select: (d) => d.data,
    refetchInterval: LIVE_INTERVAL,
  });
}

export function useMessageHeatmap() {
  return useQuery({
    queryKey: ["message-heatmap"],
    queryFn: () => fetchApi<{ data: TimeSeriesRow[] }>("/api/message-heatmap"),
    select: (d) => d.data,
    refetchInterval: LIVE_INTERVAL,
  });
}

export function useMemoryCategories() {
  return useQuery({
    queryKey: ["memory-categories"],
    queryFn: () =>
      fetchApi<{ data: CategoryRow[] }>("/api/memory-categories"),
    select: (d) => d.data,
    refetchInterval: LIVE_INTERVAL,
  });
}

export function useMemoryGraph() {
  return useQuery({
    queryKey: ["memory-graph"],
    queryFn: () => fetchApi<GraphData>("/api/memories/graph"),
    refetchInterval: LIVE_INTERVAL,
  });
}

export function useConfig() {
  return useQuery({
    queryKey: ["config"],
    queryFn: () => fetchApi<Record<string, unknown>>("/api/config"),
  });
}

export function useStatus() {
  return useQuery({
    queryKey: ["status"],
    queryFn: () => fetchApi<StatusData>("/api/status"),
    refetchInterval: 5000,
  });
}

// ── SSE event stream hook ────────────────────────────────────────────────────

export function useEventStream(maxEvents = 50) {
  const [events, setEvents] = useState<SseEvent[]>([]);
  const esRef = useRef<EventSource | null>(null);

  useEffect(() => {
    const es = new EventSource("/api/events");
    esRef.current = es;

    const handler = (e: MessageEvent) => {
      try {
        const parsed = JSON.parse(e.data) as SseEvent;
        setEvents((prev) => [parsed, ...prev].slice(0, maxEvents));
      } catch {
        // ignore parse errors
      }
    };

    // Listen to all event types we know about
    const eventTypes = [
      "core_started",
      "core_stopping",
      "message_received",
      "message_sent",
      "interface_ack",
      "interface_reply",
      "interface_delegate",
      "synapse_started",
      "synapse_progress",
      "synapse_completed",
      "synapse_failed",
      "operator_started",
      "operator_progress",
      "operator_completed",
      "operator_failed",
      "node_connected",
      "node_disconnected",
      "memory_stored",
    ];
    for (const type of eventTypes) {
      es.addEventListener(type, handler);
    }

    return () => {
      es.close();
    };
  }, [maxEvents]);

  return events;
}
