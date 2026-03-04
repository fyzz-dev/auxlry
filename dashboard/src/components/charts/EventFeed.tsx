import { useEventStream, type SseEvent } from "@/lib/queries";
import { motion, AnimatePresence } from "motion/react";

const KIND_COLORS: Record<string, string> = {
  core_started: "text-emerald-400",
  core_stopping: "text-red-400",
  message_received: "text-blue-400",
  message_sent: "text-blue-300",
  synapse_started: "text-violet-400",
  synapse_completed: "text-violet-300",
  synapse_failed: "text-red-400",
  operator_started: "text-amber-400",
  operator_completed: "text-amber-300",
  operator_failed: "text-red-400",
  node_connected: "text-emerald-400",
  node_disconnected: "text-orange-400",
  memory_stored: "text-cyan-400",
};

function summarize(event: SseEvent): string {
  const p = event.payload;
  switch (p.type) {
    case "message_received":
      return `${p.author} in #${p.channel}`;
    case "message_sent":
      return `to #${p.channel}`;
    case "synapse_started":
      return String(p.task ?? "").slice(0, 40);
    case "synapse_completed":
      return `synapse ${String(p.synapse_id ?? "").slice(0, 8)}`;
    case "operator_started":
      return `${p.task} on ${p.node}`;
    case "operator_completed":
      return `op ${String(p.operator_id ?? "").slice(0, 8)}`;
    case "node_connected":
    case "node_disconnected":
      return String(p.node ?? "");
    case "memory_stored":
      return String(p.summary ?? p.key ?? "").slice(0, 40);
    default:
      return p.type.replace(/_/g, " ");
  }
}

function formatTime(iso: string): string {
  try {
    return new Date(iso).toLocaleTimeString([], {
      hour: "2-digit",
      minute: "2-digit",
      second: "2-digit",
    });
  } catch {
    return "";
  }
}

export function EventFeed() {
  const events = useEventStream(30);

  if (events.length === 0) {
    return (
      <div className="flex items-center justify-center h-full text-muted-foreground text-sm">
        Waiting for events...
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-0.5 overflow-y-auto h-full pr-1">
      <AnimatePresence initial={false}>
        {events.map((event) => (
          <motion.div
            key={event.id}
            initial={{ opacity: 0, height: 0 }}
            animate={{ opacity: 1, height: "auto" }}
            exit={{ opacity: 0 }}
            transition={{ duration: 0.2 }}
            className="flex items-start gap-2 rounded-md px-2 py-1.5 text-xs hover:bg-muted/50"
          >
            <span className="text-muted-foreground/60 font-mono shrink-0 tabular-nums">
              {formatTime(event.timestamp)}
            </span>
            <span
              className={`font-medium shrink-0 ${KIND_COLORS[event.payload.type] ?? "text-muted-foreground"}`}
            >
              {event.payload.type.replace(/_/g, " ")}
            </span>
            <span className="text-muted-foreground truncate">
              {summarize(event)}
            </span>
          </motion.div>
        ))}
      </AnimatePresence>
    </div>
  );
}
