use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

/// IESCO A-1(a) residential, S.R.O. 279(I)/2026, for the house's two
/// single-phase connections. Checked against the Jul–Sep 2026 bills.
pub const SANCTIONED_LOAD_KW: f64 = 2.0;
pub const PROTECTED_LIMIT_UNITS: f64 = 200.0;
/// A meter keeps protected rates only while every one of the last six months
/// stayed within the limit.
pub const PROTECTED_MONTHS: usize = 6;
const ED_RATE: f64 = 0.015;
const GST_RATE: f64 = 0.18;

/// (slab ceiling in units, Rs per unit, fixed Rs per kW per month)
const PROTECTED_SLABS: [(f64, f64, f64); 2] = [(100.0, 10.54, 200.0), (200.0, 13.01, 300.0)];
const UNPROTECTED_SLABS: [(f64, f64, f64); 8] = [
    (100.0, 22.44, 275.0),
    (200.0, 28.91, 300.0),
    (300.0, 33.10, 350.0),
    (400.0, 36.46, 400.0),
    (500.0, 38.95, 500.0),
    (600.0, 40.22, 675.0),
    (700.0, 41.85, 675.0),
    (f64::INFINITY, 47.20, 675.0),
];

/// The per-unit charges that change from bill to bill, copied off the latest one.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BillRates {
    /// Fuel price adjustment, charged on the units of the bill two months back.
    pub fpa_per_unit: f64,
    pub qtr_per_unit: f64,
    pub adjustment_per_unit: f64,
}

