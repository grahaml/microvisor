# The Smelter

## The Story

Every great alloy begins as a formula — a precise declaration of what raw materials to combine, in what order, and under what conditions. In a traditional foundry, the metallurgist writes the alloy specification; the smelter does the rest.

**The Smelter** is the Microvisor build system. It takes raw declarations and transforms them, through heat and pressure, into something dense, uniform, and ready for service.

### The Lexicon

| Term | What It Is | Metallurgical Origin |
|---|---|---|
| **Alloy** | A build declaration (Dockerfile). The recipe that specifies which base metals to combine, what impurities to remove, and what properties the final product must have. | An alloy is a metal made by combining two or more metallic elements to produce a material with specific, desired properties — stronger, lighter, more resistant. |
| **Charged** | The act of submitting an Alloy for building. To *charge* The Smelter is to load it with raw materials and fire the furnace. | *Charging* is the foundry term for loading raw ore, scrap, and flux into a smelter or blast furnace. It is the first and most deliberate act — once charged, the process runs to completion. |
| **Ingot** | A built artifact — the content-addressed, layered output of a successful charge. Dense, stamped with its composition, ready for deployment. | An ingot is a block of metal cast into a standard shape for transport and further processing. Each ingot is identical if the inputs are identical — the defining property of content-addressed builds. |
| **The Reserve** | The content-addressed artifact store (`fs-storage`). Where Ingots are cataloged, deduplicated, and held until Microvisor calls for them. | A *reserve* (as in a bullion reserve) is a secured vault where refined metals are stored, inventoried, and audited. Nothing leaves without a manifest. |

### The Process

```
  ┌─────────────┐
  │   ALLOY     │   An operator writes an Alloy — a Dockerfile annotated
  │  (recipe)   │   with Steel labels declaring layer boundaries and the
  └──────┬──────┘   release manifest.
         │
         │ charge
         ▼
  ┌─────────────┐
  │ THE SMELTER │   The Smelter parses the Alloy, resolves the layer
  │  (furnace)  │   dependency graph, and fires BuildKit to execute each
  └──────┬──────┘   stage. Content hashes are computed. Unchanged layers
         │          are pulled from The Reserve — never re-smelted.
         │
         │ pour
         ▼
  ┌─────────────┐
  │   INGOT     │   The output: per-layer ext4 deltas, compressed and
  │ (artifact)  │   content-addressed, plus a release manifest stamped
  └──────┬──────┘   with the full composition of the build.
         │
         │ deposit
         ▼
  ┌─────────────┐
  │ THE RESERVE │   Ingots are deposited into The Reserve, where they
  │  (vault)    │   are deduplicated, cataloged, and held. Microvisor
  └─────────────┘   draws from The Reserve at deploy time, pulling only
                    the layers it doesn't already have.
```

### Why This Metaphor

The metallurgical metaphor isn't cosmetic. It encodes the system's core properties:

**Determinism.** The same Alloy, charged with the same inputs, produces the same Ingot. This is the content-addressing guarantee — identical inputs yield identical hashes. A smelter that produces inconsistent output from identical ore is broken.

**Composition.** Alloys are defined by their layers — base metal, hardening agents, surface treatments — just as our builds are defined by their layer stack: base OS, platform binaries, dependencies, application code. Each layer has independent identity and can be replaced without re-forging the others.

**Efficiency.** A foundry never re-smelts a bar it already has in the vault. If the base layer hasn't changed, its Ingot is in The Reserve. Only the layers whose inputs have changed are re-charged.

**Finality.** Once an Ingot is poured, it is immutable. It gets a stamp (content hash) that identifies it forever. You don't modify an Ingot — you write a new Alloy and charge a new one.

### Extended Vocabulary

These terms may appear in logs, CLI output, and internal documentation:

| Term | Meaning |
|---|---|
| **Flux** | Build context — the files, secrets, and environment passed alongside the Alloy during a charge. In metallurgy, flux is added to the charge to remove impurities and facilitate the smelt. |
| **Slag** | Intermediate build artifacts discarded after a successful charge — OCI tarballs, temp directories, build caches. Valuable during the process, waste after. |
| **Tap** | To extract the finished Ingot from The Smelter. In a foundry, tapping is the act of opening the furnace to pour molten metal into molds. In the build system, tapping is the final stage: compressing layer deltas, computing output hashes, and writing the release manifest. |
| **Assay** | Verification of an Ingot's integrity — hash validation, manifest completeness checks, layer ordering verification. An assay in metallurgy tests the composition and purity of a metal sample. |
| **Heat** | A single build run. "Heat 47 produced Ingot `a3f8c2…` from Alloy `steel-browser.Dockerfile`." In a foundry, a heat is one complete cycle of the furnace. |
| **Mold** | The output format specification — how the Ingot is shaped for consumption. Currently: zstd-compressed ext4 deltas. The mold could change without changing the smelt. |
| **Stamp** | The content-address hash embossed on an Ingot. Every Ingot in The Reserve bears a Stamp that uniquely identifies its contents. |
