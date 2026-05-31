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
      <BaseDialog.Backdrop className="fixed inset-0 z-30 bg-black/35" />
      <BaseDialog.Popup
        className={cn(
          "fixed right-3 top-3 z-40 h-[calc(100vh-24px)] w-[min(540px,calc(100vw-22px))] overflow-auto rounded-lg border border-white/10 bg-[#101318] shadow-2xl shadow-black/50 data-[starting-style]:translate-x-4 data-[starting-style]:opacity-0 transition duration-200",
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
