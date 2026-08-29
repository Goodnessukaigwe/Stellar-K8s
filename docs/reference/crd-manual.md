# Custom Resource Definition (CRD) Architecture Reference Manual

This manual is the architectural reference for every field, default, validation
constraint, status condition, and lifecycle pattern implemented by the Stellar-K8s
operator's published Custom Resource Definitions.

## Source of truth

All field names, types, enumerations, defaults, nullability, and OpenAPI-required
markers in this document are taken from the published CRD:

- `config/crd/stellarnode-crd.yaml` — `stellarnodes.stellar.org` / `v1alpha1`

Semantic (admission-time) constraints that are not expressible in the OpenAPI
schema are taken from `StellarNodeSpec::validate()` in `src/crd/stellar_node.rs`
and the related helpers in that file. Those rules are labeled **semantic** so they
are not confused with Kubernetes API-server schema validation.

This manual does **not** invent fields, defaults, or validation rules. Fields that
exist only in Rust source and are absent from the published CRD OpenAPI schema
are omitted.

The auto-generated field listing at [`docs/api-reference.md`](../api-reference.md)
is produced from the same CRD (`make generate-api-docs`). This manual adds
node-type architecture, required-versus-optional guidance, semantic constraints,
status conditions, and lifecycle patterns.

## Architecture

Stellar-K8s publishes **one** namespaced Custom Resource, `StellarNode`, for
operator-managed Stellar infrastructure. Horizon and Soroban RPC are **not**
separate CRD kinds. They are values of `spec.nodeType` on `StellarNode`:

| Node type | Kind | Role |
|---|---|---|
| `Validator` | `StellarNode` | Stellar Core validator that participates in consensus |
| `Horizon` | `StellarNode` | Horizon REST API server for ledger queries and transaction submission |
| `SorobanRpc` | `StellarNode` | Soroban JSON-RPC node for smart-contract simulation and submission |

The published CRD also defines `StellarBenchmark` and `BenchmarkReport` kinds
(`config/crd/stellarbenchmark-crd.yaml`, `config/crd/benchmarkreport-crd.yaml`).
Those are out of scope for this operator-node architecture manual.

### Identity

| | |
|---|---|
| CRD name | `stellarnodes.stellar.org` |
| API group | `stellar.org` |
| Kind | `StellarNode` |
| Plural | `stellarnodes` |
| Short name | `sn` |
| Scope | Namespaced |
| Version | `v1alpha1` (served, storage) |
| Subresource | `status` |

`kubectl` printer columns from the CRD:

| Name | Type | JSON path |
|---|---|---|
| Type | string | `.spec.nodeType` |
| Network | string | `.spec.network` |
| Ready | string | `.status.conditions[?(@.type=='Ready')].status` |
| Replicas | integer | `.spec.replicas` |
| Age | date | `.metadata.creationTimestamp` |

Typical manifest header:

```yaml
apiVersion: stellar.org/v1alpha1
kind: StellarNode
metadata:
  name: example
  namespace: stellar
spec:
  nodeType: Validator   # or Horizon, or SorobanRpc
  network: testnet
  version: "v21.0.0"
```

## Required versus optional fields

### OpenAPI required fields (API server)

The published OpenAPI schema requires these `spec` fields on every `StellarNode`.
`kubectl apply` and the API server reject manifests that omit them.

| Field | Type | Notes from schema |
|---|---|---|
| `spec.nodeType` | string | Enum: `Validator`, `Horizon`, `SorobanRpc` |
| `spec.network` | string | Enum: `mainnet`, `testnet`, `futurenet`, `custom` |
| `spec.version` | string | Image tag or digest (for example `v21.0.0`) |
| `spec.minAvailable` | integer \| string | Kubernetes `IntOrString` (`x-kubernetes-int-or-string`) |
| `spec.maxUnavailable` | integer \| string | Kubernetes `IntOrString` (`x-kubernetes-int-or-string`) |
| `spec.topologySpreadConstraints` | array of object | May be an empty array. Items preserve unknown fields |

The published schema also marks `spec` itself required on the resource, and
`status.phase` required on the status subresource (the operator writes status;
users do not set it).

When a nested object is present, the schema may require child fields. Those
nested requirements appear in the field catalog below (column **Required**).
Examples:

- `spec.horizonConfig` requires `databaseSecretRef` and `stellarCoreUrl`
- `spec.sorobanConfig` requires `stellarCoreUrl`
- `spec.autoscaling` requires `minReplicas` and `maxReplicas`
- `spec.ingress` requires `hosts`
- `spec.storage` (when set explicitly without relying on the object default) requires `size` and `storageClass`
- `spec.restoreFromSnapshot` requires `volumeSnapshotName`
- `spec.ociSnapshot` requires `credentialSecretName`, `image`, and `registry`
- `spec.drConfig` requires `peerClusterId` and `role`
- `spec.managedDatabase` requires `storage`
- `spec.database` requires `secretKeyRef`

### Semantic required fields (operator `validate()`)

These rules are enforced by the operator, not by the published OpenAPI schema:

| Node type | Required object | Additional semantic rules |
|---|---|---|
| `Validator` | `spec.validatorConfig` | `spec.replicas` must be `1`. PDB fields, autoscaling, ingress, and canary strategy are rejected. If `enableHistoryArchive` is `true`, `historyArchiveUrls` must be non-empty. |
| `Horizon` | `spec.horizonConfig` | `snapshotSchedule` / `restoreFromSnapshot` are rejected. Autoscaling `minReplicas` must be ≥ 1 and `maxReplicas` ≥ `minReplicas`. Gas autoscaling is rejected. |
| `SorobanRpc` | `spec.sorobanConfig` | Same snapshot restriction as Horizon. Same HPA replica bounds. If `sorobanConfig.cache.enabled` is `true`, cache bounds must be positive. |

Mutual-exclusion and cross-field semantic rules (all node types):

