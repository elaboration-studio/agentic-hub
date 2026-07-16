import { useState } from "react";
import { toast } from "sonner";
import type { ColorScheme } from "@/types";
import { messageOf } from "@/shared";
import { useAppearanceStore } from "@/state/appearance";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";

const COLOR_SCHEMES: readonly ColorScheme[] = ["system", "light", "dark"];

function isColorScheme(value: string): value is ColorScheme {
  return COLOR_SCHEMES.some((colorScheme) => colorScheme === value);
}

interface AppearancePanelProps {
  colorScheme: ColorScheme;
  onChanged: () => void;
}

export function AppearancePanel({ colorScheme, onChanged }: AppearancePanelProps) {
  const savePreference = useAppearanceStore((state) => state.savePreference);
  const [busy, setBusy] = useState(false);

  const persist = async (next: string) => {
    if (!isColorScheme(next)) return;
    setBusy(true);
    try {
      await savePreference(next);
      onChanged();
    } catch (error) {
      toast.error(messageOf(error));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Card className="p-4">
      <CardHeader className="p-0">
        <CardTitle className="text-xs font-semibold uppercase tracking-[0.08em] text-muted-foreground">
          Appearance
        </CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col gap-2 p-0">
        <p className="text-xs text-muted-foreground">
          Choose an app-wide color scheme. Follow system updates automatically when your operating
          system appearance changes.
        </p>
        <div className="flex items-center gap-3">
          <Label htmlFor="color-scheme">Color scheme</Label>
          <Select value={colorScheme} disabled={busy} onValueChange={(next) => void persist(next)}>
            <SelectTrigger id="color-scheme" className="w-[180px]" aria-label="Color scheme">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="system">Follow system</SelectItem>
              <SelectItem value="light">Light</SelectItem>
              <SelectItem value="dark">Dark</SelectItem>
            </SelectContent>
          </Select>
        </div>
      </CardContent>
    </Card>
  );
}
