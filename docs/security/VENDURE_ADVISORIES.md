# Vendure advisory triage

Review date: 2026-08-19

Scope: installed production dependencies under `commerce/`

Commands: `npm ci`, `npm audit --omit=dev --json`, and targeted `npm ls --all --json`

## Result and boundary

The current audit reports 12 affected package nodes: 6 high and 6 moderate, with no critical finding. Those nodes resolve to 11 distinct GitHub Security Advisories. They are **not fixed**. Every affected path is rooted in Vendure 3.7.2 (`@vendure/core` or `@vendure/asset-server-plugin`). Version 3.7.2 is also the newest published compatible 3.x release as of the review date. npm's suggested automatic fix is an incompatible downgrade to Vendure 2.0.10 or asset-server-plugin 0.11.1, so no force-fix was applied.

The Amazon pilot Compose definition starts only Core PostgreSQL, the Rust backend, and the Core frontend. It contains no Vendure database, Vendure server, Vendure worker, or storefront service. This materially removes these reachable services from the pilot attack surface; it does not remediate the dependencies retained for the tested non-pilot Commerce system.

| Advisory | Severity | Installed package and transitive path | Reachable code path | Started in Amazon pilot | Compensating control | Compatible upstream | Decision | Review date |
|---|---|---|---|---|---|---|---|---|
| GHSA-9q82-xgwf-vj6h | Moderate | `@vendure/core@3.7.2` → `@apollo/server@4.13.0` | Vendure Shop/Admin GraphQL CSRF protection; reachable only when Vendure server is running | No | Vendure services absent; pilot binds only Core UI to loopback | Safe Apollo is `>=5.5.0`; no validated Vendure 3.x update accepts it | Open; track Vendure upgrade, no override | 2026-08-19 |
| GHSA-5v7r-6r5c-r473 | Moderate | `@vendure/core@3.7.2` and `@vendure/asset-server-plugin@3.7.2` → `file-type@19.6.0` | Malformed ASF parsing through Vendure asset/file inspection | No | No asset server or Commerce upload route in pilot | Safe `file-type >=21.3.1`; no validated compatible Vendure 3.x release | Open; do not force a transitive major | 2026-08-19 |
| GHSA-w3rx-r6r6-pgpr | High | `@vendure/core@3.7.2` → `image-size@2.0.2` | ICNS dimension parsing in Vendure asset processing | No | Asset processing service absent from pilot | npm reports no fixed release for the affected installed range | Open; track upstream and constrain uploads outside pilot | 2026-08-19 |
| GHSA-5p2g-fcmc-qvqq | High | `@vendure/core@3.7.2` → `image-size@2.0.2` | JXL/HEIF dimension parsing in Vendure asset processing | No | Asset processing service absent from pilot | npm reports no fixed release for the affected installed range | Open; track upstream and constrain uploads outside pilot | 2026-08-19 |
| GHSA-r5fr-rjxr-66jc | High | `@vendure/core@3.7.2` → `@nestjs/graphql@13.1.0` → `lodash@4.17.21` | Lodash template helper is loaded transitively; an application-controlled imports-key path is not proven | No | Nest/Vendure process absent; no external AI or template execution added | Advisory-safe lodash is above 4.17.23; current 4.18.x is not accepted by the locked Nest path without an override | Open; validate only through compatible upstream | 2026-08-19 |
| GHSA-f23m-r3pf-42rh | Moderate | same Nest GraphQL path → `lodash@4.17.21` | `unset`/`omit` prototype-pollution primitives are present; concrete external-input reachability is not proven | No | Vendure process absent | Same as above | Open; no untested resolution override | 2026-08-19 |
| GHSA-xxjr-mmjv-4gpg | Moderate | same Nest GraphQL path → `lodash@4.17.21` | `unset`/`omit` prototype-pollution primitives are present; concrete external-input reachability is not proven | No | Vendure process absent | Same as above | Open; no untested resolution override | 2026-08-19 |
| GHSA-f88m-g3jw-g9cj | High | `@vendure/asset-server-plugin@3.7.2` → `sharp@0.34.5` | Image decoding through Vendure asset processing | No | Asset server and worker absent | Safe `sharp >=0.35.0`; no validated compatible Vendure 3.x release | Open; track Vendure asset plugin | 2026-08-19 |
| GHSA-w5hq-g745-h8pq | Moderate | `@vendure/core@3.7.2` → `@apollo/server@4.13.0` → `uuid@9.0.1` | Vulnerable buffer-supplied UUID variants exist; use of that specific API from external input is not proven | No | Apollo/Vendure process absent | Safe `uuid >=11.1.1`; Apollo 4 path remains pinned | Open; no transitive major override | 2026-08-19 |
| GHSA-58qx-3vcg-4xpx | Moderate | `@vendure/core@3.7.2` → `@nestjs/graphql@13.1.0` → `ws@8.18.1` | GraphQL websocket/subscription transport | No | No Vendure listener in pilot | Safe `ws >=8.20.1` for this advisory; locked Nest path is older | Open; update through Nest/Vendure | 2026-08-19 |
| GHSA-96hv-2xvq-fx4p | High | same Nest GraphQL path → `ws@8.18.1` | Fragmented websocket message resource exhaustion | No | No Vendure listener in pilot | Safe `ws >=8.21.0`; no validated compatible Vendure 3.x release | Open; update through Nest/Vendure | 2026-08-19 |

The `ws@7.5.13` instance under legacy `subscriptions-transport-ws` is outside the ranges reported by this audit. The safe `sharp@0.35.3`, `file-type@21.3.4`, and `uuid@11.1.1` instances elsewhere in the lockfile do not replace the vulnerable Vendure-owned instances above.

## SBOM and installation caveat

- [`amazon-pilot.cdx.json`](sbom/amazon-pilot.cdx.json) is a lockfile-derived CycloneDX 1.5 inventory for the Rust backend, production frontend packages, parser version, and digest-pinned pilot/base images.
- [`commerce.cdx.json`](sbom/commerce.cdx.json) is the equivalent production inventory for retained Commerce code.
- The committed inventories are deterministically identified by their lockfile hashes rather than
  claiming the not-yet-created commit that contains them. CI adds its exact `GITHUB_SHA` to the
  uploaded build artifact.
- Native `npm sbom` succeeds for the frontend. It currently fails for Commerce with `ESBOMPROBLEMS` because npm identifies invalid peer relationships involving Dashboard `ajv`/`ajv-formats` and Nest GraphQL `ts-morph`. The committed generator deliberately reads the lockfile without rewriting that tree. This caveat remains open and is not represented as an npm-native SBOM success.
- Rust advisories are covered separately in [`RUST_ADVISORIES.md`](RUST_ADVISORIES.md). Syft, Trivy, and `cyclonedx-npm` were not installed in the inspected environment, so container components are not claimed advisory-free.

Re-run the review whenever Vendure 3.x, Nest GraphQL, Apollo, or the asset plugin changes, and no later than 2026-09-19.
