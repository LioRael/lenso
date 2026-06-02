import { Dialog as BaseDialog } from "@base-ui/react/dialog";
import type { HTMLAttributes, PropsWithChildren } from "react";

import { cn } from "../../lib/cn";

function DialogRoot({
  children,
  onOpenChange,
  open,
}: PropsWithChildren<{
  open: boolean;
  onOpenChange: (open: boolean) => void;
}>) {
  return (
    <BaseDialog.Root onOpenChange={onOpenChange} open={open}>
      {children}
    </BaseDialog.Root>
  );
}

function DialogPortal({ children }: PropsWithChildren) {
  return <BaseDialog.Portal>{children}</BaseDialog.Portal>;
}

function DialogBackdrop({ className }: { className?: string }) {
  return (
    <BaseDialog.Backdrop
      className={cn(
        "fixed inset-0 z-40 bg-[color-mix(in_srgb,var(--background)_60%,transparent)] backdrop-blur-sm",
        className
      )}
    />
  );
}

function DialogPopup({
  children,
  className,
  ...props
}: PropsWithChildren<HTMLAttributes<HTMLDivElement> & { className?: string }>) {
  return (
    <BaseDialog.Popup
      className={cn(
        "fixed left-1/2 top-[12vh] z-50 w-[min(560px,calc(100vw-28px))] -translate-x-1/2 overflow-hidden border border-(--border-subtle) bg-(--surface) shadow-(--elevation-overlay) transition duration-150 data-[starting-style]:-translate-y-2 data-[starting-style]:scale-[0.985] data-[starting-style]:opacity-0",
        className
      )}
      {...props}
    >
      {children}
    </BaseDialog.Popup>
  );
}

export const Dialog = Object.assign(DialogRoot, {
  Portal: DialogPortal,
  Backdrop: DialogBackdrop,
  Popup: DialogPopup,
  Title: BaseDialog.Title,
  Description: BaseDialog.Description,
  Close: BaseDialog.Close,
});
