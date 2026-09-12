# Landau Bridge — Tauri + Nuxt desktop app for a Landau retail seller

## Goal

Build a desktop app (Tauri shell, NuxtJS UI) that does exactly two things for a retail seller on the Landau platform:

1. Import the seller's Barezpakhsh invoices onto their Landau shelf.
2. Update shelf prices whenever the seller requests it.

All code lives in /home/vahid/WorkSpace/landau-bridge/. The seller must be able to use the app without an AI agent: everything they need must be explicit in the code and UI.

## Context

- Landau: the seller's platform. Authenticate with the Landau API token the seller supplies.
- Barezpakhsh: a wholesaler outside Landau. The price shown on its site is the wholesale price to the retailer, not a shelf price.
- Other non-Landau suppliers may be added later. Structure the supplier layer so another can be added, but implement only Barezpakhsh now.
- Any browser automation runs headless.

## Feature 1 — Import invoices to the shelf

1. Retrieve every invoice the seller has ordered from Barezpakhsh to date.
2. Match each invoice item to a Landau part (see *Part matching*).
3. Price each item with Landau's own pricing rules (see *Pricing*).
4. Put each item on the shelf at the latest wholesale price of the day, not the purchase price on the invoice, even if the resulting quantity is zero.
5. Invoices may be old, and the seller may no longer hold those items or those quantities. Offer the seller a choice per import:
   - With inventory by date: transfer the invoice quantities. If a part is already on the shelf, add the invoice quantity to the existing quantity.
   - Zero quantity for review: add every item at quantity 0 and the latest price, so the seller can review and correct.

## Feature 2 — Update shelf prices on request

On the seller's request, refresh shelf prices from the current Barezpakhsh wholesale price using the same pricing rules as Feature 1.

## Pricing

Do not invent a markup scheme. Find how Landau already derives a shelf price from a cost price and apply those rules exactly, so items land on the seller's shelf priced the way Landau prices them.

## Part matching

- Use Landau's fuzzy search. Its accuracy threshold is adjustable: choose it by estimating and testing against real data, and record the threshold and the reasoning.
- Some items will not match with reasonable accuracy. For those:
  - Tell the user which items are unmatched.
  - Show candidate Landau parts so the user can pick one, or "none of these".
  - On "none", tell the user to open a ticket with Landau support to have the part added.
  - Persist the user's decision so the same item is never asked about again.

## Errors and logging

Handle edge cases and error states explicitly. Log and persist every one of them.

## Reference material

- Platform: /home/vahid/Claude/platform/
- Landau domain information: /home/vahid/Claude/examples/landau/ and /home/vahid/Claude/examples/lnd-*
- Landau's Claude Code project and its main rules: /home/vahid/Claude/ with /home/vahid/Claude/CLAUDE.md
- Prior Barezpakhsh scraper (invoices and prices, exported to a simple non-Landau system): /home/vahid/WorkSpace/barezpakhsh-scraper/
- How the current Landau part names were cleaned and imported: /home/vahid/WorkSpace/landau-parts/

Landau Bridge must work independently of all of the above, with no runtime dependency on any of them.

## Testing

Run all tests live against Landau and the running platform, using the same Barezpakhsh username and password found in the barezpakhsh-scraper code.

## Ground rules

- Never guess. If you cannot find something after searching the references above, stop and ask so the user can help.