- `spec.database` and `spec.managedDatabase` cannot both be set.
- `spec.minAvailable` and `spec.maxUnavailable` cannot both be set (**semantic**). This conflicts with the OpenAPI requirement that both fields be present; see [OpenAPI versus semantic PDB fields](#openapi-versus-semantic-pdb-fields).
- `spec.storage.mode: Local` requires `storageClass` or `nodeAffinity`.
- `spec.storage.snapshotRef` must set exactly one of `volumeSnapshotName` or `backupUrl`.
- `spec.network: custom` uses `spec.customNetworkPassphrase` for the passphrase (Rust spec comment: required if network is Custom). Custom network names are validated against DNS-1123 (`^[a-z0-9]([-a-z0-9]*[a-z0-9])?$`, 1–63 characters) when the in-memory `Custom` variant is used.
- Ingress (Horizon / SorobanRpc): `hosts` non-empty; each host and path non-empty; `pathType` if set must be `Prefix` or `Exact`.
- Load balancer BGP mode requires `bgp` with `localASN` in `1–4294967295` and at least one peer.
- Service mesh: `istio` and `linkerd` are mutually exclusive; Istio circuit-breaker and retry counters must be > 0; Linkerd `policyMode` must be `allow`, `deny`, or `audit`.
- HSM-backed validators must use `seedSecretSource.csiRef` or `seedSecretSource.vaultRef` (not `localRef`, `externalRef`, or legacy `seedSecretRef`).
- `seedSecretSource` must set exactly one of `localRef`, `externalRef`, `csiRef`, or `vaultRef`.

### OpenAPI versus semantic PDB fields

The published OpenAPI schema lists both `spec.minAvailable` and `spec.maxUnavailable`
as required. The operator's `validate()` rejects a spec that sets **both**, and
rejects either field on `Validator` nodes (`replicas` must be 1).

Client-side `kubectl apply --dry-run=client` validates only the OpenAPI schema,
so the examples in this manual include both fields (and an empty
`topologySpreadConstraints` array) so they apply cleanly against the published
CRD. The operator will still apply the semantic rules at reconcile / webhook
time.

## Node type: Validator

Use `spec.nodeType: Validator` for a Stellar Core validator.

**OpenAPI-applicable objects:** `spec.validatorConfig` (optional in the schema,
**semantically required**), plus shared fields. `spec.horizonConfig` and
`spec.sorobanConfig` are ignored for this node type.

**Semantically forbidden on Validator:** `spec.autoscaling`, `spec.ingress`,
canary `spec.strategy`, PDB fields, `replicas != 1`.

**Validator-only features in the published schema:** `spec.snapshotSchedule`,
`spec.restoreFromSnapshot`, and `spec.validatorConfig.*` (seed, quorum set,
history archives, HSM/KMS).

Production notes taken from field descriptions (not invented):

- Prefer `validatorConfig.seedSecretSource` over deprecated `seedSecretRef`.
- `localRef` is documented in the schema as development-only; CSI and Vault
  backends do not materialize the seed into etcd.
- If `enableHistoryArchive` is `true`, provide at least one history archive URL.
- `podAntiAffinity` defaults to `Hard` so validators are not co-located.

See [examples/validator.yaml](examples/validator.yaml).

## Node type: Horizon

Use `spec.nodeType: Horizon` for a Horizon API deployment.

**OpenAPI-applicable objects:** `spec.horizonConfig` (optional in the schema,
**semantically required**). Required children when the object is present:
`databaseSecretRef`, `stellarCoreUrl`.

**Published `horizonConfig` fields:**

| Field | Type | Required | Default | Purpose |
|---|---|---|---|---|
| `databaseSecretRef` | string | required | — | Kubernetes Secret with the Horizon database connection |
| `stellarCoreUrl` | string | required | — | URL of a stellar-core HTTP endpoint used for ingestion |
| `enableIngest` | boolean | optional | `true` | Enable ledger ingestion into the Horizon database |
| `ingestWorkers` | integer (uint32) | optional | `1` | Concurrent ingestion workers |
| `enableExperimentalIngestion` | boolean | optional | `false` | Experimental ingestion flag |
| `autoMigration` | boolean | optional | `true` | Automatic database schema migrations |

**Semantically forbidden on Horizon:** `spec.snapshotSchedule`,
`spec.restoreFromSnapshot`, and `spec.autoscaling.gasAutoscaling` (that field
is also absent from the published OpenAPI schema).

Horizon may use `spec.replicas` > 1, `spec.autoscaling`, `spec.ingress`, and
canary `spec.strategy`.

See [examples/horizon.yaml](examples/horizon.yaml).

## Node type: SorobanRpc

Use `spec.nodeType: SorobanRpc` for a Soroban RPC deployment.

**OpenAPI-applicable objects:** `spec.sorobanConfig` (optional in the schema,
**semantically required**). Required child when present: `stellarCoreUrl`.

**Published `sorobanConfig` fields:**

| Field | Type | Required | Default | Constraints | Purpose |
|---|---|---|---|---|---|
| `stellarCoreUrl` | string | required | — | — | Upstream Core URL for submission |
| `enablePreflight` | boolean | optional | `true` | — | Enable preflight / simulation |
| `maxEventsPerRequest` | integer (uint32) | optional | `10000` | min 0 | Max events returned per request |
| `captiveCoreConfig` | string | optional | — | nullable | Deprecated raw captive-core config |
| `captiveCoreStructuredConfig` | object | optional | — | nullable | Type-safe captive-core settings |
| `cache` | object | optional | — | nullable | Bounded fail-open cache for read-only state RPC |

**Published `captiveCoreStructuredConfig` children:**
`networkPassphrase`, `historyArchiveUrls` (default `[]`), `peerPort`, `httpPort`
(uint16, min 0, nullable), `logLevel`, `additionalConfig`.

**Published `cache` children:**

| Field | Type | Default | Constraints |
|---|---|---|---|
| `enabled` | boolean | `false` | — |
| `ttlSecs` | integer (int64) | `30` | min 1 |
| `maxEntries` | integer (int64) | `10000` | min 1, max 10000 |
| `maxBytes` | integer (int64) | `67108864` | min 1, max 67108864 |
| `image` | string | — | nullable |

When `cache.enabled` is `true`, semantic validation requires positive
`ttlSecs`, `maxEntries`, and `maxBytes`.

**Semantically forbidden on SorobanRpc:** `spec.snapshotSchedule`,
`spec.restoreFromSnapshot`. Autoscaling is allowed; gas-based autoscaling is
implemented in Rust but is **not** in the published OpenAPI schema.

See [examples/soroban-rpc.yaml](examples/soroban-rpc.yaml).

## Shared spec fields

The following top-level `spec` fields are defined on every `StellarNode` in the
published schema. Node-type applicability is noted where the operator
restricts them.

| Field | OpenAPI required | Default | Typical applicability |
|---|---|---|---|
| `nodeType` | yes | — | All |
| `network` | yes | — | All |
| `version` | yes | — | All |
| `minAvailable` | yes | — | Required by schema; semantically invalid on Validator and invalid if combined with `maxUnavailable` |
| `maxUnavailable` | yes | — | Same as `minAvailable` |
| `topologySpreadConstraints` | yes | — | All (empty array is valid) |
| `customNetworkPassphrase` | no | — | When `network` is `custom` |
| `historyMode` | no | `Recent` | All (`Full` or `Recent`) |
| `replicas` | no | `1` | Must be `1` for Validator |
| `resources` | no | requests `500m`/`1Gi`, limits `2`/`4Gi` | All |
| `storage` | no | `PersistentVolume` / `standard` / `100Gi` / `Delete` | All |
| `validatorConfig` | no | — | Semantically required for Validator |
| `horizonConfig` | no | — | Semantically required for Horizon |
| `sorobanConfig` | no | — | Semantically required for SorobanRpc |
| `readPoolEndpoint` | no | — | Read-replica pool DNS |
| `alerting` | no | `false` | All |
| `suspended` | no | `false` | All |
| `maintenanceMode` | no | `false` | All |
| `database` | no | — | External DB; mutually exclusive with `managedDatabase` |
| `managedDatabase` | no | — | CloudNativePG; mutually exclusive with `database` |
| `autoscaling` | no | — | Horizon / SorobanRpc only |
| `vpaConfig` | no | — | All |
| `ingress` | no | — | Horizon / SorobanRpc only |
| `loadBalancer` | no | — | All |
| `globalDiscovery` | no | — | All |
| `crossCluster` | no | — | All |
| `strategy` | no | `{type: rollingUpdate}` | Canary only for Horizon / SorobanRpc |
| `networkPolicy` | no | — | All |
| `drConfig` | no | — | All |
| `podAntiAffinity` | no | `Hard` | All (`Hard`, `Soft`, `Disabled`) |
| `cveHandling` | no | — | All |
| `snapshotSchedule` | no | — | Validator only |
| `restoreFromSnapshot` | no | — | Validator only |
| `readReplicaConfig` | no | — | All |
| `dbMaintenanceConfig` | no | — | Database-backed nodes |
| `ociSnapshot` | no | — | All |
| `serviceMesh` | no | — | All (Istio **or** Linkerd) |
| `forensicSnapshot` | no | — | All |

## Status fields and conditions

The operator owns the `status` subresource. The published schema requires
`status.phase` when status is present.

### Deprecated phase

`status.phase` is documented in the CRD as the current lifecycle phase and is
**deprecated since 0.2.0** in the Rust type: prefer `status.conditions`. The
operator still writes phase for compatibility. Documented phase names from the
CRD status description:

| Phase | Meaning |
|---|---|
| `Pending` | Resource creation is queued but not started |
| `Creating` | Infrastructure (Pod, Service, and so on) is being created |
| `Running` | Pod is running but not yet synced |
| `Syncing` | Node is syncing blockchain data (validators) |
| `Ready` | Node is fully synced and operational |
| `Failed` | Unrecoverable error |
| `Degraded` | Running but not fully healthy |
| `Remediating` | Operator is attempting recovery |
| `Terminating` | Resources are being cleaned up |

`StellarNodeStatus::derive_phase_from_conditions()` maps conditions back to a
compatibility phase: `Ready`, `Degraded`, `Progressing`, `Pending`, `Creating`,
`NotReady`, or `Unknown` (from `Ready` reason `PodsPending` / `Creating`).

### Standard conditions

Condition object fields (all required except `observedGeneration`): `type`,
`status`, `lastTransitionTime`, `reason`, `message`, optional `observedGeneration`.

Condition `status` values used by the operator: `True`, `False`, `Unknown`.

Standard condition types from `src/controller/conditions.rs`:

| Type | Meaning |
|---|---|
| `Ready` | All sub-resources are healthy and the node is operational |
| `Progressing` | The node is being created, updated, or syncing |
| `Degraded` | The node is operational but experiencing issues |
| `Available` | Defined as a standard type constant |

Additional condition type mentioned by the seed-source implementation:
`SeedSecretReady` (set `False` when `seedSecretSource` is invalid).

A node is treated as ready when `Ready=True` **and** `readyReplicas >= replicas`.

### Observed status besides conditions

See the status field catalog. Highlights from schema descriptions:

- `ledgerSequence` / `ledgerUpdatedAt` — validator ledger progress
- `endpoint` / `externalIp` / `bgpStatus` — service and MetalLB/BGP
- `readyReplicas` / `replicas` — replica counts (default `0`)
- `canaryReadyReplicas`, `canaryVersion`, `canaryStartTime` — canary rollout
- `lastMigratedVersion` — last successful DB schema migration
- `quorumFragility` / `quorumAnalysisTimestamp` — validator quorum analysis
- `drStatus` — disaster-recovery peer health, lag, drills
- `forensicSnapshotPhase` — `Pending`, `Capturing`, `Complete`, `Failed`
- `labelPropagationStatus` — `Synced`, `Partial`, `Failed`
- `snapshotBootstrap.phase` — `Pending`, `Restoring`, `Restored`, `Syncing`, `Synced`, `Failed`
- `vaultObservedSecretVersion` — Vault rotation-driven rollouts

## Lifecycle patterns

The operator updates conditions and (deprecated) phase as the node moves through
create → run → sync → ready, or into degraded / remediating / failed / terminating.

```
Pending ──► Creating ──► Running ──► Syncing ──► Ready
                │            │           │
                └────────────┴───────────┴──► Failed
                                             Degraded ──► Remediating ──► Ready | Failed
Ready|Running ──► Terminating
spec.suspended: true ──► Ready=False reason=NodeSuspended (replicas scaled to 0)
```

Bootstrap from snapshot (`spec.storage.snapshotRef` or
`spec.restoreFromSnapshot`) is reported on `status.snapshotBootstrap`. Phases:
`Pending` → `Restoring` → `Restored` → `Syncing` → `Synced`, or `Failed`.
`secondsToSync` ≤ 600 is the documented “synced within 10 minutes” criterion.

Canary (`spec.strategy.type: canary`, Horizon / SorobanRpc only): the operator
tracks `canaryReadyReplicas`, `canaryVersion`, and `canaryStartTime`. Published
canary spec fields are `weight` (default `10`) and `checkIntervalSeconds`
(default `300`).

Suspended nodes (`spec.suspended: true`) set `Ready=False` with reason
`NodeSuspended` and scale replicas to 0 while keeping the Service for peer
discovery.

## Configuration examples

Production-oriented manifests that use only published CRD fields:

| Deployment | File |
|---|---|
| Standard Validator | [examples/validator.yaml](examples/validator.yaml) |
| Horizon API | [examples/horizon.yaml](examples/horizon.yaml) |
| Soroban RPC | [examples/soroban-rpc.yaml](examples/soroban-rpc.yaml) |

Validate against the published CRD:

```bash
kubectl apply --dry-run=client -f config/crd/stellarnode-crd.yaml
kubectl apply --dry-run=client -f docs/reference/examples/validator.yaml
kubectl apply --dry-run=client -f docs/reference/examples/horizon.yaml
kubectl apply --dry-run=client -f docs/reference/examples/soroban-rpc.yaml
```

## Complete published field catalog

The tables below list **every** property in the published `v1alpha1` OpenAPI
schema for `StellarNode`. **Required** means required by the schema when the
parent object is present. **Default** and **Enum** values are the OpenAPI
`default` / `enum` keywords. Purpose text is the schema `description` (or `—`
when the schema has none).


### Spec fields

#### `spec.alerting`

| Path | Type | Required | Default | Enum / values | Constraints | Purpose |
|---|---|---|---|---|---|---|
| `spec.alerting` | boolean | optional | `false` | — | — | — |

#### `spec.autoscaling`

| Path | Type | Required | Default | Enum / values | Constraints | Purpose |
|---|---|---|---|---|---|---|
| `spec.autoscaling` | object | optional | — | — | nullable | Horizontal Pod Autoscaling configuration |
| `spec.autoscaling.behavior` | object | optional | — | — | nullable | Scaling behavior configuration for HPA |
| `spec.autoscaling.behavior.scaleDown` | object | optional | — | — | nullable | Scaling policy |
| `spec.autoscaling.behavior.scaleDown.policies` | array of object | optional | — | — | — | — |
| `spec.autoscaling.behavior.scaleDown.policies[].periodSeconds` | integer (int32) | required | — | — | — | — |
| `spec.autoscaling.behavior.scaleDown.policies[].policyType` | string | required | — | — | — | — |
| `spec.autoscaling.behavior.scaleDown.policies[].value` | integer (int32) | required | — | — | — | — |
| `spec.autoscaling.behavior.scaleDown.stabilizationWindowSeconds` | integer (int32) | optional | — | — | nullable | — |
| `spec.autoscaling.behavior.scaleUp` | object | optional | — | — | nullable | Scaling policy |
| `spec.autoscaling.behavior.scaleUp.policies` | array of object | optional | — | — | — | — |
| `spec.autoscaling.behavior.scaleUp.policies[].periodSeconds` | integer (int32) | required | — | — | — | — |
| `spec.autoscaling.behavior.scaleUp.policies[].policyType` | string | required | — | — | — | — |
| `spec.autoscaling.behavior.scaleUp.policies[].value` | integer (int32) | required | — | — | — | — |
| `spec.autoscaling.behavior.scaleUp.stabilizationWindowSeconds` | integer (int32) | optional | — | — | nullable | — |
| `spec.autoscaling.customMetrics` | array of string | optional | — | — | — | — |
| `spec.autoscaling.maxReplicas` | integer (int32) | required | — | — | — | — |
| `spec.autoscaling.minReplicas` | integer (int32) | required | — | — | — | — |
| `spec.autoscaling.targetCpuUtilizationPercentage` | integer (int32) | optional | — | — | nullable | — |

#### `spec.crossCluster`

| Path | Type | Required | Default | Enum / values | Constraints | Purpose |
|---|---|---|---|---|---|---|
| `spec.crossCluster` | object | optional | — | — | nullable | Cross-cluster configuration for multi-cluster federation |
| `spec.crossCluster.autoDiscovery` | boolean | optional | `false` | — | — | — |
| `spec.crossCluster.enabled` | boolean | optional | `false` | — | — | — |
| `spec.crossCluster.externalName` | object | optional | — | — | nullable | ExternalName service configuration |
| `spec.crossCluster.externalName.createExternalNameServices` | boolean | optional | `true` | — | — | — |
| `spec.crossCluster.externalName.dnsProvider` | string | optional | — | — | nullable | — |
| `spec.crossCluster.externalName.externalDnsName` | string | required | — | — | — | — |
| `spec.crossCluster.externalName.ttl` | integer (uint32) | optional | `300` | — | min 0.0 | — |
| `spec.crossCluster.healthCheck` | object | optional | — | — | nullable | Health check configuration for cross-cluster peers |
| `spec.crossCluster.healthCheck.enabled` | boolean | optional | `true` | — | — | — |
| `spec.crossCluster.healthCheck.failureThreshold` | integer (uint32) | optional | `3` | — | min 0.0 | — |
| `spec.crossCluster.healthCheck.intervalSeconds` | integer (uint32) | optional | `30` | — | min 0.0 | — |
| `spec.crossCluster.healthCheck.latencyMeasurement` | object | optional | — | — | nullable | Latency measurement configuration |
| `spec.crossCluster.healthCheck.latencyMeasurement.enabled` | boolean | optional | `true` | — | — | — |
| `spec.crossCluster.healthCheck.latencyMeasurement.method` | string | optional | `"ping"` | `ping`, `tcp`, `http`, `grpc` | — | Method for measuring cross-cluster latency |
| `spec.crossCluster.healthCheck.latencyMeasurement.percentile` | integer (uint8) | optional | `95` | — | min 0.0 | — |
| `spec.crossCluster.healthCheck.latencyMeasurement.sampleCount` | integer (uint32) | optional | `10` | — | min 0.0 | — |
| `spec.crossCluster.healthCheck.successThreshold` | integer (uint32) | optional | `1` | — | min 0.0 | — |
| `spec.crossCluster.healthCheck.timeoutSeconds` | integer (uint32) | optional | `5` | — | min 0.0 | — |
| `spec.crossCluster.latencyThresholdMs` | integer (uint32) | optional | `200` | — | min 0.0 | — |
| `spec.crossCluster.mode` | string | optional | `"serviceMesh"` | `serviceMesh`, `externalName`, `directIP` | — | Cross-cluster networking mode |
| `spec.crossCluster.peerClusters` | array of object | optional | — | — | — | — |
| `spec.crossCluster.peerClusters[].clusterId` | string | required | — | — | — | — |
| `spec.crossCluster.peerClusters[].enabled` | boolean | optional | `true` | — | — | — |
| `spec.crossCluster.peerClusters[].endpoint` | string | required | — | — | — | — |
| `spec.crossCluster.peerClusters[].latencyThresholdMs` | integer (uint32) | optional | — | — | nullable; min 0.0 | — |
| `spec.crossCluster.peerClusters[].port` | integer (uint16) | optional | — | — | nullable; min 0.0 | — |
| `spec.crossCluster.peerClusters[].priority` | integer (uint32) | optional | `100` | — | min 0.0 | — |
| `spec.crossCluster.peerClusters[].region` | string | optional | — | — | nullable | — |
| `spec.crossCluster.serviceMesh` | object | optional | — | — | nullable | Service mesh configuration for cross-cluster networking |
| `spec.crossCluster.serviceMesh.clusterSetId` | string | optional | — | — | nullable | — |
| `spec.crossCluster.serviceMesh.meshType` | string | required | — | `submariner`, `istio`, `linkerd`, `cilium` | — | Supported service mesh types for cross-cluster networking |
| `spec.crossCluster.serviceMesh.mtlsEnabled` | boolean | optional | `true` | — | — | — |
| `spec.crossCluster.serviceMesh.serviceExport` | object | optional | — | — | nullable | Service export configuration |
| `spec.crossCluster.serviceMesh.serviceExport.enabled` | boolean | optional | `true` | — | — | — |
| `spec.crossCluster.serviceMesh.serviceExport.namespace` | string | optional | — | — | nullable | — |
| `spec.crossCluster.serviceMesh.serviceExport.serviceName` | string | optional | — | — | nullable | — |
| `spec.crossCluster.serviceMesh.serviceExport.targetClusters` | array of string | optional | — | — | — | — |
| `spec.crossCluster.serviceMesh.trafficPolicy` | string | optional | `"localPreferred"` | `localPreferred`, `global`, `localOnly`, `latencyBased` | — | Traffic policy for cross-cluster routing |

#### `spec.customNetworkPassphrase`

| Path | Type | Required | Default | Enum / values | Constraints | Purpose |
|---|---|---|---|---|---|---|
| `spec.customNetworkPassphrase` | string | optional | — | — | nullable | — |

#### `spec.cveHandling`

| Path | Type | Required | Default | Enum / values | Constraints | Purpose |
|---|---|---|---|---|---|---|
| `spec.cveHandling` | object | optional | — | — | nullable | CVE handling configuration for automated patching Enables scanning for vulnerabilities and automatic rollout of patched versions |
| `spec.cveHandling.canaryPassRateThreshold` | number (double) | optional | `100.0` | — | — | — |
| `spec.cveHandling.canaryTestTimeoutSecs` | integer (uint64) | optional | `300` | — | min 0.0 | — |
| `spec.cveHandling.consensusHealthThreshold` | number (double) | optional | `0.95` | — | — | — |
| `spec.cveHandling.criticalOnly` | boolean | optional | `false` | — | — | — |
| `spec.cveHandling.enableAutoRollback` | boolean | optional | `true` | — | — | — |
| `spec.cveHandling.enabled` | boolean | optional | `true` | — | — | — |
| `spec.cveHandling.scanIntervalSecs` | integer (uint64) | optional | `3600` | — | min 0.0 | — |

#### `spec.database`

| Path | Type | Required | Default | Enum / values | Constraints | Purpose |
|---|---|---|---|---|---|---|
| `spec.database` | object | optional | — | — | nullable | External database configuration for managed Postgres databases |
| `spec.database.secretKeyRef` | object | required | — | — | — | Reference to a key within a Kubernetes Secret |
| `spec.database.secretKeyRef.key` | string | required | — | — | — | — |
| `spec.database.secretKeyRef.name` | string | required | — | — | — | — |

#### `spec.dbMaintenanceConfig`

| Path | Type | Required | Default | Enum / values | Constraints | Purpose |
|---|---|---|---|---|---|---|
| `spec.dbMaintenanceConfig` | object | optional | — | — | nullable | Database maintenance configuration for automated vacuum and reindexing Enables periodic maintenance windows for performance optimization |
| `spec.dbMaintenanceConfig.autoReindex` | boolean | optional | `true` | — | — | Automatically reindex bloated tables |
| `spec.dbMaintenanceConfig.bloatThresholdPercent` | integer (uint32) | optional | `30` | — | min 0.0 | Bloat threshold percentage to trigger VACUUM FULL (default: 30) |
| `spec.dbMaintenanceConfig.enabled` | boolean | optional | `true` | — | — | Enable automated database maintenance |
| `spec.dbMaintenanceConfig.readPoolCoordination` | boolean | optional | `true` | — | — | Coordination with read-pool for zero-downtime |
| `spec.dbMaintenanceConfig.windowDuration` | string | required | — | — | — | Maintenance window duration (e.g., "2h") |
| `spec.dbMaintenanceConfig.windowStart` | string | required | — | — | — | Maintenance window start time (24h format, e.g., "02:00") Maintenance will only trigger during this window |

#### `spec.drConfig`

| Path | Type | Required | Default | Enum / values | Constraints | Purpose |
|---|---|---|---|---|---|---|
| `spec.drConfig` | object | optional | — | — | nullable | Configuration for multi-cluster disaster recovery |
| `spec.drConfig.drillSchedule` | object | optional | — | — | nullable | Configuration for automated DR drill scheduling |
| `spec.drConfig.drillSchedule.autoRollback` | boolean | optional | `true` | — | — | Whether to automatically rollback after drill completion |
| `spec.drConfig.drillSchedule.dryRun` | boolean | optional | `false` | — | — | Whether to actually perform failover or just simulate it (dry-run) |
| `spec.drConfig.drillSchedule.rollbackDelaySeconds` | integer (uint32) | optional | `60` | — | min 0.0 | Rollback delay after drill completion (seconds) |
| `spec.drConfig.drillSchedule.schedule` | string | required | — | — | — | Cron expression for drill scheduling (e.g., "0 2 * * 0" for weekly Sunday 2 AM) |
| `spec.drConfig.drillSchedule.timeoutSeconds` | integer (uint32) | optional | `300` | — | min 0.0 | Maximum time to wait for failover to complete (seconds) |
| `spec.drConfig.enabled` | boolean | optional | `false` | — | — | — |
| `spec.drConfig.failoverDns` | object | optional | — | — | nullable | ExternalDNS configuration |
| `spec.drConfig.failoverDns.annotations` | object | optional | — | — | nullable | — |
| `spec.drConfig.failoverDns.hostname` | string | required | — | — | — | — |
| `spec.drConfig.failoverDns.provider` | string | optional | — | — | nullable | — |
| `spec.drConfig.failoverDns.ttl` | integer (uint32) | optional | `300` | — | min 0.0 | — |
| `spec.drConfig.healthCheckInterval` | integer (uint32) | optional | `30` | — | min 0.0 | — |
| `spec.drConfig.peerClusterId` | string | required | — | — | — | — |
| `spec.drConfig.role` | string | required | — | `primary`, `standby` | — | Role of a node in a DR configuration |
| `spec.drConfig.syncStrategy` | string | optional | `"consensus"` | `consensus`, `peertracking`, `archivesync` | — | Synchronization strategy for hot standby nodes |

#### `spec.forensicSnapshot`

| Path | Type | Required | Default | Enum / values | Constraints | Purpose |
|---|---|---|---|---|---|---|
| `spec.forensicSnapshot` | object | optional | — | — | nullable | Forensic snapshot: set `metadata.annotations["stellar.org/request-forensic-snapshot"]="true"` to trigger a one-shot capture (PCAP, optional core dump) uploaded to S3. |
| `spec.forensicSnapshot.credentialsSecretRef` | string | optional | — | — | nullable | Secret in the same namespace with `AWS_ACCESS_KEY_ID` and `AWS_SECRET_ACCESS_KEY` when not using IRSA/instance roles. |
| `spec.forensicSnapshot.enableShareProcessNamespace` | boolean | optional | `false` | — | — | Set `shareProcessNamespace: true` on validator pods so the capture container can see `stellar-core` for core dumps (recommended for forensic workflows). |
| `spec.forensicSnapshot.kmsKeyId` | string | optional | — | — | nullable | Optional KMS key id for SSE-KMS (`aws s3 cp --sse aws:kms`). |
| `spec.forensicSnapshot.s3Bucket` | string | required | — | — | — | Target S3 bucket for the encrypted forensic tarball. |
| `spec.forensicSnapshot.s3Prefix` | string | optional | — | — | nullable | — |

#### `spec.globalDiscovery`

| Path | Type | Required | Default | Enum / values | Constraints | Purpose |
|---|---|---|---|---|---|---|
| `spec.globalDiscovery` | object | optional | — | — | nullable | Global discovery configuration for cross-cluster discovery |
| `spec.globalDiscovery.enabled` | boolean | optional | `false` | — | — | — |
| `spec.globalDiscovery.externalDns` | object | optional | — | — | nullable | ExternalDNS configuration |
| `spec.globalDiscovery.externalDns.annotations` | object | optional | — | — | nullable | — |
| `spec.globalDiscovery.externalDns.hostname` | string | required | — | — | — | — |
| `spec.globalDiscovery.externalDns.provider` | string | optional | — | — | nullable | — |
| `spec.globalDiscovery.externalDns.ttl` | integer (uint32) | optional | `300` | — | min 0.0 | — |
| `spec.globalDiscovery.priority` | integer (uint32) | optional | `100` | — | min 0.0 | — |
| `spec.globalDiscovery.region` | string | optional | — | — | nullable | — |
| `spec.globalDiscovery.serviceMesh` | object | optional | — | — | nullable | Service mesh integration configuration |
| `spec.globalDiscovery.serviceMesh.meshType` | string | required | — | `istio`, `linkerd`, `consul` | — | Supported service mesh implementations |
| `spec.globalDiscovery.serviceMesh.mtlsMode` | string | optional | `"PERMISSIVE"` | `DISABLE`, `PERMISSIVE`, `STRICT` | — | mTLS enforcement mode |
| `spec.globalDiscovery.serviceMesh.sidecarInjection` | boolean | optional | `true` | — | — | — |
| `spec.globalDiscovery.serviceMesh.virtualServiceHost` | string | optional | — | — | nullable | — |
| `spec.globalDiscovery.topologyAwareHints` | boolean | optional | `false` | — | — | — |
| `spec.globalDiscovery.zone` | string | optional | — | — | nullable | — |

#### `spec.historyMode`

| Path | Type | Required | Default | Enum / values | Constraints | Purpose |
|---|---|---|---|---|---|---|
| `spec.historyMode` | string | optional | `"Recent"` | `Full`, `Recent` | — | History mode for the node |

#### `spec.horizonConfig`

| Path | Type | Required | Default | Enum / values | Constraints | Purpose |
|---|---|---|---|---|---|---|
| `spec.horizonConfig` | object | optional | — | — | nullable | Horizon API server configuration |
| `spec.horizonConfig.autoMigration` | boolean | optional | `true` | — | — | — |
| `spec.horizonConfig.databaseSecretRef` | string | required | — | — | — | — |
| `spec.horizonConfig.enableExperimentalIngestion` | boolean | optional | `false` | — | — | — |
| `spec.horizonConfig.enableIngest` | boolean | optional | `true` | — | — | — |
| `spec.horizonConfig.ingestWorkers` | integer (uint32) | optional | `1` | — | min 0.0 | — |
| `spec.horizonConfig.stellarCoreUrl` | string | required | — | — | — | — |

#### `spec.ingress`

| Path | Type | Required | Default | Enum / values | Constraints | Purpose |
|---|---|---|---|---|---|---|
| `spec.ingress` | object | optional | — | — | nullable | Ingress configuration |
| `spec.ingress.annotations` | object | optional | — | — | nullable | — |
| `spec.ingress.certManagerClusterIssuer` | string | optional | — | — | nullable | — |
| `spec.ingress.certManagerIssuer` | string | optional | — | — | nullable | — |
| `spec.ingress.className` | string | optional | — | — | nullable | — |
| `spec.ingress.hosts` | array of object | required | — | — | — | — |
| `spec.ingress.hosts[].host` | string | required | — | — | — | — |
| `spec.ingress.hosts[].paths` | array of object | optional | `[{"path":"/","pathType":"Prefix"}]` | — | — | — |
| `spec.ingress.hosts[].paths[].path` | string | required | — | — | — | — |
| `spec.ingress.hosts[].paths[].pathType` | string | optional | `"Prefix"` | — | nullable | — |
| `spec.ingress.tlsSecretName` | string | optional | — | — | nullable | — |

#### `spec.loadBalancer`

| Path | Type | Required | Default | Enum / values | Constraints | Purpose |
|---|---|---|---|---|---|---|
| `spec.loadBalancer` | object | optional | — | — | nullable | Load balancer configuration for external access (e.g. MetalLB) |
| `spec.loadBalancer.addressPool` | string | optional | — | — | nullable | — |
| `spec.loadBalancer.annotations` | object | optional | — | — | nullable | — |
| `spec.loadBalancer.bgp` | object | optional | — | — | nullable | BGP configuration for MetalLB anycast routing |
| `spec.loadBalancer.bgp.advertisement` | object | optional | — | — | nullable | BGP advertisement configuration |
| `spec.loadBalancer.bgp.advertisement.aggregationLength` | integer (uint8) | optional | `32` | — | min 0.0 | — |
| `spec.loadBalancer.bgp.advertisement.aggregationLengthV6` | integer (uint8) | optional | `128` | — | min 0.0 | — |
| `spec.loadBalancer.bgp.advertisement.localPref` | integer (uint32) | optional | — | — | nullable; min 0.0 | — |
| `spec.loadBalancer.bgp.advertisement.nodeSelectors` | object | optional | — | — | nullable | — |
| `spec.loadBalancer.bgp.bfdEnabled` | boolean | optional | `false` | — | — | — |
| `spec.loadBalancer.bgp.bfdProfile` | string | optional | — | — | nullable | — |
| `spec.loadBalancer.bgp.communities` | array of string | optional | — | — | — | — |
| `spec.loadBalancer.bgp.largeCommunities` | array of string | optional | — | — | — | — |
| `spec.loadBalancer.bgp.localAsn` | integer (uint32) | required | — | — | min 0.0 | — |
| `spec.loadBalancer.bgp.nodeSelectors` | object | optional | — | — | nullable | — |
| `spec.loadBalancer.bgp.peers` | array of object | optional | — | — | — | — |
| `spec.loadBalancer.bgp.peers[].address` | string | required | — | — | — | — |
| `spec.loadBalancer.bgp.peers[].asn` | integer (uint32) | required | — | — | min 0.0 | — |
| `spec.loadBalancer.bgp.peers[].ebgpMultiHop` | boolean | optional | `false` | — | — | — |
| `spec.loadBalancer.bgp.peers[].gracefulRestart` | boolean | optional | `true` | — | — | — |
| `spec.loadBalancer.bgp.peers[].holdTime` | integer (uint32) | optional | `90` | — | min 0.0 | — |
| `spec.loadBalancer.bgp.peers[].keepaliveTime` | integer (uint32) | optional | `30` | — | min 0.0 | — |
| `spec.loadBalancer.bgp.peers[].passwordSecretRef` | object | optional | — | — | nullable | Reference to a key within a Kubernetes Secret |
| `spec.loadBalancer.bgp.peers[].passwordSecretRef.key` | string | required | — | — | — | — |
| `spec.loadBalancer.bgp.peers[].passwordSecretRef.name` | string | required | — | — | — | — |
| `spec.loadBalancer.bgp.peers[].port` | integer (uint16) | optional | `179` | — | min 0.0 | — |
| `spec.loadBalancer.bgp.peers[].routerId` | string | optional | — | — | nullable | — |
| `spec.loadBalancer.bgp.peers[].sourceAddress` | string | optional | — | — | nullable | — |
| `spec.loadBalancer.enabled` | boolean | optional | `false` | — | — | — |
| `spec.loadBalancer.externalTrafficPolicy` | string | optional | `"Cluster"` | `Cluster`, `Local` | — | External traffic policy for LoadBalancer services |
| `spec.loadBalancer.healthCheckEnabled` | boolean | optional | `true` | — | — | — |
| `spec.loadBalancer.healthCheckPort` | integer (int32) | optional | `9100` | — | — | — |
| `spec.loadBalancer.loadBalancerIp` | string | optional | — | — | nullable | — |
| `spec.loadBalancer.mode` | string | optional | `"L2"` | `L2`, `BGP` | — | Load balancer mode selection |

#### `spec.maintenanceMode`

| Path | Type | Required | Default | Enum / values | Constraints | Purpose |
|---|---|---|---|---|---|---|
| `spec.maintenanceMode` | boolean | optional | `false` | — | — | — |

#### `spec.managedDatabase`

| Path | Type | Required | Default | Enum / values | Constraints | Purpose |
|---|---|---|---|---|---|---|
| `spec.managedDatabase` | object | optional | — | — | nullable | Configuration for managed High-Availability Postgres clusters via CloudNativePG |
| `spec.managedDatabase.backup` | object | optional | — | — | nullable | Backup configuration for managed databases using Barman |
| `spec.managedDatabase.backup.credentialsSecretRef` | string | required | — | — | — | — |
| `spec.managedDatabase.backup.destinationPath` | string | required | — | — | — | — |
| `spec.managedDatabase.backup.enabled` | boolean | optional | `true` | — | — | — |
| `spec.managedDatabase.backup.retentionPolicy` | string | optional | `"30d"` | — | — | — |
| `spec.managedDatabase.instances` | integer (int32) | optional | `3` | — | — | — |
| `spec.managedDatabase.pooling` | object | optional | — | — | nullable | pgBouncer connection pooling configuration |
| `spec.managedDatabase.pooling.defaultPoolSize` | integer (int32) | optional | `20` | — | — | — |
| `spec.managedDatabase.pooling.enabled` | boolean | optional | `true` | — | — | — |
| `spec.managedDatabase.pooling.maxClientConn` | integer (int32) | optional | `1000` | — | — | — |
| `spec.managedDatabase.pooling.poolMode` | string | optional | `"transaction"` | `session`, `transaction`, `statement` | — | pgBouncer pooling modes |
| `spec.managedDatabase.pooling.replicas` | integer (int32) | optional | `2` | — | — | — |
| `spec.managedDatabase.postgresVersion` | string | optional | `"16"` | — | — | — |
| `spec.managedDatabase.storage` | object | required | — | — | — | Storage configuration for persistent data |
| `spec.managedDatabase.storage.annotations` | object | optional | — | — | nullable | — |
| `spec.managedDatabase.storage.mode` | string | optional | `"PersistentVolume"` | `PersistentVolume`, `Local` | — | Storage mode for persistent data |
| `spec.managedDatabase.storage.nodeAffinity` | object | optional | — | — | preserves unknown fields | Node affinity for local storage mode (optional) |
| `spec.managedDatabase.storage.retentionPolicy` | string | optional | `"Delete"` | `Delete`, `Retain` | — | PVC retention policy on node deletion |
| `spec.managedDatabase.storage.size` | string | required | — | — | — | — |
| `spec.managedDatabase.storage.storageClass` | string | required | — | — | — | — |

#### `spec.maxUnavailable`

| Path | Type | Required | Default | Enum / values | Constraints | Purpose |
|---|---|---|---|---|---|---|
| `spec.maxUnavailable` | integer \| string | required | — | — | — | IntOrString |

#### `spec.minAvailable`

| Path | Type | Required | Default | Enum / values | Constraints | Purpose |
|---|---|---|---|---|---|---|
| `spec.minAvailable` | integer \| string | required | — | — | — | IntOrString |

#### `spec.network`

| Path | Type | Required | Default | Enum / values | Constraints | Purpose |
|---|---|---|---|---|---|---|
| `spec.network` | string | required | — | `mainnet`, `testnet`, `futurenet`, `custom` | — | Target Stellar network |

#### `spec.networkPolicy`

| Path | Type | Required | Default | Enum / values | Constraints | Purpose |
|---|---|---|---|---|---|---|
| `spec.networkPolicy` | object | optional | — | — | nullable | Network Policy configuration |
| `spec.networkPolicy.allowCidrs` | array of string | optional | — | — | — | — |
| `spec.networkPolicy.allowMetricsScrape` | boolean | optional | `true` | — | — | — |
| `spec.networkPolicy.allowNamespaces` | array of string | optional | — | — | — | — |
| `spec.networkPolicy.allowPodSelector` | object | optional | — | — | nullable | — |
| `spec.networkPolicy.enabled` | boolean | optional | `false` | — | — | — |
| `spec.networkPolicy.metricsNamespace` | string | optional | `"monitoring"` | — | — | — |

#### `spec.nodeType`

| Path | Type | Required | Default | Enum / values | Constraints | Purpose |
|---|---|---|---|---|---|---|
| `spec.nodeType` | string | required | — | `Validator`, `Horizon`, `SorobanRpc` | — | Supported Stellar node types |

#### `spec.ociSnapshot`

| Path | Type | Required | Default | Enum / values | Constraints | Purpose |
|---|---|---|---|---|---|---|
| `spec.ociSnapshot` | object | optional | — | — | nullable | OCI-based ledger snapshot sync for multi-region bootstrapping |
| `spec.ociSnapshot.credentialSecretName` | string | required | — | — | — | Name of a K8s Secret in the same namespace containing Docker registry credentials as `config.json` (standard `~/.docker/config.json` format). |
| `spec.ociSnapshot.enabled` | boolean | optional | `false` | — | — | Whether the OCI snapshot feature is enabled (default: false) |
| `spec.ociSnapshot.fixedTag` | string | optional | — | — | nullable | Fixed tag to use when `tag_strategy` is `Fixed` (e.g. `latest`) |
| `spec.ociSnapshot.image` | string | required | — | — | — | Image name within the registry, e.g. `myorg/stellar-snapshot` |
| `spec.ociSnapshot.pull` | boolean | optional | `false` | — | — | Enable pulling a snapshot to bootstrap a new node's PVC (default: false) |
| `spec.ociSnapshot.pullImageRef` | string | optional | — | — | nullable | Image reference to pull from (full `registry/image:tag` string). Required when `pull = true`; if omitted the operator constructs the reference from `registry`, `image`, and `tag_strategy`. |
| `spec.ociSnapshot.push` | boolean | optional | `false` | — | — | Enable pushing snapshots to the registry (default: false) |
| `spec.ociSnapshot.registry` | string | required | — | — | — | OCI registry host, e.g. `ghcr.io` or `registry-1.docker.io` |
| `spec.ociSnapshot.tagStrategy` | string | optional | `"latestLedger"` | `latestLedger`, `fixed` | — | Tag used when pushing/pulling the snapshot image. With `LatestLedger` the tag is `snapshot-<ledger_seq>`; with `Fixed` the literal `fixed_tag` value is used. |

#### `spec.podAntiAffinity`

| Path | Type | Required | Default | Enum / values | Constraints | Purpose |
|---|---|---|---|---|---|---|
| `spec.podAntiAffinity` | string | optional | `"Hard"` | `Hard`, `Soft`, `Disabled` | — | When not `Disabled`, the operator adds default pod anti-affinity so pods with the same `stellar-network` label (and same component) are not co-located on one node. |

#### `spec.readPoolEndpoint`

| Path | Type | Required | Default | Enum / values | Constraints | Purpose |
|---|---|---|---|---|---|---|
| `spec.readPoolEndpoint` | string | optional | — | — | nullable | DNS endpoint for the read-replica pool Service. |

#### `spec.readReplicaConfig`

| Path | Type | Required | Default | Enum / values | Constraints | Purpose |
|---|---|---|---|---|---|---|
| `spec.readReplicaConfig` | object | optional | — | — | nullable | Read replica pool configuration for horizontal scaling Enables creating read-only replicas with traffic routing strategies |
| `spec.readReplicaConfig.archiveSharding` | boolean | optional | `false` | — | — | Enable history archive sharding When true, replicas serve different archives to balance bandwidth |
| `spec.readReplicaConfig.replicas` | integer (int32) | optional | `1` | — | — | Number of read-only replicas |
| `spec.readReplicaConfig.resources` | object | optional | `{"limits":{"cpu":"2","memory":"4Gi"},"requests":{"cpu":"500m","memory":"1Gi"}}` | — | — | Compute resource requirements for read replicas |
| `spec.readReplicaConfig.resources.limits` | object | required | — | — | — | Resource specification for CPU and memory |
| `spec.readReplicaConfig.resources.limits.cpu` | string | required | — | — | — | — |
| `spec.readReplicaConfig.resources.limits.memory` | string | required | — | — | — | — |
| `spec.readReplicaConfig.resources.requests` | object | required | — | — | — | Resource specification for CPU and memory |
| `spec.readReplicaConfig.resources.requests.cpu` | string | required | — | — | — | — |
| `spec.readReplicaConfig.resources.requests.memory` | string | required | — | — | — | — |
| `spec.readReplicaConfig.strategy` | string | optional | `"RoundRobin"` | `RoundRobin`, `FreshnessPreferred` | — | Load balancing strategy |

#### `spec.replicas`

| Path | Type | Required | Default | Enum / values | Constraints | Purpose |
|---|---|---|---|---|---|---|
| `spec.replicas` | integer (int32) | optional | `1` | — | — | — |

#### `spec.resources`

| Path | Type | Required | Default | Enum / values | Constraints | Purpose |
|---|---|---|---|---|---|---|
| `spec.resources` | object | optional | `{"limits":{"cpu":"2","memory":"4Gi"},"requests":{"cpu":"500m","memory":"1Gi"}}` | — | — | Kubernetes-style resource requirements |
| `spec.resources.limits` | object | required | — | — | — | Resource specification for CPU and memory |
| `spec.resources.limits.cpu` | string | required | — | — | — | — |
| `spec.resources.limits.memory` | string | required | — | — | — | — |
| `spec.resources.requests` | object | required | — | — | — | Resource specification for CPU and memory |
| `spec.resources.requests.cpu` | string | required | — | — | — | — |
| `spec.resources.requests.memory` | string | required | — | — | — | — |

#### `spec.restoreFromSnapshot`

| Path | Type | Required | Default | Enum / values | Constraints | Purpose |
|---|---|---|---|---|---|---|
| `spec.restoreFromSnapshot` | object | optional | — | — | nullable | Bootstrap this node from an existing VolumeSnapshot instead of an empty volume (Validator only). The PVC will be created from the specified snapshot for near-instant startup. |
| `spec.restoreFromSnapshot.namespace` | string | optional | — | — | nullable | Optional: namespace of the VolumeSnapshot if different from the StellarNode. Requires CrossNamespaceVolumeDataSource where supported. |
| `spec.restoreFromSnapshot.volumeSnapshotName` | string | required | — | — | — | Name of the VolumeSnapshot to restore from (must exist in the same namespace as the StellarNode). |

#### `spec.serviceMesh`

| Path | Type | Required | Default | Enum / values | Constraints | Purpose |
|---|---|---|---|---|---|---|
| `spec.serviceMesh` | object | optional | — | — | nullable | Service mesh configuration (Istio/Linkerd) for mTLS and advanced traffic control |
| `spec.serviceMesh.istio` | object | optional | — | — | nullable | Istio-specific configuration |
| `spec.serviceMesh.istio.circuitBreaker` | object | optional | — | — | nullable | Circuit breaker configuration for outlier detection |
| `spec.serviceMesh.istio.circuitBreaker.consecutiveErrors` | integer (uint32) | optional | `5` | — | min 0.0 | Number of consecutive errors before opening circuit |
| `spec.serviceMesh.istio.circuitBreaker.minRequestVolume` | integer (uint32) | optional | `10` | — | min 0.0 | Minimum request volume before applying circuit breaking |
| `spec.serviceMesh.istio.circuitBreaker.timeWindowSecs` | integer (uint32) | optional | `30` | — | min 0.0 | Time window in seconds for counting errors |
| `spec.serviceMesh.istio.mtlsMode` | string | optional | `"STRICT"` | `STRICT`, `PERMISSIVE` | — | mTLS mode (STRICT or PERMISSIVE) |
| `spec.serviceMesh.istio.retries` | object | optional | — | — | nullable | Retry policy for failed requests |
| `spec.serviceMesh.istio.retries.backoffMs` | integer (uint32) | optional | `25` | — | min 0.0 | Backoff duration in milliseconds |
| `spec.serviceMesh.istio.retries.maxRetries` | integer (uint32) | optional | `3` | — | min 0.0 | Maximum number of retries |
| `spec.serviceMesh.istio.retries.retryableStatusCodes` | array of integer | optional | `[]` | — | — | Retryable status codes (e.g., 503, 504) |
| `spec.serviceMesh.istio.timeoutSecs` | integer (uint32) | optional | `30` | — | min 0.0 | VirtualService timeout in seconds |
| `spec.serviceMesh.linkerd` | object | optional | — | — | nullable | Linkerd-specific configuration |
| `spec.serviceMesh.linkerd.autoMtls` | boolean | optional | `true` | — | — | Enable automatic mTLS |
| `spec.serviceMesh.linkerd.policyMode` | string | optional | `"allow"` | — | — | Policy mode (deny, audit, allow) |
| `spec.serviceMesh.sidecarInjection` | boolean | optional | `true` | — | — | Enable sidecar injection for this node |

#### `spec.snapshotSchedule`

| Path | Type | Required | Default | Enum / values | Constraints | Purpose |
|---|---|---|---|---|---|---|
| `spec.snapshotSchedule` | object | optional | — | — | nullable | Schedule and options for taking CSI VolumeSnapshots of the node's data PVC (Validator only). Enables zero-downtime backups and creating new nodes from snapshots. |
| `spec.snapshotSchedule.flushBeforeSnapshot` | boolean | optional | `false` | — | — | If true, the operator will attempt to flush/lock the Stellar database briefly before creating the snapshot (e.g. via stellar-core HTTP or exec). Requires the node to be healthy. |
| `spec.snapshotSchedule.retentionCount` | integer (uint32) | optional | `0` | — | min 0.0 | Maximum number of snapshots to retain per node. Oldest snapshots are deleted when exceeded. 0 means no limit. |
| `spec.snapshotSchedule.schedule` | string | optional | — | — | nullable | Cron expression for scheduled snapshots (e.g. "0 2 * * *" for daily at 2 AM). If unset, snapshots are only taken when triggered via annotation `stellar.org/request-snapshot: "true"`. |
| `spec.snapshotSchedule.volumeSnapshotClassName` | string | optional | — | — | nullable | VolumeSnapshotClass name. If unset, the default class for the PVC's driver is used. |

#### `spec.sorobanConfig`

| Path | Type | Required | Default | Enum / values | Constraints | Purpose |
|---|---|---|---|---|---|---|
| `spec.sorobanConfig` | object | optional | — | — | nullable | Soroban RPC server configuration |
| `spec.sorobanConfig.captiveCoreConfig` | string | optional | — | — | nullable | — |
| `spec.sorobanConfig.captiveCoreStructuredConfig` | object | optional | — | — | nullable | Captive Core configuration for Soroban RPC |
| `spec.sorobanConfig.captiveCoreStructuredConfig.additionalConfig` | string | optional | — | — | nullable | — |
| `spec.sorobanConfig.captiveCoreStructuredConfig.historyArchiveUrls` | array of string | optional | `[]` | — | — | — |
| `spec.sorobanConfig.captiveCoreStructuredConfig.httpPort` | integer (uint16) | optional | — | — | nullable; min 0.0 | — |
| `spec.sorobanConfig.captiveCoreStructuredConfig.logLevel` | string | optional | — | — | nullable | — |
| `spec.sorobanConfig.captiveCoreStructuredConfig.networkPassphrase` | string | optional | — | — | nullable | — |
| `spec.sorobanConfig.captiveCoreStructuredConfig.peerPort` | integer (uint16) | optional | — | — | nullable; min 0.0 | — |
| `spec.sorobanConfig.enablePreflight` | boolean | optional | `true` | — | — | — |
| `spec.sorobanConfig.maxEventsPerRequest` | integer (uint32) | optional | `10000` | — | min 0.0 | — |
| `spec.sorobanConfig.cache` | object | optional | — | — | nullable | Bounded fail-open cache for read-only Soroban RPC state requests |
| `spec.sorobanConfig.cache.enabled` | boolean | optional | `false` | — | — | — |
| `spec.sorobanConfig.cache.ttlSecs` | integer (int64) | optional | `30` | — | min 1.0 | — |
| `spec.sorobanConfig.cache.maxEntries` | integer (int64) | optional | `10000` | — | min 1.0; max 10000.0 | — |
| `spec.sorobanConfig.cache.maxBytes` | integer (int64) | optional | `67108864` | — | min 1.0; max 67108864.0 | — |
| `spec.sorobanConfig.cache.image` | string | optional | — | — | nullable | — |
| `spec.sorobanConfig.stellarCoreUrl` | string | required | — | — | — | — |

#### `spec.storage`

| Path | Type | Required | Default | Enum / values | Constraints | Purpose |
|---|---|---|---|---|---|---|
| `spec.storage` | object | optional | `{"mode":"PersistentVolume","retentionPolicy":"Delete","size":"100Gi","storageClass":"standard"}` | — | — | Storage configuration for persistent data |
| `spec.storage.annotations` | object | optional | — | — | nullable | — |
| `spec.storage.mode` | string | optional | `"PersistentVolume"` | `PersistentVolume`, `Local` | — | Storage mode for persistent data |
| `spec.storage.nodeAffinity` | object | optional | — | — | preserves unknown fields | Node affinity for local storage mode (optional) |
| `spec.storage.retentionPolicy` | string | optional | `"Delete"` | `Delete`, `Retain` | — | PVC retention policy on node deletion |
| `spec.storage.size` | string | required | — | — | — | — |
| `spec.storage.storageClass` | string | required | — | — | — | — |
| `spec.storage.snapshotRef` | object | optional | — | — | nullable | Bootstrap this node from a pre-computed snapshot or compressed DB backup. Supports CSI VolumeSnapshot (zero-copy PVC clone) or a compressed archive (.tar.gz / .tar.zst) downloaded by an init container before Stellar Core starts. Reduces catch-up time from days to minutes. |
| `spec.storage.snapshotRef.volumeSnapshotName` | string | optional | — | — | nullable | Name of an existing VolumeSnapshot (snapshot.storage.k8s.io/v1) in the same namespace. The PVC is provisioned from this snapshot — no init container is needed. |
| `spec.storage.snapshotRef.volumeSnapshotNamespace` | string | optional | — | — | nullable | Optional namespace of the VolumeSnapshot when it lives in a different namespace. Requires CrossNamespaceVolumeDataSource feature gate. |
| `spec.storage.snapshotRef.backupUrl` | string | optional | — | — | nullable | URL of a compressed DB backup archive (.tar.gz or .tar.zst). Supported schemes: s3://bucket/path/backup.tar.gz or https://host/path/backup.tar.gz. An init container (snapshot-restore) downloads and extracts the archive into /data before Stellar Core starts. |
| `spec.storage.snapshotRef.credentialsSecretRef` | string | optional | — | — | nullable | Name of a Kubernetes Secret containing credentials for the backup URL. For S3: keys AWS_ACCESS_KEY_ID, AWS_SECRET_ACCESS_KEY, AWS_DEFAULT_REGION. For HTTPS: key BEARER_TOKEN. |
| `spec.storage.snapshotRef.restoreImage` | string | optional | — | — | nullable | Container image for the restore init container. Defaults to amazon/aws-cli:latest for S3 URLs, alpine:3 for HTTPS. |

#### `spec.strategy`

| Path | Type | Required | Default | Enum / values | Constraints | Purpose |
|---|---|---|---|---|---|---|
| `spec.strategy` | object | optional | `{"type":"rollingUpdate"}` | — | — | Rollout strategy for updates (RollingUpdate or Canary) |
| `spec.strategy.canary` | object | optional | — | — | nullable | Configuration for Canary rollout |
| `spec.strategy.canary.checkIntervalSeconds` | integer (int32) | optional | `300` | — | — | — |
| `spec.strategy.canary.weight` | integer (int32) | optional | `10` | — | — | — |
| `spec.strategy.type` | string | required | — | `rollingUpdate`, `canary` | — | Rollout strategy type |

#### `spec.suspended`

| Path | Type | Required | Default | Enum / values | Constraints | Purpose |
|---|---|---|---|---|---|---|
| `spec.suspended` | boolean | optional | `false` | — | — | — |

#### `spec.topologySpreadConstraints`

| Path | Type | Required | Default | Enum / values | Constraints | Purpose |
|---|---|---|---|---|---|---|
| `spec.topologySpreadConstraints` | array of object | required | — | — | — | — |

#### `spec.validatorConfig`

| Path | Type | Required | Default | Enum / values | Constraints | Purpose |
|---|---|---|---|---|---|---|
| `spec.validatorConfig` | object | optional | — | — | nullable | Validator-specific configuration |
| `spec.validatorConfig.catchupComplete` | boolean | optional | `false` | — | — | Node is in catchup mode (syncing historical data) |
| `spec.validatorConfig.enableHistoryArchive` | boolean | optional | `false` | — | — | Enable history archive for this validator |
| `spec.validatorConfig.historyArchiveUrls` | array of string | optional | — | — | — | History archive URLs to fetch from |
| `spec.validatorConfig.hsmConfig` | object | optional | — | — | nullable | Cloud HSM configuration for secure key loading (optional) |
| `spec.validatorConfig.hsmConfig.hsmCredentialsSecretRef` | string | optional | — | — | nullable | — |
| `spec.validatorConfig.hsmConfig.hsmIp` | string | optional | — | — | nullable | — |
| `spec.validatorConfig.hsmConfig.pkcs11LibPath` | string | required | — | — | — | — |
| `spec.validatorConfig.hsmConfig.provider` | string | required | — | `AWS`, `Azure` | — | Supported HSM Providers |
| `spec.validatorConfig.keySource` | string | optional | `"secret"` | `secret`, `kMS` | — | Source of the validator seed (Secret or KMS) |
| `spec.validatorConfig.kmsConfig` | object | optional | — | — | nullable | KMS configuration for fetching the validator seed |
| `spec.validatorConfig.kmsConfig.fetcherImage` | string | optional | — | — | nullable | — |
| `spec.validatorConfig.kmsConfig.keyId` | string | required | — | — | — | — |
| `spec.validatorConfig.kmsConfig.provider` | string | required | — | — | — | — |
| `spec.validatorConfig.kmsConfig.region` | string | optional | — | — | nullable | — |
| `spec.validatorConfig.quorumSet` | string | optional | — | — | nullable | Quorum set configuration as TOML string |
| `spec.validatorConfig.seedSecretRef` | string | optional | `""` | — | — | Secret name containing the validator seed (key: STELLAR_CORE_SEED) DEPRECATED: Use seed_secret_source for KMS/ESO/CSI-backed secrets in production |
| `spec.validatorConfig.seedSecretSource` | object | optional | — | — | nullable | Production seed source: ESO (AWS SM / GCP SM / Vault) or CSI Secret Store Driver. Takes precedence over seed_secret_ref when present. |
| `spec.validatorConfig.seedSecretSource.csiRef` | object | optional | — | — | nullable | Secrets Store CSI Driver — **recommended for production**. Mounts the seed directly from a KMS/Vault into the pod filesystem via a CSI volume. The seed is never written to etcd. The controller injects `STELLAR_SEED_FILE` into the container pointing at the mount path; stellar-core reads the key from that file path. |
| `spec.validatorConfig.seedSecretSource.csiRef.mountPath` | string | optional | `"/mnt/secrets/validator"` | — | nullable | Directory inside the container where the CSI driver mounts secrets. Defaults to `/mnt/secrets/validator`. |
| `spec.validatorConfig.seedSecretSource.csiRef.secretProviderClassName` | string | required | — | — | — | Name of the `SecretProviderClass` CR (from secrets-store.csi.x-k8s.io) that defines which secrets to mount and from which provider. |
| `spec.validatorConfig.seedSecretSource.csiRef.seedFileName` | string | optional | `"seed"` | — | nullable | File name within `mount_path` that contains the seed value. Defaults to `seed`. |
| `spec.validatorConfig.seedSecretSource.externalRef` | object | optional | — | — | nullable | External Secrets Operator — **recommended for production**. The operator creates an `ExternalSecret` CR which causes ESO to pull the seed from AWS Secrets Manager, GCP Secret Manager, HashiCorp Vault, or any other supported backend and materialise it as a Kubernetes Secret in the same namespace. The seed value is never stored in the CRD itself. |
| `spec.validatorConfig.seedSecretSource.externalRef.name` | string | required | — | — | — | Name of the `ExternalSecret` CR the operator will create/manage. Must be unique within the namespace. |
| `spec.validatorConfig.seedSecretSource.externalRef.refreshInterval` | string | optional | `"1h"` | — | nullable | How often ESO should re-sync the secret from the remote backend. Kubernetes duration string, e.g. `"1h"`, `"30m"`. Defaults to `"1h"` if not specified. |
| `spec.validatorConfig.seedSecretSource.externalRef.remoteKey` | string | required | — | — | — | Path / identifier of the secret in the remote backend. Examples: - AWS Secrets Manager: `"prod/stellar/validator-seed"` - GCP Secret Manager: `"projects/MY_PROJECT/secrets/stellar-validator-seed"` - HashiCorp Vault: `"secret/data/stellar/validator"` |
| `spec.validatorConfig.seedSecretSource.externalRef.remoteProperty` | string | optional | — | — | nullable | Property (field) inside the remote secret to extract. Required for secrets that store a JSON object (e.g., `{"seed": "S..."}`) and you only want the `seed` value. Leave empty to use the whole secret value as the seed. |
| `spec.validatorConfig.seedSecretSource.externalRef.secretStoreRef` | object | required | — | — | — | Reference to the `SecretStore` or `ClusterSecretStore` that connects ESO to the remote backend (AWS SM, GCP SM, Vault, etc.). |
| `spec.validatorConfig.seedSecretSource.externalRef.secretStoreRef.kind` | string | optional | `"ClusterSecretStore"` | — | — | Kind of the store resource. - `"SecretStore"` — namespaced store (only works within the same namespace) - `"ClusterSecretStore"` — cluster-wide store (recommended for production) |
| `spec.validatorConfig.seedSecretSource.externalRef.secretStoreRef.name` | string | required | — | — | — | Name of the `SecretStore` / `ClusterSecretStore` resource. |
| `spec.validatorConfig.seedSecretSource.localRef` | object | optional | — | — | nullable | Plain Kubernetes Secret — **development only**. Points to an existing `Secret` in the same namespace. The secret must contain the key specified in `key` (defaults to `STELLAR_CORE_SEED`). |
| `spec.validatorConfig.seedSecretSource.localRef.key` | string | optional | `"STELLAR_CORE_SEED"` | — | nullable | Key within the secret that holds the seed value. Defaults to `STELLAR_CORE_SEED` if not specified. |
| `spec.validatorConfig.seedSecretSource.localRef.name` | string | required | — | — | — | Name of the `Secret` in the same namespace. |
| `spec.validatorConfig.seedSecretSource.vaultRef` | object | optional | — | — | nullable | HashiCorp Vault via the **Vault Agent Injector** (init + sidecar). Requires the Vault Agent Injector mutating webhook in the cluster. The operator sets standard `vault.hashicorp.com/*` pod annotations; the injector adds the Vault Agent containers and renders the secret file under `/vault/secrets/`. |
| `spec.validatorConfig.seedSecretSource.vaultRef.extraPodAnnotations` | array of object | optional | — | — | — | Additional `vault.hashicorp.com/*` or other pod annotations to merge. |
| `spec.validatorConfig.seedSecretSource.vaultRef.extraPodAnnotations[].name` | string | required | — | — | — | — |
| `spec.validatorConfig.seedSecretSource.vaultRef.extraPodAnnotations[].value` | string | required | — | — | — | — |
| `spec.validatorConfig.seedSecretSource.vaultRef.restartOnSecretRotation` | boolean | optional | `false` | — | — | When true, the operator compares Vault secret-version annotations on pods and rolls the StatefulSet when the version changes after sync. |
| `spec.validatorConfig.seedSecretSource.vaultRef.role` | string | required | — | — | — | Vault Kubernetes auth role bound to this pod's ServiceAccount. |
| `spec.validatorConfig.seedSecretSource.vaultRef.secretFileName` | string | optional | — | — | nullable | Base file name rendered under `/vault/secrets/` (default `stellar-seed`). |
| `spec.validatorConfig.seedSecretSource.vaultRef.secretKey` | string | optional | — | — | nullable | JSON field under `.Data.data` for KV v2 (default `seed`). Ignored if `template` is set. |
| `spec.validatorConfig.seedSecretSource.vaultRef.secretPath` | string | required | — | — | — | Path passed to `vault.hashicorp.com/agent-inject-secret-<file>` (KV v1/v2 path as in Vault). |
| `spec.validatorConfig.seedSecretSource.vaultRef.template` | string | optional | — | — | nullable | Custom Agent template; when set, overrides the default KV v2 template. |
| `spec.validatorConfig.vlSource` | string | optional | — | — | nullable | Trusted source for Validator Selection List (VSL) |

#### `spec.version`

| Path | Type | Required | Default | Enum / values | Constraints | Purpose |
|---|---|---|---|---|---|---|
| `spec.version` | string | required | — | — | — | — |

#### `spec.vpaConfig`

| Path | Type | Required | Default | Enum / values | Constraints | Purpose |
|---|---|---|---|---|---|---|
| `spec.vpaConfig` | object | optional | — | — | nullable | VPA configuration |
| `spec.vpaConfig.containerPolicies` | array of object | optional | — | — | — | — |
| `spec.vpaConfig.containerPolicies[].containerName` | string | required | — | — | — | — |
| `spec.vpaConfig.containerPolicies[].maxAllowed` | object | optional | — | — | nullable | — |
| `spec.vpaConfig.containerPolicies[].minAllowed` | object | optional | — | — | nullable | — |
| `spec.vpaConfig.updateMode` | string | optional | `"Initial"` | `Initial`, `Auto` | — | VPA update mode |

### Status fields

| Path | Type | Required | Default | Enum / values | Constraints | Purpose |
|---|---|---|---|---|---|---|
| `status.bgpStatus` | object | optional | — | — | nullable | BGP advertisement status (when using BGP mode) |
| `status.bgpStatus.activePeers` | integer (int32) | required | — | — | — | Number of active BGP peers |
| `status.bgpStatus.advertisedPrefixes` | array of string | optional | — | — | — | Advertised IP prefixes |
| `status.bgpStatus.lastUpdate` | string | optional | — | — | nullable | Last BGP update time |
| `status.bgpStatus.sessionsEstablished` | boolean | required | — | — | — | Whether BGP sessions are established |
| `status.canaryReadyReplicas` | integer (int32) | optional | `0` | — | — | Current number of ready canary replicas (for canary deployments) |
| `status.canaryStartTime` | string | optional | — | — | nullable | Timestamp when the canary was created (RFC3339) |
| `status.canaryVersion` | string | optional | — | — | nullable | Version deployed in the canary deployment (if active) |
| `status.conditions` | array of object | optional | — | — | — | Readiness conditions following Kubernetes conventions Standard conditions include: - Ready: True when all sub-resources are healthy and the node is operational - Progressing: True when the node is being created, updated, or syncing - Degraded: True when the node is operational but experiencing issues |
| `status.conditions[].lastTransitionTime` | string | required | — | — | — | — |
| `status.conditions[].message` | string | required | — | — | — | — |
| `status.conditions[].observedGeneration` | integer (int64) | optional | — | — | nullable | — |
| `status.conditions[].reason` | string | required | — | — | — | — |
| `status.conditions[].status` | string | required | — | — | — | — |
| `status.conditions[].type` | string | required | — | — | — | — |
| `status.drStatus` | object | optional | — | — | nullable | Status of the cross-region disaster recovery setup (if enabled) |
| `status.drStatus.currentRole` | string | optional | — | `primary`, `standby` | nullable | Role of a node in a DR configuration |
| `status.drStatus.failoverActive` | boolean | required | — | — | — | — |
| `status.drStatus.lastDrillResult` | object | optional | — | — | nullable | Result of a DR drill execution |
| `status.drStatus.lastDrillResult.applicationAvailability` | boolean | required | — | — | — | Whether application remained available during drill |
| `status.drStatus.lastDrillResult.completedAt` | string | optional | — | — | nullable | Timestamp when drill completed |
| `status.drStatus.lastDrillResult.message` | string | required | — | — | — | Human-readable message about drill result |
| `status.drStatus.lastDrillResult.standbyTakeoverSuccess` | boolean | required | — | — | — | Whether standby successfully took over |
| `status.drStatus.lastDrillResult.startedAt` | string | required | — | — | — | Timestamp when drill started |
| `status.drStatus.lastDrillResult.status` | string | required | — | `pending`, `running`, `success`, `failed`, `rolledback` | — | Drill execution status |
| `status.drStatus.lastDrillResult.timeToRecoveryMs` | integer (uint64) | optional | — | — | nullable; min 0.0 | Time to recovery in milliseconds |
| `status.drStatus.lastDrillTime` | string | optional | — | — | nullable | — |
| `status.drStatus.lastPeerContact` | string | optional | — | — | nullable | — |
| `status.drStatus.peerHealth` | string | optional | — | — | nullable | — |
| `status.drStatus.syncLag` | integer (uint64) | optional | — | — | nullable; min 0.0 | — |
| `status.endpoint` | string | optional | — | — | nullable | Endpoint where the node is accessible (Service ClusterIP or external) |
| `status.externalIp` | string | optional | — | — | nullable | External load balancer IP assigned by MetalLB |
| `status.forensicSnapshotPhase` | string | optional | — | — | nullable | Phase of the last forensic snapshot request (`Pending`, `Capturing`, `Complete`, `Failed`). |
| `status.lastMigratedVersion` | string | optional | — | — | nullable | Version of the database schema after last successful migration |
| `status.ledgerSequence` | integer (uint64) | optional | — | — | nullable; min 0.0 | For validators: current ledger sequence number |
| `status.ledgerUpdatedAt` | string | optional | — | — | nullable | Timestamp of the last ledger update (RFC3339) |
| `status.message` | string | optional | — | — | nullable | Human-readable message about current state |
| `status.observedGeneration` | integer (int64) | optional | — | — | nullable | Observed generation for status sync detection |
| `status.phase` | string | required | — | — | — | Current phase of the node lifecycle (Pending, Creating, Running, Syncing, Ready, Failed, Degraded, Remediating, Terminating) DEPRECATED: Use the conditions array instead. This field is maintained for backward compatibility and will be removed in a future version. The phase is now derived from the conditions. |
| `status.quorumAnalysisTimestamp` | string | optional | — | — | nullable | Timestamp of last quorum analysis (RFC3339) |
| `status.quorumFragility` | number (double) | optional | — | — | nullable | Quorum fragility score (0.0 = resilient, 1.0 = fragile) Only populated for validator nodes |
| `status.readyReplicas` | integer (int32) | optional | `0` | — | — | Current number of ready replicas |
| `status.replicas` | integer (int32) | optional | `0` | — | — | Total number of desired replicas |
| `status.vaultObservedSecretVersion` | string | optional | — | — | nullable | Last observed Vault secret version annotation (for rotation-driven rollouts). |
| `status.labelPropagationStatus` | string | optional | — | — | nullable | Result of the last label propagation pass. One of "Synced", "Partial", "Failed" |
| `status.snapshotBootstrap` | object | optional | — | — | nullable | Bootstrap status when the node was started from a snapshot or compressed backup. Tracks the restore phase and time-to-sync for observability. A secondsToSync value ≤ 600 satisfies the "synced within 10 minutes" acceptance criterion. |
| `status.snapshotBootstrap.phase` | string | required | — | — | — | Current phase of the bootstrap operation. One of: Pending, Restoring, Restored, Syncing, Synced, Failed |
| `status.snapshotBootstrap.source` | string | optional | — | — | nullable | Source used for bootstrap (VolumeSnapshot name or backup URL). |
| `status.snapshotBootstrap.restoreStartedAt` | string | optional | — | — | nullable | RFC3339 timestamp when the restore init container started. |
| `status.snapshotBootstrap.restoreCompletedAt` | string | optional | — | — | nullable | RFC3339 timestamp when the restore init container completed successfully. |
| `status.snapshotBootstrap.syncedAt` | string | optional | — | — | nullable | RFC3339 timestamp when the node first reached Synced state after bootstrap. |
| `status.snapshotBootstrap.secondsToSync` | integer (uint64) | optional | — | — | nullable; min 0.0 | Elapsed seconds from restore completion to first Synced state. A value ≤ 600 satisfies the "synced within 10 minutes" acceptance criterion. |
| `status.snapshotBootstrap.message` | string | optional | — | — | nullable | Human-readable message about the current bootstrap state. |

## Related documentation

- [`docs/api-reference.md`](../api-reference.md) — auto-generated field listing
- [`docs/adr/0004-crd-versioning-strategy.md`](../adr/0004-crd-versioning-strategy.md) — CRD versioning
- `src/crd/stellar_node.rs` — spec validation and status types
- `src/controller/conditions.rs` — condition helpers
