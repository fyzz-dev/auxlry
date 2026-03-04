import { createFileRoute } from "@tanstack/react-router";
import { useConfig } from "@/lib/queries";

export const Route = createFileRoute("/config")({
  component: ConfigPage,
});

function ConfigPage() {
  const { data, isLoading } = useConfig();

  if (isLoading) {
    return <div className="text-muted-foreground">Loading...</div>;
  }

  if (!data) {
    return <div className="text-muted-foreground">No config available</div>;
  }

  const yaml = jsonToYaml(data);

  return (
    <div className="mx-auto max-w-screen-xl">
      <div className="rounded-xl border border-border bg-card overflow-hidden">
        <div className="flex items-center justify-between border-b border-border px-4 py-2">
          <span className="text-sm font-medium text-muted-foreground">
            config.yml
          </span>
          <span className="text-xs text-muted-foreground">read-only</span>
        </div>
        <pre className="overflow-auto p-4 text-sm leading-relaxed font-mono">
          <code>
            {yaml.map((line, i) => (
              <span key={i} className="block">
                {renderYamlLine(line)}
              </span>
            ))}
          </code>
        </pre>
      </div>
    </div>
  );
}

interface YamlLine {
  indent: number;
  key?: string;
  value?: string;
  kind: "key-value" | "key-only" | "list-item";
}

function jsonToYaml(
  obj: unknown,
  indent = 0,
  result: YamlLine[] = [],
): YamlLine[] {
  if (Array.isArray(obj)) {
    for (const item of obj) {
      if (typeof item === "object" && item !== null) {
        result.push({ indent, kind: "list-item", value: "" });
        jsonToYaml(item, indent + 1, result);
      } else {
        result.push({
          indent,
          kind: "list-item",
          value: formatValue(item),
        });
      }
    }
  } else if (typeof obj === "object" && obj !== null) {
    for (const [key, value] of Object.entries(obj)) {
      if (typeof value === "object" && value !== null) {
        result.push({ indent, key, kind: "key-only" });
        jsonToYaml(value, indent + 1, result);
      } else {
        result.push({
          indent,
          key,
          value: formatValue(value),
          kind: "key-value",
        });
      }
    }
  }
  return result;
}

function formatValue(v: unknown): string {
  if (v === null || v === undefined) return "~";
  if (typeof v === "string") return v.includes(":") ? `"${v}"` : v;
  return String(v);
}

function renderYamlLine(line: YamlLine) {
  const pad = "  ".repeat(line.indent);

  if (line.kind === "list-item") {
    return (
      <>
        <span className="text-muted-foreground/50">{pad}</span>
        <span className="text-muted-foreground">- </span>
        <span className="text-foreground">{line.value}</span>
      </>
    );
  }

  if (line.kind === "key-only") {
    return (
      <>
        <span className="text-muted-foreground/50">{pad}</span>
        <span className="text-primary">{line.key}</span>
        <span className="text-muted-foreground">:</span>
      </>
    );
  }

  return (
    <>
      <span className="text-muted-foreground/50">{pad}</span>
      <span className="text-primary">{line.key}</span>
      <span className="text-muted-foreground">: </span>
      <span className="text-foreground">{line.value}</span>
    </>
  );
}
