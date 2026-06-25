import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Sparkles } from "lucide-react";

interface UpdateAnnouncementDialogProps {
  version: string | null;
  onClose: () => void;
}

export function UpdateAnnouncementDialog({
  version,
  onClose,
}: UpdateAnnouncementDialogProps) {
  return (
    <Dialog open={Boolean(version)} onOpenChange={(open) => !open && onClose()}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Sparkles className="h-5 w-5 text-primary" />
            Voicetypr Updated
          </DialogTitle>
          <DialogDescription>
            Successfully updated to version {version}
          </DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button onClick={onClose}>Dismiss</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
