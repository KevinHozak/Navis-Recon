# Household history, movements and cash scenarios

## Validated source and currency boundary

The native `household_analysis` module reads one ledger byte snapshot, validates it with the shared supported parser, and returns its SHA-256 revision. Monetary values remain scale-18 decimal strings. Requests select one currency/commodity explicitly; no foreign exchange, price valuation or investment liquidation is inferred. Unsupported or unreadable ledgers withhold the result. An invalid subscription registry withholds forecast output rather than silently dropping confirmed bills.

The IPC commands are `get_household_analysis({ request })`, `get_cash_forecast({ request })`, and `save_forecast_settings({ settings, expectedRevision })`; they have no HTTP fallback. All work is local and this feature never changes ledger amounts or runs an import.

## S014: chronological history and ranges

The original history command still returns the latest 24 ledger months by default. Stable date sorting preserves source order for same-date transactions; each carried month gets its own unique YYYY-MM label and `carried_forward` coverage marker. Other-unit-only transactions cannot manufacture a selected-unit observation.

The Wealth view adds start/end dates, unit selection and optional explicit comparison dates. Applying the range updates both its charts and the explanation table using the same native snapshot. Monthly rows are clipped at the exact selected start/end dates; chart points are cumulative balances at each period end, including a partial final month. Charts omit dates before the first selected-unit entry. The table shows those months as `no_currency_evidence` with unavailable balances, and later empty months as carried balances with no new entries. Carry-forward does not certify statement coverage or imply fresh observations. The default latest-24 charts remain available through their reset button. Date ranges are bounded to 100 years; forecast horizons to 1–366 days.

## S015: movement explanation and transaction drill-down

For each month or clipped period, net-worth movement is decomposed as:

`income - net expenses + equity/adjustment postings + unexplained remainder`

Net worth sums signed Assets and Liabilities postings in the selected unit. Income reverses the normal credit sign; expenses retain debit signs. Refunds and corrections therefore retain their signed category effects rather than being counted as extra spending. Internal asset transfers and credit-card repayments have no Income/Expenses posting, so they are excluded from spending without hiding their source transactions. Purchases and interest posted to Expenses remain spending regardless of whether paid from cash or a card.

Equity postings include initial capital, opening balances and explicit equity adjustments. Their description is not reclassified as a market valuation; prices/costs remain outside the supported subset. The exact unexplained remainder is returned explicitly even when zero. The first observed month can have a known closing balance/posting movement but an unavailable prior opening baseline.

Each category comparison and monthly aggregate carries source IDs bound to ledger revision and stable chronological transaction index. Expand a row to inspect dates, descriptions, pending flags and exact account postings in each comparison period. Same-date order is deterministic for one source revision. Comparison periods are user selected rather than silently shifted. Category amounts and differences are unavailable when a chosen period predates the first selected-unit evidence; absence of evidence is not reported as zero spending. Stale or out-of-order async replies cannot replace a newer range/unit selection; failed reads clear displayed results.

## S016: liquid-cash scenarios and upcoming bills

The Subscriptions view reuses `analyze_recurring_obligations_with_registry`; it does not add a second detector. Select bank/cash accounts explicitly. The native allowlist is exact `Assets:Bank`, `Assets:Cash`, `Assets:Checking`, `Assets:Savings` and their descendants, in the chosen unit. Retirement, investment and property assets cannot become spendable cash through a default selection. Opening cash is end-of-day as-of cash, including flagged pending ledger postings; pending movement is disclosed. Availability remains unavailable unless every selected account has ledger evidence by the as-of date.

Dated future ledger movements within the horizon are counted once in both scenarios, whether pending or cleared. The **confirmed** scenario adds user-confirmed bills; the **estimate** scenario optionally adds detected recurring charges. No future salary, other unrecorded income, investment sale or credit capacity is invented. The display highlights negative end-of-day cash. Events on the same date share one daily closing balance; unknown intraday order is not used to invent a temporary shortfall.

Detected expenses require explicit payee identity and either unambiguous observed cash funding or an explicit cash-account mapping. Card-funded or ambiguous split bills are excluded with an actionable mapping warning rather than pretending a purchase is another cash payment; recorded card repayments still affect cash once. The estimate uses the latest observed positive charge. Dates advance using the existing cadence; confidence labels report pattern support, observation count and staleness, not calibrated probabilities. Price deltas and trial endings remain visible, but a free trial's later paid rate is not inferred.

To prevent payment duplication, a projected occurrence is suppressed when a dated future ledger payment from selected cash has the same payee, expense account and unit within three days. Matching is conservative and one payment/category can suppress one occurrence. Splits across different expense categories may suppress each category once. A due payment already observed by as-of is in opening cash, not another one-off bill. Users should verify fuzzy-date matches against statement evidence; arbitrary payee aliases and narration-only identities are not guessed.

Confirmed bills can be entered even with insufficient or irregular detector history. Enter a positive amount, explicit due date, expense account and bank/cash funding account; cadence is optional for a manual one-off, or inherits an existing detected pattern. Cancelled settings remove future projections without erasing recorded ledger movements. A confirmed unresolved overdue bill is reserved on the next day with its overdue status/assumption disclosed; detector-only elapsed cycles are not invented as accumulated debt. Additional historical arrears and missing imports are not inferred. Overdue recurring manual schedules keep their original cadence anchor.

Ledger observations older than 30 days, old obligation observations, missing funding, irregular/insufficient patterns and elapsed projected cycles produce explicit warnings. Future scenario rows expose last observation, stale/overdue status, basis, confidence, trial end and price changes. These are scenarios based on recorded evidence and user assertions, not guarantees of bank availability or proof a historical bill was paid.

## Settings and persistence

Forecast-specific fields live alongside existing subscription metadata under `forecast_*` keys. Settings saves take a raw-byte revision, a cooperative writer lock, synced unique staging, atomic write-through publication and disk readback through the shared document store. Unrelated notes, labels and other metadata are preserved. The general subscription editor retains forecast fields from the latest stored entry, so its stale copy cannot undo a cash mapping or cancellation. Competing settings edits reject with a refresh conflict; no hidden retry is performed. Existing whole-registry replacement remains a separate explicit operation.

## Verification boundary

Synthetic fixtures exercise out-of-order/same-date entries, unique carried labels, missing evidence, clipped dates, year boundaries, trailing-24 behavior, refunds, corrections, transfers, card repayment, mixed units, exact category-to-source sums and net-worth reconciliation. Forecast fixtures cover recorded future payments, already paid one-offs, confirmed overdue bills, manual commitments, cancelled subscriptions, irregular/insufficient history, stale observations, price increases, trial endings, cash shortfalls and investment exclusion. Metadata tests preserve unrelated fields and reject stale revisions; frontend tests reject superseded/unmounted responses and verify exact native IPC requests. No household ledger, statement PDF or credentials are test inputs.
