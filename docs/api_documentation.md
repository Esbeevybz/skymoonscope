# API Documentation

Sky Moon Scope Core generates its own OpenAPI description from the route handlers
using [`utoipa`](https://docs.rs/utoipa). The description is produced from the
same Rust types the handlers use, so the published contract cannot drift away
from the implementation.

## Interactive Swagger UI

Start the server and open the documentation browser:

```bash
cargo run -p Sky Moon Scope-core
# then visit:
#   http://localhost:8080/docs
```

`/docs` is the canonical entry point. `/swagger-ui` is kept as an alias so
existing bookmarks and deployment probes that point at the previous path keep
working; both serve the same document.

The raw specification is served as JSON at:

```
http://localhost:8080/api-docs/openapi.json
```

## Generating the specification file

Client SDKs are usually generated in CI, where booting the server (and its
database and Redis dependencies) is inconvenient. The `openapi` subcommand
writes the document straight to disk instead:

```bash
# writes ./openapi.json
cargo run -p Sky Moon Scope-core -- openapi

# or choose the destination
cargo run -p Sky Moon Scope-core -- openapi --out dist/openapi.json
```

The command exits non-zero and logs the cause if serialization or the write
fails, so it is safe to use as a CI build step. The emitted document follows
OpenAPI 3.0.3, which is what `utoipa` 4 produces.

Once generated, the file can be fed to any standard generator, for example:

```bash
npx @openapitools/openapi-generator-cli generate \
  -i openapi.json -g typescript-fetch -o web/generated/api
```

## Documented endpoints

| Tag | Endpoints |
| --- | --- |
| Analysis | `/analyze`, `/analyze/wasm`, `/analyze/wasm/branches`, `/analyze/optimize-limits`, `/analyze/compare`, `/analyze/gas-golfing` |
| Auth | `/auth/challenge`, `/auth/verify`, `/auth/emergency-pause`, `/auth/jwks` |
| Fee Market | `/fees/recommend`, `/fees/history`, `/fees/analytics` |
| Operations | `/health`, `/healthz`, `/readyz`, `/metrics`, `/api/v1/webhooks/incoming` |
| Analysis (batch) | `/api/v1/contracts/batch-state` |

## Custom fee mode (`fee_mode`)

The `/analyze` endpoint accepts an optional `fee_mode` field that controls how
`cost_stroops` is computed in the response.

### Default (simulated)

When `fee_mode` is omitted, the engine derives the fee from the measured
Soroban resources:

```
cost_stroops = cpu_instructions / 10_000
             + ram_bytes / 1_024
             + (ledger_read_bytes + ledger_write_bytes) / 1_024
```

### Custom fee override

Set `fee_mode` to `{"mode":"custom","cost_stroops":<value>}` to replace the
simulated fee with a caller-supplied stroop amount. This is useful for
what-if analysis — for example, estimating what the same call would cost at
mainnet fee levels without running a separate simulation.

```json
POST /analyze
{
  "contract_id": "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC",
  "function_name": "hello",
  "fee_mode": {
    "mode": "custom",
    "cost_stroops": 50000
  }
}
```

The response `ResourceReport` includes a `fee_override_active` boolean field:

| Value | Meaning |
| ----- | ------- |
| `false` | `cost_stroops` was computed from the simulation (default). |
| `true` | `cost_stroops` reflects the caller-supplied override; CPU/RAM/ledger metrics are still the real simulation values. |

**Validation:** `cost_stroops` must be greater than zero; a value of `0`
returns `400 Bad Request`. Omitting `fee_mode` or setting
`{"mode":"simulated"}` keeps the default behaviour unchanged.

## Custom fee mode (`fee_mode`)

The `/analyze` endpoint accepts an optional `fee_mode` field that controls how
`cost_stroops` is calculated in the response.

### Default (simulated)

When `fee_mode` is omitted, the engine computes the fee automatically from the
measured Soroban resources using the standard formula:

```
cost_stroops = cpu_instructions / 10_000
             + ram_bytes / 1_024
             + (ledger_read_bytes + ledger_write_bytes) / 1_024
```

### Custom fee override

Set `fee_mode` to `{"mode":"custom","cost_stroops":<value>}` to replace the
simulated fee with a caller-supplied stroop amount. This is useful for
what-if analysis — for example, estimating what the same call would cost at
mainnet fee levels without running a separate simulation.

```json
POST /analyze
{
  "contract_id": "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC",
  "function_name": "hello",
  "fee_mode": {
    "mode": "custom",
    "cost_stroops": 50000
  }
}
```

The response `ResourceReport` includes a `fee_override_active` boolean field:

- `false` — `cost_stroops` reflects the simulated value computed by the engine.
- `true` — `cost_stroops` reflects the caller-supplied override; the CPU, RAM,
  and ledger metrics are still the real simulation values.

### Validation

- `cost_stroops` must be greater than zero. A value of `0` returns
  `400 Bad Request` with an error message.
- The `fee_mode` field is optional; omitting it (or setting
  `{"mode":"simulated"}`) keeps the default behaviour.

The `/ws/jobs/:job_id` WebSocket upgrade and the `/graphql` endpoint are not
part of the REST description; WebSocket and GraphQL are outside the scope of
an OpenAPI document and are documented separately.

## Adding a new endpoint

1. Annotate the handler with `#[utoipa::path(...)]`, giving it a `tag`, a
   `path` that matches the router registration, and a `responses(...)` entry
   for every status code it can return.
2. Derive `ToSchema` on any request or response type the annotation mentions.
3. Register the handler in the `paths(...)` list of the `ApiDoc` derive, and
   any new types in `components(schemas(...))`.

A handler that is annotated but not listed in `paths(...)` silently stays out
of the document, so step 3 is the one worth double-checking.
