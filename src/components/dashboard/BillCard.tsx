import { TriangleAlert } from "lucide-react";
import { useState, type FormEvent } from "react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { useEnergyUnits } from "@/hooks/useEnergyUnits";
import { fmtNumber } from "@/lib/format";
import type { BillEstimate, BillRates, MeterBill, PeriodSavings } from "@/lib/types";
import { cn } from "@/lib/utils";

const FIELD =
  "h-7 rounded-md border border-border bg-background px-1.5 text-xs pointer-coarse:h-10 pointer-coarse:text-base tabular-nums text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring";
const TEXT_BUTTON =
  "rounded px-1 font-medium text-foreground underline underline-offset-2 hover:text-foreground/80 disabled:opacity-50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring";

function rupees(value: number): string {
  return `Rs ${Math.round(value).toLocaleString("en-US")}`;
}

function dayMonth(at: string): string {
  const [year, month, day] = at.slice(0, 10).split("-").map(Number);
  return new Date(year, month - 1, day).toLocaleDateString("en-GB", { day: "numeric", month: "short" });
}

function monthOf(at: string): string {
  const [year, month] = at.slice(0, 10).split("-").map(Number);
  return new Date(year, month - 1, 1).toLocaleDateString("en-GB", { month: "short", year: "numeric" });
}

export function BillCard() {
  const { summary, saveBillUnits, saveRates } = useEnergyUnits();
  if (!summary || !summary.configured) return null;
  const estimate = summary.expected_bill;

  return (
    <section className="overflow-hidden rounded-xl border border-border bg-card" aria-labelledby="bill-title">
      <div className="border-b px-4 sm:px-5 py-3">
        <h2 id="bill-title" className="text-xs font-semibold uppercase tracking-wide text-muted-foreground">
          Bill &amp; savings
        </h2>
        <p className="text-micro text-muted-foreground">IESCO A-1 protected residential · both connections, 2 kW each</p>
      </div>
      <div className="grid grid-cols-1 lg:grid-cols-5 lg:divide-x">
        <div className="px-4 sm:px-5 py-4 lg:col-span-2">
          {estimate ? <ExpectedBill estimate={estimate} /> : <p className="text-sm text-muted-foreground">The expected bill shows up once this bill has a few hours of data.</p>}
        </div>
        <div className="border-t px-4 sm:px-5 py-4 lg:col-span-3 lg:border-t-0">
          <SavingsTable
            rows={summary.savings}
            pending={saveBillUnits.isPending}
            error={saveBillUnits.error ? String(saveBillUnits.error) : null}
            onSave={(start, meter1, meter2) => saveBillUnits.mutateAsync({ start, meter1, meter2 })}
          />
        </div>
      </div>
      <RatesRow
        rates={summary.bill_rates}
        pending={saveRates.isPending}
        error={saveRates.error ? String(saveRates.error) : null}
        onSave={(fpa, qtr, adjustment) => saveRates.mutate({ fpa, qtr, adjustment })}
      />
    </section>
  );
}

