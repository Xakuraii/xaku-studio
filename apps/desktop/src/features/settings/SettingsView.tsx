import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { useAppStore } from "@/stores/appStore";

export function SettingsView() {
  const theme = useAppStore((s) => s.theme);
  const setTheme = useAppStore((s) => s.setTheme);

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-xl font-semibold tracking-tight">Settings</h1>
        <p className="mt-1 text-sm text-muted-foreground">
          Application appearance and runtime preferences.
        </p>
      </div>

      <Card>
        <CardHeader>
          <CardTitle>Appearance</CardTitle>
        </CardHeader>
        <CardContent className="flex items-center justify-between">
          <div>
            <div className="text-sm font-medium">Color Theme</div>
            <div className="text-xs text-muted-foreground">
              Dark mode is recommended for extended timeline and subtitle editing.
            </div>
          </div>
          <div className="flex items-center gap-2">
            <Button
              size="sm"
              variant={theme === "dark" ? "default" : "secondary"}
              onClick={() => setTheme("dark")}
            >
              Dark
            </Button>
            <Button
              size="sm"
              variant={theme === "light" ? "default" : "secondary"}
              onClick={() => setTheme("light")}
            >
              Light
            </Button>
          </div>
        </CardContent>
      </Card>
    </div>
  );
}
