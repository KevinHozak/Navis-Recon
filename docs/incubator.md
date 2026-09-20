# Incubator: Household Records and Readiness Hub

This document captures a possible future direction for Navis-Recon: extending the application beyond ledger review into a private, household-level hub for important records, renewal dates, maintenance history, and financial obligations.

This is an exploration document, not a commitment to build every idea listed here. The current product remains a focused financial dashboard and ledger triage tool. Any expansion should preserve that focus, avoid turning the application into an uncontrolled document dump, and be introduced behind clear privacy and data-integrity boundaries.

## Product idea

Navis-Recon could become a calm, searchable overview of the records a household needs to maintain over time. The application would track the existence, location, status, expiration date, and follow-up actions for important records while linking relevant costs and obligations back to the financial ledger.

The goal would not be to replace specialist systems, insurers, lenders, medical providers, mechanics, or secure password managers. Instead, Navis-Recon could provide a local-first index and reminder layer so that important information is easier to find and keep current.

## Possible record areas

### Identity and important documentation

- Passports and passport renewal dates
- Driver's licenses and state identification
- Birth certificates, Social Security records, and other vital documents
- Vehicle titles and registrations
- Home deeds, warranties, permits, and inspection records
- Document owner, issuing authority, issue date, expiration date, and secure storage location

The app should generally track document metadata and a pointer to the protected file rather than duplicating sensitive documents unnecessarily.

### Vehicles and car maintenance

- Vehicle profile, VIN, license plate, mileage, and purchase date
- Oil changes, tire rotations, inspections, repairs, recalls, and part replacements
- Service provider, date, mileage, cost, warranty, and attached receipt
- Upcoming service intervals based on time or mileage
- Registration and insurance renewal reminders

Vehicle expenses could optionally be associated with ledger transactions, but the maintenance record should remain useful even when no matching transaction is available.

### Home maintenance

- Home systems and assets such as HVAC, plumbing, appliances, roof, and electrical equipment
- Installation dates, model and serial numbers, warranty periods, and expected service intervals
- Repairs, inspections, contractors, costs, permits, and before/after notes
- Seasonal maintenance checklists
- Utility or property-related documents that need periodic review

### Mortgage and property obligations

- Mortgage account overview, servicer, rate type, term, and important dates
- Escrow, property tax, homeowners association, and insurance obligations
- Renewal, refinance, payoff, and annual statement reminders
- Links to statements and contracts stored in the protected document system

The app should display a useful summary without attempting to become a loan-servicing system or calculate authoritative payoff figures from incomplete data.

### Insurance

- Life insurance policies and beneficiary-review reminders
- Medical, dental, vision, auto, homeowners, renters, and umbrella coverage
- Policy number, carrier, coverage period, renewal date, deductible, and contact details
- Claim records and links to supporting documentation
- A clear distinction between policy metadata and confidential policy documents

Insurance data is especially sensitive. The application should minimize stored content and make access controls, backups, and export behavior explicit before this area is implemented.

### Medical and care information

- Insurance-card metadata and coverage dates
- Provider contacts and recurring administrative reminders
- Immunization, prescription, or care-document references where appropriate
- Health-document locations and renewal or review dates

This area should begin with administrative metadata only. Detailed medical records would require a separate security and privacy review, strong encryption, careful access controls, and a much narrower product definition.

## Proposed organizing model

The central object could be a `Record` or `HouseholdItem` with:

- A category and human-readable name
- Person, household, vehicle, property, or asset association
- Status such as active, expired, archived, or needs review
- Important dates and recurrence rules
- A protected-file reference or external location
- Optional notes and contacts
- Optional links to one or more ledger transactions or accounts
- Audit metadata showing when the item was created or last reviewed

Events such as a service visit, renewal, payment, repair, claim, or document replacement could be stored separately from the item itself. This would keep the current state easy to view while preserving a useful history.

## Design principles

1. **Local-first and privacy-conscious.** Sensitive metadata and files should remain under the user's control. Do not introduce cloud synchronization by default.
2. **Index before storage.** Start by tracking what exists, where it is, and when it needs attention. Store copies of sensitive documents only when there is a clear, protected workflow.
3. **Ledger remains authoritative for money.** Costs shown in maintenance, insurance, or mortgage views may link to the ledger, but Navis-Recon must not create a competing financial source of truth.
4. **Reminders must be explainable.** Every reminder should show the date, source, recurrence rule, and reason it was generated.
5. **Safe handling of uncertainty.** Incomplete, estimated, or user-entered information should be labeled rather than presented as verified fact.
6. **Portable data.** Records should be exportable in a readable format, with attachments and sensitive fields handled deliberately.
7. **Small, composable modules.** Identity documents, vehicles, homes, insurance, and medical administration should be separable modules so the core dashboard does not become overwhelming.

## Possible delivery path

### Phase 1: Record index

Add a lightweight records view for names, categories, owners, statuses, expiration dates, storage locations, and review notes. Include filtering for items that are expired or due soon. No document upload is required for this phase.

### Phase 2: Recurring obligations

Add reminder rules, recurring maintenance intervals, renewal tasks, and a review history. Make generated reminders visible and dismissible without silently deleting the underlying record.

### Phase 3: Vehicle and home modules

Add structured maintenance history, mileage or time-based service schedules, warranties, receipts, and optional links to ledger transactions.

### Phase 4: Insurance and property obligations

Add policy, mortgage, property tax, escrow, and coverage metadata with conservative summaries and explicit source references.

### Phase 5: Protected document workflow

Only after the security model is established, consider encrypted document references, controlled imports, backup guidance, access logging, and secure export. Passwords, recovery codes, and other secrets must remain in a dedicated password manager rather than this application.

## Open questions and safeguards

- Should the application store files, or only references to files in the user's existing vault?
- How should records be encrypted at rest and protected from accidental export?
- What is the backup and recovery story for the index and its attachments?
- Which household members can view or edit medical, insurance, and identity information?
- How should reminders behave when a date is approximate or a source document has not been verified?
- How can the UI keep everyday ledger review fast when additional modules exist?
- Which data should be retained in Navis-Recon versus a document system, password manager, calendar, or specialist provider?

## Non-goals

- Replacing Beancount or creating a second financial ledger
- Acting as a password manager
- Providing medical advice or a complete electronic health-record system
- Becoming an authoritative mortgage, insurance, tax, or vehicle-maintenance provider
- Uploading sensitive household records to a third-party service without explicit user choice

The strongest version of this direction would make Navis-Recon a trusted household readiness layer: a clear place to see what matters, what is due, where the authoritative record lives, and what action is needed next.