impl Default for BillRates {
    /// From the Sep 2026 bills.
    fn default() -> Self {
        Self { fpa_per_unit: 2.0581, qtr_per_unit: 0.43, adjustment_per_unit: -0.3695 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct MeterBill {
    pub units: f64,
    pub fpa_units: f64,
    pub protected: bool,
    pub energy: f64,
    pub fixed: f64,
    /// QTR surcharge plus the tariff adjustment.
    pub surcharges: f64,
    /// Electricity duty and GST on everything but the FPA.
    pub taxes: f64,
    /// FPA with its own duty and GST.
    pub fpa: f64,
    pub total: f64,
}

/// One connection's bill. Protected consumers get one previous slab's benefit:
/// the first 100 units stay at the first slab's rate. Unprotected consumers pay
/// every unit at the rate of the slab their total falls in.
pub fn meter_bill(units: f64, fpa_units: f64, protected: bool, rates: &BillRates) -> MeterBill {
    let units = units.max(0.0).round();
    let fpa_units = fpa_units.max(0.0).round();
    let protected = protected && units <= PROTECTED_LIMIT_UNITS;
    let (energy, fixed_per_kw) = if protected {
        let (first_ceiling, first_rate, first_fixed) = PROTECTED_SLABS[0];
        if units <= first_ceiling {
            (units * first_rate, first_fixed)
        } else {
            let (_, rate, fixed) = PROTECTED_SLABS[1];
            (first_ceiling * first_rate + (units - first_ceiling) * rate, fixed)
        }
    } else {
        let (_, rate, fixed) = UNPROTECTED_SLABS
            .iter()
            .copied()
            .find(|(ceiling, _, _)| units <= *ceiling)
            .unwrap_or(UNPROTECTED_SLABS[UNPROTECTED_SLABS.len() - 1]);
        (units * rate, fixed)
    };
    let fixed = fixed_per_kw * SANCTIONED_LOAD_KW;
    let adjustment = units * rates.adjustment_per_unit;
    let surcharges = units * rates.qtr_per_unit + adjustment;
    let duty = (energy + adjustment) * ED_RATE;
    let gst = (energy + fixed + surcharges + duty) * GST_RATE;
    let fpa_base = fpa_units * rates.fpa_per_unit;
    let fpa = fpa_base * (1.0 + ED_RATE + GST_RATE);
    let taxes = duty + gst;
    MeterBill {
        units,
        fpa_units,
        protected,
        energy,
        fixed,
        surcharges,
        taxes,
        fpa,
        total: energy + fixed + surcharges + taxes + fpa,
    }
}

/// One bill period's use, per meter as billed (or as counted when the bill
/// isn't in yet), plus what the house used in all.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PeriodUsage {
    pub start: NaiveDateTime,
    pub end: NaiveDateTime,
    pub partial: bool,
    pub billed: bool,
    pub meter1: f64,
    pub meter2: f64,
    pub house_kwh: f64,
    pub grid_kwh: f64,
    pub coverage: f64,
}

impl PeriodUsage {
    /// What the meters would have read with no solar or battery: the whole house
    /// load from the grid, split between the meters as it really was.
    fn without_solar(&self) -> (f64, f64) {
        let covered = (self.house_kwh - self.grid_kwh).max(0.0);
        let total = self.meter1 + self.meter2;
        let share = if total > 0.0 { self.meter1 / total } else { 0.5 };
        (self.meter1 + covered * share, self.meter2 + covered * (1.0 - share))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct PeriodSavings {
    pub start: NaiveDateTime,
    pub end: NaiveDateTime,
    pub partial: bool,
    pub billed: bool,
    /// Share of the period the inverter's log covers.
    pub coverage: f64,
    pub units: f64,
    pub units_without: f64,
    pub bill: f64,
    pub bill_without: f64,
    pub saved: f64,
}

fn protected_at(units: &[(f64, f64)], index: usize, meter: usize) -> bool {
    let from = index.saturating_sub(PROTECTED_MONTHS - 1);
    units[from..=index].iter().all(|pair| if meter == 1 { pair.0 } else { pair.1 } <= PROTECTED_LIMIT_UNITS)
}

fn bills_for(units: &[(f64, f64)], index: usize, rates: &BillRates) -> f64 {
    let fpa = index.checked_sub(2).map_or((0.0, 0.0), |back| units[back]);
    let (meter1, meter2) = units[index];
    meter_bill(meter1, fpa.0, protected_at(units, index, 1), rates).total
        + meter_bill(meter2, fpa.1, protected_at(units, index, 2), rates).total
}

/// Both meters' bills each period, as they were and as they would have been
/// without solar and the battery. `periods` runs oldest first; the FPA and the
/// protected check look back over the periods before.
pub fn savings(periods: &[PeriodUsage], rates: &BillRates) -> Vec<PeriodSavings> {
    let actual: Vec<(f64, f64)> = periods.iter().map(|period| (period.meter1, period.meter2)).collect();
    let without: Vec<(f64, f64)> = periods.iter().map(PeriodUsage::without_solar).collect();
    periods
        .iter()
        .enumerate()
        .map(|(index, period)| {
            let bill = bills_for(&actual, index, rates);
            let bill_without = bills_for(&without, index, rates);
            PeriodSavings {
                start: period.start,
                end: period.end,
                partial: period.partial,
                billed: period.billed,
                coverage: period.coverage,
                units: actual[index].0 + actual[index].1,
                units_without: without[index].0 + without[index].1,
                bill,
                bill_without,
                saved: bill_without - bill,
            }
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BillEstimate {
    pub start: NaiveDateTime,
    pub end: NaiveDateTime,
    /// "plan" when the units assume the switch plan is followed, else "pace".
    pub basis: &'static str,
    pub meter1: MeterBill,
    pub meter2: MeterBill,
    pub total: f64,
}

/// The running bill's expected amount from each meter's expected units, with
/// the FPA on the bill two periods back. `history` runs oldest first and ends
/// with the running period.
pub fn expected_bill(
    history: &[PeriodUsage],
    expected: (f64, f64),
    basis: &'static str,
    rates: &BillRates,
) -> Option<BillEstimate> {
    let current = history.last()?;
    let mut units: Vec<(f64, f64)> = history.iter().map(|period| (period.meter1, period.meter2)).collect();
    let index = units.len() - 1;
    units[index] = expected;
    let fpa = index.checked_sub(2).map_or((0.0, 0.0), |back| units[back]);
    let meter1 = meter_bill(expected.0, fpa.0, protected_at(&units, index, 1), rates);
    let meter2 = meter_bill(expected.1, fpa.1, protected_at(&units, index, 2), rates);
    Some(BillEstimate {
        start: current.start,
        end: current.end,
        basis,
        meter1,
        meter2,
        total: meter1.total + meter2.total,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn near(a: f64, b: f64, within: f64) -> bool {
        (a - b).abs() <= within
    }

    #[test]
    fn matches_the_sep_2026_bills() {
        let rates = BillRates::default();
        let meter1 = meter_bill(78.0, 104.0, true, &rates);
        assert!(near(meter1.energy, 822.12, 0.01));
        assert!(near(meter1.fixed, 400.0, 0.01));
        assert!(near(meter1.surcharges, 33.54 - 28.82, 0.01));
        assert!(near(meter1.taxes, 11.90 + 223.0, 0.5));
        assert!(near(meter1.total, 1719.0, 2.0), "bill says 1719, got {}", meter1.total);
        let meter2 = meter_bill(57.0, 86.0, true, &rates);
        assert!(near(meter2.total, 1407.0, 1.0), "bill says 1407, got {}", meter2.total);
    }

    #[test]
    fn matches_the_jul_2026_bill_above_100_units() {
        let rates = BillRates { fpa_per_unit: 0.3364, qtr_per_unit: 0.43, adjustment_per_unit: -206.51 / 104.0 };
        let bill = meter_bill(104.0, 63.0, true, &rates);
        assert!(near(bill.energy, 1106.04, 0.01), "first 100 units keep the 10.54 rate");
        assert!(near(bill.fixed, 600.0, 0.01));
        assert!(near(bill.total, 1863.0, 1.0), "bill says 1863, got {}", bill.total);
    }

    #[test]
    fn going_over_200_loses_protected_rates() {
        let rates = BillRates::default();
        let bill = meter_bill(201.0, 0.0, true, &rates);
        assert!(!bill.protected);
        assert!(near(bill.energy, 201.0 * 33.10, 0.01));
        assert!(near(bill.fixed, 700.0, 0.01));
    }

    fn period(month: u32, meter1: f64, meter2: f64, house: f64, grid: f64) -> PeriodUsage {
        let start = NaiveDate::from_ymd_opt(2026, month, 21).unwrap().and_hms_opt(11, 57, 0).unwrap();
        PeriodUsage {
            start,
            end: start,
            partial: false,
            billed: true,
            meter1,
            meter2,
            house_kwh: house,
            grid_kwh: grid,
            coverage: 1.0,
        }
    }

    #[test]
    fn savings_put_the_covered_units_back_on_the_meters() {
        let rates = BillRates::default();
        let periods = [period(7, 78.0, 57.0, 400.0, 135.0)];
        let result = savings(&periods, &rates);
        assert!(near(result[0].units_without, 400.0, 1e-9));
        assert!(near(result[0].bill, meter_bill(78.0, 0.0, true, &rates).total + meter_bill(57.0, 0.0, true, &rates).total, 1e-6));
        let without1 = 78.0 + 265.0 * 78.0 / 135.0;
        assert!(!meter_bill(without1, 0.0, true, &rates).protected, "meter 1 would have gone over 200");
        assert!(result[0].saved > 0.0);
    }

    #[test]
    fn a_month_over_200_keeps_the_meter_unprotected_for_six_months() {
        let units = [(250.0, 50.0), (50.0, 50.0), (50.0, 50.0), (50.0, 50.0), (50.0, 50.0), (50.0, 50.0), (50.0, 50.0)];
        assert!(!protected_at(&units, 5, 1));
        assert!(protected_at(&units, 6, 1));
        assert!(protected_at(&units, 5, 2));
    }
}
