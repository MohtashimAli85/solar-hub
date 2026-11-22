import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Label, Select } from "@/components/ui/input";
import { useToastPosition, type ToastPosition } from "@/hooks/useToastPosition";

const POSITIONS: { value: ToastPosition; label: string }[] = [
  { value: "bottom-right", label: "Bottom right" },
  { value: "top-right", label: "Top right" },
];

export function RemindersCard() {
  const { position, setPosition } = useToastPosition();
  return (
    <Card>
      <CardHeader>
        <CardTitle>Reminders</CardTitle>
        <CardDescription>Things to do now (plug in the scooter, answer a question) show as small cards in a corner of the window.</CardDescription>
      </CardHeader>
      <CardContent>
        <div className="max-w-xs space-y-1.5">
          <Label htmlFor="toast-position">Position</Label>
          <Select id="toast-position" value={position} onChange={(event) => setPosition(event.target.value as ToastPosition)}>
            {POSITIONS.map((option) => (
              <option key={option.value} value={option.value}>
                {option.label}
              </option>
            ))}
          </Select>
        </div>
      </CardContent>
    </Card>
  );
}
