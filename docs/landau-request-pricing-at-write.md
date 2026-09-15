# Request to Landau: a seller-stated cost is priced by the rule the moment it is stated

Raised by Landau Bridge (the retail seller's desktop app that moves Barezpakhsh invoices onto the
Landau shelf and refreshes costs). Please take this through your usual path — brainstorming, a plan
under `examples/landau/plans/` (next free number), a decision row in `REQUIREMENTS.md` §11 (next
free number; 143 is the last today) — the way plans 89 and 92 were landed. A proposed decision row
and a contract are below so nothing has to be reverse-engineered from the bridge.

## 1. What is measured today (all through the client gateway, strict `apt-…` key)

- `POST /career/stocks {…, cost}` and `PATCH /career/stocks/:id {cost}` stamp the cost
  (`StocksService.applySellerCost`, `cost_source: 'SELLER'`, plan 92 / decision 135) and **stop
  there**: neither `assertAttachable` nor `assertEditable` in
  `apps/services/src/modules/career/submodules/stocks/stocks.service.ts` calls
  `PricingService.patchFor`. The row goes live at the price it had (on create: whatever `price` the
  caller sent, which the bridge sets to `cost`, mirroring the wholesale delivery).
- The restock receipt already does the right thing: `RestockOrdersService.priced()` in
  `apps/services/src/customs/wholesale/submodules/restock-orders/restock-orders.service.ts`
  stamps `cost_source: 'RECEIPT'` and then `this.pricing.patchFor(stamped, rules)` — "the margin is
  decided against the cost this delivery has just stamped". That is decision 132's principle,
  *applied the moment the goods become theirs*. A seller-stated cost has no such moment.
- The only way a rule price reaches a seller-costed row is `POST /career/pricing-rules/reprice`
  (`PricingRulesService.reprice`, `apps/services/src/customs/career/submodules/pricing-rules/`),
  which (a) pages `this.db.career.stocks` — the CDC mirror, ~1 s behind, sometimes 15 s — so rows
  written a moment ago are not in its page yet, and (b) walks the whole shelf per call
  (`REPRICE_PAGE` rows a page, `skip` to resume).
- `PATCH {price}` stamps `source: 'MANUAL'` (`applyManualPrice`) and later re-prices skip the row
  unless `overwrite` — correct, and it means a caller cannot set a provisional rule price itself.

Consequence for a retail seller: after the bridge writes a new cost, the row sells at its **old**
price (or at cost, when just created) until a reprice runs and the mirror has caught up — seconds
on a small shelf, minutes on a large refresh. An order placed in that window has no margin and the
seller has to cancel it. The bridge is narrowing the window on its side (provisional price on
create, a reprice every 50 rows); only Landau can close it.

## 2. The ask — contract

**A. Price at write.** In `assertAttachable` and `assertEditable`, when the request carries `cost`
and **no** `price`, do what `priced()` does on receipt: stamp the cost, load the caller's active
rules the way `reprice` does (business-fenced, plan 102), and apply
`this.pricing.patchFor(stamped, rules)` **without** `overwrite`, so:

- a row no human priced gets `price` from the matching rule (`PRODUCT > BAND > SHELF`, the same
  arithmetic and rounding as reprice) and `props.pricing = {source: 'RULE', rule, discount?, at}`;
- a `MANUAL` row keeps its price (the seller's sticky price stays sticky; `reprice {overwrite}` is
  still the only way past it);
- with no matching rule, or `patchFor` answering `undefined` (unchanged), the write is the cost
  stamp alone — today's behaviour, unchanged.

The reply is the row as today, with the rule price in `price`. If the reply can also carry
`priced` the way the shelf projection does (`shelf.serializer.ts`), the bridge will read
`priced.source`; otherwise it will compare `price` with `cost`.

**B. A targeted re-price that reads live rows.** Add `stocks?: string[]` (`@IsMongoId({each: true})`,
at most 100) to `RepriceDto` (gateway `reprice.dto.ts` and the `lnd-sdk` type). When present,
`reprice` prices exactly those rows, resolved as the **authoritative** rows (the same read the
commit path uses, LND-412) rather than the mirror page, fenced to rows the caller may touch as plan
102 fences the store-wide walk; `store`/`skip` are ignored for this form. Response shape unchanged:
`{rows, total, changed, applied, limit, skip}` with the usual per-row `next`, `rule`, `floored`,
`skipped: MANUAL | NO_COST | NO_RULE | UNCHANGED`.

**C. Unchanged.** `PATCH {price}` ⇒ `MANUAL`; `props` stripped from client bodies
(`assertPricingIsLandaus`); the seller-vs-receipt cost provenance (decision 135 c); the store-wide
`reprice` form and its preview default.

## 3. Proposed decision row (for REQUIREMENTS.md §11)

> **A seller-stated cost is priced by the rule the moment it is stated, as a delivered one is.**
> Raised by the retail seller's Barezpakhsh bridge, 2026-09-15. Decision 132 applies the margin
> "the moment the goods become theirs" and decision 135 let a seller state a cost themselves — but
> only the receipt path called the rule, so a seller-costed row went live at its old price (or at
> cost, when created) until a store-wide re-price found it in the lagging mirror; an order in that
> window carries no margin. (a) `POST/PATCH /career/stocks` carrying `cost` and no `price` apply
> the matching active rule on the write itself, exactly as the receipt does, never overriding a
> `MANUAL` price. (b) `reprice` accepts `stocks: [ids]` and prices those rows from the
> authoritative read, so a caller can price what it just wrote without waiting for CDC. (c)
> Nothing changes for a row a human priced, for `props` a client sends, or for the store-wide form.

## 4. Acceptance (through the gateway; fixture below)

1. Create `{cost: 1_000_000, price: 1_000_000, …}` under an active SHELF rule
   `MARKUP_PERCENT 15, rounding 1000, floor_at_cost` → reply `price = 1_150_000`; the stored
   `props.pricing` has `source: 'RULE'`, the rule id, `cost_source: 'SELLER'`.
2. `PATCH {cost: 2_000_000}` on that row → reply `price = 2_300_000`, `at` moved forward.
3. `PATCH {price: 999_000}` then `PATCH {cost: 3_000_000}` → price stays `999_000`
   (`MANUAL`); `reprice {stocks: [id], commit: true}` → `skipped: 'MANUAL'`; with
   `overwrite: true` → `next = 3_450_000`.
4. A seller with no active rule: create and patch behave exactly as before.
5. `reprice {stocks: [id]}` **immediately** after a write returns that row priced (no mirror wait);
   the store-wide form's paging, preview default and `total` are unchanged.
6. A `stocks` id belonging to another company is refused the way plan 102 refuses rows.

Fixture on the live stack: retail-supplier store `6aa6c145f65d2a0b1a8dd070` carries an active 15 %
SHELF rule (rounding 1 000, id `6aa72aaaaab3b347d5402a2a`); accounts in
`/home/vahid/Claude/.memory/.local/landau-roles.json`.

## 5. What the bridge will do once this ships

Drop its provisional price on create and its every-50-rows reprice, read the price from the write
reply, and call `reprice {stocks}` only for rows that came back unpriced.
