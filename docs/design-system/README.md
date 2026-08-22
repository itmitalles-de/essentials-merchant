# Simple Business design system

This product does not maintain a local copy of the visual rules. The exact
canonical source, commit, package, and version are pinned in
`/.simple-business-design-system.json`.

In the standard workspace, read the sibling checkout at
`../simple-business-design-system/docs/design-system/`. The canonical remote is
`itmitalles-de/simple-business-design-system`. Do not follow an unpinned branch.

The product-owned frontend installs the exact public `v0.1.1` release artifact
named in the manifest. Its lockfile records the artifact integrity, the central
token stylesheet loads before the product theme, and the existing frontend lint
job runs the shared icon-semantics architecture check. Existing UI remains
legacy; this activation does not claim that every historical visual rule
violation has already been migrated. Upstream Vendure UI remains outside the
owned-surface lint boundary.
