import { Dialog as BaseDialog } from "@base-ui/react/dialog";
import type { HTMLAttributes, PropsWithChildren } from "react";

import { cn } from "../../lib/cn";

function DrawerRoot({
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

function DrawerContent({
  children,
  className,
  ...props
}: PropsWithChildren<HTMLAttributes<HTMLDivElement> & { className?: string }>) {
  return (
    <BaseDialog.Portal>
      <BaseDialog.Backdrop className="fixed inset-0 z-30 bg-[color-mix(in_srgb,var(--background)_35%,transparent)]" />
      <BaseDialog.Popup
        className={cn(
          "fixed right-2 top-2 z-40 h-[calc(100vh-16px)] w-[min(540px,calc(100vw-16px))] overflow-auto border border-[var(--border-subtle)] bg-[var(--surface)] shadow-2xl shadow-[var(--shadow-strong)] data-[starting-style]:translate-x-4 data-[starting-style]:opacity-0 transition duration-200",
          className
        )}
        {...props}
      >
        {children}
      </BaseDialog.Popup>
    </BaseDialog.Portal>
  );
}

export const Drawer = Object.assign(DrawerRoot, {
  Content: DrawerContent,
  Title: BaseDialog.Title,
  Close: BaseDialog.Close,
});
