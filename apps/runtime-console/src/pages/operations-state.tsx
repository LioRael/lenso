import { cn } from "../lib/cn";

export function OperationsLoadingRows() {
  return (
    <>
      <div className="h-14 animate-pulse border-b border-(--border-subtle) bg-(--elevated)" />
      <div className="h-14 animate-pulse border-b border-(--border-subtle) bg-(--elevated)" />
      <div className="h-14 animate-pulse border-b border-(--border-subtle) bg-(--elevated)" />
    </>
  );
}

export function OperationsMessageRow({
  message,
  tone = "muted",
}: {
  message: string;
  tone?: "error" | "muted";
}) {
  return (
    <div
      className={cn(
        "border-b border-(--border-subtle) px-3 py-3 font-mono text-[11px]",
        tone === "error" ? "text-[#ef4444]" : "text-(--muted)"
      )}
    >
      {message}
    </div>
  );
}