function ExpectedBill({ estimate }: { estimate: BillEstimate }) {
  const meters = [
    { meter: 1, bill: estimate.meter1 },
    { meter: 2, bill: estimate.meter2 },
  ];
  return (
    <div>
      <p className="flex flex-wrap items-baseline justify-between gap-x-2 text-micro font-medium uppercase tracking-wide text-muted-foreground">
        <span>Expected this bill</span>
        <span className="normal-case tracking-normal tabular-nums">
          {dayMonth(estimate.start)} → {dayMonth(estimate.end)}
        </span>
      </p>
      <p className="mt-2 flex items-baseline gap-2 tabular-nums">
        <span className="text-2xl font-semibold">~{rupees(estimate.total)}</span>
        <span className="text-xs text-muted-foreground">{estimate.basis === "plan" ? "if you follow the switch plan" : "at this pace"}</span>
      </p>
      <table className="mt-3 w-full text-xs tabular-nums">
        <thead className="text-muted-foreground">
          <tr>
            <th scope="col" className="pb-1 text-left font-normal">
              <span className="sr-only">Charge</span>
            </th>
            {meters.map(({ meter }) => (
              <th key={meter} scope="col" className="pb-1 text-right font-normal">
                Meter {meter}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          <BillRow label="Units" values={meters.map(({ bill }) => `~${fmtNumber(bill.units, 0)}`)} />
          <BillRow label="Energy" values={meters.map(({ bill }) => rupees(bill.energy))} />
          <BillRow label="Fixed" values={meters.map(({ bill }) => rupees(bill.fixed))} />
          <BillRow label="Surcharge & adjustment" values={meters.map(({ bill }) => rupees(bill.surcharges))} />
          <BillRow label="Duty & GST" values={meters.map(({ bill }) => rupees(bill.taxes))} />
          <BillRow label="Fuel adjustment" values={meters.map(({ bill }) => `${rupees(bill.fpa)} (${fmtNumber(bill.fpa_units, 0)} u)`)} />
          <tr className="border-t">
            <th scope="row" className="pt-1 text-left font-normal text-muted-foreground">
              Total
            </th>
            {meters.map(({ meter, bill }) => (
              <td key={meter} className="pt-1 text-right font-semibold">
                {rupees(bill.total)}
              </td>
            ))}
          </tr>
        </tbody>
      </table>
      <ul className="mt-2 space-y-1 text-xs text-muted-foreground">
        {meters.map(({ meter, bill }) => (
          <SlabWarning key={meter} meter={meter} bill={bill} />
        ))}
      </ul>
      <p className="mt-2 text-micro leading-snug text-muted-foreground">
        The fuel adjustment is charged on the units of the bill two months back, at the rate below.
      </p>
    </div>
  );
}

function BillRow({ label, values }: { label: string; values: string[] }) {
  return (
    <tr>
      <th scope="row" className="py-0.5 text-left font-normal text-muted-foreground">
        {label}
      </th>
      {values.map((value, index) => (
        <td key={index} className="py-0.5 text-right">
          {value}
        </td>
      ))}
    </tr>
  );
}

function SlabWarning({ meter, bill }: { meter: number; bill: MeterBill }) {
  if (!bill.protected) {
    return (
      <li className="flex items-start gap-1.5">
        <TriangleAlert className="mt-0.5 size-3.5 shrink-0 text-destructive" aria-hidden />
        <span>
          Meter {meter} is over 200 units: it loses protected rates for six months.
        </span>
      </li>
    );
  }
  if (bill.units > 100) {
    return (
      <li className="flex items-start gap-1.5">
        <TriangleAlert className="mt-0.5 size-3.5 shrink-0 text-amber-600 dark:text-amber-400" aria-hidden />
        <span>
          Meter {meter} goes over 100 units: fixed charges rise from Rs 400 to Rs 600 and units above 100 cost 13.01.
        </span>
      </li>
    );
  }
  return null;
}

function SavingsTable({
  rows,
  pending,
  error,
  onSave,
}: {
  rows: PeriodSavings[];
  pending: boolean;
  error: string | null;
  onSave: (start: string, meter1: number, meter2: number) => Promise<unknown>;
}) {
  const [editing, setEditing] = useState<string | null>(null);
  const finished = rows.filter((row) => !row.partial);
  const total = finished.reduce((sum, row) => sum + row.saved, 0);
  const newestFirst = [...rows].reverse();

  return (
    <div>
      <p className="flex flex-wrap items-baseline justify-between gap-x-2 text-micro font-medium uppercase tracking-wide text-muted-foreground">
        <span>Saved by solar &amp; battery</span>
        {finished.length > 0 ? (
          <span className="normal-case tracking-normal tabular-nums">
            {rupees(total)} over {finished.length} {finished.length === 1 ? "bill" : "bills"}
          </span>
        ) : null}
      </p>
      {rows.length === 0 ? (
        <p className="mt-2 text-sm text-muted-foreground">Savings show up once a bill period has data.</p>
      ) : (
        <table className="mt-2 w-full text-xs tabular-nums">
          <thead className="text-muted-foreground">
            <tr>
              <th scope="col" className="pb-1 text-left font-normal">
                Bill
              </th>
              <th scope="col" className="pb-1 text-right font-normal">
                Units
              </th>
              <th scope="col" className="pb-1 text-right font-normal">
                Without solar
              </th>
              <th scope="col" className="pb-1 text-right font-normal">
                Bill
              </th>
              <th scope="col" className="pb-1 text-right font-normal">
                Without solar
              </th>
              <th scope="col" className="pb-1 text-right font-normal">
                Saved
              </th>
            </tr>
          </thead>
          <tbody>
            {newestFirst.map((row) =>
              editing === row.start ? (
                <tr key={row.start}>
                  <td colSpan={6} className="py-1">
                    <BilledUnitsForm
                      label={monthOf(row.end)}
                      pending={pending}
                      onCancel={() => setEditing(null)}
                      onSave={async (meter1, meter2) => {
                        await onSave(row.start, meter1, meter2);
                        setEditing(null);
                      }}
                    />
                  </td>
                </tr>
              ) : (
                <tr key={row.start} className="align-baseline">
                  <th scope="row" className="py-1 text-left font-normal">
                    <span className="text-foreground">{monthOf(row.end)}</span>
                    <span className="ml-1.5 inline-flex items-center gap-1">
                      {row.partial ? (
                        <Badge variant="secondary">so far</Badge>
                      ) : row.billed ? null : (
                        <button type="button" onClick={() => setEditing(row.start)} className={cn(TEXT_BUTTON, "text-micro")}>
                          enter bill
                        </button>
                      )}
                      {row.coverage < 0.95 ? (
                        <span className="text-micro text-muted-foreground" title="Share of the period the inverter's log covers">
                          {fmtNumber(row.coverage * 100, 0)}% logged
                        </span>
                      ) : null}
                    </span>
                  </th>
                  <td className="py-1 text-right">{fmtNumber(row.units, 0)}</td>
                  <td className="py-1 text-right text-muted-foreground">{fmtNumber(row.units_without, 0)}</td>
                  <td className="py-1 text-right">{rupees(row.bill)}</td>
                  <td className="py-1 text-right text-muted-foreground">{rupees(row.bill_without)}</td>
                  <td className="py-1 text-right font-semibold">{rupees(row.saved)}</td>
                </tr>
              ),
            )}
          </tbody>
        </table>
      )}
      {error ? (
        <p role="alert" className="mt-1 text-xs text-destructive">
          {error}
        </p>
      ) : null}
      <p className="mt-2 text-micro leading-snug text-muted-foreground">
        Without solar the house would take its whole load from the grid, split between the meters as it was, so a meter can cross 100 or
        200 units. Bills you haven't entered use the app's count. Today's rates are used for every month.
      </p>
    </div>
  );
}

function BilledUnitsForm({
  label,
  pending,
  onSave,
  onCancel,
}: {
  label: string;
  pending: boolean;
  onSave: (meter1: number, meter2: number) => Promise<unknown>;
  onCancel: () => void;
}) {
  const [draft, setDraft] = useState({ meter1: "", meter2: "" });
  const values = [draft.meter1, draft.meter2].map((text) => (text.trim() === "" ? NaN : Number(text)));
  const ready = values.every((value) => Number.isFinite(value) && value >= 0);
  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (!ready) return;
    try {
      await onSave(values[0], values[1]);
    } catch {
      return;
    }
  };
  return (
    <form onSubmit={submit} className="flex flex-wrap items-center gap-2">
      <span className="text-muted-foreground">{label} bill units</span>
      {(["meter1", "meter2"] as const).map((key, index) => (
        <label key={key} className="inline-flex items-center gap-1.5">
          Meter {index + 1}
          <input
            type="number"
            inputMode="decimal"
            min={0}
            step="any"
            className={cn(FIELD, "w-16 pointer-coarse:w-20")}
            value={draft[key]}
            onChange={(event) => setDraft({ ...draft, [key]: event.target.value })}
          />
        </label>
      ))}
      <Button type="submit" size="sm" disabled={pending || !ready}>
        Save
      </Button>
      <Button type="button" variant="ghost" size="sm" onClick={onCancel}>
        Cancel
      </Button>
    </form>
  );
}

function RatesRow({
  rates,
  pending,
  error,
  onSave,
}: {
  rates: BillRates;
  pending: boolean;
  error: string | null;
  onSave: (fpa: number, qtr: number, adjustment: number) => void;
}) {
  const [draft, setDraft] = useState({
    fpa: String(rates.fpa_per_unit),
    qtr: String(rates.qtr_per_unit),
    adjustment: String(rates.adjustment_per_unit),
  });
  const [shown, setShown] = useState(rates);
  if (rates !== shown) {
    setShown(rates);
    setDraft({ fpa: String(rates.fpa_per_unit), qtr: String(rates.qtr_per_unit), adjustment: String(rates.adjustment_per_unit) });
  }
  const values = [draft.fpa, draft.qtr, draft.adjustment].map((text) => (text.trim() === "" ? NaN : Number(text)));
  const ready = values.every(Number.isFinite);
  const changed =
    ready && (values[0] !== rates.fpa_per_unit || values[1] !== rates.qtr_per_unit || values[2] !== rates.adjustment_per_unit);
  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (changed) onSave(values[0], values[1], values[2]);
  };
  const fields = [
    { key: "fpa", label: "Fuel adjustment", hint: "FPA … @ rate" },
    { key: "qtr", label: "QTR surcharge", hint: "per unit" },
    { key: "adjustment", label: "Tariff adjustment", hint: "per unit, negative is a discount" },
  ] as const;
  return (
    <form onSubmit={submit} className="flex flex-wrap items-center gap-x-4 gap-y-2 border-t px-4 sm:px-5 py-3 text-xs text-muted-foreground">
      <span className="font-semibold text-foreground">Rates from your latest bill</span>
      {fields.map((field) => (
        <label key={field.key} className="inline-flex items-center gap-1.5" title={field.hint}>
          {field.label}
          <input
            type="number"
            inputMode="decimal"
            step="any"
            className={cn(FIELD, "w-20 pointer-coarse:w-24")}
            value={draft[field.key]}
            onChange={(event) => setDraft({ ...draft, [field.key]: event.target.value })}
          />
        </label>
      ))}
      <Button type="submit" size="sm" variant="outline" disabled={pending || !changed}>
        Save rates
      </Button>
      {error ? (
        <span role="alert" className="text-destructive">
          {error}
        </span>
      ) : null}
    </form>
  );
}
