# Project instructions

## Design workflow: Penpot MCP

- Use **Penpot MCP** for visual design and design-system work in this project. Inspect the current Penpot file, its pages, tokens, and components before changing the design.
- Preserve the existing white/lilac character unless the user requests a new direction. Use the shared tokens and reusable components rather than introducing isolated styling.
- Keep the Penpot design system and the Rust implementation (`src/theme.rs`, `src/app.rs`, `src/charts.rs`) synchronized. Document the actual changes and the file/page link in `docs/design-system.md`.
- Do not claim that Penpot was inspected, updated, or synchronized when the MCP connection is unavailable. Record the blocker and the remaining synchronization work explicitly.
- Validate interaction behavior in egui tests and inspect rendered layout when possible; a Penpot mockup alone does not verify the running application.

The user's Context7 documentation, delegation, and resource-hygiene instructions remain applicable.
